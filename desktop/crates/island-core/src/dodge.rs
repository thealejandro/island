//! The closed island making way for windows (#160): when the top edge of a
//! window sits under it (a maximized or snapped window's title bar and tabs),
//! it shrinks to a small notch so the controls underneath can be clicked.
//!
//! Pure geometry, no OS: the apps feed it rectangles in the same pixels.

use crate::shape::Rect;

/// The size of the notch the dodging island shrinks to, in DIPs (times the
/// island scale).
pub const NOTCH: (f32, f32) = (48.0, 6.0);

/// How far above the island a window's top may be and still count, in DIPs
/// (a maximized window sits a few pixels above the screen).
pub const BAND: f32 = 40.0;

/// Extra clearance, in DIPs, a window needs before the island stops
/// dodging it: leaving takes more than entering, so it never flaps.
pub const STICK: f32 = 8.0;

/// A window is "under" the island: it overlaps it sideways, its top edge is
/// above the island's bottom (but no more than `band` above its top), and it
/// extends down past the island's top.
pub fn under(island: Rect, window: Rect, band: f32) -> bool {
    overlaps_x(island, window)
        && window.y < island.y + island.height
        && window.y > island.y - band
        && window.y + window.height > island.y
}

fn overlaps_x(a: Rect, b: Rect) -> bool {
    let left = a.x.max(b.x);
    let right = (a.x + a.width).min(b.x + b.width);
    right > left
}

/// `window` shares some area with `island`.
pub fn touches(island: Rect, window: Rect) -> bool {
    overlaps_x(island, window)
        && window.y < island.y + island.height
        && window.y + window.height > island.y
}

/// `island` grown by `by` on every side.
pub fn grown(island: Rect, by: f32) -> Rect {
    Rect {
        x: island.x - by,
        y: island.y - by,
        width: island.width + 2.0 * by,
        height: island.height + 2.0 * by,
    }
}

/// What the OS says of a window, to know whether it counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    pub visible: bool,
    pub minimized: bool,
    /// Hidden by the shell (another virtual desktop, a hidden workspace).
    pub cloaked: bool,
    /// One of the island's own windows.
    pub own: bool,
    /// The desktop, the taskbar and the like.
    pub shell: bool,
    /// A tool or overlay window.
    pub tool: bool,
    /// Lets the pointer through (an overlay).
    pub click_through: bool,
}

/// A real window the user could be working in.
pub fn counts(f: Flags) -> bool {
    f.visible && !f.minimized && !f.cloaked && !f.own && !f.shell && !f.tool && !f.click_through
}

/// Whether the island must dodge. `windows_top_down` are the counted
/// windows' rectangles from the topmost down: the first one that touches the
/// island decides (the ones below it are hidden by it). `was` is the island's
/// current state: it adds [`STICK`] of clearance, so leaving needs more than
/// entering. `island` is the resting closed body, never the notch.
pub fn decide(
    was: bool,
    island: Rect,
    scale: f32,
    windows_top_down: impl IntoIterator<Item = Rect>,
) -> bool {
    let reach = if was {
        grown(island, STICK * scale)
    } else {
        island
    };
    windows_top_down
        .into_iter()
        .find(|w| touches(reach, *w))
        .is_some_and(|w| under(reach, w, BAND * scale))
}

/// How much of the closed content shows at `height`: 1 at the closed height
/// and above, 0 once 60 % of the way down to the notch, rising smoothly in
/// between. It multiplies the content's alpha, the glow and the shadow.
pub fn presence(height: f32, closed_h: f32, notch_h: f32) -> f32 {
    let span = closed_h - notch_h;
    if span <= 0.0 {
        return 1.0;
    }
    let down = ((closed_h - height) / span).clamp(0.0, 1.0);
    (1.0 - down / 0.6).clamp(0.0, 1.0)
}

