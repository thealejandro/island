//! Notifications as they arrive: the system's list read again when a new
//! one shows, and only those not seen before told. The list read at start
//! is what was there already: no news.

use crate::shape::Rect;

/// The notification bell.
pub const ICON: crate::icons::Icon = crate::icons::Icon::Bell;

/// What the user is doing, as Windows tells it (`QUERY_USER_NOTIFICATION_STATE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// Nothing special: the desktop, or an app in a window.
    Free,
    /// An app in full screen (a video, a till, a browser).
    FullScreen,
    /// A game in exclusive full screen.
    Game,
    /// Presenting (slides, or "presentation mode").
    Presenting,
    /// Locked, the screen saver, or another user's session.
    Away,
    /// Windows' quiet time (as after setting up a new account).
    Quiet,
}

impl Moment {
    /// From `SHQueryUserNotificationState`'s value; one it doesn't know is
    /// taken as free.
    pub fn from_state(state: i32) -> Self {
        match state {
            1 => Moment::Away,
            2 => Moment::FullScreen,
            3 => Moment::Game,
            4 => Moment::Presenting,
            6 => Moment::Quiet,
            _ => Moment::Free,
        }
    }
}

/// Whether a notification that just arrived gets a glance. Never while
/// presenting, gaming or away; with Do Not Disturb (`quiet`) neither, but
/// in full screen the setting (`in_full_screen`) decides: Windows 11 turns
/// Do Not Disturb on by itself there, which is just when the island should
/// still tell. Windows' own banner plays no part: with an app's banners
/// turned off, the island still shows its notifications.
pub fn glances(moment: Moment, quiet: bool, in_full_screen: bool) -> bool {
    match moment {
        Moment::Free => !quiet,
        Moment::FullScreen => in_full_screen,
        Moment::Game | Moment::Presenting | Moment::Away | Moment::Quiet => false,
    }
}

/// A notification's glance: bigger than the song's, so it never passes for
/// it, with room for the app's logo, its name, the title and two lines of
/// body; without a body, shorter (DIPs).
pub fn size(has_body: bool) -> (f32, f32) {
    (480.0, if has_body { 100.0 } else { 80.0 })
}

/// How long a notification's glance stays, in ms: longer when its body
/// takes two lines, to read them.
pub fn stays_ms(body_lines: u32) -> u32 {
    if body_lines >= 2 { 6500 } else { 5000 }
}

/// How long the arrival is animated, in ms: as long as the light's lap.
pub const PULSE_MS: f32 = ORBIT_DELAY + ORBIT_MS;
/// The ring flashes once as one arrives, this long.
const FLASH_MS: f32 = 600.0;

/// Eased in (fast, then slowing) over `ms` from `start`, for `length`: 0
/// before, 1 after.
fn ease_in(ms: f32, start: f32, length: f32) -> f32 {
    let t = ((ms - start) / length).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// The logo as it arrives, `ms` after: its scale (0.8 to 1) and opacity.
pub fn logo_in(ms: f32) -> (f32, f32) {
    let e = ease_in(ms, 80.0, 220.0);
    (0.8 + 0.2 * e, e)
}

/// How far the rise of a line is (DIPs).
const RISE: f32 = 6.0;

/// Line `i` (the app, the title, the body) as it arrives, `ms` after: how
/// far below its place it still is, and its opacity. Each starts 40 ms
/// after the one above.
pub fn line_in(ms: f32, i: usize) -> (f32, f32) {
    let e = ease_in(ms, 60.0 + 40.0 * i as f32, 180.0);
    (RISE * (1.0 - e), e)
}

/// How bright the ring in the app's color is, `ms` after it arrived: one
/// flash that fades, then a steady glow while the glance stays.
pub fn ring(ms: f32) -> f32 {
    const REST: f32 = 0.35;
    if !(0.0..FLASH_MS).contains(&ms) {
        return REST;
    }
    let left = 1.0 - ms / FLASH_MS;
    REST + (1.0 - REST) * left * left
}

/// How long the light runs around the island's edge as one arrives (ms).
pub const ORBIT_MS: f32 = 1100.0;
/// It waits this long first, for the island to grow into the glance.
pub const ORBIT_DELAY: f32 = 250.0;

/// How far around the edge the light is, `ms` after it arrived (0 to 1:
/// once around, fast at first and slowing to a stop), and how bright;
/// `None` once it is done.
pub fn orbit(ms: f32) -> Option<(f32, f32)> {
    let ms = ms - ORBIT_DELAY;
    if !(0.0..ORBIT_MS).contains(&ms) {
        return None;
    }
    let t = ms / ORBIT_MS;
    let at = 1.0 - (1.0 - t).powi(3);
    // In quickly, out with the end of the lap.
    let bright = (t / 0.12).min(1.0) * (1.0 - t * t);
    Some((at, bright))
}

/// Where a notification's glance puts things: the logo at the left; the
/// app and "now" on top, then the title and, if there is one, the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToastLayout {
    pub logo: Rect,
    pub app: Rect,
    pub when: Rect,
    pub title: Rect,
    pub body: Rect,
}

