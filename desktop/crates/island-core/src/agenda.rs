//! The agenda's events and their reminders. An event has a day, maybe a
//! time, and may repeat; a reminder is due in the minutes before it. Time
//! here is "wall seconds": seconds since 1970-01-01 00:00 on the wall clock
//! of the user, whatever the zone, so the logic never needs one.

use serde_json::Value;

use crate::texts::{self, Texts};

/// The most the events file may weigh.
pub const MAX_FILE: usize = 256 * 1024;
/// The most events kept, from the file and from a calendar together.
pub const MAX_EVENTS: usize = 2000;
/// The most characters of a title.
pub const MAX_TITLE: usize = 80;
/// The longest calendar link taken.
pub const MAX_URL: usize = 2048;
/// How long a reminder stays on the island.
pub const GLANCE_MS: u32 = 8000;
/// How often the sources are read again.
pub const REFRESH_SECS: u64 = 15 * 60;
/// How soon to try again after a calendar fails.
pub const RETRY_SECS: u64 = 2 * 60;
/// Seconds in a day.
pub const DAY: i64 = 86_400;

/// A calendar day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub y: i32,
    pub m: u8,
    pub d: u8,
}

impl Date {
    /// A real day between 1970 and 9999.
    pub fn new(y: i32, m: u8, d: u8) -> Option<Self> {
        if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) {
            return None;
        }
        (1..=days_in_month(y, m))
            .contains(&d)
            .then_some(Self { y, m, d })
    }

    /// `YYYY-MM-DD`, strictly.
    pub fn parse(iso: &str) -> Option<Self> {
        let b = iso.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let num = |r: std::ops::Range<usize>| -> Option<u32> {
            let s = b.get(r)?;
            s.iter()
                .all(u8::is_ascii_digit)
                .then(|| s.iter().fold(0, |n, &c| n * 10 + u32::from(c - b'0')))
        };
        let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
        Self::new(
            i32::try_from(y).ok()?,
            u8::try_from(m).ok()?,
            u8::try_from(d).ok()?,
        )
    }

    /// Days since 1970-01-01.
    pub fn days(self) -> i64 {
        let y = i64::from(self.y) - i64::from(self.m <= 2);
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let mp = (i64::from(self.m) + 9) % 12;
        let doy = (153 * mp + 2) / 5 + i64::from(self.d) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The day `n` days after 1970-01-01 (far-off values are held in range).
    pub fn from_days(n: i64) -> Self {
        let z = n.clamp(-1_000_000_000, 1_000_000_000) + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        Self {
            y: i32::try_from(y).unwrap_or(0),
            m: u8::try_from(m).unwrap_or(1),
            d: u8::try_from(d).unwrap_or(1),
        }
    }

    /// 0 is Sunday.
    pub fn weekday(self) -> u8 {
        (self.days() + 4).rem_euclid(7) as u8
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

pub fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 30,
    }
}

/// The wall seconds of `h`:`mi` on `date`.
pub fn wall(date: Date, h: u8, mi: u8) -> i64 {
    date.days() * DAY + i64::from(h) * 3600 + i64::from(mi) * 60
}

/// The day a wall moment falls on.
pub fn date_of(wall: i64) -> Date {
    Date::from_days(wall.div_euclid(DAY))
}

/// Minutes since midnight of a wall moment.
pub fn minute_of(wall: i64) -> u16 {
    (wall.rem_euclid(DAY) / 60) as u16
}

/// `09:05`.
pub fn time_text(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    None,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// What tells it apart from the others, for the once-only reminder.
    pub id: String,
    /// The day it is on, or starts on if it repeats.
    pub date: Date,
    /// Minutes since midnight; none for an all-day event.
    pub time: Option<u16>,
    pub title: String,
    pub repeat: Repeat,
    /// Minutes before it to remind; none for the setting's.
    pub remind: Option<u32>,
}

