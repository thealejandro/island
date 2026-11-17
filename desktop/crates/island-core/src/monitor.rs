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

#[cfg(test)]
mod tests {
    use super::*;

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