/// The logo's side and its corners' radius (DIPs).
pub const LOGO: f32 = 52.0;
pub const LOGO_RADIUS: f32 = 13.0;

pub fn layout((w, h): (f32, f32), has_body: bool) -> ToastLayout {
    let (pad, side) = (20.0, LOGO);
    let logo = Rect {
        x: pad,
        y: (h - side) / 2.0,
        width: side,
        height: side,
    };
    let x = pad + side + 14.0;
    let right = w - pad;
    // The lines, centered as a block; the body has room for two.
    let (app_h, title_h) = (17.0, 24.0);
    let body_h = if has_body { 36.0 } else { 0.0 };
    let top = (h - app_h - title_h - body_h) / 2.0;
    let when = Rect {
        x: right - 64.0,
        y: top,
        width: 64.0,
        height: app_h,
    };
    let app = Rect {
        x,
        y: top,
        width: when.x - 8.0 - x,
        height: app_h,
    };
    let title = Rect {
        x,
        y: top + app_h,
        width: right - x,
        height: title_h,
    };
    let body = Rect {
        x,
        y: title.y + title_h,
        width: right - x,
        height: body_h,
    };
    ToastLayout {
        logo,
        app,
        when,
        title,
        body,
    }
}

/// A notification, as far as a glance tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u32,
    /// The app that sent it.
    pub app: String,
    /// That app's id in the shell (its AppUserModelID), to open it with a
    /// click; empty if Windows doesn't tell.
    pub app_id: String,
    /// Its title (its first line of text).
    pub title: String,
    /// Its second line, if any.
    pub body: String,
}

