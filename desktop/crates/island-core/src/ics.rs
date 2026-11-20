//! Calendars in iCalendar (RFC 5545) form, the private links that Google
//! Calendar, Outlook, iCloud and others give out. The text becomes the
//! agenda's events, one for each time an event happens inside a window, so
//! the repeats are already worked out.
//!
//! What it understands: VEVENT with SUMMARY, DTSTART (UTC, local and TZID
//! times), STATUS:CANCELLED, RRULE (DAILY, WEEKLY, MONTHLY and YEARLY with
//! INTERVAL, COUNT, UNTIL, BYDAY, BYMONTHDAY and BYMONTH), EXDATE and
//! RECURRENCE-ID. A rule it cannot follow keeps only the first time. All-day
//! events are left out: they have no moment to remind. The calendar is
//! untrusted text, so nothing here may panic: no unchecked indexing, bounded
//! loops.
//!
//! Zones: a TZID is placed with the calendar's own VTIMEZONE (Outlook's
//! "Romance Standard Time" and the like). A time in a zone it cannot place
//! is read as local.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::agenda::{self, DAY, Date, Event, MAX_EVENTS, Repeat};

/// The most calendar text read.
pub const MAX_ICS: usize = 8 << 20;
/// The most VEVENTs kept (those outside the window are not counted).
const MAX_VEVENTS: usize = 50_000;
/// The most steps of one rule's walk.
const MAX_ITER: i64 = 20_000;
/// The most steps of all the walks of a calendar.
const MAX_WORK: u64 = 2_000_000;
/// The most parts (STANDARD, DAYLIGHT) kept for a zone.
const MAX_PARTS: usize = 32;
/// The most distinct BYDAY items kept (every weekday with 11 ordinals).
const MAX_BY_DAY: usize = 77;
/// The most days around the window a one-time event may be and still be kept.
const MARGIN_DAYS: i64 = 3;
/// The latest moment there is (the end of 9999) in seconds since 1970.
const LAST: i64 = 253_402_300_799;
const WEEKDAYS: [&str; 7] = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];

/// A DATE or DATE-TIME value on the event's own wall clock.
#[derive(Debug, Clone)]
struct Stamp {
    date: Date,
    h: u8,
    mi: u8,
    all_day: bool,
    utc: bool,
    tzid: String,
}

/// A moment: absolute (UTC seconds) or on the local wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum At {
    Utc(i64),
    Local(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone)]
struct Rule {
    freq: Freq,
    interval: u32,
    /// None: no end.
    count: Option<u64>,
    until: Option<Stamp>,
    /// (ordinal, weekday); ordinal 0 is every one.
    by_day: Vec<(i8, u8)>,
    by_month_day: Vec<i8>,
    by_month: Vec<u8>,
}

/// The yearly rule of a zone part.
#[derive(Debug, Clone)]
struct ZoneRule {
    month: u8,
    /// The (ordinal, weekday) it falls on.
    by_day: Option<(i8, u8)>,
    /// The days of the month it may fall on, as listed.
    month_days: Vec<i8>,
}

/// A STANDARD or DAYLIGHT part of a VTIMEZONE.
#[derive(Debug, Clone, Default)]
struct ZonePart {
    start: Option<Stamp>,
    /// Offsets in minutes east of UTC.
    from: Option<i32>,
    to: Option<i32>,
    yearly: Option<ZoneRule>,
}

#[derive(Debug, Default)]
struct Vev {
    uid: String,
    summary: String,
    start: Option<Stamp>,
    cancelled: bool,
    rrule: Option<Rule>,
    exdates: Vec<Stamp>,
    recurrence_id: Option<Stamp>,
}

type Zones = HashMap<String, Vec<ZonePart>>;

/// Long lines are folded onto the next one, which starts with a space or tab.
fn unfold(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("\n ", "")
        .replace("\n\t", "")
}

/// A content line: `DTSTART;TZID=Europe/Madrid:20261110T090000`.
struct Line<'a> {
    name: String,
    tzid: String,
    /// The VALUE parameter, upper case.
    kind: String,
    value: &'a str,
}

fn parse_line(line: &str) -> Option<Line<'_>> {
    let mut quoted = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon.filter(|&i| i >= 1)?;
    let head = line.get(..colon)?;
    let value = line.get(colon + 1..)?;
    let mut parts = head.split(';');
    let name = parts.next()?.to_uppercase();
    let (mut tzid, mut kind) = (String::new(), String::new());
    for p in parts {
        let Some((k, v)) = p.split_once('=') else {
            continue;
        };
        if k.is_empty() {
            continue;
        }
        let v = v.strip_prefix('"').unwrap_or(v);
        let v = v.strip_suffix('"').unwrap_or(v);
        match k.to_uppercase().as_str() {
            "TZID" => tzid = v.to_string(),
            "VALUE" => kind = v.to_uppercase(),
            _ => {}
        }
    }
    Some(Line {
        name,
        tzid,
        kind,
        value,
    })
}

