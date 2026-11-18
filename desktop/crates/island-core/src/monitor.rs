//! Which screens the island lives on.

use crate::shape::Rect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    /// In virtual-screen pixels: secondary screens can have negative x or y.
    pub rect: Rect,
    /// The screen without the taskbar (and other docked bars), in the same
    /// pixels.
    pub work: Rect,
    pub primary: bool,
}

/// One screen in a list: by its number, or by the id Windows gives it,
/// which stays the same when screens are added, removed or rearranged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// 1-based, as [`MonitorChoice::Number`].
    Number(usize),
    /// Its device id (`\\?\DISPLAY#…`).
    Id(String),
}

impl Pick {
    /// A number (1 or more) or, anything else not empty, an id.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        match s.parse::<i64>() {
            Ok(n) => usize::try_from(n)
                .ok()
                .filter(|n| *n >= 1)
                .map(Self::Number),
            Err(_) => Some(Self::Id(s.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MonitorChoice {
    #[default]
    Primary,
    /// 1-based, counting screens left to right (then top to bottom), the way
    /// they sit in Windows' display settings.
    Number(usize),
    /// Every connected screen: an island on each.
    All,
    /// These screens, each with an island.
    List(Vec<Pick>),
}

impl MonitorChoice {
    /// `primary`, `all`, a number, or a comma-separated list of numbers
    /// (`1,3`). Anything else: `None`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        match s {
            "primary" => Some(Self::Primary),
            "all" => Some(Self::All),
            _ if s.contains(',') => {
                let ns: Option<Vec<Pick>> = s
                    .split(',')
                    .map(|p| p.trim().parse().ok().filter(|n| *n >= 1).map(Pick::Number))
                    .collect();
                ns.map(Self::List)
            }
            _ => s.parse().ok().filter(|n| *n >= 1).map(Self::Number),
        }
    }
}

/// Indices into `monitors` of the screens to use: the chosen ones while they
/// are connected, deduplicated, in the order they sit left to right (then
/// top to bottom); the primary when none of them is there, else the first.
/// Empty only with no screens. `ids` are the screens' device ids, in the
/// same order as `monitors` (a missing one matches no [`Pick::Id`]).
pub fn pick_all(monitors: &[Monitor], ids: &[&str], choice: &MonitorChoice) -> Vec<usize> {
    let order = order(monitors);
    let numbered = |n: usize| n.checked_sub(1).and_then(|i| order.get(i).copied());
    let chosen: Vec<usize> = match choice {
        MonitorChoice::Primary => Vec::new(),
        MonitorChoice::Number(n) => numbered(*n).into_iter().collect(),
        MonitorChoice::All => order.clone(),
        MonitorChoice::List(picks) => picks
            .iter()
            .filter_map(|p| match p {
                Pick::Number(n) => numbered(*n),
                Pick::Id(id) => (0..monitors.len())
                    .find(|&i| ids.get(i).is_some_and(|m| m.eq_ignore_ascii_case(id))),
            })
            .collect(),
    };
    // Connected, deduplicated, left to right.
    let picked: Vec<usize> = order.into_iter().filter(|i| chosen.contains(i)).collect();
    if !picked.is_empty() {
        return picked;
    }
    monitors
        .iter()
        .position(|m| m.primary)
        .or(if monitors.is_empty() { None } else { Some(0) })
        .into_iter()
        .collect()
}

/// Indices into `monitors` as the screens sit: left to right, then top to
/// bottom. Their numbers count in this order.
pub fn order(monitors: &[Monitor]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..monitors.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (monitors[a].rect, monitors[b].rect);
        ra.x.total_cmp(&rb.x).then(ra.y.total_cmp(&rb.y))
    });
    order
}

fn connected(ids: &[&str], id: &str) -> bool {
    ids.iter().any(|m| m.eq_ignore_ascii_case(id))
}

