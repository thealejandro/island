//! Two activities on the closed island at once: the song (primary, in the
//! island) and the focus timer (secondary, in a small bubble beside it).
//! The song always wins the island; the timer only gets it alone.

use crate::shape::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Media,
    Timer,
}

/// What the closed island shows: (primary, secondary). `media`: a track is
/// loaded (playing or paused); `timer`: a focus session is on (running or
/// paused part way); `dual`: the `dualActivity` setting.
pub fn closed(media: bool, timer: bool, dual: bool) -> (Option<Activity>, Option<Activity>) {
    match (media, timer) {
        (true, true) if dual => (Some(Activity::Media), Some(Activity::Timer)),
        (true, _) => (Some(Activity::Media), None),
        (false, true) => (Some(Activity::Timer), None),
        (false, false) => (None, None),
    }
}

/// The distance from the island to its bubble where the island floats, in
/// DIPs (a hanging island's ears touch instead).
pub const BUBBLE_GAP: f32 = 8.0;
/// The bubble's width without ears, for mm:ss and for h:mm:ss, in DIPs.
pub const BUBBLE_WIDTH: f32 = 74.0;
pub const BUBBLE_WIDTH_LONG: f32 = 92.0;

/// The bubble's width: the long one when the clock shows hours.
pub fn bubble_width(long: bool) -> f32 {
    if long {
        BUBBLE_WIDTH_LONG
    } else {
        BUBBLE_WIDTH
    }
}

/// Space at the bubble's ends, the icon's size and the space after it, in
/// DIPs.
const EDGE: f32 = 10.0;
const ICON: f32 = 14.0;
const AFTER_ICON: f32 = 5.0;

/// Where the bubble's content goes, in DIPs from its top left (without
/// ears).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubbleParts {
    pub icon: Rect,
    pub text: Rect,
}

pub fn bubble_parts(width: f32, height: f32) -> BubbleParts {
    let icon = Rect {
        x: EDGE,
        y: (height - ICON) / 2.0,
        width: ICON,
        height: ICON,
    };
    let x = icon.x + ICON + AFTER_ICON;
    BubbleParts {
        icon,
        text: Rect {
            x,
            y: 0.0,
            width: (width - EDGE - x).max(0.0),
            height,
        },
    }
}

/// The room the window needs on each side of the island for its bubble, in
/// DIPs: the gap, the widest bubble at `islandScale` `k`, and its two ears.
pub fn bubble_room(k: f32, ear: f32) -> f32 {
    BUBBLE_GAP + BUBBLE_WIDTH_LONG * k + 2.0 * ear
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_song_wins_the_island() {
        use Activity::{Media, Timer};
        for (media, timer, dual, want) in [
            (false, false, true, (None, None)),
            (false, false, false, (None, None)),
            (true, false, true, (Some(Media), None)),
            (true, false, false, (Some(Media), None)),
            (false, true, true, (Some(Timer), None)),
            (false, true, false, (Some(Timer), None)),
            (true, true, true, (Some(Media), Some(Timer))),
            // Off: as it was, a track hides the timer.
            (true, true, false, (Some(Media), None)),
        ] {
            assert_eq!(closed(media, timer, dual), want, "{media} {timer} {dual}");
        }
    }

    #[test]
    fn the_bubble_is_wider_with_hours() {
        assert_eq!(bubble_width(false), BUBBLE_WIDTH);
        assert_eq!(bubble_width(true), BUBBLE_WIDTH_LONG);
    }

    #[test]
    fn the_parts_are_in_order_and_inside() {
        for long in [false, true] {
            let w = bubble_width(long);
            let mut h = 24.0;
            while h <= 48.0 {
                let p = bubble_parts(w, h);
                let inside =
                    |r: Rect| r.x >= 0.0 && r.y >= 0.0 && r.x + r.width <= w && r.y + r.height <= h;
                assert!(inside(p.icon) && inside(p.text), "{w}x{h}");
                assert!(p.icon.x + p.icon.width <= p.text.x, "{w}x{h}");
                assert!(p.text.width > 0.0);
                // The icon sits in the middle.
                assert!((p.icon.y * 2.0 + p.icon.height - h).abs() < 1e-4);
                h += 4.0;
            }
        }
    }

    #[test]
    fn the_window_has_room_for_the_widest_bubble() {
        for k in [0.8, 1.0, 1.4, 2.0] {
            for ear in [0.0, 10.0] {
                assert!(bubble_room(k, ear) >= BUBBLE_GAP + BUBBLE_WIDTH_LONG * k);
            }
        }
    }
}
