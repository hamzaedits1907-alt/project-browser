//! Per-process resource readings from Win32. Nothing here is estimated: memory
//! is the private working set (what Task Manager's "Memory" column shows), CPU
//! is derived from kernel+user time deltas, GPU from the PDH "GPU Engine"
//! counters that Task Manager also uses.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_CSTATUS_VALID_DATA,
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

#[derive(Debug, Clone, Copy)]
pub struct ProcReading {
    /// Private working set in bytes.
    pub memory: u64,
    /// Total kernel + user CPU time in 100ns units.
    pub cpu_time: u64,
}

fn filetime_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

pub fn read_process(pid: u32) -> Option<ProcReading> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX2 { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32, ..Default::default() };
        let mem_ok =
            GetProcessMemoryInfo(handle, &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS, counters.cb).is_ok();
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let times_ok = GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u).is_ok();
        let _ = CloseHandle(handle);
        if !mem_ok || !times_ok {
            return None;
        }
        Some(ProcReading { memory: counters.PrivateWorkingSetSize as u64, cpu_time: filetime_u64(k) + filetime_u64(u) })
    }
}

/// When this process was created, as Unix ms. Used for real startup timing.
pub fn process_start_unix_ms() -> Option<i64> {
    unsafe {
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).ok()?;
        // FILETIME is 100ns ticks since 1601-01-01.
        Some((filetime_u64(c) / 10_000) as i64 - 11_644_473_600_000)
    }
}

pub fn logical_cpu_count() -> u32 {
    let mut info = SYSTEM_INFO::default();
    unsafe { GetSystemInfo(&mut info) };
    info.dwNumberOfProcessors.max(1)
}

/// Turns cumulative CPU time into a percentage of total machine capacity.
pub struct CpuTracker {
    prev: HashMap<u32, (u64, Instant)>,
    cpus: f64,
}

impl Default for CpuTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuTracker {
    pub fn new() -> Self {
        CpuTracker { prev: HashMap::new(), cpus: logical_cpu_count() as f64 }
    }

    /// Returns `None` on the first sample of a process (no delta yet).
    pub fn percent(&mut self, pid: u32, cpu_time: u64, now: Instant) -> Option<f64> {
        let prev = self.prev.insert(pid, (cpu_time, now));
        let (prev_time, prev_at) = prev?;
        let wall = now.duration_since(prev_at).as_secs_f64();
        if wall <= 0.0 {
            return None;
        }
        let used = cpu_time.saturating_sub(prev_time) as f64 / 10_000_000.0;
        Some((used / wall / self.cpus * 100.0).clamp(0.0, 100.0))
    }

    pub fn retain(&mut self, alive: &HashSet<u32>) {
        self.prev.retain(|pid, _| alive.contains(pid));
    }
}

/// GPU utilisation for a set of processes via PDH `\GPU Engine(*)\Utilization Percentage`.
pub struct GpuSampler {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    primed: bool,
}

// PDH handles are process-wide and may be used from any thread; the sampler is
// owned by the controller thread only.
unsafe impl Send for GpuSampler {}

impl GpuSampler {
    pub fn new() -> Option<GpuSampler> {
        unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            let path = windows::core::w!("\\GPU Engine(*)\\Utilization Percentage");
            if PdhAddEnglishCounterW(query, path, 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            PdhCollectQueryData(query);
            Some(GpuSampler { query, counter, primed: false })
        }
    }

    /// Highest per-engine-type utilisation summed across `pids`, matching how
    /// Task Manager reports per-process GPU. `None` until two samples exist.
    pub fn sample(&mut self, pids: &HashSet<u32>) -> Option<f64> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            if !self.primed {
                self.primed = true;
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let status = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if status != PDH_MORE_DATA || size == 0 {
                return Some(0.0);
            }
            // Buffer must be 8-byte aligned for the f64 values it holds.
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                return None;
            }
            let mut per_engine: HashMap<String, f64> = HashMap::new();
            for i in 0..count as usize {
                let item = &*items.add(i);
                if item.FmtValue.CStatus != PDH_CSTATUS_VALID_DATA {
                    continue;
                }
                let Ok(name) = item.szName.to_string() else { continue };
                let Some((pid, engine)) = parse_gpu_instance(&name) else { continue };
                if pids.contains(&pid) {
                    *per_engine.entry(engine).or_default() += item.FmtValue.Anonymous.doubleValue;
                }
            }
            Some(per_engine.values().copied().fold(0.0, f64::max).clamp(0.0, 100.0))
        }
    }
}

impl Drop for GpuSampler {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

/// Parses `pid_1234_luid_0x0_0x1_phys_0_eng_3_engtype_3D` into `(1234, "3D")`.
pub fn parse_gpu_instance(name: &str) -> Option<(u32, String)> {
    let rest = name.strip_prefix("pid_")?;
    let pid: u32 = rest.split('_').next()?.parse().ok()?;
    let engine = name.rsplit_once("engtype_")?.1.to_string();
    Some((pid, engine))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gpu_instance_names() {
        assert_eq!(parse_gpu_instance("pid_1234_luid_0x00000000_0x0000D1A5_phys_0_eng_0_engtype_3D"), Some((1234, "3D".into())));
        assert_eq!(parse_gpu_instance("_Total"), None);
    }

    #[test]
    fn reads_own_process() {
        let r = read_process(std::process::id()).expect("own process is readable");
        assert!(r.memory > 0);
        assert!(process_start_unix_ms().unwrap() <= crate::error::now_ms());
    }

    #[test]
    fn cpu_percent_needs_two_samples() {
        let mut t = CpuTracker::new();
        let start = Instant::now();
        assert_eq!(t.percent(1, 0, start), None);
        let later = start + std::time::Duration::from_secs(1);
        let pct = t.percent(1, 10_000_000, later).unwrap();
        assert!(pct > 0.0 && pct <= 100.0);
    }
}