impl Toast {
    /// One line of text, whatever the app sent (line breaks, runs of spaces).
    pub fn line(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// The notifications seen, by id.
#[derive(Debug, Default)]
pub struct Seen {
    ids: Vec<u32>,
    primed: bool,
}

impl Seen {
    /// The list now; the ones in it not seen before, oldest first (none the
    /// first time). The ones gone are forgotten.
    pub fn new_ones(&mut self, now: Vec<Toast>) -> Vec<Toast> {
        let ids: Vec<u32> = now.iter().map(|t| t.id).collect();
        let fresh = if self.primed {
            now.into_iter()
                .filter(|t| !self.ids.contains(&t.id))
                .collect()
        } else {
            Vec::new()
        };
        self.ids = ids;
        self.primed = true;
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: u32) -> Toast {
        Toast {
            id,
            app: "Mail".into(),
            app_id: String::new(),
            title: format!("n{id}"),
            body: String::new(),
        }
    }

    #[test]
    fn only_new_ones_are_told() {
        let mut s = Seen::default();
        assert_eq!(s.new_ones(vec![t(1), t(2)]), vec![]);
        assert_eq!(s.new_ones(vec![t(1), t(2), t(3)]), vec![t(3)]);
        // Read again with nothing new (another window showed): nothing.
        assert_eq!(s.new_ones(vec![t(1), t(2), t(3)]), vec![]);
        // One dismissed, two new.
        assert_eq!(s.new_ones(vec![t(2), t(3), t(4), t(5)]), vec![t(4), t(5)]);
        // A dismissed one is forgotten; its id coming back is new.
        assert_eq!(s.new_ones(vec![t(1)]), vec![t(1)]);
    }

    #[test]
    fn full_screen_glances_but_presenting_gaming_and_quiet_never() {
        use Moment::*;
        assert!(glances(Free, false, false));
        assert!(glances(FullScreen, false, true));
        assert!(!glances(FullScreen, false, false));
        for m in [Game, Presenting, Away, Quiet] {
            assert!(!glances(m, false, true));
        }
        assert!(!glances(Free, true, true));
        // Windows 11 turns Do Not Disturb on by itself in full screen.
        assert!(glances(FullScreen, true, true));
        assert!(!glances(FullScreen, true, false));
        assert_eq!(Moment::from_state(2), FullScreen);
        assert_eq!(Moment::from_state(3), Game);
        assert_eq!(Moment::from_state(4), Presenting);
        assert_eq!(Moment::from_state(6), Quiet);
        assert_eq!(Moment::from_state(5), Free);
        assert_eq!(Moment::from_state(42), Free);
    }

    #[test]
    fn the_light_goes_once_around_and_fades() {
        let orbit = |ms: f32| orbit(ms + ORBIT_DELAY);
        assert_eq!(orbit(-1.0), None);
        assert_eq!(orbit(ORBIT_MS), None);
        let (start, dark) = orbit(0.0).unwrap();
        assert_eq!((start, dark), (0.0, 0.0));
        let mut last = 0.0;
        for i in 1..30 {
            let (at, bright) = orbit(ORBIT_MS * i as f32 / 30.0).unwrap();
            assert!(at > last && at <= 1.0 && (0.0..=1.0).contains(&bright));
            last = at;
        }
        assert!(orbit(ORBIT_MS * 0.2).unwrap().1 > 0.9);
        assert!(orbit(ORBIT_MS * 0.99).unwrap().1 < 0.05);
    }

    #[test]
    fn the_ring_flashes_once_then_rests() {
        assert!(ring(0.0) > 0.99);
        assert!(ring(FLASH_MS / 2.0) < ring(FLASH_MS / 4.0));
        assert_eq!(ring(FLASH_MS), ring(PULSE_MS));
        assert_eq!(ring(10_000.0), ring(PULSE_MS));
        // No jump where the flash ends.
        assert!((ring(FLASH_MS - 1.0) - ring(FLASH_MS)).abs() < 0.01);
        // The arrival lasts as long as the light runs around.
        assert!(orbit(PULSE_MS - 1.0).is_some());
        assert_eq!(orbit(PULSE_MS), None);
    }

    #[test]
    fn the_glance_fits_logo_and_lines() {
        for has_body in [true, false] {
            let s = size(has_body);
            let l = layout(s, has_body);
            let inside =
                |r: Rect| r.x >= 0.0 && r.y >= 0.0 && r.x + r.width <= s.0 && r.y + r.height <= s.1;
            for r in [l.logo, l.app, l.when, l.title, l.body] {
                assert!(inside(r), "{r:?}");
            }
            assert!(l.logo.x + l.logo.width < l.app.x);
            assert!(l.app.x + l.app.width < l.when.x);
            assert!(l.app.y + l.app.height <= l.title.y);
            assert!(l.title.y + l.title.height <= l.body.y);
        }
        assert_eq!(layout(size(false), false).body.height, 0.0);
        // Two lines of 13 px fit the body's box.
        assert!(layout(size(true), true).body.height >= 2.0 * 13.0 * 1.33);
        // Taller than the song's glance, so it never passes for it.
        assert!(size(false).1 > crate::media::PEEK.1 + 20.0);
        // Not a gap left under the lines without a body.
        let l = layout(size(false), false);
        assert!(l.app.y > 15.0 && size(false).1 - (l.title.y + l.title.height) > 15.0);
    }

    #[test]
    fn it_arrives_in_steps_and_is_done_with_the_light() {
        assert_eq!(logo_in(0.0), (0.8, 0.0));
        assert_eq!(logo_in(PULSE_MS), (1.0, 1.0));
        assert_eq!(line_in(0.0, 0), (RISE, 0.0));
        // Each line a little after the one above.
        assert!(line_in(120.0, 0).1 > line_in(120.0, 1).1);
        assert!(line_in(120.0, 1).1 > line_in(120.0, 2).1);
        for i in 0..3 {
            assert_eq!(line_in(PULSE_MS, i), (0.0, 1.0));
            // Never past its place, nor more than opaque.
            for ms in 0..400 {
                let (dy, a) = line_in(ms as f32, i);
                assert!((0.0..=RISE).contains(&dy) && (0.0..=1.0).contains(&a));
            }
        }
        // Without an arrival (animations off), all is in place.
        assert_eq!(line_in(f32::MAX, 2), (0.0, 1.0));
        assert_eq!(logo_in(f32::MAX), (1.0, 1.0));
        assert_eq!(PULSE_MS, 1350.0);
        assert_eq!((stays_ms(0), stays_ms(1), stays_ms(2)), (5000, 5000, 6500));
    }

    #[test]
    fn text_is_one_line() {
        assert_eq!(Toast::line("  Hola\n  mundo\t !"), "Hola mundo !");
        assert_eq!(Toast::line("\n"), "");
    }
}
