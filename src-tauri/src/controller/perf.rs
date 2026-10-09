//! Real resource sampling for the performance page, gaming dashboard and the
//! new-tab RAM indicator. Runs only while something is subscribed (or Gaming
//! Mode needs heavy-tab detection); otherwise nothing polls.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::Controller;
use crate::error::now_ms;
use crate::native::metrics::{read_process, CpuTracker, GpuSampler};
use crate::native::{self, ProcSnapshot};
use crate::tabs::{Lifecycle, TabId};

static FIRST_SAMPLE_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const SUBSCRIBED_INTERVAL: Duration = Duration::from_secs(2);
const GAMING_INTERVAL: Duration = Duration::from_secs(5);
const HEAVY_CPU_PERCENT: f64 = 25.0;

pub struct PerfState {
    pub(crate) subscribers: HashSet<String>,
    pub(crate) next_at: Option<Instant>,
    in_flight: bool,
    cpu: CpuTracker,
    gpu: Option<GpuSampler>,
    gpu_failed: bool,
    pub(crate) last: Option<PerfSample>,
}

impl PerfState {
    pub fn new() -> Self {
        PerfState { subscribers: HashSet::new(), next_at: None, in_flight: false, cpu: CpuTracker::new(), gpu: None, gpu_failed: false, last: None }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStat {
    pub pid: u32,
    pub kind: String,
    pub label: String,
    pub memory: u64,
    pub cpu: Option<f64>,
    pub tab_ids: Vec<TabId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabStat {
    pub id: TabId,
    pub memory: u64,
    pub cpu: Option<f64>,
    /// The renderer also hosts other tabs, so `memory` is shared.
    pub shared: bool,
    pub heavy: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfSample {
    pub timestamp: i64,
    pub total_memory: u64,
    pub shell_memory: u64,
    pub webview_memory: u64,
    pub total_cpu: Option<f64>,
    pub gpu: Option<f64>,
    pub processes: Vec<ProcessStat>,
    pub tabs: Vec<TabStat>,
    pub tab_count: usize,
    pub live_tabs: usize,
    pub sleeping_tabs: usize,
    pub unloaded_tabs: usize,
    /// Whether renderer processes could be attributed to tabs (per-tab memory is unavailable when false).
    pub tab_attribution: bool,
    pub startup: super::StartupMetrics,
    pub gaming: bool,
}

impl Controller {
    fn perf_wanted_interval(&self) -> Option<Duration> {
        if !self.perf.subscribers.is_empty() {
            Some(SUBSCRIBED_INTERVAL)
        } else if self.settings.gaming_mode {
            Some(GAMING_INTERVAL)
        } else {
            None
        }
    }

    pub(crate) fn perf_reschedule(&mut self) {
        match self.perf_wanted_interval() {
            None => self.perf.next_at = None,
            Some(_) if self.perf.next_at.is_none() && !self.perf.in_flight => self.perf.next_at = Some(Instant::now()),
            Some(_) => {}
        }
    }

    pub fn perf_subscribe(&mut self, key: String, on: bool) -> Option<PerfSample> {
        if on {
            self.perf.subscribers.insert(key);
        } else {
            self.perf.subscribers.remove(&key);
        }
        self.perf_reschedule();
        self.perf.last.clone()
    }

    pub(crate) fn perf_tick(&mut self) {
        self.perf.next_at = None;
        if self.perf.in_flight {
            return;
        }
        self.perf.in_flight = true;
        if !FIRST_SAMPLE_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            log::info!("perf: first process sample requested ({} subscribers, gaming={})", self.perf.subscribers.len(), self.settings.gaming_mode);
        }
        let ctl = self.handle.clone();
        if self.app.run_on_main_thread(move || native::collect_processes(ctl)).is_err() {
            self.perf.in_flight = false;
        }
    }

    pub fn on_process_snapshot(&mut self, snap: ProcSnapshot) {
        self.perf.in_flight = false;
        if self.perf.last.is_none() {
            log::info!("perf: first process sample complete ({} renderer entries)", snap.renderer_frames.len());
        }
        let snap_frames_unavailable = snap.frames_unavailable;
        let now = Instant::now();
        let host_pid = std::process::id();

        let frame_to_tab: HashMap<u32, TabId> = snap.tab_frames.iter().map(|(t, f)| (*f, *t)).collect();
        let renderer_tabs: HashMap<u32, Vec<TabId>> =
            snap.renderer_frames.iter().map(|(pid, frames)| (*pid, frames.iter().filter_map(|f| frame_to_tab.get(f).copied()).collect())).collect();
        let shell_renderer: Option<u32> =
            snap.shell_frame.and_then(|sf| snap.renderer_frames.iter().find(|(_, frames)| frames.contains(&sf)).map(|(pid, _)| *pid));

        let mut procs: Vec<(u32, &'static str)> = vec![(host_pid, "host")];
        procs.extend(snap.processes.iter().copied().filter(|(pid, _)| *pid != host_pid));

        let mut stats = Vec::with_capacity(procs.len());
        let mut alive = HashSet::new();
        for (pid, kind) in procs {
            let Some(reading) = read_process(pid) else { continue };
            alive.insert(pid);
            let cpu = self.perf.cpu.percent(pid, reading.cpu_time, now);
            let tab_ids = renderer_tabs.get(&pid).cloned().unwrap_or_default();
            let label = match kind {
                "host" => "Browser host process".to_string(),
                "browser" => "WebView2 browser process".to_string(),
                "gpu" => "GPU process".to_string(),
                "utility" => "Utility (network, audio, storage)".to_string(),
                "renderer" if Some(pid) == shell_renderer => "Browser UI".to_string(),
                "renderer" if !tab_ids.is_empty() => {
                    let titles: Vec<String> = tab_ids
                        .iter()
                        .filter_map(|id| self.tabs.get(*id))
                        .map(|t| if t.title.is_empty() { crate::navigation::host_of(&t.url).unwrap_or_default() } else { t.title.clone() })
                        .collect();
                    format!("Tab: {}", titles.join(", "))
                }
                "renderer" => "Renderer (frames, workers or popups)".to_string(),
                other => other.to_string(),
            };
            stats.push(ProcessStat { pid, kind: kind.to_string(), label, memory: reading.memory, cpu, tab_ids });
        }
        self.perf.cpu.retain(&alive);

        let heavy_bytes = self.settings.heavy_tab_memory_mb as u64 * 1024 * 1024;
        let mut tab_stats = Vec::new();
        for p in stats.iter().filter(|p| p.kind == "renderer" && !p.tab_ids.is_empty()) {
            let shared = p.tab_ids.len() > 1;
            for id in &p.tab_ids {
                let heavy = p.memory >= heavy_bytes || p.cpu.is_some_and(|c| c >= HEAVY_CPU_PERCENT);
                tab_stats.push(TabStat { id: *id, memory: p.memory, cpu: p.cpu, shared, heavy });
            }
        }
        let mut heavy_changed = false;
        let heavy_ids: HashSet<TabId> = tab_stats.iter().filter(|t| t.heavy).map(|t| t.id).collect();
        let ids: Vec<TabId> = self.tabs.tabs().iter().map(|t| t.id).collect();
        for id in ids {
            if let Some(t) = self.tabs.get_mut(id) {
                let heavy = heavy_ids.contains(&id);
                if t.heavy != heavy {
                    t.heavy = heavy;
                    heavy_changed = true;
                }
            }
        }
        if heavy_changed {
            self.mark_state();
        }

        if self.perf.gpu.is_none() && !self.perf.gpu_failed {
            self.perf.gpu = GpuSampler::new();
            self.perf.gpu_failed = self.perf.gpu.is_none();
        }
        let gpu = self.perf.gpu.as_mut().and_then(|g| g.sample(&alive));

        let shell_memory = stats.iter().filter(|p| p.kind == "host").map(|p| p.memory).sum();
        let total_memory: u64 = stats.iter().map(|p| p.memory).sum();
        let total_cpu = if stats.iter().any(|p| p.cpu.is_some()) { Some(stats.iter().filter_map(|p| p.cpu).sum()) } else { None };
        let tabs = self.tabs.tabs();
        let sample = PerfSample {
            timestamp: now_ms(),
            total_memory,
            shell_memory,
            webview_memory: total_memory - shell_memory,
            total_cpu,
            gpu,
            processes: stats,
            tabs: tab_stats,
            tab_count: tabs.len(),
            live_tabs: tabs.iter().filter(|t| t.lifecycle == Lifecycle::Live).count(),
            sleeping_tabs: tabs.iter().filter(|t| t.lifecycle == Lifecycle::Sleeping).count(),
            unloaded_tabs: tabs.iter().filter(|t| t.lifecycle == Lifecycle::Unloaded).count(),
            tab_attribution: !snap_frames_unavailable,
            startup: self.startup.clone(),
            gaming: self.settings.gaming_mode,
        };
        if !self.perf.subscribers.is_empty() {
            self.emit("pb:perf", &sample);
        }
        self.perf.last = Some(sample);
        self.perf.next_at = self.perf_wanted_interval().map(|i| now + i);
    }
}