/// `""` is all day (`Some(None)`); `9:05` and `0905` are 09:05; anything
/// that cannot be a time is `None`.
pub fn normalize_time(text: &str) -> Option<Option<u16>> {
    let t = text.trim();
    if t.is_empty() {
        return Some(None);
    }
    if !t.is_ascii() {
        return None;
    }
    let (h, m) = match t.split_once(':') {
        Some(parts) => parts,
        None if t.len() == 3 || t.len() == 4 => t.split_at(t.len() - 2),
        None => return None,
    };
    let digits =
        |s: &str, lens: &[usize]| lens.contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit());
    if !digits(h, &[1, 2]) || !digits(m, &[2]) {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(Some(h * 60 + m))
}

/// A title made safe to show: no control characters, spaces collapsed,
/// cut to `MAX_TITLE` characters.
pub fn clean_title(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let words = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = words.chars().take(MAX_TITLE).collect();
    cut.trim_end().to_string()
}

/// A stored event made safe to use; anything malformed is dropped.
pub fn clean(v: &Value) -> Option<Event> {
    let o = v.as_object()?;
    let title = clean_title(o.get("title")?.as_str()?);
    if title.is_empty() {
        return None;
    }
    let date = Date::parse(o.get("date")?.as_str()?)?;
    let time = o
        .get("time")
        .and_then(Value::as_str)
        .and_then(normalize_time)
        .flatten();
    let repeat = match o.get("repeat").and_then(Value::as_str) {
        Some("weekly") => Repeat::Weekly,
        Some("monthly") => Repeat::Monthly,
        Some("yearly") => Repeat::Yearly,
        _ => Repeat::None,
    };
    let remind = match o.get("remind") {
        Some(Value::Number(n)) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite() && *f >= 0.0)
                .map(|f| f as u64)
        }),
        Some(Value::String(s)) => {
            let s = s.trim();
            (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
                .then(|| s.parse::<u64>().unwrap_or(u64::MAX))
        }
        _ => None,
    }
    .map(|m| m.min(1440) as u32);
    let id = match o.get("id") {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    Some(Event {
        id: if id.is_empty() { title.clone() } else { id },
        date,
        time,
        title,
        repeat,
        remind,
    })
}

/// The events of a file: a list, or an object with an `events` list. Bad
/// entries are dropped; a file that is not JSON of that shape is `None`.
pub fn parse_events(text: &str) -> Option<Vec<Event>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).trim();
    if text.is_empty() {
        return Some(Vec::new());
    }
    let value: Value = serde_json::from_str(text).ok()?;
    let list = match &value {
        Value::Array(list) => list,
        Value::Object(o) => o.get("events")?.as_array()?,
        _ => return None,
    };
    Some(list.iter().filter_map(clean).take(MAX_EVENTS).collect())
}

/// Whether `ev` happens on day `d`.
pub fn occurs_on(ev: &Event, d: Date) -> bool {
    let start = ev.date;
    if d < start {
        return false;
    }
    if d == start {
        return true;
    }
    match ev.repeat {
        Repeat::None => false,
        Repeat::Weekly => d.weekday() == start.weekday(),
        Repeat::Monthly => d.d == start.d,
        Repeat::Yearly => d.d == start.d && d.m == start.m,
    }
}

/// A reminder to tell now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// Day, event and time: told once for each.
    pub key: String,
    pub title: String,
    /// Minutes left until the event.
    pub minutes: u32,
}

/// The timed events of today and tomorrow as (start, lead in minutes, key,
/// title), start in wall seconds. Tomorrow counts so that an event just
/// after midnight is reminded before it.
fn upcoming(events: &[Event], now: i64, default_lead: u32) -> Vec<(i64, u32, String, &str)> {
    let today = date_of(now);
    let mut out = Vec::new();
    for day in [today, Date::from_days(today.days() + 1)] {
        for ev in events {
            let Some(t) = ev.time else { continue };
            if !occurs_on(ev, day) {
                continue;
            }
            let start = wall(day, (t / 60) as u8, (t % 60) as u8);
            let key = format!("{}|{}|{}", day.iso(), ev.id, time_text(t));
            out.push((
                start,
                ev.remind.unwrap_or(default_lead),
                key,
                ev.title.as_str(),
            ));
        }
    }
    out
}

