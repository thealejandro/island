//! The closed island making way for windows (#160): when the top edge of a
//! window sits under it (a maximized or snapped window's title bar and tabs),
//! it makes way (a mini island, a compact one, a faded one or a thin line,
//! see [`DodgeStyle`]) so the controls underneath can be clicked.
//!
//! Pure geometry, no OS: the apps feed it rectangles in the same pixels.

use crate::shape::Rect;

/// How the closed island looks while it makes way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DodgeStyle {
    /// A mini island with a thin rim, and a dot while music plays.
    #[default]
    Mini,
    /// A thin island with the song, the focus timer or the time in small text.
    Compact,
    /// Keeps its size and content, at a low opacity.
    Faded,
    /// The thin line of 0.1.18.
    Line,
}

impl DodgeStyle {
    pub const ALL: [Self; 4] = [Self::Mini, Self::Compact, Self::Faded, Self::Line];

    /// The name in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mini => "mini",
            Self::Compact => "compact",
            Self::Faded => "faded",
            Self::Line => "line",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        Self::ALL
            .into_iter()
            .find(|v| v.name().eq_ignore_ascii_case(s))
    }

    /// The next one in the menu's cycle, wrapping around.
    pub fn next(self) -> Self {
        match self {
            Self::Mini => Self::Compact,
            Self::Compact => Self::Faded,
            Self::Faded => Self::Line,
            Self::Line => Self::Mini,
        }
    }
}

/// The size of the line, in DIPs (times the island scale).
pub const LINE: (f32, f32) = (48.0, 6.0);

/// The size of the mini island, in DIPs.
pub const MINI: (f32, f32) = (110.0, 12.0);

/// The compact island's height, in DIPs.
pub const COMPACT_HEIGHT: f32 = 16.0;

/// Room on each side of the compact island's text, in DIPs.
pub const COMPACT_PAD: f32 = 10.0;

/// The least and the most the compact island is wide, in DIPs.
pub const COMPACT_WIDTH: (f32, f32) = (48.0, 220.0);

/// The least and the most opaque a faded island is.
pub const OPACITY: (f32, f32) = (0.1, 0.9);

/// The default opacity of a faded island.
pub const OPACITY_DEFAULT: f32 = 0.35;

/// The least height, in DIPs, of the region where hovering opens a dodging
/// island that floats (one that hangs from the edge keeps its own).
pub const HOVER: f32 = 10.0;

/// The compact island's width for a text `text_w` wide, in DIPs.
pub fn compact_width(text_w: f32) -> f32 {
    let w = if text_w.is_nan() {
        0.0
    } else {
        text_w.max(0.0)
    };
    (w + 2.0 * COMPACT_PAD)
        .ceil()
        .clamp(COMPACT_WIDTH.0, COMPACT_WIDTH.1)
}

/// The size a dodging island takes, in unscaled DIPs, given the closed size
/// it leaves and the compact text's width. `None` for [`DodgeStyle::Faded`]:
/// it keeps its size. Never wider or taller than `closed`.
pub fn size(style: DodgeStyle, closed: (f32, f32), text_w: f32) -> Option<(f32, f32)> {
    let (w, h) = match style {
        DodgeStyle::Mini => MINI,
        DodgeStyle::Compact => (compact_width(text_w), COMPACT_HEIGHT),
        DodgeStyle::Line => LINE,
        DodgeStyle::Faded => return None,
    };
    Some((w.min(closed.0), h.min(closed.1)))
}

/// `size` times the island scale.
pub fn scaled((w, h): (f32, f32), k: f32) -> (f32, f32) {
    (w * k, h * k)
}

/// How much of the dodged look (rim, dot, text) shows at `height`: the
/// complement of [`presence`]. 0 until 60 % of the way down from the closed
/// height, 1 at `small_h`.
pub fn arrival(height: f32, closed_h: f32, small_h: f32) -> f32 {
    let span = closed_h - small_h;
    if span <= 0.0 {
        return 0.0;
    }
    let down = ((closed_h - height) / span).clamp(0.0, 1.0);
    ((down - 0.6) / 0.4).clamp(0.0, 1.0)
}

/// The height of the region where hovering opens a dodging island.
pub fn hover_height(h: f32, hangs: bool, scale: f32) -> f32 {
    if hangs { h } else { h.max(HOVER * scale) }
}

