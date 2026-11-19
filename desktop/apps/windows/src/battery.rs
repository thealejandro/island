//! The battery's glances: Windows tells when the power source or the
//! charge level changes (a power setting notice to the anchor window), so
//! nothing is polled. The decisions are `island_core::battery`'s.

use island_core::battery::{Note, Watch};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
    SYSTEM_POWER_STATUS,
};
use windows::Win32::System::SystemServices::{
    GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING,
};
use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE;
use windows::core::{GUID, Owned};

/// The level (0 to 100) and whether the charger is in; `None` when the
/// machine has no battery (or Windows can't tell).
pub fn status() -> Option<(u8, bool)> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    // 128: no battery; 255: unknown.
    if s.BatteryFlag == 128 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some((s.BatteryLifePercent, s.ACLineStatus == 1))
}

/// The two power setting notices, while the glance is on; they are
/// unregistered when this is dropped.
pub struct Battery {
    _source: Owned<HPOWERNOTIFY>,
    _level: Owned<HPOWERNOTIFY>,
}

/// What a power setting notice said.
pub enum Change {
    Level(u8),
    Charging(bool),
}

impl Battery {
    /// Starts watching; `None` if Windows won't register.
    pub fn start(hwnd: HWND) -> Option<Self> {
        let register = |guid: &GUID| unsafe {
            RegisterPowerSettingNotification(HANDLE(hwnd.0), guid, DEVICE_NOTIFY_WINDOW_HANDLE)
                .ok()
                .map(|h| Owned::new(h))
        };
        Some(Self {
            _source: register(&GUID_ACDC_POWER_SOURCE)?,
            _level: register(&GUID_BATTERY_PERCENTAGE_REMAINING)?,
        })
    }

    /// The change a `WM_POWERBROADCAST`'s `lParam` carries, if it is one of
    /// the two watched.
    pub fn read(lp: LPARAM) -> Option<Change> {
        let p = lp.0 as *const POWERBROADCAST_SETTING;
        if p.is_null() {
            return None;
        }
        // SAFETY: for a power setting change, `lParam` points at the
        // setting, with `DataLength` bytes of data after it.
        unsafe {
            let s = &*p;
            if s.DataLength < 4 {
                return None;
            }
            let v = std::ptr::read_unaligned(s.Data.as_ptr().cast::<u32>());
            if s.PowerSetting == GUID_BATTERY_PERCENTAGE_REMAINING {
                Some(Change::Level(v.min(100) as u8))
            } else if s.PowerSetting == GUID_ACDC_POWER_SOURCE {
                // 0: the charger; 1: the battery; 2: a short-term one (UPS).
                Some(Change::Charging(v == 0))
            } else {
                None
            }
        }
    }
}

/// What was last told, and the decisions on it.
#[derive(Default)]
pub struct Power {
    watch: Watch,
    level: u8,
    charging: bool,
}

impl Power {
    /// Starts from what the battery is now: that is no news, and neither
    /// are the same values Windows sends right after registering.
    pub fn new((level, charging): (u8, bool)) -> Self {
        let mut p = Self::default();
        p.set(level, charging);
        p
    }

    /// Both values as now: news of them, and the level then.
    pub fn set(&mut self, level: u8, charging: bool) -> Option<(Note, u8)> {
        self.level = level.min(100);
        self.charging = charging;
        self.watch
            .feed(self.level, self.charging)
            .map(|n| (n, self.level))
    }

    /// One value changed.
    pub fn change(&mut self, c: Change) -> Option<(Note, u8)> {
        match c {
            Change::Level(l) => self.set(l, self.charging),
            Change::Charging(c) => self.set(self.level, c),
        }
    }
}