/// The reminders due at `now`: events starting in at most their lead
/// minutes. A window rather than the exact minute, so one is not lost to a
/// restart or a suspended laptop.
pub fn due_reminders(events: &[Event], now: i64, default_lead: u32) -> Vec<Due> {
    let now_min = now - now.rem_euclid(60);
    let mut out: Vec<Due> = upcoming(events, now, default_lead)
        .into_iter()
        .filter_map(|(start, lead, key, title)| {
            let mins = u32::try_from((start - now_min) / 60).ok()?;
            (mins <= lead).then(|| Due {
                key,
                title: title.to_string(),
                minutes: mins,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.minutes, &a.title).cmp(&(b.minutes, &b.title)));
    out
}

/// The wall moment of the next reminder that is not due yet, if any.
pub fn next_due(events: &[Event], now: i64, default_lead: u32) -> Option<i64> {
    let now_min = now - now.rem_euclid(60);
    upcoming(events, now, default_lead)
        .into_iter()
        .map(|(start, lead, _, _)| start - i64::from(lead) * 60)
        .filter(|&at| at > now_min)
        .min()
}

/// What a reminder says under the title.
pub fn detail(t: &Texts, minutes: u32) -> String {
    if minutes == 0 {
        t.now.to_string()
    } else {
        texts::fill(t.event_in, &[minutes])
    }
}

/// A calendar link as (host, path and query) for an HTTPS request:
/// `https://` and `webcal://` only, no port, no user, no spaces.
pub fn split_url(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    if url.is_empty()
        || url.len() > MAX_URL
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let has = |p: &str| {
        url.get(..p.len())
            .is_some_and(|h| h.eq_ignore_ascii_case(p))
    };
    let rest = if has("https://") {
        url.get(8..)?
    } else if has("webcal://") {
        url.get(9..)?
    } else {
        return None;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = rest.get(..end)?;
    if host.is_empty()
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
    {
        return None;
    }
    let tail = rest.get(end..)?.split('#').next().unwrap_or("");
    let path = match tail {
        "" => "/".to_string(),
        t if t.starts_with('?') => format!("/{t}"),
        t => t.to_string(),
    };
    Some((host.to_string(), path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texts::{EN, ES};
    use serde_json::json;

    fn date(y: i32, m: u8, d: u8) -> Date {
        Date::new(y, m, d).expect("a real date")
    }

    fn event(id: &str, d: Date, time: &str, repeat: Repeat) -> Event {
        Event {
            id: id.into(),
            date: d,
            time: normalize_time(time).flatten(),
            title: format!("T {id}"),
            repeat,
            remind: None,
        }
    }

    /// 2026-11-19 at `h`:`m`:`s`.
    fn at(h: u8, m: u8, s: i64) -> i64 {
        wall(date(2026, 11, 19), h, m) + s
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(date(1970, 1, 1).days(), 0);
        assert_eq!(Date::from_days(0), date(1970, 1, 1));
        let leap = date(2024, 2, 29);
        assert_eq!(Date::from_days(leap.days()), leap);
        assert_eq!(date(2026, 11, 19).weekday(), 4);
        assert_eq!(date(1970, 1, 1).weekday(), 4);
        for n in (0..60_000).step_by(37) {
            let d = Date::from_days(n);
            assert_eq!(d.days(), n);
            assert_eq!(Date::parse(&d.iso()), Some(d));
        }
        assert_eq!(date_of(at(23, 59, 59)), date(2026, 11, 19));
        assert_eq!(minute_of(at(9, 5, 30)), 545);
        assert_eq!(time_text(545), "09:05");
        assert_eq!(date_of(-1).y, 1969);
    }

    #[test]
    fn dates_must_be_real() {
        assert_eq!(Date::parse("2026-02-30"), None);
        assert_eq!(Date::parse("2026-1-5"), None);
        assert_eq!(Date::parse("2026-11-19x"), None);
        assert_eq!(Date::parse("２０２６-11-19"), None);
        assert_eq!(Date::parse("1969-12-31"), None);
        assert_eq!(Date::parse("2023-02-29"), None);
        assert_eq!(Date::parse("2024-02-29"), Some(date(2024, 2, 29)));
        assert_eq!(Date::new(2026, 13, 1), None);
        assert_eq!(Date::new(2026, 0, 1), None);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
    }

    #[test]
    fn times_are_read_loosely() {
        assert_eq!(normalize_time(""), Some(None));
        assert_eq!(normalize_time("  "), Some(None));
        assert_eq!(normalize_time("9:05"), Some(Some(545)));
        assert_eq!(normalize_time("0905"), Some(Some(545)));
        assert_eq!(normalize_time("905"), Some(Some(545)));
        assert_eq!(normalize_time("15:00"), Some(Some(900)));
        assert_eq!(normalize_time("9:5"), None);
        assert_eq!(normalize_time("24:00"), None);
        assert_eq!(normalize_time("12:60"), None);
        assert_eq!(normalize_time("noon"), None);
        assert_eq!(normalize_time("９:05"), None);
    }

    #[test]
    fn events_are_cleaned() {
        let ok = clean(
            &json!({"date": "2026-11-20", "time": "15:00", "title": " Dentist ",
            "repeat": "weekly", "remind": 15, "id": "d1"}),
        )
        .expect("an event");
        assert_eq!(ok.title, "Dentist");
        assert_eq!(ok.time, Some(900));
        assert_eq!(ok.repeat, Repeat::Weekly);
        assert_eq!(ok.remind, Some(15));
        assert_eq!(ok.id, "d1");
        assert_eq!(clean(&json!({"date": "2026-11-20"})), None);
        assert_eq!(clean(&json!({"date": "2026-11-20", "title": "  "})), None);
        assert_eq!(clean(&json!({"date": "2026-13-20", "title": "x"})), None);
        assert_eq!(clean(&json!("event")), None);
        let odd = clean(
            &json!({"date": "2026-11-20", "title": "x", "repeat": "hourly",
            "time": "late", "remind": -5}),
        )
        .expect("an event");
        assert_eq!(odd.repeat, Repeat::None);
        assert_eq!(odd.time, None);
        assert_eq!(odd.remind, None);
        assert_eq!(odd.id, "x");
    }

    #[test]
    fn remind_comes_in_several_shapes() {
        let remind = |v: Value| {
            clean(&json!({"date": "2026-11-20", "title": "x", "remind": v})).and_then(|e| e.remind)
        };
        assert_eq!(remind(json!(0)), Some(0));
        assert_eq!(remind(json!("20")), Some(20));
        assert_eq!(remind(json!(99_999)), Some(1440));
        assert_eq!(remind(json!("99999999999999999999999")), Some(1440));
        assert_eq!(remind(json!(7.9)), Some(7));
        assert_eq!(remind(json!(-1.5)), None);
        assert_eq!(remind(json!("-3")), None);
        assert_eq!(remind(json!("soon")), None);
        assert_eq!(remind(json!(null)), None);
        let numeric_id = clean(&json!({"date": "2026-11-20", "title": "x", "id": 42}));
        assert_eq!(numeric_id.map(|e| e.id), Some("42".to_string()));
    }

    #[test]
    fn titles_are_made_safe() {
        assert_eq!(clean_title("a\u{7}b\r\nc\t  d"), "a b c d");
        let long = "x".repeat(300);
        assert_eq!(clean_title(&long).chars().count(), MAX_TITLE);
        let spaced = format!("{} {}", "y".repeat(MAX_TITLE - 1), "z");
        assert_eq!(clean_title(&spaced), "y".repeat(MAX_TITLE - 1));
        assert_eq!(clean_title("ñandú ✓").chars().count(), 7);
    }

    #[test]
    fn files_parse_as_list_or_object() {
        let list = r#"[{"date":"2026-11-20","title":"a"},{"nope":1}]"#;
        assert_eq!(parse_events(list).map(|v| v.len()), Some(1));
        let obj = r#"{"events":[{"date":"2026-11-20","title":"a"}]}"#;
        assert_eq!(parse_events(obj).map(|v| v.len()), Some(1));
        let bom = format!("\u{feff}{list}");
        assert_eq!(parse_events(&bom).map(|v| v.len()), Some(1));
        assert_eq!(parse_events(""), Some(Vec::new()));
        assert_eq!(parse_events("  \r\n"), Some(Vec::new()));
        assert_eq!(parse_events("{"), None);
        assert_eq!(parse_events("42"), None);
        assert_eq!(parse_events("{\"other\": []}"), None);
        let many = format!(
            "[{}]",
            vec![r#"{"date":"2026-11-20","title":"a"}"#; MAX_EVENTS + 50].join(",")
        );
        assert_eq!(parse_events(&many).map(|v| v.len()), Some(MAX_EVENTS));
        assert_eq!(parse_events(&"[".repeat(5000)), None);
    }

    #[test]
    fn repeats_never_go_before_the_start() {
        let start = date(2026, 11, 19);
        let once = event("a", start, "09:00", Repeat::None);
        assert!(occurs_on(&once, start));
        assert!(!occurs_on(&once, date(2026, 11, 26)));
        let week = event("a", start, "09:00", Repeat::Weekly);
        assert!(occurs_on(&week, date(2026, 11, 26)));
        assert!(occurs_on(&week, date(2027, 1, 7)));
        assert!(!occurs_on(&week, date(2026, 11, 27)));
        assert!(!occurs_on(&week, date(2026, 11, 12)));
        let month = event("a", start, "09:00", Repeat::Monthly);
        assert!(occurs_on(&month, date(2027, 3, 19)));
        assert!(!occurs_on(&month, date(2027, 3, 20)));
        assert!(!occurs_on(&month, date(2026, 10, 19)));
        let year = event("a", start, "09:00", Repeat::Yearly);
        assert!(occurs_on(&year, date(2030, 11, 19)));
        assert!(!occurs_on(&year, date(2030, 12, 19)));
        assert!(!occurs_on(&year, date(2025, 11, 19)));
    }

    #[test]
    fn reminders_come_inside_their_window() {
        let today = date(2026, 11, 19);
        let ev = [event("e1", today, "15:00", Repeat::None)];
        assert!(due_reminders(&ev, at(14, 49, 0), 10).is_empty());
        let due = due_reminders(&ev, at(14, 50, 0), 10);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].minutes, 10);
        assert_eq!(due[0].key, "2026-11-19|e1|15:00");
        assert_eq!(due[0].title, "T e1");
        assert_eq!(due_reminders(&ev, at(14, 57, 0), 10)[0].minutes, 3);
        assert_eq!(due_reminders(&ev, at(15, 0, 0), 10)[0].minutes, 0);
        assert!(due_reminders(&ev, at(15, 1, 0), 10).is_empty());
        // The seconds do not matter.
        assert_eq!(due_reminders(&ev, at(14, 49, 59), 10).len(), 0);
        assert_eq!(due_reminders(&ev, at(14, 50, 59), 10).len(), 1);
        assert_eq!(due_reminders(&ev, at(15, 0, 59), 10).len(), 1);
    }

    #[test]
    fn each_event_may_set_its_own_lead() {
        let today = date(2026, 11, 19);
        let mut ev = [event("e1", today, "15:00", Repeat::None)];
        ev[0].remind = Some(30);
        assert_eq!(due_reminders(&ev, at(14, 30, 0), 10).len(), 1);
        ev[0].remind = Some(0);
        assert!(due_reminders(&ev, at(14, 59, 0), 10).is_empty());
        assert_eq!(due_reminders(&ev, at(15, 0, 0), 10).len(), 1);
    }

    #[test]
    fn all_day_events_never_remind() {
        let ev = [event("e1", date(2026, 11, 19), "", Repeat::None)];
        assert!(due_reminders(&ev, at(0, 0, 0), 1440).is_empty());
        assert_eq!(next_due(&ev, at(0, 0, 0), 1440), None);
    }

    #[test]
    fn repeats_remind_today_and_tomorrow() {
        let ev = [event("w", date(2026, 11, 12), "15:00", Repeat::Weekly)];
        assert_eq!(due_reminders(&ev, at(14, 55, 0), 10).len(), 1);
        let late = [event("n", date(2026, 11, 20), "00:05", Repeat::None)];
        let due = due_reminders(&late, at(23, 56, 0), 10);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].minutes, 9);
        assert_eq!(due[0].key, "2026-11-20|n|00:05");
        assert!(due_reminders(&late, at(23, 40, 0), 10).is_empty());
    }

    #[test]
    fn reminders_are_ordered() {
        let today = date(2026, 11, 19);
        let ev = [
            event("b", today, "15:05", Repeat::None),
            event("a", today, "15:02", Repeat::None),
        ];
        let due = due_reminders(&ev, at(14, 55, 0), 10);
        assert_eq!(due.iter().map(|d| d.minutes).collect::<Vec<_>>(), [7, 10]);
    }

    #[test]
    fn the_next_reminder_is_found() {
        let today = date(2026, 11, 19);
        let ev = [
            event("a", today, "15:00", Repeat::None),
            event("b", today, "18:00", Repeat::None),
        ];
        assert_eq!(next_due(&ev, at(9, 0, 30), 10), Some(at(14, 50, 0)));
        // Inside the first one's window: the second is next.
        assert_eq!(next_due(&ev, at(14, 55, 0), 10), Some(at(17, 50, 0)));
        assert_eq!(next_due(&ev, at(18, 5, 0), 10), None);
        assert_eq!(next_due(&[], at(9, 0, 0), 10), None);
        let late = [event("n", date(2026, 11, 20), "00:05", Repeat::None)];
        assert_eq!(next_due(&late, at(22, 0, 0), 10), Some(at(23, 55, 0)));
        let far = [event("f", date(2026, 11, 22), "08:00", Repeat::None)];
        assert_eq!(next_due(&far, at(9, 0, 0), 10), None);
    }

    #[test]
    fn detail_says_how_long() {
        assert_eq!(detail(&EN, 0), "now");
        assert_eq!(detail(&EN, 10), "in 10 min");
        assert_eq!(detail(&ES, 0), "ahora");
        assert_eq!(detail(&ES, 5), "en 5 min");
    }

    #[test]
    fn links_are_taken_only_when_safe() {
        let split = |u: &str| split_url(u);
        let pair = |h: &str, p: &str| Some((h.to_string(), p.to_string()));
        assert_eq!(
            split("https://cal.example.com/a/b.ics"),
            pair("cal.example.com", "/a/b.ics")
        );
        assert_eq!(
            split("  HTTPS://cal.example.com  "),
            pair("cal.example.com", "/")
        );
        assert_eq!(
            split("webcal://cal.example.com/x.ics"),
            pair("cal.example.com", "/x.ics")
        );
        assert_eq!(
            split("https://cal.example.com/x?k=1&b=2"),
            pair("cal.example.com", "/x?k=1&b=2")
        );
        assert_eq!(
            split("https://cal.example.com?k=1"),
            pair("cal.example.com", "/?k=1")
        );
        assert_eq!(
            split("https://cal.example.com/x#frag"),
            pair("cal.example.com", "/x")
        );
        assert_eq!(split("http://cal.example.com/x"), None);
        assert_eq!(split("ftp://cal.example.com/x"), None);
        assert_eq!(split("https://cal.example.com:8443/x"), None);
        assert_eq!(split("https://user@cal.example.com/x"), None);
        assert_eq!(split("https:///x"), None);
        assert_eq!(split("https://"), None);
        assert_eq!(split(""), None);
        assert_eq!(split("   "), None);
        assert_eq!(split("https://cal.example.com/a b"), None);
        assert_eq!(split("https://cal.example.com/a\u{7}"), None);
        assert_eq!(split("https://cál.example.com/"), None);
        let long = format!("https://a.com/{}", "x".repeat(MAX_URL));
        assert_eq!(split(&long), None);
        assert_eq!(split("é"), None);
    }
}