/// A TEXT value: `\n`, `\,`, `\;` and `\\` undone.
fn unescape_text(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('n' | 'N') => {
                    chars.next();
                    out.push(' ');
                    continue;
                }
                Some(&e @ (',' | ';' | '\\')) => {
                    chars.next();
                    out.push(e);
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    agenda::clean_title(&out)
}

/// A DATE or DATE-TIME value; None when it is not one.
fn parse_stamp(value: &str, tzid: &str, value_date: bool) -> Option<Stamp> {
    let v = value.trim();
    if !v.is_ascii() {
        return None;
    }
    let b = v.as_bytes();
    let num = |from: usize, len: usize| -> Option<u32> {
        let s = b.get(from..from.checked_add(len)?)?;
        s.iter()
            .all(u8::is_ascii_digit)
            .then(|| s.iter().fold(0u32, |n, &c| n * 10 + u32::from(c - b'0')))
    };
    let date = Date::new(
        i32::try_from(num(0, 4)?).ok()?,
        u8::try_from(num(4, 2)?).ok()?,
        u8::try_from(num(6, 2)?).ok()?,
    )?;
    let (mut h, mut mi, mut utc) = (0, 0, false);
    let mut all_day = true;
    if b.len() != 8 {
        if b.get(8) != Some(&b'T') {
            return None;
        }
        h = u8::try_from(num(9, 2)?).ok()?;
        mi = u8::try_from(num(11, 2)?).ok()?;
        let mut pos = 13;
        if b.len() > 14 {
            num(13, 2)?;
            pos = 15;
        }
        match b.len().checked_sub(pos)? {
            0 => {}
            1 if b.get(pos) == Some(&b'Z') => utc = true,
            _ => return None,
        }
        if h > 23 || mi > 59 {
            return None;
        }
        all_day = false;
    }
    if value_date {
        all_day = true;
    }
    if all_day {
        (h, mi) = (0, 0);
    }
    Some(Stamp {
        date,
        h,
        mi,
        all_day,
        utc,
        tzid: tzid.to_string(),
    })
}

/// A TZOFFSETFROM or TZOFFSETTO value, `+0100`, in minutes.
fn offset_minutes(text: &str) -> Option<i32> {
    let t = text.trim();
    if !t.is_ascii() {
        return None;
    }
    let b = t.as_bytes();
    let sign = match b.first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let two = |i: usize| -> Option<i32> {
        let s = b.get(i..i + 2)?;
        s.iter()
            .all(u8::is_ascii_digit)
            .then(|| i32::from(s[0] - b'0') * 10 + i32::from(s[1] - b'0'))
    };
    Some(sign * (two(1)? * 60 + two(3)?))
}

fn weekday_index(name: &str) -> Option<u8> {
    WEEKDAYS.iter().position(|w| *w == name).map(|i| i as u8)
}

/// One BYDAY item: `MO`, `2TU`, `-1SU`.
fn by_day_item(s: &str) -> Option<(i8, u8)> {
    if s.len() < 2 || !s.is_ascii() {
        return None;
    }
    let (n, day) = s.split_at(s.len() - 2);
    let wd = weekday_index(day)?;
    if n.is_empty() {
        return Some((0, wd));
    }
    let digits = n.strip_prefix(['+', '-']).unwrap_or(n);
    if !(1..=2).contains(&digits.len()) || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((n.parse::<i8>().ok()?, wd))
}

/// The nth (negative: from the end) weekday `wd` of a month: its day, or 0
/// when the month has none.
fn nth_weekday(y: i32, m: u8, wd: u8, n: i8) -> u8 {
    let Some(first) = Date::new(y, m, 1) else {
        return 0;
    };
    let len = i32::from(agenda::days_in_month(y, m));
    let wd = i32::from(wd);
    let n = i32::from(n);
    let day = if n > 0 {
        1 + (wd - i32::from(first.weekday()) + 7) % 7 + (n - 1) * 7
    } else if n < 0 {
        let last = (i32::from(first.weekday()) + len - 1) % 7;
        len - (last - wd + 7) % 7 + (n + 1) * 7
    } else {
        0
    };
    if (1..=len).contains(&day) {
        day as u8
    } else {
        0
    }
}

/// A VTIMEZONE's offset (minutes east of UTC) at a wall moment, from its
/// STANDARD and DAYLIGHT parts and their yearly rules.
fn vtz_offset(parts: &[ZonePart], at: i64) -> Option<i32> {
    let y = agenda::date_of(at).y;
    let mut best: Option<(i64, &ZonePart)> = None;
    let mut first: Option<(i64, &ZonePart)> = None;
    for p in parts {
        let (Some(start), Some(_)) = (&p.start, p.to) else {
            continue;
        };
        let day0 = start.date.days();
        if first.is_none_or(|(f, _)| day0 < f) {
            first = Some((day0, p));
        }
        let mut take = |t: i64| {
            if t <= at && best.is_none_or(|(b, _)| t > b) {
                best = Some((t, p));
            }
        };
        match &p.yearly {
            Some(rule) => {
                for yy in y.saturating_sub(1)..=y {
                    let day = zone_day(rule, yy, start.date.d);
                    if let Some(d) = Date::new(yy, rule.month, day) {
                        take(agenda::wall(d, start.h, start.mi));
                    }
                }
            }
            None => take(agenda::wall(start.date, start.h, start.mi)),
        }
    }
    if let Some((_, p)) = best {
        return p.to;
    }
    first.and_then(|(_, p)| p.from.or(p.to))
}

/// The day of a month a zone rule falls on in a year (0: none). A weekday
/// with no ordinal is the first one; with BYMONTHDAY it is the first listed
/// day that is that weekday (Exchange's way of saying "the second Sunday").
fn zone_day(rule: &ZoneRule, y: i32, start_day: u8) -> u8 {
    let len = i32::from(agenda::days_in_month(y, rule.month));
    let listed = rule.month_days.iter().filter_map(|&md| {
        let d = if md > 0 {
            i32::from(md)
        } else {
            len + i32::from(md) + 1
        };
        (1..=len).contains(&d).then_some(d as u8)
    });
    match rule.by_day {
        Some((n, wd)) if rule.month_days.is_empty() => {
            nth_weekday(y, rule.month, wd, if n == 0 { 1 } else { n })
        }
        Some((_, wd)) => {
            let mut days = listed;
            days.find(|&d| Date::new(y, rule.month, d).is_some_and(|x| x.weekday() == wd))
                .unwrap_or(0)
        }
        None => {
            let mut days = listed;
            days.next().unwrap_or(start_day)
        }
    }
}

/// The yearly rule of a zone part: `FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU`.
fn zone_yearly(value: &str) -> Option<ZoneRule> {
    let (mut yearly, mut month, mut by_day) = (false, None, None);
    let mut month_days = Vec::new();
    for part in value.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.to_uppercase();
        match k.to_uppercase().as_str() {
            "FREQ" => yearly = v == "YEARLY",
            "BYMONTH" => month = v.parse::<u8>().ok().filter(|m| (1..=12).contains(m)),
            "BYDAY" => by_day = by_day_item(&v).filter(|(n, _)| n.abs() <= 9),
            "BYMONTHDAY" => {
                month_days = v
                    .split(',')
                    .filter_map(|s| s.trim().parse::<i8>().ok())
                    .filter(|n| *n != 0 && n.abs() <= 31)
                    .take(31)
                    .collect();
            }
            _ => {}
        }
    }
    yearly.then_some(month?).map(|month| ZoneRule {
        month,
        by_day,
        month_days,
    })
}

/// A recurrence rule; None when it has anything this cannot follow.
fn parse_rule(value: &str) -> Option<Rule> {
    let mut rule = Rule {
        freq: Freq::Daily,
        interval: 1,
        count: None,
        until: None,
        by_day: Vec::new(),
        by_month_day: Vec::new(),
        by_month: Vec::new(),
    };
    let mut freq = None;
    for part in value.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        if k.is_empty() {
            continue;
        }
        let v = v.to_uppercase();
        match k.to_uppercase().as_str() {
            "FREQ" => {
                freq = Some(match v.as_str() {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                });
            }
            "INTERVAL" => rule.interval = v.parse::<u32>().unwrap_or(1).clamp(1, 1000),
            "COUNT" => rule.count = v.parse::<u64>().ok().filter(|&n| n >= 1),
            "UNTIL" => rule.until = parse_stamp(&v, "", false),
            // Lists are kept as sets: a long one repeating itself costs
            // nothing.
            "BYDAY" => {
                let set: BTreeSet<(i8, u8)> = v.split(',').filter_map(by_day_item).collect();
                rule.by_day = set.into_iter().take(MAX_BY_DAY).collect();
            }
            "BYMONTHDAY" => {
                let set: BTreeSet<i8> = v
                    .split(',')
                    .filter_map(|s| s.trim().parse::<i8>().ok())
                    .filter(|n| *n != 0 && n.abs() <= 31)
                    .collect();
                rule.by_month_day = set.into_iter().collect();
            }
            "BYMONTH" => {
                let set: BTreeSet<u8> = v
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u8>().ok())
                    .filter(|m| (1..=12).contains(m))
                    .collect();
                rule.by_month = set.into_iter().collect();
            }
            "WKST" => {}
            _ => return None,
        }
    }
    rule.freq = freq?;
    Some(rule)
}

