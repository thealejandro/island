//! The built-in screen's brightness glance. Fed the percent the system
//! tells, it says whether that is news: the first one is not (Windows
//! sends the current value right after the watch starts), and neither is
//! the same one again.

/// Windows also moves the brightness by itself: dimming an idle screen, or
/// following the light around it. A change with no input for this long
/// before it is one of those, and gets no glance.
pub const IDLE_MS: u32 = 10_000;

/// A change `idle_ms` after the last key, click or touch was the user's.
pub fn by_hand(idle_ms: u32) -> bool {
    idle_ms < IDLE_MS
}

#[derive(Debug, Default)]
pub struct Watch {
    last: Option<u8>,
}

impl Watch {
    /// The brightness (0 to 100) as now; true if it is worth a glance.
    pub fn feed(&mut self, percent: u8) -> bool {
        let percent = percent.min(100);
        self.last.replace(percent).is_some_and(|p| p != percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_value_is_no_news() {
        for p in [0, 50, 100] {
            assert!(!Watch::default().feed(p));
        }
    }

    #[test]
    fn only_a_different_value_is_news() {
        let mut w = Watch::default();
        assert!(!w.feed(50));
        assert!(!w.feed(50));
        assert!(w.feed(52));
        assert!(!w.feed(52));
        assert!(w.feed(0));
        assert!(w.feed(100));
    }

    #[test]
    fn only_a_change_while_in_use_is_by_hand() {
        assert!(by_hand(0));
        assert!(by_hand(IDLE_MS - 1));
        assert!(!by_hand(IDLE_MS));
        assert!(!by_hand(u32::MAX));
    }

    #[test]
    fn values_over_100_count_as_100() {
        let mut w = Watch::default();
        w.feed(100);
        assert!(!w.feed(250));
    }
}
