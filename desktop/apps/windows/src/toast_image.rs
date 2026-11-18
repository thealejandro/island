//! A notification's own image. A toast may carry one
//! (`<image placement="appLogoOverride">`, a web app's profile picture,
//! say); the system's public listener gives only the text, but the toast's
//! XML is in Windows' notification database. This reads it there — the
//! database opened read-only, only the one about to show, nothing ever
//! written — and takes the image's file, for the window to draw instead of
//! the app's logo, and whether it asks for silence (the island then plays
//! no sound for it). Any failure falls back to the logo, and to a sound.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::PathBuf;

use island_core::toast_xml;
use std::ptr;

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_OPEN_READONLY: c_int = 0x1;
const SQLITE_OPEN_URI: c_int = 0x40;

/// Nothing bigger is an icon.
const MAX: u64 = 4 << 20;

#[link(name = "winsqlite3", kind = "raw-dylib")]
unsafe extern "system" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        db: *mut *mut c_void,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
    fn sqlite3_prepare_v2(
        db: *mut c_void,
        sql: *const c_char,
        n_byte: c_int,
        stmt: *mut *mut c_void,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut c_void, index: c_int, value: i64) -> c_int;
    fn sqlite3_step(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_blob(stmt: *mut c_void, column: c_int) -> *const c_void;
    fn sqlite3_column_bytes(stmt: *mut c_void, column: c_int) -> c_int;
    fn sqlite3_finalize(stmt: *mut c_void) -> c_int;
    fn sqlite3_close_v2(db: *mut c_void) -> c_int;
}

/// Closes the database even on error (it is a handle even when opening
/// failed).
struct Db(*mut c_void);

impl Db {
    /// The notification database, read-only; `None` when it can't be opened.
    fn open() -> Option<Self> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let path = PathBuf::from(local).join("Microsoft/Windows/Notifications/wpndatabase.db");
        // The URI form: forward slashes and `?mode=ro` on top of the
        // read-only flag, so nothing can touch it.
        let uri = format!("file:{}?mode=ro", uri_path(&path.to_string_lossy()));
        let uri = CString::new(uri).ok()?;
        let mut db = ptr::null_mut();
        let r = unsafe {
            sqlite3_open_v2(
                uri.as_ptr(),
                &mut db,
                SQLITE_OPEN_READONLY | SQLITE_OPEN_URI,
                ptr::null(),
            )
        };
        if r != SQLITE_OK || db.is_null() {
            if !db.is_null() {
                unsafe {
                    sqlite3_close_v2(db);
                }
            }
            return None;
        }
        let db = Self(db);
        // The database is busy while the shell works on it: wait briefly.
        unsafe {
            sqlite3_busy_timeout(db.0, 200);
        }
        Some(db)
    }

    fn prepare(&self, sql: &std::ffi::CStr) -> Option<Stmt> {
        let mut stmt = ptr::null_mut();
        let r = unsafe { sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut stmt, ptr::null_mut()) };
        (r == SQLITE_OK && !stmt.is_null()).then(|| Stmt(stmt))
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close_v2(self.0);
        }
    }
}

/// Finalized even when stepping fails.
struct Stmt(*mut c_void);

impl Stmt {
    /// The one row's first column, as bytes; `None` without a row or with an
    /// empty one.
    fn row(&self) -> Option<Vec<u8>> {
        (unsafe { sqlite3_step(self.0) } == SQLITE_ROW).then(|| {
            let n = unsafe { sqlite3_column_bytes(self.0, 0) };
            let p = unsafe { sqlite3_column_blob(self.0, 0) };
            if n <= 0 || p.is_null() {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(p.cast::<u8>(), n as usize) }.to_vec()
            }
        })
    }
}

impl Drop for Stmt {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.0);
        }
    }
}