/// `v` as a faded island's opacity: clamped, the default if it is not a number.
pub fn opacity(v: f32) -> f32 {
    if v.is_nan() {
        OPACITY_DEFAULT
    } else {
        v.clamp(OPACITY.0, OPACITY.1)
    }
}

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
/// entering. `island` is the resting closed body, never the dodged one.
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
/// and above, 0 once 60 % of the way down to the dodged height, rising
/// smoothly in between. It multiplies the content's alpha, the glow and the
/// shadow. A faded island keeps its size and does not use it.
pub fn presence(height: f32, closed_h: f32, small_h: f32) -> f32 {
    let span = closed_h - small_h;
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
    fn styles_round_trip_and_cycle() {
        assert_eq!(DodgeStyle::default(), DodgeStyle::Mini);
        for v in DodgeStyle::ALL {
            assert_eq!(DodgeStyle::parse(v.name()), Some(v));
        }
        let mut v = DodgeStyle::Mini;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(v);
            v = v.next();
        }
        assert_eq!(v, DodgeStyle::Mini);
        assert_eq!(seen, DodgeStyle::ALL.to_vec());
        assert_eq!(DodgeStyle::parse("  Compact "), Some(DodgeStyle::Compact));
        assert_eq!(DodgeStyle::parse("FADED"), Some(DodgeStyle::Faded));
        assert_eq!(DodgeStyle::parse("notch"), None);
        assert_eq!(DodgeStyle::parse(""), None);
    }

    #[test]
    fn sizes_per_style() {
        let closed = (180.0, 32.0);
        assert_eq!(size(DodgeStyle::Mini, closed, 0.0), Some((110.0, 12.0)));
        assert_eq!(size(DodgeStyle::Line, closed, 0.0), Some((48.0, 6.0)));
        assert_eq!(size(DodgeStyle::Faded, closed, 0.0), None);
        assert_eq!(size(DodgeStyle::Compact, closed, 0.0), Some((48.0, 16.0)));
        assert_eq!(
            size(DodgeStyle::Compact, closed, 100.0),
            Some((120.0, 16.0))
        );
        // Never wider than the closed island.
        assert_eq!(
            size(DodgeStyle::Compact, closed, 900.0),
            Some((180.0, 16.0))
        );
        let wide = (400.0, 32.0);
        assert_eq!(size(DodgeStyle::Compact, wide, 900.0), Some((220.0, 16.0)));
    }

    #[test]
    fn sizes_fit_in_the_least_closed_size() {
        for v in DodgeStyle::ALL {
            for text in [0.0, 80.0, 1000.0] {
                if let Some((w, h)) = size(v, (120.0, 24.0), text) {
                    assert!(h < 24.0, "{v:?} is {h} tall");
                    assert!(w <= 120.0, "{v:?} is {w} wide");
                }
            }
        }
    }

    #[test]
    fn scaled_multiplies() {
        for k in [0.8, 1.0, 1.4] {
            let (w, h) = scaled(MINI, k);
            assert!((w - 110.0 * k).abs() < 1e-4);
            assert!((h - 12.0 * k).abs() < 1e-4);
        }
        assert_eq!(scaled(LINE, 1.0), LINE);
    }

    #[test]
    fn compact_width_is_padded_and_bounded() {
        assert_eq!(compact_width(0.0), 48.0);
        assert_eq!(compact_width(50.2), 71.0);
        assert_eq!(compact_width(1000.0), 220.0);
        assert_eq!(compact_width(-5.0), 48.0);
        assert_eq!(compact_width(f32::NAN), 48.0);
        assert_eq!(compact_width(f32::INFINITY), 220.0);
    }

    #[test]
    fn arrival_complements_presence() {
        let (closed, small) = (32.0, 12.0);
        assert_eq!(arrival(closed, closed, small), 0.0);
        assert_eq!(arrival(closed + 5.0, closed, small), 0.0);
        let sixty = closed - 0.6 * (closed - small);
        assert!(arrival(sixty, closed, small).abs() < 1e-5);
        assert!((arrival(small, closed, small) - 1.0).abs() < 1e-5);
        assert!((arrival(0.0, closed, small) - 1.0).abs() < 1e-5);
        let mut last = 1.0;
        let mut h = small;
        while h <= closed {
            let a = arrival(h, closed, small);
            assert!((0.0..=1.0).contains(&a));
            assert!(a <= last);
            last = a;
            // Never both visible.
            assert!(presence(h, closed, small) * a < 1e-5);
            h += 0.5;
        }
        assert_eq!(arrival(5.0, 6.0, 6.0), 0.0);
    }

    #[test]
    fn hover_height_floors_only_floating_islands() {
        assert_eq!(hover_height(6.0, true, 1.0), 6.0);
        assert_eq!(hover_height(6.0, false, 1.0), 10.0);
        assert_eq!(hover_height(6.0, false, 2.0), 20.0);
        assert_eq!(hover_height(16.0, false, 1.0), 16.0);
    }

    #[test]
    fn opacity_is_clamped() {
        assert_eq!(opacity(0.0), 0.1);
        assert_eq!(opacity(5.0), 0.9);
        assert_eq!(opacity(0.5), 0.5);
        assert_eq!(opacity(f32::NAN), OPACITY_DEFAULT);
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
