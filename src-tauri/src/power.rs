//! Energy Saver, like Chrome's: while it is on, background tabs go to sleep after `SLEEP_MINUTES` at most and the
//! browser's own animations stop. When it is on depends on the setting and on Windows' power status.

use crate::settings::EnergySaver;

/// Background tabs sleep after this many minutes at most while Energy Saver is on.
pub const SLEEP_MINUTES: u32 = 5;
/// "Low battery" means this many percent or less.
pub const LOW_PERCENT: u8 = 20;

/// Windows' power status: (running on battery, battery percent if known).
#[cfg(windows)]
pub fn status() -> (bool, Option<u8>) {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return (false, None);
    }
    // ACLineStatus: 0 = on battery, 1 = plugged in, 255 = unknown. BatteryLifePercent 255 = unknown.
    (s.ACLineStatus == 0, (s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent))
}

#[cfg(not(windows))]
pub fn status() -> (bool, Option<u8>) {
    (false, None)
}

/// Whether Energy Saver is on for this setting and power status. Pure.
pub fn active_for(setting: EnergySaver, on_battery: bool, percent: Option<u8>) -> bool {
    match setting {
        EnergySaver::Off => false,
        EnergySaver::Always => true,
        EnergySaver::Battery => on_battery,
        EnergySaver::LowBattery => on_battery && percent.is_some_and(|p| p <= LOW_PERCENT),
    }
}

static LAST: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);

/// Records whether Energy Saver is on; true when that differs from the last time (the shell is then told).
pub fn changed(active: bool) -> bool {
    LAST.swap(active as u8, std::sync::atomic::Ordering::Relaxed) != active as u8
}

/// Whether Energy Saver is on now.
pub fn active(setting: EnergySaver) -> bool {
    if setting == EnergySaver::Off {
        return false;
    }
    let (battery, percent) = status();
    active_for(setting, battery, percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_saver_follows_the_setting_and_the_battery() {
        assert!(!active_for(EnergySaver::Off, true, Some(5)));
        assert!(active_for(EnergySaver::Always, false, None));
        assert!(active_for(EnergySaver::Battery, true, Some(90)));
        assert!(!active_for(EnergySaver::Battery, false, Some(10)));
        assert!(active_for(EnergySaver::LowBattery, true, Some(20)));
        assert!(!active_for(EnergySaver::LowBattery, true, Some(21)));
        assert!(!active_for(EnergySaver::LowBattery, true, None));
    }
}
