//! The agenda's events: `%LOCALAPPDATA%\island\events.json`, written by hand,
//! and an optional private iCalendar link (`agendaUrl`). A worker thread
//! reads both every 15 minutes and hands the events to the window; the
//! reminders are `island_core::agenda`'s.
//!
//! The link is a secret: it is never logged, shown or written anywhere.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use island_core::agenda::{self, DAY, Date, Event};
use island_core::ics;
use windows::Win32::Foundation::{HWND, LPARAM, SYSTEMTIME, WPARAM};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::http;

/// The events written by hand, in [`folder`].
pub const FILE: &str = "events.json";

/// What the worker is told.
enum Cmd {
    /// The calendar link changed.
    Url(String),
    /// `events.json` changed.
    Reload,
}

/// The events the worker last found, until the window takes them.
type Latest = Arc<Mutex<Option<Vec<Event>>>>;

/// The worker's handle. No `Debug`: it holds the calendar link.
pub struct Agenda {
    tx: Sender<Cmd>,
    latest: Latest,
    /// Tells the worker the window went (it ends on its own anyway).
    gone: Arc<AtomicBool>,
}

impl Agenda {
    /// Starts the worker; `msg` goes to `hwnd` each time it has events.
    pub fn start(hwnd: HWND, msg: u32, url: String) -> Self {
        let (tx, rx) = mpsc::channel();
        let latest = Arc::new(Mutex::new(None));
        let gone = Arc::new(AtomicBool::new(false));
        let shared = (latest.clone(), gone.clone());
        let hwnd = hwnd.0 as isize;
        let _ = std::thread::Builder::new()
            .name("agenda".into())
            .spawn(move || run(rx, shared, hwnd, msg, url));
        Self { tx, latest, gone }
    }

    /// A new calendar link (empty: none).
    pub fn set_url(&self, url: String) {
        let _ = self.tx.send(Cmd::Url(url));
    }

    /// `events.json` changed: read it again.
    pub fn reload(&self) {
        let _ = self.tx.send(Cmd::Reload);
    }

    /// The events the worker last found, once.
    pub fn take(&self) -> Option<Vec<Event>> {
        self.latest.lock().ok()?.take()
    }
}

impl Drop for Agenda {
    fn drop(&mut self) {
        // The channel closing ends the worker; this stops a fetch under way
        // from posting to a window that no longer wants it.
        self.gone.store(true, Ordering::Release);
    }
}

/// `%LOCALAPPDATA%\island`.
pub fn folder() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("island"))
}

/// Seconds since 1970 of a UTC moment, as the system clock tells.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Local wall-clock seconds, as [`island_core::agenda::wall`] counts them.
pub fn now_local() -> i64 {
    let t = unsafe { GetLocalTime() };
    Date::new(i32::from(t.wYear), t.wMonth as u8, t.wDay as u8).map_or(0, |d| {
        agenda::wall(d, t.wHour as u8, t.wMinute as u8) + i64::from(t.wSecond)
    })
}

/// A UTC moment (seconds since 1970) on the local wall clock, with the
/// daylight saving of that date; the same moment when Windows can't say.
fn to_local(utc: i64) -> i64 {
    let date = agenda::date_of(utc);
    let rest = utc.rem_euclid(DAY);
    let from = SYSTEMTIME {
        wYear: date.y as u16,
        wMonth: u16::from(date.m),
        wDay: u16::from(date.d),
        wHour: (rest / 3600) as u16,
        wMinute: (rest % 3600 / 60) as u16,
        wSecond: (rest % 60) as u16,
        ..Default::default()
    };
    let mut local = SYSTEMTIME::default();
    if unsafe { SystemTimeToTzSpecificLocalTime(None, &from, &mut local) }.is_err() {
        return utc;
    }
    Date::new(i32::from(local.wYear), local.wMonth as u8, local.wDay as u8).map_or(utc, |d| {
        agenda::wall(d, local.wHour as u8, local.wMinute as u8) + i64::from(local.wSecond)
    })
}

