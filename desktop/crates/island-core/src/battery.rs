//! The battery's glances: when charging starts or stops, when it is full,
//! and a warning at 20 % and again at 10 % while it runs down. Fed what the
//! system tells (the level and whether the charger is in), it says what
//! is news. What it is first told is not news.

use crate::icons::Icon;

/// Linear RGB of the low-battery warning.
pub const LOW_TINT: [f32; 3] = [1.0, 0.36, 0.30];
/// The levels that warn, highest first.
pub const LOW_AT: [u8; 2] = [20, 10];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    Charging,
    OnBattery,
    Low,
    Full,
}

impl Note {
    pub fn icon(self) -> Icon {
        match self {
            Note::Charging | Note::Full => Icon::BatteryCharging,
            Note::OnBattery | Note::Low => Icon::Battery,
        }
    }

    /// Only the warning is urgent.
    pub fn urgent(self) -> bool {
        self == Note::Low
    }
}

#[derive(Debug, Default)]
pub struct Watch {
    /// The last level and charger told.
    last: Option<(u8, bool)>,
    /// How many of `LOW_AT` have warned already.
    warned: usize,
    /// Told that it is full (or was, when first seen).
    full: bool,
}

impl Watch {
    /// How many warnings a level has passed already.
    fn passed(level: u8) -> usize {
        LOW_AT.iter().filter(|&&l| level <= l).count()
    }

    /// The level (0 to 100) and whether the charger is in, as now.
    pub fn feed(&mut self, level: u8, charging: bool) -> Option<Note> {
        let level = level.min(100);
        let before = self.last.replace((level, charging));
        let Some((_, was)) = before else {
            self.warned = if charging { 0 } else { Self::passed(level) };
            self.full = charging && level >= 100;
            return None;
        };
        if charging != was {
            self.full = charging && level >= 100;
            // Plugged in: warnings start over. Unplugged: only a lower
            // level than this one will warn.
            self.warned = if charging { 0 } else { Self::passed(level) };
            return Some(if charging {
                Note::Charging
            } else {
                Note::OnBattery
            });
        }
        if charging {
            if level < 100 {
                self.full = false;
            } else if !self.full {
                self.full = true;
                return Some(Note::Full);
            }
            return None;
        }
        let passed = Self::passed(level);
        if passed > self.warned {
            self.warned = passed;
            return Some(Note::Low);
        }
        // Back up (the system's estimate moves): it can warn again.
        self.warned = passed;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_values_are_no_news() {
        for (level, charging) in [
            (50, false),
            (50, true),
            (100, true),
            (15, false),
            (5, false),
        ] {
            assert_eq!(Watch::default().feed(level, charging), None);
        }
    }

    #[test]
    fn plugging_and_unplugging() {
        let mut w = Watch::default();
        assert_eq!(w.feed(50, false), None);
        assert_eq!(w.feed(50, true), Some(Note::Charging));
        assert_eq!(w.feed(51, true), None);
        assert_eq!(w.feed(51, true), None);
        assert_eq!(w.feed(51, false), Some(Note::OnBattery));
    }

    #[test]
    fn warns_once_at_each_level() {
        let mut w = Watch::default();
        w.feed(25, false);
        assert_eq!(w.feed(21, false), None);
        assert_eq!(w.feed(20, false), Some(Note::Low));
        assert_eq!(w.feed(19, false), None);
        assert_eq!(w.feed(11, false), None);
        assert_eq!(w.feed(10, false), Some(Note::Low));
        assert_eq!(w.feed(9, false), None);
        assert_eq!(w.feed(0, false), None);
    }

    #[test]
    fn a_jump_past_both_warns_once() {
        let mut w = Watch::default();
        w.feed(30, false);
        assert_eq!(w.feed(8, false), Some(Note::Low));
        assert_eq!(w.feed(7, false), None);
    }

    #[test]
    fn charging_never_warns_and_rearms() {
        let mut w = Watch::default();
        w.feed(25, false);
        assert_eq!(w.feed(20, false), Some(Note::Low));
        assert_eq!(w.feed(15, true), Some(Note::Charging));
        assert_eq!(w.feed(10, true), None);
        assert_eq!(w.feed(10, false), Some(Note::OnBattery));
        // 20 % was passed on the way up; unplugged at 10, only below warns.
        assert_eq!(w.feed(9, false), None);
        let mut w = Watch::default();
        w.feed(30, false);
        w.feed(20, false);
        w.feed(40, true);
        assert_eq!(w.feed(40, false), Some(Note::OnBattery));
        assert_eq!(w.feed(20, false), Some(Note::Low));
    }

    #[test]
    fn unplugged_when_already_low_tells_only_that() {
        let mut w = Watch::default();
        w.feed(15, true);
        assert_eq!(w.feed(15, false), Some(Note::OnBattery));
        assert_eq!(w.feed(14, false), None);
        assert_eq!(w.feed(10, false), Some(Note::Low));
    }

    #[test]
    fn full_once_while_plugged() {
        let mut w = Watch::default();
        w.feed(98, true);
        assert_eq!(w.feed(99, true), None);
        assert_eq!(w.feed(100, true), Some(Note::Full));
        assert_eq!(w.feed(100, true), None);
        // Below 100 (a discharge while plugged) and up again.
        assert_eq!(w.feed(99, true), None);
        assert_eq!(w.feed(100, true), Some(Note::Full));
    }

    #[test]
    fn full_rearms_when_unplugged() {
        let mut w = Watch::default();
        w.feed(100, true);
        assert_eq!(w.feed(100, true), None);
        assert_eq!(w.feed(100, false), Some(Note::OnBattery));
        assert_eq!(w.feed(100, true), Some(Note::Charging));
        assert_eq!(w.feed(100, true), None);
    }

    #[test]
    fn levels_over_100_count_as_full() {
        let mut w = Watch::default();
        w.feed(99, true);
        assert_eq!(w.feed(200, true), Some(Note::Full));
    }

    #[test]
    fn notes_have_icons_and_one_is_urgent() {
        assert_eq!(Note::Charging.icon(), Icon::BatteryCharging);
        assert_eq!(Note::Full.icon(), Icon::BatteryCharging);
        assert_eq!(Note::OnBattery.icon(), Icon::Battery);
        assert_eq!(Note::Low.icon(), Icon::Battery);
        assert!(Note::Low.urgent());
        assert!(!Note::Charging.urgent());
    }
}