/// The days of a month a monthly (or yearly) rule picks, in order.
fn days_in_month_for(rule: &Rule, y: i32, m: u8, start_day: u8) -> Vec<u8> {
    let len = agenda::days_in_month(y, m);
    let weekday = |d: u8| Date::new(y, m, d).map(Date::weekday);
    let mut days = Vec::new();
    if !rule.by_month_day.is_empty() {
        for &md in &rule.by_month_day {
            let d = if md > 0 {
                i32::from(md)
            } else {
                i32::from(len) + i32::from(md) + 1
            };
            if (1..=i32::from(len)).contains(&d) {
                days.push(d as u8);
            }
        }
    } else if !rule.by_day.is_empty() {
        for &(n, wd) in &rule.by_day {
            if n != 0 {
                let d = nth_weekday(y, m, wd, n);
                if d != 0 {
                    days.push(d);
                }
            } else {
                days.extend((1..=len).filter(|&d| weekday(d) == Some(wd)));
            }
        }
    } else if start_day <= len {
        days.push(start_day);
    }
    if !rule.by_month_day.is_empty() && !rule.by_day.is_empty() {
        // Both: a day has to match the two.
        days.retain(|&d| weekday(d).is_some_and(|w| rule.by_day.iter().any(|&(_, wd)| wd == w)));
    }
    days.sort_unstable();
    days.dedup();
    days
}

/// What a rule's walk collects.
struct Walk<'a> {
    /// The days from `from_wall` on.
    out: Vec<Date>,
    /// The days taken so far, the ones before `from_wall` too (for COUNT).
    seen: u64,
    count: Option<u64>,
    start_wall: i64,
    from_wall: i64,
    limit_wall: i64,
    past_until: &'a dyn Fn(Date) -> bool,
    /// Steps left for all the walks of the calendar.
    work: u64,
}

impl Walk<'_> {
    /// Takes a day; false when the walk is over.
    fn push(&mut self, date: Option<Date>) -> bool {
        let Some(date) = date else { return false };
        if self.work == 0 {
            return false;
        }
        self.work -= 1;
        let w = date.days() * DAY;
        if w < self.start_wall {
            return true;
        }
        if w > self.limit_wall || (self.past_until)(date) {
            return false;
        }
        self.seen += 1;
        if w >= self.from_wall {
            self.out.push(date);
        }
        self.count.is_none_or(|c| self.seen < c)
    }
}

/// The dates a rule gives from `st`, in order, until `limit_wall`, UNTIL,
/// COUNT or the work is used up; the ones before `from_wall` are counted but
/// not kept. A rule that can be worked out jumps ahead to `from_wall`.
fn expand_dates(
    st: &Stamp,
    rule: &Rule,
    from_wall: i64,
    limit_wall: i64,
    past_until: &dyn Fn(Date) -> bool,
    work: &mut u64,
) -> Vec<Date> {
    let interval = i64::from(rule.interval);
    let start_days = st.date.days();
    let mut walk = Walk {
        out: Vec::new(),
        seen: 0,
        count: rule.count,
        start_wall: start_days * DAY,
        from_wall,
        limit_wall,
        past_until,
        work: *work,
    };
    // The week's days, Monday first, and the year's months: once.
    let mut wds: Vec<u8> = rule.by_day.iter().map(|&(_, wd)| wd).collect();
    if wds.is_empty() {
        wds.push(st.date.weekday());
    }
    wds.sort_by_key(|wd| (wd + 6) % 7);
    wds.dedup();
    let months = if rule.by_month.is_empty() {
        vec![st.date.m]
    } else {
        rule.by_month.clone()
    };
    let from = agenda::date_of(from_wall);
    // The steps that come before the window, which need no walking.
    let mut k0 = 0;
    match rule.freq {
        Freq::Daily | Freq::Weekly => {
            let per = interval * if rule.freq == Freq::Daily { 1 } else { 7 } * DAY;
            let before = ((from_wall - walk.start_wall).div_euclid(per) - 1).max(0);
            if rule.count.is_none() {
                k0 = before;
            } else if (rule.freq == Freq::Daily && rule.by_month.is_empty())
                || (rule.freq == Freq::Weekly && rule.by_day.is_empty())
            {
                // One day for each step: COUNT is counted by the step.
                k0 = before;
                walk.seen = before as u64;
            }
        }
        Freq::Monthly if rule.count.is_none() => {
            let gap = i64::from(from.y - st.date.y) * 12 + i64::from(from.m) - i64::from(st.date.m);
            k0 = (gap.div_euclid(interval) - 1).max(0);
        }
        Freq::Yearly if rule.count.is_none() => {
            k0 = (i64::from(from.y - st.date.y).div_euclid(interval) - 1).max(0);
        }
        _ => {}
    }
    if rule.count.is_some_and(|c| walk.seen >= c) {
        return walk.out;
    }
    let in_months = |m: u8| rule.by_month.is_empty() || rule.by_month.contains(&m);
    for k in k0..k0 + MAX_ITER {
        if walk.work == 0 {
            break;
        }
        walk.work -= 1;
        let go = match rule.freq {
            Freq::Daily => {
                let day = Date::from_days(start_days + k * interval);
                if !in_months(day.m) {
                    continue;
                }
                let go = walk.push(Some(day));
                go && day.days() * DAY <= limit_wall
            }
            Freq::Weekly => {
                // Weeks start on Monday (the default WKST).
                let monday = start_days - i64::from((st.date.weekday() + 6) % 7) + k * interval * 7;
                if monday * DAY > limit_wall {
                    break;
                }
                let mut go = true;
                for &wd in &wds {
                    if !go {
                        break;
                    }
                    go = walk.push(Some(Date::from_days(monday + i64::from((wd + 6) % 7))));
                }
                go
            }
            Freq::Monthly => {
                let index = i64::from(st.date.m - 1) + k * interval;
                let y = i64::from(st.date.y) + index / 12;
                let m = (index % 12 + 1) as u8;
                let Some(first) = i32::try_from(y).ok().and_then(|y| Date::new(y, m, 1)) else {
                    break;
                };
                if first.days() * DAY > limit_wall {
                    break;
                }
                if !in_months(m) {
                    continue;
                }
                let mut go = true;
                for d in days_in_month_for(rule, first.y, m, st.date.d) {
                    if !go {
                        break;
                    }
                    go = walk.push(Date::new(first.y, m, d));
                }
                go
            }
            Freq::Yearly => {
                let y = i64::from(st.date.y) + k * interval;
                let Some(first) = i32::try_from(y).ok().and_then(|y| Date::new(y, 1, 1)) else {
                    break;
                };
                if first.days() * DAY > limit_wall {
                    break;
                }
                // Without BYMONTHDAY or BYDAY a yearly rule keeps the start
                // day; BYDAY without BYMONTH would mean every such weekday of
                // the year, which nobody uses: it is read within the month.
                let mut go = true;
                'months: for &m in &months {
                    for d in days_in_month_for(rule, first.y, m, st.date.d) {
                        go = walk.push(Date::new(first.y, m, d));
                        if !go {
                            break 'months;
                        }
                    }
                }
                go
            }
        };
        if !go {
            break;
        }
    }
    *work = walk.work;
    walk.out
}