/// A path as an SQLite URI's: `/` for `\`, and anything that could end or
/// bend the URI (`?`, `#`, `%`, a space, any non-ASCII) as `%XX`.
fn uri_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.replace('\\', "/").bytes() {
        if b.is_ascii_alphanumeric() || b"/:-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The toast XML of the notification `id`, kept in Windows' database.
fn payload(id: u32) -> Option<String> {
    let db = Db::open()?;
    let sql = c"SELECT Payload FROM Notification WHERE Id = ?1";
    let stmt = db.prepare(sql)?;
    unsafe {
        sqlite3_bind_int64(stmt.0, 1, i64::from(id));
    }
    String::from_utf8(stmt.row()?).ok()
}

/// What the island takes from a notification's XML.
#[derive(Default)]
pub struct Read {
    /// Its own image (PNG/JPEG bytes), `None` to use the app's logo instead.
    pub image: Option<Vec<u8>>,
    /// It asks to make no sound (`<audio silent="true"/>`).
    pub silent: bool,
}

/// Notification `id`'s image and whether it is silent, from one read of
/// its XML; nothing of either if it can't be read.
pub fn read(id: u32) -> Read {
    let Some(xml) = payload(id) else {
        return Read::default();
    };
    Read {
        image: image(&xml),
        silent: toast_xml::silent(&xml),
    }
}

/// Whether the newest notification in the database asks for silence.
#[cfg(feature = "qa")]
pub fn newest_silent() -> Option<bool> {
    let db = Db::open()?;
    let stmt = db.prepare(c"SELECT Payload FROM Notification ORDER BY Id DESC LIMIT 1")?;
    let xml = String::from_utf8(stmt.row()?).ok()?;
    Some(toast_xml::silent(&xml))
}

fn image(xml: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let path = local_path(&toast_xml::unescape(toast_xml::app_logo_override(xml)?)?)?;
    if !on_fixed_drive(&path) {
        return None;
    }
    // At most MAX read, however the file grows meanwhile.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX)
        .read_to_end(&mut bytes)
        .ok()?;
    (!bytes.is_empty() && (bytes.len() as u64) < MAX).then_some(bytes)
}

/// The path is on a disk of this computer: not a drive mapped to a share.
fn on_fixed_drive(path: &std::path::Path) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let Some(letter) = path.to_str().and_then(|p| p.chars().next()) else {
        return false;
    };
    let root = windows::core::HSTRING::from(format!("{letter}:\\"));
    // DRIVE_FIXED
    unsafe { GetDriveTypeW(&root) == 3 }
}

/// A local file path for an image `src`: an absolute one (`C:\...`, also
/// with forward slashes) or a `file:///...` one; anything remote or
/// packaged (`http(s)`, `ms-appx`, `ms-appdata`) is no picture of ours.
fn local_path(src: &str) -> Option<PathBuf> {
    let src = src.trim();
    let path = src.strip_prefix("file:///").unwrap_or(src);
    if src != path {
        let decoded = percent_decode(path)?;
        return windows_path(&decoded);
    }
    windows_path(path)
}

/// A real path on this computer's drives: nothing else (no relative paths,
/// no other schemes, and no network shares, which a toast could point at
/// to have the island connect to another machine).
fn windows_path(path: &str) -> Option<PathBuf> {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    drive.then(|| PathBuf::from(path))
}

/// `%XX` sequences decoded (as UTF-8); `None` if one is invalid.
fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut rest = s.as_bytes();
    while let Some(at) = rest.iter().position(|&b| b == b'%') {
        out.extend_from_slice(&rest[..at]);
        let hex = std::str::from_utf8(rest.get(at + 1..at + 3)?).ok()?;
        out.push(u8::from_str_radix(hex, 16).ok()?);
        rest = &rest[at + 3..];
    }
    out.extend_from_slice(rest);
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri() {
        assert_eq!(
            uri_path("C:\\Users\\Jo #1?%\\x.db"),
            "C:/Users/Jo%20%231%3F%25/x.db"
        );
        assert_eq!(uri_path("C:\\Users\\José"), "C:/Users/Jos%C3%A9");
    }

    #[test]
    fn local_paths_only() {
        assert_eq!(
            local_path("C:\\a\\p.png"),
            Some(PathBuf::from("C:\\a\\p.png"))
        );
        assert_eq!(local_path("C:/a/p.png"), Some(PathBuf::from("C:/a/p.png")));
        assert_eq!(
            local_path("file:///C:/a/p%20c.png"),
            Some(PathBuf::from("C:/a/p c.png"))
        );
        assert_eq!(local_path(r"\\srv\share\p.png"), None);
        assert_eq!(local_path("file:////srv/share/p.png"), None);
        assert_eq!(
            local_path("file:///C:/fotos/Jos%C3%A9.png"),
            Some(PathBuf::from("C:/fotos/José.png"))
        );
        assert_eq!(local_path("p.png"), None);
        assert_eq!(local_path("C:"), None);
        assert_eq!(local_path("http://x/p.png"), None);
        assert_eq!(local_path("https://x/p.png"), None);
        assert_eq!(local_path("ms-appx:///p.png"), None);
        assert_eq!(local_path("ms-appdata:///local/p.png"), None);
        assert_eq!(local_path("file:///http:/bad"), None);
    }
}
