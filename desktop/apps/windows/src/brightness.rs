//! The built-in screen's brightness glance: Windows tells when it changes
//! (a power setting notice to the anchor window), so nothing is polled.
//! Screens that don't report it never send one. The decision is
//! `island_core::brightness`'s.

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::System::Power::{
    HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::SystemServices::GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE;
use windows::core::Owned;

/// How long since the last key, click or touch, in ms.
pub fn idle_ms() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return 0;
    }
    unsafe { GetTickCount() }.wrapping_sub(info.dwTime)
}

/// The power setting notice, while the glance is on; it is unregistered
/// when this is dropped.
pub struct Brightness {
    _notice: Owned<HPOWERNOTIFY>,
}

impl Brightness {
    /// Starts watching; `None` if Windows won't register.
    pub fn start(hwnd: HWND) -> Option<Self> {
        let h = unsafe {
            RegisterPowerSettingNotification(
                HANDLE(hwnd.0),
                &GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
        }
        .ok()?;
        Some(Self {
            _notice: unsafe { Owned::new(h) },
        })
    }

    /// The percent a `WM_POWERBROADCAST`'s `lParam` carries, if it is the
    /// brightness.
    pub fn read(lp: LPARAM) -> Option<u8> {
        let p = lp.0 as *const POWERBROADCAST_SETTING;
        if p.is_null() {
            return None;
        }
        // SAFETY: for a power setting change, `lParam` points at the
        // setting, with `DataLength` bytes of data after it.
        unsafe {
            let s = &*p;
            if s.PowerSetting != GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS || s.DataLength < 4 {
                return None;
            }
            let v = std::ptr::read_unaligned(s.Data.as_ptr().cast::<u32>());
            Some(v.min(100) as u8)
        }
    }
}