/// djb2 with xor, over UTF-16 units: a short stable name for a UID.
fn hash(s: &str) -> u32 {
    s.encode_utf16()
        .fold(5381u32, |h, c| h.wrapping_mul(33) ^ u32::from(c))
}

/// Wall-clock components in a zone to a moment.
fn instant(date: Date, h: u8, mi: u8, utc: bool, tzid: &str, zones: &Zones) -> At {
    let w = agenda::wall(date, h, mi);
    if utc {
        return At::Utc(w);
    }
    if let Some(parts) = zones.get(tzid)
        && let Some(off) = vtz_offset(parts, w)
    {
        return At::Utc(w - i64::from(off) * 60);
    }
    // Floating, or a zone it cannot place: local time.
    At::Local(w)
}

fn instant_of(st: &Stamp, date: Date, zones: &Zones) -> At {
    instant(date, st.h, st.mi, st.utc, &st.tzid, zones)
}

/// Whether a VEVENT can matter for the days `lo..=hi` (day numbers): a
/// repeating one, or one that moves another, always can; a one-time event
/// when it is on those days.
fn matters(e: &Vev, lo: i64, hi: i64) -> bool {
    if e.rrule.is_some() || e.recurrence_id.is_some() {
        return true;
    }
    e.start
        .as_ref()
        .is_some_and(|s| !s.all_day && (lo..=hi).contains(&s.date.days()))
}