/// `events.json` as last read: when it changed and how long it was.
type Stamp = (SystemTime, u64);

/// Reads `events.json` into `local` when it changed. A file that can't be
/// read or understood now (half saved) leaves the events in use.
fn read_file(local: &mut Vec<Event>, stamp: &mut Option<Stamp>) {
    let Some(path) = folder().map(|f| f.join(FILE)) else {
        return;
    };
    match std::fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            local.clear();
            *stamp = None;
        }
        Ok(m) => {
            let seen = m.modified().ok().map(|t| (t, m.len()));
            if seen.is_some() && seen == *stamp {
                return;
            }
            if m.len() > agenda::MAX_FILE as u64 {
                return;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                return;
            };
            if let Some(list) = agenda::parse_events(&String::from_utf8_lossy(&bytes)) {
                *local = list;
                *stamp = seen;
            }
        }
        Err(_) => {}
    }
}

/// What asking for the calendar gave.
enum Fetched {
    Events(Vec<Event>),
    /// Bigger than [`ics::MAX_ICS`]: asking again won't help.
    TooBig,
    /// Not had now; try again soon.
    Failed,
}

/// The calendar's events for the days around now.
fn fetch(url: &str) -> Fetched {
    let Some((host, path)) = agenda::split_url(url) else {
        return Fetched::Events(Vec::new());
    };
    match http::get(&host, &path, ics::MAX_ICS) {
        Ok((200, body)) => {
            let now = unix_now();
            let text = String::from_utf8_lossy(&body);
            Fetched::Events(ics::events(&text, now - DAY, now + 8 * DAY, &to_local))
        }
        Err(http::NetError::TooBig) => Fetched::TooBig,
        _ => Fetched::Failed,
    }
}

fn run(
    rx: Receiver<Cmd>,
    (latest, gone): (Latest, Arc<AtomicBool>),
    hwnd: isize,
    msg: u32,
    mut url: String,
) {
    let mut local: Vec<Event> = Vec::new();
    let mut stamp: Option<Stamp> = None;
    let mut remote: Vec<Event> = Vec::new();
    // When the calendar is asked for next; a reload doesn't move it.
    let mut next_fetch = Instant::now();
    loop {
        read_file(&mut local, &mut stamp);
        if Instant::now() >= next_fetch {
            let wait = match fetch(&url) {
                Fetched::Events(list) => {
                    remote = list;
                    agenda::REFRESH_SECS
                }
                Fetched::TooBig => {
                    remote.clear();
                    agenda::REFRESH_SECS
                }
                Fetched::Failed => agenda::RETRY_SECS,
            };
            next_fetch = Instant::now() + Duration::from_secs(wait);
        }
        if gone.load(Ordering::Acquire) {
            break;
        }
        let mut all = local.clone();
        all.extend(remote.iter().cloned());
        all.truncate(agenda::MAX_EVENTS);
        if let Ok(mut l) = latest.lock() {
            *l = Some(all);
        }
        // Always: the window sets its timer again, which also puts right
        // what a sleep, a clock change or daylight saving moved.
        let posted = unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0)) };
        if posted.is_err() {
            break;
        }
        let left = next_fetch.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(cmd) => {
                if matches!(cmd, Cmd::Reload) {
                    // A save comes as several changes: let them settle.
                    std::thread::sleep(Duration::from_millis(150));
                }
                let mut changed = |c: Cmd| {
                    if let Cmd::Url(u) = c {
                        url = u;
                        remote.clear();
                        next_fetch = Instant::now();
                    }
                };
                changed(cmd);
                while let Ok(c) = rx.try_recv() {
                    changed(c);
                }
                stamp = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_is_within_a_day_of_utc() {
        let now = unix_now();
        assert!((to_local(now) - now).abs() <= 14 * 3600);
        assert!((now_local() - to_local(unix_now())).abs() <= 5);
    }

    #[test]
    fn the_folder_is_the_islands() {
        if let Some(f) = folder() {
            assert!(f.ends_with("island"));
        }
    }
}