/// The choice after turning the island on screen `i` (an index into
/// `monitors`) on or off: the screens that have one now, by id (by number
/// where Windows gives none), and the chosen ones not plugged in now, kept
/// for when they come back. The last island is never turned off; the
/// primary alone is [`MonitorChoice::Primary`].
pub fn toggle(
    monitors: &[Monitor],
    ids: &[&str],
    current: &MonitorChoice,
    i: usize,
) -> MonitorChoice {
    let order = order(monitors);
    let mut on = pick_all(monitors, ids, current);
    match on.iter().position(|&j| j == i) {
        Some(_) if on.len() == 1 => return current.clone(),
        Some(p) => {
            on.remove(p);
        }
        None if i < monitors.len() => on.push(i),
        None => return current.clone(),
    }
    let away: Vec<Pick> = match current {
        MonitorChoice::List(ps) => ps
            .iter()
            .filter(|p| matches!(p, Pick::Id(id) if !connected(ids, id)))
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    let primary = monitors.iter().position(|m| m.primary);
    if away.is_empty() && on.len() == 1 && Some(on[0]) == primary {
        return MonitorChoice::Primary;
    }
    let mut picks: Vec<Pick> = order
        .iter()
        .enumerate()
        .filter(|(_, j)| on.contains(j))
        .map(|(n, &j)| match ids.get(j) {
            Some(id) if !id.is_empty() => Pick::Id((*id).to_owned()),
            _ => Pick::Number(n + 1),
        })
        .collect();
    picks.extend(away);
    MonitorChoice::List(picks)
}

/// Where a glance that tells something (a notification, the volume, a
/// device) shows: on every island, or only on the active screen's. The
/// song and its lyrics show on all either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlanceScreen {
    #[default]
    All,
    /// The screen with the window in front (the pointer's, without one).
    Active,
}

impl GlanceScreen {
    pub const ALL: [Self; 2] = [Self::All, Self::Active];

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Active => "active",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Self::All),
            "active" => Some(Self::Active),
            _ => None,
        }
    }
}

/// Where a notification's glance goes while an app is in full screen on
/// an island's screen (`notificationsInFullScreen`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FullScreen {
    /// Over the app, on that screen too.
    #[default]
    Here,
    /// Not over it: on another screen's island instead.
    Other,
    /// Not over it, and nowhere else for it.
    Hide,
}

impl FullScreen {
    pub const ALL: [Self; 3] = [Self::Here, Self::Other, Self::Hide];

    /// `here`, `other`, `hide`; `true` and `false` (as it was) are `here`
    /// and `hide`.
    pub fn parse(v: &serde_json::Value) -> Option<Self> {
        match v {
            serde_json::Value::Bool(true) => Some(Self::Here),
            serde_json::Value::Bool(false) => Some(Self::Hide),
            serde_json::Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "here" => Some(Self::Here),
                "other" => Some(Self::Other),
                "hide" => Some(Self::Hide),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Here => "here",
            Self::Other => "other",
            Self::Hide => "hide",
        }
    }
}