/// The closed height that fits in a menu bar of `bar_h` (macOS, on screens
/// without a notch): the bar's, if it is smaller than the island's.
pub fn bar_fit(closed_h: f32, bar_h: f32) -> f32 {
    if bar_h > 0.0 && bar_h < closed_h {
        bar_h
    } else {
        closed_h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// A closed island centered on a 1920 px screen.
    fn island() -> Rect {
        r(870.0, 0.0, 180.0, 32.0)
    }

    #[test]
    fn maximized_window_dodges() {
        assert!(decide(
            false,
            island(),
            1.0,
            [r(-8.0, -8.0, 1936.0, 1040.0)]
        ));
    }

    #[test]
    fn half_snapped_window_under_a_centered_island_dodges() {
        assert!(decide(false, island(), 1.0, [r(960.0, 0.0, 960.0, 1040.0)]));
    }

    #[test]
    fn window_beside_or_below_does_not_dodge() {
        assert!(!decide(false, island(), 1.0, [r(0.0, 0.0, 800.0, 600.0)]));
        assert!(!decide(
            false,
            island(),
            1.0,
            [r(500.0, 32.0, 900.0, 600.0)]
        ));
        assert!(!decide(
            false,
            island(),
            1.0,
            [r(500.0, 200.0, 900.0, 600.0)]
        ));
    }

    #[test]
    fn window_spanning_from_far_above_does_not_dodge() {
        // From a monitor above: its top is way over the band.
        assert!(!decide(
            false,
            island(),
            1.0,
            [r(0.0, -900.0, 1920.0, 1000.0)]
        ));
    }

    #[test]
    fn floating_island_over_a_title_bar_dodges() {
        let floating = r(870.0, 8.0, 180.0, 32.0);
        assert!(decide(
            false,
            floating,
            1.0,
            [r(300.0, 20.0, 1000.0, 700.0)]
        ));
    }

    #[test]
    fn hysteresis_enters_at_one_pixel_and_stays_until_clear() {
        let at = |y: f32| [r(300.0, y, 1000.0, 700.0)];
        assert!(!decide(false, island(), 1.0, at(32.0)));
        assert!(decide(false, island(), 1.0, at(31.0)));
        // Already dodging: stays while within STICK of the island.
        assert!(decide(true, island(), 1.0, at(32.0)));
        assert!(decide(true, island(), 1.0, at(39.0)));
        assert!(!decide(true, island(), 1.0, at(40.0)));
        // The clearance scales with the island.
        assert!(decide(true, island(), 2.0, at(47.0)));
        assert!(!decide(true, island(), 2.0, at(48.0)));
    }

    #[test]
    fn it_does_not_flap_over_an_oscillating_window() {
        let mut dodging = false;
        let mut flips = 0;
        for y in [40.0, 36.0, 33.0, 34.0, 38.0, 35.0, 33.0, 36.0] {
            let now = decide(dodging, island(), 1.0, [r(300.0, y, 1000.0, 700.0)]);
            if now != dodging {
                flips += 1;
            }
            dodging = now;
        }
        // Never entered: 33 px is still clear of a 32 px island.
        assert_eq!(flips, 0);
        for y in [31.0, 33.0, 37.0, 35.0, 39.0, 36.0] {
            let now = decide(dodging, island(), 1.0, [r(300.0, y, 1000.0, 700.0)]);
            if now != dodging {
                flips += 1;
            }
            dodging = now;
        }
        // Entered once at 31 and stayed through the wobble up to 39.
        assert_eq!(flips, 1);
        assert!(dodging);
    }

    #[test]
    fn a_window_just_beside_does_not_hide_the_maximized_one_below() {
        // 4 px to the right of the island: it touches the grown rect (the
        // reach of a dodging island) but not the resting one.
        let beside = r(1054.0, 0.0, 500.0, 600.0);
        let big = r(-8.0, -8.0, 1936.0, 1040.0);
        assert!(decide(false, island(), 1.0, [beside, big]));
    }

    #[test]
    fn the_first_touching_window_decides() {
        // A small window on top that misses the island is skipped.
        let small = r(0.0, 0.0, 200.0, 200.0);
        let big = r(-8.0, -8.0, 1936.0, 1040.0);
        assert!(decide(false, island(), 1.0, [small, big]));
        // A window on top that touches but does not dodge hides the
        // maximized one below it.
        let low = r(300.0, 10.0, 1000.0, 700.0);
        let spanning = r(0.0, -900.0, 1920.0, 1000.0);
        assert!(!decide(false, island(), 1.0, [spanning, big]));
        assert!(decide(false, island(), 1.0, [low, big]));
        // No window, no dodge.
        assert!(!decide(false, island(), 1.0, []));
    }

    #[test]
    fn counts_needs_a_plain_visible_window() {
        let ok = Flags {
            visible: true,
            ..Flags::default()
        };
        assert!(counts(ok));
        assert!(!counts(Flags::default()));
        assert!(!counts(Flags {
            minimized: true,
            ..ok
        }));
        assert!(!counts(Flags {
            cloaked: true,
            ..ok
        }));
        assert!(!counts(Flags { own: true, ..ok }));
        assert!(!counts(Flags { shell: true, ..ok }));
        assert!(!counts(Flags { tool: true, ..ok }));
        assert!(!counts(Flags {
            click_through: true,
            ..ok
        }));
    }

    #[test]
    fn presence_is_bounded_and_monotonic() {
        let (closed, notch) = (32.0, 6.0);
        assert_eq!(presence(closed, closed, notch), 1.0);
        assert_eq!(presence(closed + 10.0, closed, notch), 1.0);
        assert_eq!(presence(notch, closed, notch), 0.0);
        assert_eq!(presence(0.0, closed, notch), 0.0);
        // Gone 60 % of the way down.
        let sixty = closed - 0.6 * (closed - notch);
        assert!(presence(sixty, closed, notch).abs() < 1e-5);
        let mut last = 0.0;
        let mut h = notch;
        while h <= closed {
            let p = presence(h, closed, notch);
            assert!((0.0..=1.0).contains(&p));
            assert!(p >= last);
            last = p;
            h += 0.5;
        }
        // Degenerate sizes do not divide by zero.
        assert_eq!(presence(5.0, 6.0, 6.0), 1.0);
    }

    #[test]
    fn notch_is_smaller_than_the_least_closed_size() {
        assert!(NOTCH.0 < 120.0);
        assert!(NOTCH.1 < 24.0);
    }

    #[test]
    fn bar_fit_keeps_the_island_in_the_bar() {
        assert_eq!(bar_fit(32.0, 24.0), 24.0);
        assert_eq!(bar_fit(32.0, 32.0), 32.0);
        assert_eq!(bar_fit(32.0, 40.0), 32.0);
        // No bar (auto-hidden) or nonsense: unchanged.
        assert_eq!(bar_fit(32.0, 0.0), 32.0);
        assert_eq!(bar_fit(32.0, -3.0), 32.0);
    }
}