/// The VEVENTs and VTIMEZONEs of a calendar; of the one-time events only
/// those on the days `lo..=hi` (day numbers).
fn read(text: &str, lo: i64, hi: i64) -> (Vec<Vev>, Zones) {
    let src = unfold(text);
    let mut vevents = Vec::new();
    let mut zones = Zones::new();
    let mut stack: Vec<String> = Vec::new();
    let mut ev: Option<Vev> = None;
    let mut tz: Option<(String, Vec<ZonePart>)> = None;
    let mut part: Option<ZonePart> = None;
    for raw in src.split('\n') {
        if raw.is_empty() {
            continue;
        }
        let Some(p) = parse_line(raw) else { continue };
        match p.name.as_str() {
            "BEGIN" => {
                let comp = p.value.trim().to_uppercase();
                if comp == "VEVENT" && !stack.is_empty() {
                    ev = Some(Vev::default());
                } else if comp == "VTIMEZONE" {
                    tz = Some((String::new(), Vec::new()));
                } else if tz.is_some() && (comp == "STANDARD" || comp == "DAYLIGHT") {
                    part = Some(ZonePart::default());
                }
                stack.push(comp);
                continue;
            }
            "END" => {
                match stack.pop().as_deref() {
                    Some("VEVENT") => {
                        if let Some(e) = ev.take()
                            && vevents.len() < MAX_VEVENTS
                            && matters(&e, lo, hi)
                        {
                            vevents.push(e);
                        }
                    }
                    Some("VTIMEZONE") => {
                        if let Some((id, parts)) = tz.take()
                            && !id.is_empty()
                        {
                            zones.entry(id).or_insert(parts);
                        }
                    }
                    Some("STANDARD" | "DAYLIGHT") => {
                        if let (Some((_, parts)), Some(zp)) = (tz.as_mut(), part.take())
                            && parts.len() < MAX_PARTS
                        {
                            parts.push(zp);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            _ => {}
        }
        let top = stack.last().map(String::as_str);
        if top == Some("VEVENT")
            && let Some(e) = ev.as_mut()
        {
            let stamp = || parse_stamp(p.value, &p.tzid, p.kind == "DATE");
            match p.name.as_str() {
                "UID" => e.uid = p.value.trim().to_string(),
                "SUMMARY" => e.summary = unescape_text(p.value),
                "DTSTART" => e.start = stamp(),
                "STATUS" => e.cancelled = p.value.trim().eq_ignore_ascii_case("CANCELLED"),
                "RRULE" => e.rrule = parse_rule(p.value),
                "EXDATE" => {
                    let room = MAX_EVENTS.saturating_sub(e.exdates.len());
                    e.exdates.extend(
                        p.value
                            .split(',')
                            .filter_map(|v| parse_stamp(v, &p.tzid, p.kind == "DATE"))
                            .take(room),
                    );
                }
                "RECURRENCE-ID" => e.recurrence_id = stamp(),
                _ => {}
            }
        } else if top == Some("VTIMEZONE")
            && p.name == "TZID"
            && let Some((id, _)) = tz.as_mut()
        {
            *id = p.value.trim().to_string();
        } else if matches!(top, Some("STANDARD" | "DAYLIGHT"))
            && let Some(zp) = part.as_mut()
        {
            match p.name.as_str() {
                "DTSTART" => zp.start = parse_stamp(p.value, "", false),
                "TZOFFSETFROM" => zp.from = offset_minutes(p.value),
                "TZOFFSETTO" => zp.to = offset_minutes(p.value),
                "RRULE" => zp.yearly = zone_yearly(p.value),
                _ => {}
            }
        }
    }
    (vevents, zones)
}

/// A moment on the local wall clock.
fn local_of(at: At, to_local: &dyn Fn(i64) -> i64) -> i64 {
    match at {
        At::Utc(t) => to_local(t),
        At::Local(w) => w,
    }
}

/// The calendar's timed events that happen between `from` and `to` (UTC
/// seconds), as agenda events on the local clock: `to_local` turns UTC
/// seconds into local wall seconds. All-day events are left out.
pub fn events(text: &str, from: i64, to: i64, to_local: &dyn Fn(i64) -> i64) -> Vec<Event> {
    // Only 1970 to 9999 can be told: keeps every sum below in range.
    let (from, to) = (from.clamp(0, LAST), to.clamp(0, LAST));
    if from > to {
        return Vec::new();
    }
    let (lo, hi) = (to_local(from), to_local(to));
    let (lo_day, hi_day) = (lo.div_euclid(DAY), hi.div_euclid(DAY));
    let (vevents, zones) = read(text, lo_day - MARGIN_DAYS, hi_day + MARGIN_DAYS);
    let inside = |at: At| match at {
        At::Utc(t) => (from..=to).contains(&t),
        At::Local(w) => (lo..=hi).contains(&w),
    };
    // Every moment is compared on the local wall clock, so that a time in
    // UTC and the same one written locally are the same.
    let local = |st: &Stamp, date: Date| local_of(instant_of(st, date, &zones), to_local);

    // Moved or cancelled single occurrences: the master skips them.
    let mut overridden: HashMap<&str, HashSet<i64>> = HashMap::new();
    for v in &vevents {
        if let Some(rid) = &v.recurrence_id
            && !rid.all_day
            && !v.uid.is_empty()
        {
            overridden
                .entry(v.uid.as_str())
                .or_default()
                .insert(local(rid, rid.date));
        }
    }

    let mut out: Vec<Event> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut work = MAX_WORK;
    for v in &vevents {
        if out.len() >= MAX_EVENTS {
            break;
        }
        let Some(st) = &v.start else { continue };
        if st.all_day || v.summary.is_empty() || v.cancelled {
            continue;
        }
        let uid = if v.uid.is_empty() {
            format!("{}|{}{}{}", v.summary, st.date.y, st.date.m, st.date.d)
        } else {
            v.uid.clone()
        };
        let dates = match &v.rrule {
            Some(rule) if v.recurrence_id.is_none() => {
                // The end of the rule: found once. A day well before or
                // after it needs no zone to tell.
                let end = rule.until.as_ref().map(|u| {
                    let exact = (!u.all_day).then(|| local(u, u.date));
                    (u.date.days(), exact)
                });
                let until = |day: Date| {
                    let Some((end_day, exact)) = end else {
                        return false;
                    };
                    let gap = day.days().saturating_sub(end_day);
                    match exact {
                        None => gap > 0,
                        Some(_) if gap > 2 => true,
                        Some(_) if gap < -2 => false,
                        Some(e) => local(st, day) > e,
                    }
                };
                expand_dates(
                    st,
                    rule,
                    from.saturating_sub(2 * DAY),
                    to.saturating_add(2 * DAY),
                    &until,
                    &mut work,
                )
            }
            _ => vec![st.date],
        };
        let excluded: HashSet<i64> = v
            .exdates
            .iter()
            .filter(|x| !x.all_day)
            .map(|x| local(x, x.date))
            .collect();
        let moved = overridden.get(uid.as_str());
        for date in dates {
            let at = instant_of(st, date, &zones);
            let wall = local_of(at, to_local);
            if excluded.contains(&wall)
                || !inside(at)
                || (v.recurrence_id.is_none() && moved.is_some_and(|m| m.contains(&wall)))
            {
                continue;
            }
            let (day, minute) = (agenda::date_of(wall), agenda::minute_of(wall));
            let id = format!(
                "i{:x}-{:04}{:02}{:02}{:02}{:02}",
                hash(&uid),
                day.y,
                day.m,
                day.d,
                minute / 60,
                minute % 60
            );
            if out.len() < MAX_EVENTS && ids.insert(id.clone()) {
                out.push(Event {
                    id,
                    date: day,
                    time: Some(minute),
                    title: v.summary.clone(),
                    repeat: Repeat::None,
                    remind: None,
                });
            }
        }
    }
    out.sort_by_key(|e| (e.date, e.time));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).expect("a real date")
    }

    /// UTC seconds of a moment.
    fn utc(y: i32, m: u8, day: u8, h: u8, mi: u8) -> i64 {
        agenda::wall(d(y, m, day), h, mi)
    }

    /// The window of the tests: 2026-11-18 .. 2026-11-27 (midnights), UTC.
    fn window() -> (i64, i64) {
        (utc(2026, 11, 18, 0, 0), utc(2026, 11, 27, 0, 0))
    }

    fn run(body: &str, offset_min: i64) -> Vec<Event> {
        let text = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{body}END:VCALENDAR\r\n");
        let (from, to) = window();
        events(&text, from, to, &|t| t + offset_min * 60)
    }

    fn vevent(lines: &str) -> String {
        format!("BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Standup\r\n{lines}END:VEVENT\r\n")
    }

    fn days_of(list: &[Event]) -> Vec<String> {
        list.iter().map(|e| e.date.iso()).collect()
    }

    #[test]
    fn a_line_is_unfolded_and_unescaped() {
        let list = run(
            &vevent("DTSTART:20261120T150000Z\r\nDESCRIPTION:x\r\n").replace(
                "SUMMARY:Standup",
                "SUMMARY:Dentist\\, Dr. Ruiz\\; floor\\n2 \r\n  and more",
            ),
            0,
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Dentist, Dr. Ruiz; floor 2 and more");
        assert_eq!(list[0].time, Some(900));
        assert_eq!(list[0].date, d(2026, 11, 20));
        assert_eq!(list[0].repeat, Repeat::None);
        assert_eq!(list[0].remind, None);
        assert!(list[0].id.starts_with('i') && list[0].id.ends_with("-202611201500"));
    }

    #[test]
    fn utc_times_move_to_the_local_clock() {
        let body = vevent("DTSTART:20261120T030000Z\r\n");
        let list = run(&body, -6 * 60);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].date, d(2026, 11, 19));
        assert_eq!(list[0].time, Some(21 * 60));
        let east = run(&vevent("DTSTART:20261120T230000Z\r\n"), 2 * 60);
        assert_eq!(east[0].date, d(2026, 11, 21));
        assert_eq!(east[0].time, Some(60));
    }

    #[test]
    fn floating_times_stay_as_written() {
        let list = run(&vevent("DTSTART:20261120T090000\r\n"), -6 * 60);
        assert_eq!(list[0].date, d(2026, 11, 20));
        assert_eq!(list[0].time, Some(540));
    }

    const MADRID: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Madrid\r\n\
BEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\n\
RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\r\nEND:STANDARD\r\n\
BEGIN:DAYLIGHT\r\nDTSTART:19700329T020000\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\n\
RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n";

    #[test]
    fn a_zone_follows_its_own_daylight_saving() {
        // The local clock is UTC here, so the hour shows the zone's offset.
        let winter = run(
            &format!(
                "{MADRID}{}",
                vevent("DTSTART;TZID=Europe/Madrid:20261120T100000\r\n")
            ),
            0,
        );
        assert_eq!(winter[0].time, Some(9 * 60));
        let (from, to) = (utc(2026, 7, 1, 0, 0), utc(2026, 7, 31, 0, 0));
        let text = format!(
            "BEGIN:VCALENDAR\r\n{MADRID}{}END:VCALENDAR\r\n",
            vevent("DTSTART;TZID=Europe/Madrid:20260715T100000\r\n")
        );
        let summer = events(&text, from, to, &|t| t);
        assert_eq!(summer[0].time, Some(8 * 60));
    }

    #[test]
    fn a_zone_the_calendar_does_not_describe_is_local() {
        let list = run(
            &vevent("DTSTART;TZID=Mars/Olympus:20261120T100000\r\n"),
            -6 * 60,
        );
        assert_eq!(list[0].time, Some(10 * 60));
        let quoted = run(
            &vevent("DTSTART;TZID=\"Mars/Olympus\":20261120T100000\r\n"),
            0,
        );
        assert_eq!(quoted[0].time, Some(10 * 60));
    }

    #[test]
    fn all_day_and_cancelled_events_are_left_out() {
        assert!(run(&vevent("DTSTART;VALUE=DATE:20261120\r\n"), 0).is_empty());
        assert!(run(&vevent("DTSTART:20261120\r\n"), 0).is_empty());
        assert!(
            run(
                &vevent("DTSTART:20261120T100000Z\r\nSTATUS:CANCELLED\r\n"),
                0
            )
            .is_empty()
        );
        assert!(run(&vevent("SUMMARY:Again\r\n"), 0).is_empty());
        let silent = "BEGIN:VEVENT\r\nDTSTART:20261120T100000Z\r\nEND:VEVENT\r\n";
        assert!(run(silent, 0).is_empty());
    }

    #[test]
    fn daily_weekly_and_interval_rules() {
        let daily = run(
            &vevent("DTSTART:20261118T100000Z\r\nRRULE:FREQ=DAILY\r\n"),
            0,
        );
        assert_eq!(daily.len(), 9);
        assert_eq!(daily[0].date, d(2026, 11, 18));
        let every_other = run(
            &vevent("DTSTART:20261118T100000Z\r\nRRULE:FREQ=DAILY;INTERVAL=2\r\n"),
            0,
        );
        assert_eq!(
            days_of(&every_other),
            [
                "2026-11-18",
                "2026-11-20",
                "2026-11-22",
                "2026-11-24",
                "2026-11-26"
            ]
        );
        let week = run(
            &vevent("DTSTART:20261116T100000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR\r\n"),
            0,
        );
        assert_eq!(
            days_of(&week),
            ["2026-11-18", "2026-11-20", "2026-11-23", "2026-11-25"]
        );
        let biweekly = run(
            &vevent("DTSTART:20261109T100000Z\r\nRRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO\r\n"),
            0,
        );
        assert_eq!(days_of(&biweekly), ["2026-11-23"]);
        let plain = run(
            &vevent("DTSTART:20261112T100000Z\r\nRRULE:FREQ=WEEKLY\r\n"),
            0,
        );
        assert_eq!(days_of(&plain), ["2026-11-19", "2026-11-26"]);
    }

    #[test]
    fn count_and_until_end_a_rule() {
        let done = run(
            &vevent("DTSTART:20261001T100000Z\r\nRRULE:FREQ=DAILY;COUNT=5\r\n"),
            0,
        );
        assert!(done.is_empty());
        let reaching = run(
            &vevent("DTSTART:20261116T100000Z\r\nRRULE:FREQ=DAILY;COUNT=4\r\n"),
            0,
        );
        assert_eq!(days_of(&reaching), ["2026-11-18", "2026-11-19"]);
        let over = run(
            &vevent("DTSTART:20261001T100000Z\r\nRRULE:FREQ=DAILY;UNTIL=20261101T000000Z\r\n"),
            0,
        );
        assert!(over.is_empty());
        let cut = run(
            &vevent("DTSTART:20261001T100000Z\r\nRRULE:FREQ=DAILY;UNTIL=20261120T100000Z\r\n"),
            0,
        );
        assert_eq!(days_of(&cut), ["2026-11-18", "2026-11-19", "2026-11-20"]);
        let by_date = run(
            &vevent("DTSTART:20261001T100000Z\r\nRRULE:FREQ=DAILY;UNTIL=20261119\r\n"),
            0,
        );
        assert_eq!(days_of(&by_date), ["2026-11-18", "2026-11-19"]);
    }

    #[test]
    fn monthly_and_yearly_rules() {
        let nth = run(
            &vevent("DTSTART:20260811T100000Z\r\nRRULE:FREQ=MONTHLY;BYDAY=2TU\r\n"),
            0,
        );
        assert!(nth.is_empty());
        let (from, to) = (utc(2026, 11, 1, 0, 0), utc(2027, 3, 1, 0, 0));
        let text = format!(
            "BEGIN:VCALENDAR\r\n{}END:VCALENDAR\r\n",
            vevent("DTSTART:20260811T100000Z\r\nRRULE:FREQ=MONTHLY;BYDAY=2TU\r\n")
        );
        let list = events(&text, from, to, &|t| t);
        assert_eq!(
            days_of(&list),
            ["2026-11-10", "2026-12-08", "2027-01-12", "2027-02-09"]
        );
        let thirty_first = format!(
            "BEGIN:VCALENDAR\r\n{}END:VCALENDAR\r\n",
            vevent("DTSTART:20260831T100000Z\r\nRRULE:FREQ=MONTHLY\r\n")
        );
        let (from, to) = (utc(2026, 8, 1, 0, 0), utc(2027, 1, 15, 0, 0));
        let list = events(&thirty_first, from, to, &|t| t);
        assert_eq!(days_of(&list), ["2026-08-31", "2026-10-31", "2026-12-31"]);
        let last = format!(
            "BEGIN:VCALENDAR\r\n{}END:VCALENDAR\r\n",
            vevent("DTSTART:20260101T100000Z\r\nRRULE:FREQ=MONTHLY;BYMONTHDAY=-1\r\n")
        );
        let (from, to) = (utc(2026, 2, 1, 0, 0), utc(2026, 4, 5, 0, 0));
        let list = events(&last, from, to, &|t| t);
        assert_eq!(days_of(&list), ["2026-02-28", "2026-03-31"]);
        let yearly = run(
            &vevent("DTSTART:20201120T100000Z\r\nRRULE:FREQ=YEARLY\r\n"),
            0,
        );
        assert_eq!(days_of(&yearly), ["2026-11-20"]);
        let in_month = run(
            &vevent("DTSTART:20200101T100000Z\r\nRRULE:FREQ=YEARLY;BYMONTH=11;BYMONTHDAY=25\r\n"),
            0,
        );
        assert_eq!(days_of(&in_month), ["2026-11-25"]);
    }

    #[test]
    fn a_rule_started_years_ago_reaches_the_window() {
        let daily = run(
            &vevent("DTSTART:20200101T100000Z\r\nRRULE:FREQ=DAILY\r\n"),
            0,
        );
        assert_eq!(daily.len(), 9);
        let weekly = run(
            &vevent("DTSTART:20200102T100000Z\r\nRRULE:FREQ=WEEKLY\r\n"),
            0,
        );
        assert_eq!(days_of(&weekly), ["2026-11-19", "2026-11-26"]);
    }

    #[test]
    fn single_times_can_be_excluded_or_moved() {
        let base = "DTSTART:20261118T100000Z\r\nRRULE:FREQ=DAILY\r\n";
        let excluded = run(
            &vevent(&format!(
                "{base}EXDATE:20261119T100000Z,20261121T100000Z\r\n"
            )),
            0,
        );
        assert!(!days_of(&excluded).contains(&"2026-11-19".to_string()));
        assert!(!days_of(&excluded).contains(&"2026-11-21".to_string()));
        assert_eq!(excluded.len(), 7);
        let master = vevent(base);
        let moved = "BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Standup moved\r\n\
RECURRENCE-ID:20261120T100000Z\r\nDTSTART:20261120T160000Z\r\nEND:VEVENT\r\n";
        let list = run(&format!("{master}{moved}"), 0);
        assert_eq!(list.len(), 9);
        let on_20: Vec<_> = list.iter().filter(|e| e.date == d(2026, 11, 20)).collect();
        assert_eq!(on_20.len(), 1);
        assert_eq!(on_20[0].title, "Standup moved");
        assert_eq!(on_20[0].time, Some(16 * 60));
    }

    #[test]
    fn a_rule_it_cannot_follow_keeps_the_first_time() {
        for rule in [
            "FREQ=HOURLY",
            "FREQ=DAILY;BYSETPOS=1",
            "FREQ=SECONDLY;INTERVAL=2",
            "COUNT=3",
        ] {
            let list = run(
                &vevent(&format!("DTSTART:20261120T100000Z\r\nRRULE:{rule}\r\n")),
                0,
            );
            assert_eq!(days_of(&list), ["2026-11-20"], "{rule}");
        }
        let before = run(
            &vevent("DTSTART:20261001T100000Z\r\nRRULE:FREQ=HOURLY\r\n"),
            0,
        );
        assert!(before.is_empty());
    }

    #[test]
    fn the_same_event_twice_is_one() {
        let one = vevent("DTSTART:20261120T100000Z\r\n");
        let list = run(&format!("{one}{one}"), 0);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn events_come_in_order() {
        let a = "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:A\r\nDTSTART:20261121T080000Z\r\nEND:VEVENT\r\n";
        let b = "BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:B\r\nDTSTART:20261120T230000Z\r\nEND:VEVENT\r\n";
        let list = run(&format!("{a}{b}"), 0);
        assert_eq!(
            list.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["B", "A"]
        );
    }

    #[test]
    fn outside_the_window_is_left_out() {
        assert!(run(&vevent("DTSTART:20261201T100000Z\r\n"), 0).is_empty());
        assert!(run(&vevent("DTSTART:20261101T100000Z\r\n"), 0).is_empty());
    }

    #[test]
    fn garbage_never_panics() {
        let (from, to) = window();
        let ident = |t: i64| t;
        let ugly = [
            "",
            "\0\0\0",
            "BEGIN:VCALENDAR",
            "BEGIN:VEVENT\r\nDTSTART:2026",
            "BEGIN:VEVENT\r\nSUMMARY:é\r\nDTSTART:20261120T1000",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:２０２６1120T100000Z\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261320T100000Z\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:99991231T235959Z\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:19700101T000000Z\r\nRRULE:FREQ=DAILY;INTERVAL=99999999999\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261118T100000Z\r\nRRULE:FREQ=DAILY;COUNT=99999999999999999999\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261118T100000Z\r\nRRULE:FREQ=MONTHLY;INTERVAL=1000;BYMONTH=2;BYDAY=-99XX,+5MO,\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261118T100000Z\r\nRRULE:FREQ=YEARLY;INTERVAL=1000;BYMONTHDAY=-31,0,99\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261118T100000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO;INTERVAL=0\r\nEND:VEVENT",
            "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261118T100000Z\r\nRRULE:FREQ=DAILY;UNTIL=zzzz\r\nEND:VEVENT",
            "BEGIN:VTIMEZONE\r\nTZID:Z\r\nBEGIN:STANDARD\r\nDTSTART:00010101T000000\r\nTZOFFSETTO:+9999\r\nRRULE:FREQ=YEARLY;BYMONTH=13;BYDAY=9SU\r\nEND:STANDARD\r\nEND:VTIMEZONE",
            "END:VEVENT\r\nEND:VEVENT\r\nEND:VTIMEZONE\r\nEND:STANDARD",
            ":::;;;==\r\n;\r\n\"\r\n\"\"\":",
            "DTSTART;TZID=\"unterminated:2026",
        ];
        for text in ugly {
            let _ = events(text, from, to, &ident);
            let _ = events(&format!("BEGIN:VCALENDAR\r\n{text}"), from, to, &ident);
        }
        // Inside a calendar, so that the rules are really walked.
        for text in [ugly[7], ugly[8], ugly[9], ugly[10], ugly[11], ugly[12]] {
            let cal = format!("BEGIN:VCALENDAR\r\n{text}\r\nEND:VCALENDAR\r\n");
            for t in [i64::MIN, i64::MAX, 0, -1] {
                let _ = events(&cal, t, t, &ident);
                let _ = events(&cal, t.saturating_sub(1), t.saturating_add(1), &ident);
            }
            let _ = events(&cal, i64::MIN / 4, i64::MAX / 4, &ident);
            let _ = events(&cal, i64::MIN, i64::MAX, &|t| {
                t.saturating_add(i64::MAX / 2)
            });
        }
    }

    /// A calendar of one rule; the time it takes to read, in milliseconds.
    fn millis(text: &str) -> (Vec<Event>, u128) {
        let (from, to) = window();
        let started = std::time::Instant::now();
        let list = events(text, from, to, &|t| t);
        (list, started.elapsed().as_millis())
    }

    fn calendar(body: &str) -> String {
        format!("BEGIN:VCALENDAR\r\n{body}END:VCALENDAR\r\n")
    }

    #[test]
    fn a_flood_of_listed_days_is_cheap() {
        for freq in ["WEEKLY", "MONTHLY"] {
            let byday = vec!["MO"; 200_000].join(",");
            let text = calendar(&vevent(&format!(
                "DTSTART:19700105T100000Z\r\nRRULE:FREQ={freq};COUNT=20000;BYDAY={byday}\r\n"
            )));
            let (_, ms) = millis(&text);
            assert!(ms < 2000, "{freq} took {ms} ms");
        }
        let months = vec!["1,2,3,4,5,6,7,8,9,10,11,12"; 20_000].join(",");
        let days = vec!["1,-1,15"; 20_000].join(",");
        let text = calendar(&vevent(&format!(
            "DTSTART:20200105T100000Z\r\nRRULE:FREQ=YEARLY;BYMONTH={months};BYMONTHDAY={days}\r\n"
        )));
        let (_, ms) = millis(&text);
        assert!(ms < 2000, "yearly lists took {ms} ms");
    }

    #[test]
    fn many_old_counted_rules_are_cheap() {
        let zone = MADRID;
        let mut text = String::from(zone);
        for i in 0..500 {
            text.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:e{i}\r\nSUMMARY:x\r\n\
DTSTART;TZID=Europe/Madrid:19700101T100000\r\n\
RRULE:FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR;COUNT=99999;UNTIL=20991231T000000Z\r\nEND:VEVENT\r\n"
            ));
        }
        let (_, ms) = millis(&calendar(&text));
        assert!(ms < 2000, "took {ms} ms");
    }

    #[test]
    fn a_zone_with_many_parts_is_cheap() {
        let mut text = String::from("BEGIN:VTIMEZONE\r\nTZID:Z\r\n");
        for _ in 0..2000 {
            text.push_str(
                "BEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nTZOFFSETTO:+0100\r\n\
RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\r\nEND:STANDARD\r\n",
            );
        }
        text.push_str("END:VTIMEZONE\r\n");
        for i in 0..20 {
            text.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:e{i}\r\nSUMMARY:x\r\nDTSTART;TZID=Z:19700101T100000\r\n\
RRULE:FREQ=DAILY;COUNT=99999;UNTIL=20991231T000000Z\r\nEND:VEVENT\r\n"
            ));
        }
        let (_, ms) = millis(&calendar(&text));
        assert!(ms < 2000, "took {ms} ms");
    }

    #[test]
    fn a_calendar_of_old_events_keeps_the_current_ones() {
        // More one-time events outside the window than the cap, then one in.
        let mut text = String::new();
        for i in 0..MAX_VEVENTS + 10 {
            text.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:o{i}\r\nSUMMARY:Old\r\nDTSTART:20150101T100000Z\r\nEND:VEVENT\r\n"
            ));
        }
        text.push_str(&vevent("DTSTART:20261120T100000Z\r\n"));
        let (list, ms) = millis(&calendar(&text));
        assert_eq!(list.len(), 1);
        assert!(ms < 1000, "took {ms} ms");
    }

    #[test]
    fn the_exclusions_tell_utc_from_local_times() {
        // The master is local (floating); its exclusion and its moved
        // occurrence are in UTC (the local clock is UTC here).
        let master = "BEGIN:VEVENT\r\nUID:m\r\nSUMMARY:Daily\r\n\
DTSTART:20261118T100000\r\nRRULE:FREQ=DAILY;COUNT=4\r\nEXDATE:20261119T100000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:m\r\nSUMMARY:Moved\r\nRECURRENCE-ID:20261120T100000Z\r\n\
DTSTART:20261120T150000Z\r\nEND:VEVENT\r\n";
        let list = run(master, 0);
        assert_eq!(
            list.iter()
                .map(|e| (e.date.iso(), e.time.unwrap_or(0), e.title.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("2026-11-18".to_string(), 600, "Daily"),
                ("2026-11-20".to_string(), 900, "Moved"),
                ("2026-11-21".to_string(), 600, "Daily"),
            ]
        );
    }

    #[test]
    fn a_zone_rule_with_no_ordinal_fires() {
        // "SU" is the first Sunday; BYMONTHDAY picks the second one.
        let zone = |rule: &str| {
            format!(
                "BEGIN:VTIMEZONE\r\nTZID:Z\r\n\
BEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETFROM:+0000\r\nTZOFFSETTO:+0000\r\nEND:STANDARD\r\n\
BEGIN:DAYLIGHT\r\nDTSTART:19700101T020000\r\nTZOFFSETFROM:+0000\r\nTZOFFSETTO:+0200\r\n\
RRULE:{rule}\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n"
            )
        };
        let at = |rule: &str, day: &str| {
            let text = format!(
                "{}{}",
                zone(rule),
                vevent(&format!("DTSTART;TZID=Z:{day}T100000\r\n"))
            );
            run(&text, 0)
        };
        // November 2026: Sundays are the 1st, 8th, 15th, 22nd. From the 1st
        // the zone is +0200 (a time of 10:00 is 08:00 UTC, the 20th is a
        // Friday), so a rule that fires moves the event to 08:00 on 2026-11-20.
        let first = at("FREQ=YEARLY;BYMONTH=11;BYDAY=SU", "20261120");
        assert_eq!(first[0].time, Some(8 * 60));
        let second = at(
            "FREQ=YEARLY;BYMONTH=11;BYDAY=SU;BYMONTHDAY=8,9,10,11,12,13,14",
            "20261120",
        );
        assert_eq!(second[0].time, Some(8 * 60));
    }

    #[test]
    fn a_big_calendar_is_read_quickly() {
        let mut text = String::from("BEGIN:VCALENDAR\r\n");
        for i in 0..MAX_VEVENTS + 100 {
            text.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:e{i}\r\nSUMMARY:Event {i}\r\n\
DTSTART:20261120T100000Z\r\nRRULE:FREQ=DAILY;COUNT=99999999\r\nEND:VEVENT\r\n"
            ));
        }
        text.push_str("END:VCALENDAR\r\n");
        let (from, to) = window();
        let started = std::time::Instant::now();
        let list = events(&text, from, to, &|t| t);
        assert!(list.len() <= MAX_EVENTS);
        assert!(started.elapsed().as_secs() < 20);
    }

    #[test]
    fn the_pieces_read_what_they_should() {
        assert_eq!(offset_minutes("+0100"), Some(60));
        assert_eq!(offset_minutes("-0530"), Some(-330));
        assert_eq!(offset_minutes("+010000"), Some(60));
        assert_eq!(offset_minutes("0100"), None);
        assert_eq!(offset_minutes("+1"), None);
        assert_eq!(nth_weekday(2026, 3, 0, -1), 29);
        assert_eq!(nth_weekday(2026, 10, 0, -1), 25);
        assert_eq!(nth_weekday(2026, 11, 2, 2), 10);
        assert_eq!(nth_weekday(2026, 2, 1, 5), 0);
        assert_eq!(nth_weekday(2026, 2, 1, 0), 0);
        assert_eq!(nth_weekday(2026, 13, 1, 1), 0);
        assert_eq!(by_day_item("-1SU"), Some((-1, 0)));
        assert_eq!(by_day_item("MO"), Some((0, 1)));
        assert_eq!(by_day_item("2TU"), Some((2, 2)));
        assert_eq!(by_day_item("XX"), None);
        assert_eq!(by_day_item("123MO"), None);
        assert!(parse_stamp("20261120T10", "", false).is_none());
        assert!(parse_stamp("20261120T100000X", "", false).is_none());
        assert!(parse_stamp("20261120T2500", "", false).is_none());
        assert!(parse_stamp("20261120T1000", "", false).is_some_and(|s| s.mi == 0));
        assert!(parse_stamp("20261120T100000Z", "", false).is_some_and(|s| s.utc));
        assert!(parse_stamp("20261120T100000", "", true).is_some_and(|s| s.all_day));
        assert!(rule_ok("FREQ=DAILY;WKST=MO"));
        assert!(!rule_ok("FREQ=DAILY;BYSETPOS=1"));
        assert!(!rule_ok("INTERVAL=2"));
    }

    fn rule_ok(text: &str) -> bool {
        parse_rule(text).is_some()
    }
}