/// Which islands show a glance. `full` has one entry per island: an app is
/// in full screen on its screen. `active` is the active screen's island;
/// `fallback`, the islands in the order a glance moved off a full screen
/// takes (the main screen's first, then left to right).
///
/// `screen` picks all of them or the active one (all, with the active
/// screen having no island, so nothing is missed). Then, under a
/// full-screen app, `fs`: `Here` keeps the island, `Hide` drops it, and
/// `Other` drops it too, moving the glance to the first island in
/// `fallback` without one when none is left; with every island under one,
/// it stays where it was (missing a notification is worse).
pub fn glance_targets(
    full: &[bool],
    active: Option<usize>,
    fallback: &[usize],
    screen: GlanceScreen,
    fs: FullScreen,
) -> Vec<bool> {
    let n = full.len();
    let chosen: Vec<bool> = match (screen, active) {
        (GlanceScreen::Active, Some(a)) if a < n => (0..n).map(|i| i == a).collect(),
        _ => vec![true; n],
    };
    if fs == FullScreen::Here {
        return chosen;
    }
    let mut on: Vec<bool> = chosen.iter().zip(full).map(|(&c, &f)| c && !f).collect();
    if fs == FullScreen::Other && !on.contains(&true) {
        match fallback.iter().find(|&&j| j < n && !full[j]) {
            Some(&j) => on[j] = true,
            None => return chosen,
        }
    }
    on
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glances_go_to_the_active_screen_or_to_all() {
        use GlanceScreen::*;
        let t = |n: usize, a, s| glance_targets(&vec![false; n], a, &[], s, FullScreen::Other);
        assert_eq!(t(2, Some(1), All), vec![true, true]);
        assert_eq!(t(2, Some(1), Active), vec![false, true]);
        assert_eq!(t(3, Some(0), Active), vec![true, false, false]);
        // The active screen has no island: all of them.
        assert_eq!(t(2, None, Active), vec![true, true]);
        assert_eq!(t(2, Some(5), Active), vec![true, true]);
        assert_eq!(t(1, Some(0), Active), vec![true]);
        assert_eq!(t(0, None, Active), Vec::<bool>::new());
        assert_eq!(GlanceScreen::parse(" ACTIVE "), Some(Active));
        assert_eq!(GlanceScreen::parse("one"), None);
    }

    #[test]
    fn full_screen_keeps_drops_or_moves_the_glance() {
        use FullScreen::*;
        use GlanceScreen::*;
        // Island 0 under a full-screen app; 1 (the main screen's) and 2 not.
        let full = [true, false, false];
        let fb = [1, 0, 2];
        let t = |a, s, fs| glance_targets(&full, a, &fb, s, fs);
        assert_eq!(t(None, All, Here), vec![true, true, true]);
        assert_eq!(t(None, All, Hide), vec![false, true, true]);
        assert_eq!(t(None, All, Other), vec![false, true, true]);
        // Only on the active screen, the full one.
        assert_eq!(t(Some(0), Active, Here), vec![true, false, false]);
        assert_eq!(t(Some(0), Active, Hide), vec![false, false, false]);
        assert_eq!(t(Some(0), Active, Other), vec![false, true, false]);
        assert_eq!(t(Some(2), Active, Other), vec![false, false, true]);
        // The main screen's under one too: the next, left to right.
        let full = [false, true, true];
        assert_eq!(
            glance_targets(&full, Some(1), &fb, Active, Other),
            vec![true, false, false]
        );
        // Every one under one (or a single screen): it stays.
        assert_eq!(
            glance_targets(&[true, true], None, &[0, 1], All, Other),
            vec![true, true]
        );
        assert_eq!(
            glance_targets(&[true], Some(0), &[0], Active, Other),
            vec![true]
        );
        assert_eq!(
            glance_targets(&[true], Some(0), &[0], Active, Hide),
            vec![false]
        );
        use serde_json::json;
        assert_eq!(FullScreen::parse(&json!(true)), Some(Here));
        assert_eq!(FullScreen::parse(&json!(false)), Some(Hide));
        assert_eq!(FullScreen::parse(&json!(" Other ")), Some(Other));
        assert_eq!(FullScreen::parse(&json!("away")), None);
        assert_eq!(FullScreen::parse(&json!(1)), None);
        for f in FullScreen::ALL {
            assert_eq!(FullScreen::parse(&json!(f.name())), Some(f));
        }
    }

    fn mon(x: f32, y: f32, primary: bool) -> Monitor {
        let rect = Rect {
            x,
            y,
            width: 2560.0,
            height: 1440.0,
        };
        Monitor {
            rect,
            work: Rect {
                height: rect.height - 48.0,
                ..rect
            },
            primary,
        }
    }

    // As Windows lists them: the primary first, a screen on its left second.
    fn two() -> Vec<Monitor> {
        vec![mon(0.0, 0.0, true), mon(-2560.0, 0.0, false)]
    }

    fn three() -> Vec<Monitor> {
        vec![
            mon(0.0, 0.0, true),
            mon(-2560.0, 0.0, false),
            mon(2560.0, 0.0, false),
        ]
    }

    fn list(ns: &[usize]) -> MonitorChoice {
        MonitorChoice::List(ns.iter().map(|&n| Pick::Number(n)).collect())
    }

    #[test]
    fn primary_by_default() {
        assert_eq!(pick_all(&two(), &[], &MonitorChoice::Primary), vec![0]);
    }

    #[test]
    fn numbers_count_left_to_right_then_top_to_bottom() {
        assert_eq!(pick_all(&two(), &[], &MonitorChoice::Number(1)), vec![1]);
        assert_eq!(pick_all(&two(), &[], &MonitorChoice::Number(2)), vec![0]);
        let stacked = vec![mon(0.0, 1440.0, false), mon(0.0, 0.0, true)];
        assert_eq!(pick_all(&stacked, &[], &MonitorChoice::Number(1)), vec![1]);
    }

    #[test]
    fn an_unplugged_screen_falls_back_to_the_primary() {
        assert_eq!(pick_all(&two(), &[], &MonitorChoice::Number(3)), vec![0]);
        assert_eq!(
            pick_all(&[mon(0.0, 0.0, false)], &[], &MonitorChoice::Number(2)),
            vec![0]
        );
        assert_eq!(
            pick_all(&[], &[], &MonitorChoice::Primary),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn all_takes_every_screen_left_to_right() {
        assert_eq!(pick_all(&three(), &[], &MonitorChoice::All), vec![1, 0, 2]);
        assert_eq!(pick_all(&two(), &[], &MonitorChoice::All), vec![1, 0]);
        assert_eq!(pick_all(&[], &[], &MonitorChoice::All), Vec::<usize>::new());
    }

    #[test]
    fn a_list_takes_its_connected_screens() {
        // The left screen (1) and one that isn't there (3): only the first.
        assert_eq!(pick_all(&two(), &[], &list(&[1, 3])), vec![1]);
        // Repeated and out of order: deduplicated, left to right.
        assert_eq!(pick_all(&three(), &[], &list(&[3, 1, 1])), vec![1, 2]);
        // An empty list is nothing chosen: the primary.
        assert_eq!(pick_all(&two(), &[], &list(&[])), vec![0]);
        assert_eq!(pick_all(&two(), &[], &list(&[9])), vec![0]);
    }

    #[test]
    fn ids_follow_their_screen_wherever_it_sits() {
        let ids = ["\\\\?\\DISPLAY#A", "\\\\?\\DISPLAY#B", "\\\\?\\DISPLAY#C"];
        let by_id = |id: &str| MonitorChoice::List(vec![Pick::Id(id.into())]);
        assert_eq!(
            pick_all(&three(), &ids, &by_id("\\\\?\\DISPLAY#C")),
            vec![2]
        );
        // Windows may change the case; it's the same screen.
        assert_eq!(
            pick_all(&three(), &ids, &by_id("\\\\?\\display#b")),
            vec![1]
        );
        // Rearranged (B now on the right): still B.
        let moved = vec![
            mon(0.0, 0.0, true),
            mon(5120.0, 0.0, false),
            mon(2560.0, 0.0, false),
        ];
        assert_eq!(pick_all(&moved, &ids, &by_id("\\\\?\\DISPLAY#B")), vec![1]);
        // Unplugged: the primary, until it comes back.
        assert_eq!(
            pick_all(&two(), &ids[..2], &by_id("\\\\?\\DISPLAY#C")),
            vec![0]
        );
        // Ids and numbers mix; one not there takes nothing.
        let mixed = MonitorChoice::List(vec![
            Pick::Id("\\\\?\\DISPLAY#Z".into()),
            Pick::Number(3),
            Pick::Id("\\\\?\\DISPLAY#B".into()),
        ]);
        assert_eq!(pick_all(&three(), &ids, &mixed), vec![1, 2]);
        // Without ids, an id matches nothing.
        assert_eq!(pick_all(&three(), &[], &by_id("\\\\?\\DISPLAY#B")), vec![0]);
    }

    #[test]
    fn choice_names() {
        assert_eq!(
            MonitorChoice::parse("primary"),
            Some(MonitorChoice::Primary)
        );
        assert_eq!(MonitorChoice::parse("all"), Some(MonitorChoice::All));
        assert_eq!(MonitorChoice::parse("2"), Some(MonitorChoice::Number(2)));
        assert_eq!(MonitorChoice::parse(" 2 "), Some(MonitorChoice::Number(2)));
        assert_eq!(MonitorChoice::parse(" all"), Some(MonitorChoice::All));
        assert_eq!(MonitorChoice::parse("1,3"), Some(list(&[1, 3])));
        assert_eq!(MonitorChoice::parse("2, 1"), Some(list(&[2, 1])));
        for bad in ["0", "-1", "dos", "", "1,", ",2", "1,0", "1,dos"] {
            assert_eq!(MonitorChoice::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn toggling_adds_and_removes_screens_by_id() {
        let ids = ["A", "B", "C"];
        let id = |s: &str| Pick::Id(s.into());
        let m = three();
        // From the primary (A, in the middle), the left screen (B) too.
        let both = toggle(&m, &ids, &MonitorChoice::Primary, 1);
        assert_eq!(both, MonitorChoice::List(vec![id("B"), id("A")]));
        assert_eq!(pick_all(&m, &ids, &both), vec![1, 0]);
        // B off again: the primary alone.
        assert_eq!(toggle(&m, &ids, &both, 1), MonitorChoice::Primary);
        // The primary off: B alone, by id.
        assert_eq!(
            toggle(&m, &ids, &both, 0),
            MonitorChoice::List(vec![id("B")])
        );
        // The last one stays on.
        let b = MonitorChoice::List(vec![id("B")]);
        assert_eq!(toggle(&m, &ids, &b, 1), b);
        assert_eq!(
            toggle(&m, &ids, &MonitorChoice::Primary, 0),
            MonitorChoice::Primary
        );
        // Numbers and "all" become ids.
        assert_eq!(
            toggle(&m, &ids, &MonitorChoice::All, 2),
            MonitorChoice::List(vec![id("B"), id("A")])
        );
        assert_eq!(
            toggle(&m, &ids, &MonitorChoice::Number(1), 2),
            MonitorChoice::List(vec![id("B"), id("C")])
        );
        // Out of range: as it was.
        assert_eq!(toggle(&m, &ids, &b, 7), b);
    }

    #[test]
    fn toggling_keeps_unplugged_screens_and_numbers_without_ids() {
        let id = |s: &str| Pick::Id(s.into());
        // C chosen but unplugged: the primary shows; adding B keeps C.
        let away = MonitorChoice::List(vec![id("A"), id("C")]);
        let got = toggle(&two(), &["A", "B"], &away, 1);
        assert_eq!(got, MonitorChoice::List(vec![id("B"), id("A"), id("C")]));
        // B off again: the primary, but not "primary": C is still wanted.
        assert_eq!(
            toggle(&two(), &["A", "B"], &got, 1),
            MonitorChoice::List(vec![id("A"), id("C")])
        );
        // No ids from Windows: numbers, left to right.
        assert_eq!(
            toggle(&two(), &[], &MonitorChoice::Primary, 1),
            MonitorChoice::List(vec![Pick::Number(1), Pick::Number(2)])
        );
        assert_eq!(order(&three()), vec![1, 0, 2]);
    }

    #[test]
    fn picks_are_numbers_or_ids() {
        assert_eq!(Pick::parse(" 2 "), Some(Pick::Number(2)));
        assert_eq!(Pick::parse("0"), None);
        assert_eq!(Pick::parse("-3"), None);
        assert_eq!(Pick::parse("  "), None);
        assert_eq!(
            Pick::parse("\\\\?\\DISPLAY#DEL41B8#5&1"),
            Some(Pick::Id("\\\\?\\DISPLAY#DEL41B8#5&1".into()))
        );
    }
}
