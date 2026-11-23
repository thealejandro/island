//! Golden vectors shared with the Omarchy plugin's JavaScript models.
//!
//! The core is the source: every case here is run through the core's real
//! functions and the result is stored in `test/vectors/*.json` at the repo
//! root. This test fails when a stored file differs from what the core
//! produces now; `ISLAND_BLESS=1 cargo test -p island-core --test vectors`
//! rewrites the files. `test/vectors.test.mjs` runs the same cases through
//! `LyricsModel.js`, `FocusModel.js`, `MediaModel.js`, `IcsModel.js` and
//! `IslandSettings.js`, so the two sides cannot drift apart unnoticed.
//!
//! A case is `{"fn", "args", "expect"}`; `fn` is a neutral name that each
//! side maps to its own function. A case where the two sides really differ
//! carries `"skip": {"js": "why"}`: the JS test leaves it out and says so,
//! so every exception is written down. All the text in the cases is made up.

use std::fmt::Write as _;
use std::path::PathBuf;

use island_core::focus::{Lengths, Phase, Timer};
use island_core::lyrics::{self, Found, Kind, Line};
use island_core::media::{self, SongText};
use island_core::settings::Settings;
use island_core::{ics, lyrics::cache_key};
use serde_json::{Map, Value, json};

type Cases = Vec<Value>;

fn case(name: &str, args: Value, expect: Value) -> Value {
    json!({ "fn": name, "args": args, "expect": expect })
}

/// The case, marked as one where the JS model is known to differ.
fn skip_js(mut c: Value, reason: &str) -> Value {
    if let Value::Object(o) = &mut c {
        o.insert("skip".into(), json!({ "js": reason }));
    }
    c
}

/// A whole number stays a whole number; anything else is cut to 6 decimals,
/// so f32 results compare with a small tolerance in JS.
fn num(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        json!(v as i64)
    } else {
        json!((v * 1e6).round() / 1e6)
    }
}

fn lines_json(lines: &[Line]) -> Value {
    Value::Array(
        lines
            .iter()
            .map(|l| json!({ "t": l.t, "text": l.text }))
            .collect(),
    )
}

fn lines_of(v: &Value) -> Vec<Line> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| Line {
            t: l["t"].as_u64().unwrap(),
            text: l["text"].as_str().unwrap().to_string(),
        })
        .collect()
}

fn found_json(f: &Found) -> Value {
    let kind = match f.kind {
        Kind::Synced => "synced",
        Kind::Plain => "plain",
        Kind::Instrumental => "instrumental",
        Kind::Nothing => "none",
    };
    json!({ "kind": kind, "lines": lines_json(&f.lines), "offset": f.offset, "id": f.id })
}

// ------------------------------------------------------------------ lyrics

fn lyrics_cases() -> Cases {
    let mut out = Cases::new();

    for tag in [
        "00:17.12",
        "01:02.345",
        "1:02",
        "00:05.5",
        "1:02:03.50",
        "00:12:50",
        "00:61.00",
        "1:61:00",
        "ar:Alguien",
        "",
        "00:00",
        "99:59.999",
        "0:5",
        "12",
        "00:12.1234",
    ] {
        let ms = lyrics::parse_time(tag).map_or(-1, |t| t as i64);
        out.push(case("parse_time", json!([tag]), json!(ms)));
    }

    let lrc = [
        "[00:12.00][00:45.50]Estribillo\n[00:20.00]Verso",
        "[ti:Canción de prueba]\n[ar:Alguien]\n[al:Disco]\n[by:yo]\n[length:03:30]\n[00:01.00]Hola\n[00:02.00]Adiós",
        "[offset:500]\n[00:10.00]Una\n[00:20.00]Dos",
        "[offset:-250]\n[00:10.00]Una\n[00:20.00]Dos",
        "[offset:99999999]\n[00:10.00]Una",
        "[offset:15000]\n[00:10.00]Una\n[00:30.00]Dos",
        "[00:05.00]\n[00:08.00]Letra\n[00:12.00]   \n[00:15.00]Fin",
        "[00:05.00]♪\n[00:09.00]Letra\n[00:13.00]",
        "[00:01.00]<00:01.00>Pa<00:01.50>la<00:02.00>bras",
        "\u{feff}[00:01.00]Con marca de orden",
        "[00:01.00]Uno\r\n[00:02.00]Dos\r[00:03.00]Tres",
        "[00:02.00]Segundo\n[00:01.00]Primero\n[00:01.00]Empate",
        "sin etiquetas\notra línea",
        "[00:01.00]\u{1b}[31mrojo\u{1b}[0m \u{1b}]0;titulo\u{7}limpio",
        "[00:01.00]con\u{200b}\u{202e}ocultos\u{a0}y\ttabs",
        "[00:03.00][00:01.00][00:02.00]Tres sellos",
        "[xx:yy]no es una etiqueta\n[00:01.00]Sí",
        "[00:01.00][nota]texto con corchetes",
        " [00:01.00]  con espacios",
        "[re:player]\n[ve:1.0]\n[tool:x]\n[la:es]\n[id:abc]\n[#:comentario]\n[00:01.00]X",
        "",
    ];
    for text in lrc {
        let r = lyrics::parse_lrc(text);
        out.push(case(
            "parse_lrc",
            json!([text]),
            json!({ "lines": lines_json(&r.lines), "offset": r.offset }),
        ));
    }
    // The line cap.
    let many: String = (0..650)
        .map(|i| format!("[{:02}:{:02}.00]l{i}\n", i / 60, i % 60))
        .collect();
    let r = lyrics::parse_lrc(&many);
    out.push(case(
        "parse_lrc",
        json!([many]),
        json!({ "lines": lines_json(&r.lines), "offset": r.offset }),
    ));
    let long = format!("[00:01.00]{}", "á".repeat(260));
    let r = lyrics::parse_lrc(&long);
    out.push(case(
        "parse_lrc",
        json!([long]),
        json!({ "lines": lines_json(&r.lines), "offset": r.offset }),
    ));

    for (raw, max) in [
        ("  hola   mundo  ", 200),
        ("abcdef", 3),
        ("abcd😀", 5),
        ("abcd😀", 6),
        ("a\u{0}b\u{7f}c\u{85}d", 200),
        ("\u{1b}[1;32mverde\u{1b}[m", 200),
        ("\u{1b}]8;;http://x\u{7}enlace\u{1b}]8;;\u{7}", 200),
        ("\u{1b}Mx", 200),
        ("ab  ", 3),
        ("ab cd", 3),
        ("", 200),
    ] {
        out.push(case(
            "clean_line",
            json!([raw, max]),
            json!(lyrics::clean_line(raw, max)),
        ));
    }

    let song = lines_of(&json!([
        { "t": 1000, "text": "Uno" },
        { "t": 5000, "text": "" },
        { "t": 9000, "text": "Tres" },
        { "t": 9000, "text": "Tres bis" },
        { "t": 20000, "text": "Fin" },
    ]));
    let song_json = lines_json(&song);
    for pos in [
        0, 999, 1000, 1001, 4999, 5000, 8999, 9000, 19999, 20000, 99999,
    ] {
        let at = lyrics::line_at(&song, pos).map_or(-1, |i| i as i64);
        out.push(case("line_at", json!([song_json, pos]), json!(at)));
        let w = lyrics::window(&song, pos);
        out.push(case(
            "window",
            json!([song_json, pos]),
            json!({
                "index": w.index.map_or(-1, |i| i as i64),
                "current": w.current,
                "next": w.next,
                "nextAt": w.next_at.map_or(-1, |t| t as i64),
            }),
        ));
    }
    out.push(case("line_at", json!([[], 500]), json!(-1)));
    out.push(case(
        "window",
        json!([[], 500]),
        json!({ "index": -1, "current": "", "next": "", "nextAt": -1 }),
    ));

    for raw in [
        "Canción - 2011 Remaster",
        "Canción - Remastered 2009",
        "Canción - Remastered Version",
        "Canción - Digital Remaster",
        "Canción (Remastered 2011)",
        "Canción [2011 Remaster]",
        "Canción (feat. Otra Persona)",
        "Canción [ft Otra] (Live)",
        "Canción feat. Alguien Más",
        "Canción (Official Music Video)",
        "Canción (Lyric Video)",
        "Canción (Visualizer)",
        "Canción (HD)",
        "Canción (Deluxe Edition)",
        "Canción (Explicit)",
        "Canción - Bonus Track",
        "Canción - Single Version",
        "Canción (Live)",
        "Canción (Acoustic)",
        "Canción (Radio Edit)",
        "(feat. Solo)",
        "  Canción   (with Otro)   ",
        "CANCIÓN (REMASTERED)",
        "Sin cambios",
        "",
    ] {
        out.push(case(
            "normalize_title",
            json!([raw]),
            json!(lyrics::normalize_title(raw)),
        ));
    }

    for raw in [
        "Alguien",
        "Alguien, Otra Persona",
        "Alguien & Otra",
        "Alguien; Otra",
        "Alguien x Otra",
        "Alguien feat. Otra",
        "Alguien ft Otra",
        "Alguien featuring Otra",
        "Alguien with Otra",
        "Max Power",
        "Dúo x",
        ", Raro",
        "  Alguien   ",
        "",
    ] {
        out.push(case(
            "normalize_artist",
            json!([raw]),
            json!(lyrics::normalize_artist(raw)),
        ));
    }

    for (artist, title, album, dur) in [
        ("Alguien", "Canción", "Disco", 211.0),
        ("  ALGUIEN ", "CANCIÓN", "DISCO", 211.4),
        ("Alguien", "Canción", "Disco", 211.5),
        ("Alguien", "Canción", "", 0.0),
        ("Alguien", "Canción", "Disco", -0.5),
        ("Alguien", "Canción", "Disco", 0.5),
        ("Ñandú", "Ça va", "Ünï", 99.49),
        ("ΟΔΥΣΣΕΥΣ", "Σ", "", 10.0),
        ("A\u{1b}[0mB", "x\ty", "z", 1.0),
    ] {
        out.push(case(
            "cache_key",
            json!([artist, title, album, dur]),
            json!(cache_key(artist, title, album, dur)),
        ));
    }

    // lrclib records, as the plugin's tests make them.
    let rec = |extra: Value| -> Value {
        let mut m = Map::new();
        m.insert("id".into(), json!(42));
        m.insert("duration".into(), json!(200));
        if let Value::Object(e) = extra {
            for (k, v) in e {
                m.insert(k, v);
            }
        }
        Value::Object(m)
    };
    let synced = "[00:01.00]Uno\n[00:02.00]Dos";
    let records = [
        (rec(json!({ "syncedLyrics": synced })), 200.0),
        (rec(json!({ "syncedLyrics": synced })), 203.0),
        (rec(json!({ "syncedLyrics": synced })), 204.0),
        (rec(json!({ "syncedLyrics": synced })), 197.0),
        (rec(json!({ "syncedLyrics": synced })), 0.0),
        (
            rec(json!({ "syncedLyrics": "[offset:100]\n[00:01.00]Uno" })),
            200.0,
        ),
        (
            rec(json!({ "syncedLyrics": "sin sellos", "plainLyrics": "Texto" })),
            200.0,
        ),
        (
            rec(json!({ "syncedLyrics": "", "plainLyrics": "Texto plano" })),
            200.0,
        ),
        (rec(json!({ "plainLyrics": "   " })), 200.0),
        (rec(json!({ "plainLyrics": "Texto plano" })), 200.0),
        (rec(json!({ "instrumental": true })), 200.0),
        (
            rec(json!({ "instrumental": true, "syncedLyrics": synced })),
            200.0,
        ),
        (rec(json!({ "instrumental": false })), 200.0),
        (json!({ "syncedLyrics": synced }), 200.0),
        (
            json!({ "id": 7, "duration": 0, "syncedLyrics": synced }),
            200.0,
        ),
        (
            json!({ "id": 7, "duration": "200", "syncedLyrics": synced }),
            100.0,
        ),
        (json!({ "id": "7", "syncedLyrics": synced }), 100.0),
        (json!([1, 2]), 200.0),
        (json!(null), 200.0),
        (json!(12), 200.0),
        (rec(json!({})), 200.0),
    ];
    for (r, dur) in &records {
        let c = case(
            "validate",
            json!([r, dur]),
            found_json(&lyrics::validate(r, *dur)),
        );
        out.push(if r["duration"].is_string() {
            skip_js(c, "the plugin reads a duration sent as text (Number()) and checks it; the core only reads real numbers")
        } else {
            c
        });
    }
    // JSON text (what the fetch script stores).
    let text = rec(json!({ "syncedLyrics": synced })).to_string();
    out.push(case(
        "validate",
        json!([text, 200]),
        found_json(&lyrics::validate_text(&text, 200.0)),
    ));
    for broken in ["{no json", "", "[]", "null"] {
        out.push(case(
            "validate",
            json!([broken, 200]),
            found_json(&lyrics::validate_text(broken, 200.0)),
        ));
    }

    let list = json!([
        { "id": 1, "duration": 190, "syncedLyrics": synced },
        { "id": 2, "duration": 199, "syncedLyrics": synced },
        { "id": 3, "duration": 201, "syncedLyrics": synced },
        { "id": 4, "duration": 200, "plainLyrics": "solo plana" },
        { "id": 5, "duration": 200, "syncedLyrics": "" },
        "texto",
        null,
    ]);
    for dur in [200.0, 199.2, 150.0, 192.0, 0.0] {
        let id = lyrics::pick_search(&list, dur).map_or(Value::Null, |r| r["id"].clone());
        out.push(case("pick_search_id", json!([list, dur]), id));
    }
    out.push(case("pick_search_id", json!([{}, 200]), Value::Null));
    out.push(case("pick_search_id", json!([[], 200]), Value::Null));
    // Same gap on both sides of the duration: the first one wins.
    let tie = json!([
        { "id": 8, "duration": 198, "syncedLyrics": synced },
        { "id": 9, "duration": 202, "syncedLyrics": synced },
    ]);
    let id = lyrics::pick_search(&tie, 200.0).map_or(Value::Null, |r| r["id"].clone());
    out.push(case("pick_search_id", json!([tie, 200]), id));

    out
}

// ------------------------------------------------------------------- focus

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Focus => "focus",
        Phase::Break => "break",
        Phase::LongBreak => "longBreak",
    }
}

fn lengths_of(v: &Value) -> Lengths {
    let n = |k: &str| v[k].as_u64().unwrap() as u32;
    Lengths {
        focus: n("focus"),
        short: n("break"),
        long: n("longBreak"),
        every: n("every"),
    }
}

/// Runs the ops on a timer and records the state after each one.
fn run_focus(lengths: &Value, ops: &Value) -> Value {
    let mut l = lengths_of(lengths);
    let mut t = Timer::new(&l);
    let mut out = Vec::new();
    for op in ops.as_array().unwrap() {
        let now = op["now"].as_u64().unwrap();
        match op["op"].as_str().unwrap() {
            "toggle" => t.toggle(now),
            "skip" => {
                t.skip(now, &l);
            }
            "finish" => {
                t.finish(now, &l);
            }
            "reset" => t.reset(&l),
            "lengths" => {
                let new = lengths_of(&op["lengths"]);
                t.set_lengths(&l, &new);
                l = new;
            }
            other => panic!("unknown op {other}"),
        }
        out.push(json!({
            "phase": phase_name(t.phase),
            "rounds": t.rounds,
            "running": t.running(),
            "left": t.left_ms(now),
            "active": t.active(&l),
        }));
    }
    Value::Array(out)
}

fn focus_cases() -> Cases {
    const MIN: u64 = 60_000;
    let std = json!({ "focus": 25, "break": 5, "longBreak": 15, "every": 4 });
    let short = json!({ "focus": 5, "break": 1, "longBreak": 5, "every": 2 });
    let op = |o: &str, now: u64| json!({ "op": o, "now": now });
    let resize = |now: u64, l: Value| json!({ "op": "lengths", "now": now, "lengths": l });

    let mut scenarios: Vec<(Value, Value)> = Vec::new();
    // Starts, pauses and resumes without drift.
    scenarios.push((
        std.clone(),
        json!([
            op("toggle", 0),
            op("toggle", 10 * MIN),
            op("toggle", 20 * MIN),
            op("toggle", 20 * MIN + 30_000),
            op("finish", 20 * MIN + 30_000),
        ]),
    ));
    // A whole cycle with a long break after every second focus.
    let mut cycle = Vec::new();
    let mut now = 0;
    for _ in 0..3 {
        cycle.push(op("toggle", now));
        now += 5 * MIN;
        cycle.push(op("finish", now - 1));
        cycle.push(op("finish", now));
        let brk = if cycle.len() > 6 { 5 } else { 1 };
        now += brk * MIN;
        cycle.push(op("finish", now));
        now += 1000;
    }
    scenarios.push((short.clone(), Value::Array(cycle)));
    // Four rounds with the usual lengths.
    let mut four = Vec::new();
    let mut now = 0;
    for _ in 0..4 {
        four.push(op("toggle", now));
        now += 25 * MIN;
        four.push(op("finish", now));
        now += 15 * MIN;
        four.push(op("finish", now));
    }
    scenarios.push((std.clone(), Value::Array(four)));
    // Skips: running, paused, in a break, in a long break.
    scenarios.push((
        short.clone(),
        json!([
            op("toggle", 0),
            op("skip", 1000),
            op("skip", 2000),
            op("skip", 3000),
            op("skip", 4000),
            op("skip", 5000),
            op("skip", 6000),
        ]),
    ));
    scenarios.push((
        std.clone(),
        json!([
            op("skip", 0),
            op("skip", 1000),
            op("toggle", 2000),
            op("skip", 3000)
        ]),
    ));
    // The computer slept past the end: a full break, not the missed rounds.
    scenarios.push((
        std.clone(),
        json!([
            op("toggle", 0),
            op("finish", 25 * MIN + 3 * 3600 * 1000),
            op("finish", 25 * MIN + 3 * 3600 * 1000 + 10 * MIN)
        ]),
    ));
    scenarios.push((
        std.clone(),
        json!([
            op("toggle", 0),
            op("finish", 10 * MIN),
            op("finish", 25 * MIN - 1)
        ]),
    ));
    // Reset in the middle.
    scenarios.push((
        std.clone(),
        json!([
            op("toggle", 0),
            op("toggle", 3 * MIN),
            op("reset", 4 * MIN),
            op("toggle", 5 * MIN)
        ]),
    ));
    // New lengths: a fresh phase takes them, one under way keeps its own.
    let long_focus = json!({ "focus": 50, "break": 10, "longBreak": 30, "every": 3 });
    scenarios.push((
        std.clone(),
        json!([
            resize(0, long_focus.clone()),
            op("toggle", 1000),
            resize(2000, std.clone()),
            op("toggle", 3000),
            resize(4000, std.clone()),
        ]),
    ));
    scenarios.push((
        std.clone(),
        json!([
            op("toggle", 0),
            op("toggle", MIN),
            resize(MIN, long_focus.clone()),
            op("reset", 2 * MIN),
            resize(2 * MIN, short.clone()),
            op("skip", 3 * MIN),
            resize(3 * MIN, std.clone()),
        ]),
    ));
    // Pausing exactly at, and after, the deadline.
    scenarios.push((
        std,
        json!([
            op("toggle", 0),
            op("toggle", 25 * MIN),
            op("toggle", 26 * MIN),
            op("finish", 26 * MIN)
        ]),
    ));

    scenarios
        .into_iter()
        .map(|(l, ops)| {
            let expect = run_focus(&l, &ops);
            case("run", json!([l, ops]), expect)
        })
        .collect()
}

// ------------------------------------------------------------------- media

fn media_cases() -> Cases {
    let mut out = Cases::new();

    // JS `fitWidth(setting, screen, 1, 0)`. It is the same rule while the
    // 60 % share of the screen is not under the smallest width.
    for (setting, screen) in [
        (960.0, 1920.0),
        (960.0, 1280.0),
        (960.0, 1024.0),
        (960.0, 1000.0),
        (960.0, 800.0),
        (560.0, 1920.0),
        (560.0, 3840.0),
        (560.0, 1366.0),
        (480.0, 1920.0),
        (500.0, 900.0),
        (560.0, 400.0),
        (560.0, 0.0),
        (960.0, 1.0),
    ] {
        out.push(case(
            "fit_width",
            json!([num(setting), num(screen)]),
            num(f64::from(media::fit_open_width(
                setting as f32,
                screen as f32,
            ))),
        ));
    }
    // The floor differs: the core never goes below 480 on a small screen,
    // the plugin keeps the 420 it always had.
    for (setting, screen) in [
        (560.0, 700.0),
        (560.0, 750.0),
        (480.0, 600.0),
        (960.0, 760.0),
        (560.0, 480.0),
    ] {
        out.push(skip_js(
            case(
                "fit_width",
                json!([num(setting), num(screen)]),
                num(f64::from(media::fit_open_width(setting as f32, screen as f32))),
            ),
            "the smallest open width is 480 in the core and 420 in the plugin (kept for settings saved before the desktop apps)",
        ));
    }

    for mode in ["title-artist", "artist-title", "title", "artist"] {
        let m = SongText::parse(mode).unwrap();
        for (title, artist) in [
            ("Canción", "Alguien"),
            ("Canción", ""),
            ("", "Alguien"),
            ("", ""),
            ("A", "A"),
            ("Con  espacios", "Y · punto"),
        ] {
            out.push(case(
                "song_line",
                json!([title, artist, mode]),
                json!(m.line(title, artist)),
            ));
        }
    }

    for d in [
        -10.0f32, -3.0, -2.0, -1.5, -1.0, -0.5, -0.25, 0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0,
        10.0,
    ] {
        out.push(case(
            "lyric_alpha",
            json!([num(f64::from(d))]),
            num(f64::from(media::lyrics_alpha(d))),
        ));
    }
    out
}

// --------------------------------------------------------------------- ics

/// Seconds since 1970 of a day, at midnight UTC.
fn epoch(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400
}

fn calendar(events: &[&str]) -> String {
    let mut s = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\n");
    for e in events {
        s.push_str(e);
        s.push_str("\r\n");
    }
    s.push_str("END:VCALENDAR\r\n");
    s
}

fn vevent(lines: &[&str]) -> String {
    let mut s = String::from("BEGIN:VEVENT");
    for l in lines {
        write!(s, "\r\n{l}").unwrap();
    }
    s.push_str("\r\nEND:VEVENT");
    s
}

const MADRID: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Madrid\r\n\
BEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nDTSTART:19700329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\n\
BEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nTZNAME:CET\r\nDTSTART:19701025T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\n\
END:VTIMEZONE";

fn ics_events(text: &str, from: i64, to: i64) -> Value {
    let list = ics::events(text, from, to, &|t| t);
    Value::Array(
        list.iter()
            .map(|e| {
                let time = e
                    .time
                    .map_or(String::new(), |m| format!("{:02}:{:02}", m / 60, m % 60));
                json!({
                    "date": format!("{:04}-{:02}-{:02}", e.date.y, e.date.m, e.date.d),
                    "time": time,
                    "title": e.title,
                })
            })
            .collect(),
    )
}

fn ics_cases() -> Cases {
    let mut out = Cases::new();
    let window = (epoch(2026, 11, 1), epoch(2026, 12, 31));
    let summer = (epoch(2026, 6, 15), epoch(2026, 8, 1));
    let mut add = |text: String, w: (i64, i64), skip: Option<&str>| {
        let c = case(
            "events",
            json!([text, w.0, w.1]),
            ics_events(&text, w.0, w.1),
        );
        out.push(match skip {
            Some(r) => skip_js(c, r),
            None => c,
        });
    };

    // Single events: UTC, floating, folded and escaped text.
    add(
        calendar(&[
            &vevent(&[
                "UID:b",
                "DTSTART:20261112T143000Z",
                "SUMMARY:Reunión con\\, coma y una",
                " línea plegada",
            ]),
            &vevent(&["UID:c", "DTSTART:20261113T080000", "SUMMARY:Flotante"]),
            &vevent(&[
                "UID:d",
                "DTSTART:20261114T080000Z",
                "SUMMARY:Con\\nsalto\\; y \\\\ barra",
            ]),
        ]),
        window,
        None,
    );
    // Only LF line ends, and a tab used to fold.
    add(
        "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:lf\nDTSTART:20261112T100000Z\nSUMMARY:Solo LF\n\tcontinuada\nEND:VEVENT\nEND:VCALENDAR\n".to_string(),
        window,
        None,
    );
    // Cancelled, no summary, no start, outside the window.
    add(
        calendar(&[
            &vevent(&[
                "UID:x",
                "DTSTART:20261112T100000Z",
                "SUMMARY:Cancelado",
                "STATUS:CANCELLED",
            ]),
            &vevent(&["UID:y", "DTSTART:20261112T100000Z"]),
            &vevent(&["UID:z", "SUMMARY:Sin inicio"]),
            &vevent(&["UID:w", "DTSTART:20300101T100000Z", "SUMMARY:Lejos"]),
            &vevent(&["UID:v", "DTSTART:20261112T100000Z", "SUMMARY:Ese sí"]),
        ]),
        window,
        None,
    );
    // Time zones: a TZID placed with the calendar's own VTIMEZONE.
    add(
        calendar(&[
            MADRID,
            &vevent(&[
                "UID:t1",
                "DTSTART;TZID=Europe/Madrid:20261112T090000",
                "SUMMARY:Invierno Madrid",
            ]),
        ]),
        window,
        None,
    );
    add(
        calendar(&[
            MADRID,
            &vevent(&[
                "UID:t2",
                "DTSTART;TZID=Europe/Madrid:20260715T090000",
                "SUMMARY:Verano Madrid",
            ]),
            &vevent(&[
                "UID:t3",
                "DTSTART;TZID=Europe/Madrid:20260628T093000",
                "SUMMARY:Domingo de verano",
            ]),
        ]),
        summer,
        None,
    );
    // Around the change to winter time (last Sunday of October 2026: the 25th).
    add(
        calendar(&[
            MADRID,
            &vevent(&[
                "UID:t4",
                "DTSTART;TZID=Europe/Madrid:20261025T013000",
                "SUMMARY:Antes del cambio",
            ]),
            &vevent(&[
                "UID:t5",
                "DTSTART;TZID=Europe/Madrid:20261025T040000",
                "SUMMARY:Después del cambio",
            ]),
            &vevent(&[
                "UID:t6",
                "DTSTART;TZID=Europe/Madrid:20261024T230000",
                "SUMMARY:La víspera",
            ]),
        ]),
        (epoch(2026, 10, 20), epoch(2026, 10, 30)),
        None,
    );
    // Repeats: daily with a count, an interval, an end date.
    add(
        calendar(&[
            &vevent(&[
                "UID:r1",
                "DTSTART:20261110T080000Z",
                "SUMMARY:Diario",
                "RRULE:FREQ=DAILY;COUNT=4",
            ]),
            &vevent(&[
                "UID:r2",
                "DTSTART:20261110T090000Z",
                "SUMMARY:Cada tres días",
                "RRULE:FREQ=DAILY;INTERVAL=3;COUNT=5",
            ]),
            &vevent(&[
                "UID:r3",
                "DTSTART:20261110T100000Z",
                "SUMMARY:Hasta el 14",
                "RRULE:FREQ=DAILY;UNTIL=20261114T235959Z",
            ]),
        ]),
        window,
        None,
    );
    // Weekly, by day, with an interval.
    add(
        calendar(&[
            &vevent(&[
                "UID:w1",
                "DTSTART:20261102T120000Z",
                "SUMMARY:Lunes",
                "RRULE:FREQ=WEEKLY;COUNT=4",
            ]),
            &vevent(&[
                "UID:w2",
                "DTSTART:20261102T130000Z",
                "SUMMARY:Lunes y jueves",
                "RRULE:FREQ=WEEKLY;BYDAY=MO,TH;COUNT=6",
            ]),
            &vevent(&[
                "UID:w3",
                "DTSTART:20261103T140000Z",
                "SUMMARY:Cada dos semanas",
                "RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,FR;COUNT=6",
            ]),
        ]),
        window,
        None,
    );
    // Monthly: by day of the month, by weekday and ordinal, the last day.
    add(
        calendar(&[
            &vevent(&[
                "UID:m1",
                "DTSTART:20260115T100000Z",
                "SUMMARY:El 15",
                "RRULE:FREQ=MONTHLY;BYMONTHDAY=15",
            ]),
            &vevent(&[
                "UID:m2",
                "DTSTART:20260105T100000Z",
                "SUMMARY:Segundo lunes",
                "RRULE:FREQ=MONTHLY;BYDAY=2MO",
            ]),
            &vevent(&[
                "UID:m3",
                "DTSTART:20260131T100000Z",
                "SUMMARY:Fin de mes",
                "RRULE:FREQ=MONTHLY;BYMONTHDAY=-1",
            ]),
            &vevent(&[
                "UID:m4",
                "DTSTART:20260131T110000Z",
                "SUMMARY:El 31",
                "RRULE:FREQ=MONTHLY",
            ]),
            &vevent(&[
                "UID:m5",
                "DTSTART:20260101T120000Z",
                "SUMMARY:Último viernes",
                "RRULE:FREQ=MONTHLY;BYDAY=-1FR",
            ]),
        ]),
        (epoch(2026, 11, 3), epoch(2027, 3, 1)),
        None,
    );
    // The window's start: the core keeps what is inside it, the plugin a
    // day before it too (a multi-day event that began earlier still shows).
    add(
        calendar(&[
            &vevent(&["UID:p1", "DTSTART:20261031T200000Z", "SUMMARY:La víspera"]),
            &vevent(&[
                "UID:p2",
                "DTSTART:20261029T200000Z",
                "SUMMARY:Antes de ayer",
            ]),
            &vevent(&[
                "UID:p3",
                "DTSTART:20261101T000000Z",
                "SUMMARY:Justo al inicio",
            ]),
        ]),
        window,
        Some(
            "the plugin also keeps events up to a day before the window's start; the core only those inside",
        ),
    );
    // Yearly.
    add(
        calendar(&[
            &vevent(&[
                "UID:y1",
                "DTSTART:20200612T100000Z",
                "SUMMARY:Aniversario",
                "RRULE:FREQ=YEARLY",
            ]),
            &vevent(&[
                "UID:y2",
                "DTSTART:20200229T100000Z",
                "SUMMARY:Bisiesto",
                "RRULE:FREQ=YEARLY",
            ]),
            &vevent(&[
                "UID:y3",
                "DTSTART:20200301T100000Z",
                "SUMMARY:Marzo y mayo",
                "RRULE:FREQ=YEARLY;BYMONTH=3,5",
            ]),
        ]),
        (epoch(2026, 1, 1), epoch(2029, 1, 1)),
        None,
    );
    // Exceptions: EXDATE and a moved occurrence (RECURRENCE-ID).
    add(
        calendar(&[
            &vevent(&[
                "UID:e1",
                "DTSTART:20261109T100000Z",
                "SUMMARY:Con excepción",
                "RRULE:FREQ=DAILY;COUNT=5",
                "EXDATE:20261111T100000Z",
            ]),
            &vevent(&[
                "UID:e2",
                "DTSTART:20261109T110000Z",
                "SUMMARY:Movida",
                "RRULE:FREQ=DAILY;COUNT=3",
            ]),
            &vevent(&[
                "UID:e2",
                "RECURRENCE-ID:20261110T110000Z",
                "DTSTART:20261110T150000Z",
                "SUMMARY:Movida",
            ]),
            &vevent(&[
                "UID:e3",
                "DTSTART:20261109T120000Z",
                "SUMMARY:Cancelada una",
                "RRULE:FREQ=DAILY;COUNT=3",
            ]),
            &vevent(&[
                "UID:e3",
                "RECURRENCE-ID:20261110T120000Z",
                "DTSTART:20261110T120000Z",
                "SUMMARY:Cancelada una",
                "STATUS:CANCELLED",
            ]),
        ]),
        window,
        None,
    );
    // A repeat is cut by the window, and one that starts before it shows.
    add(
        calendar(&[&vevent(&[
            "UID:c1",
            "DTSTART:20260101T080000Z",
            "SUMMARY:Desde antes",
            "RRULE:FREQ=WEEKLY",
        ])]),
        (epoch(2026, 11, 9), epoch(2026, 11, 23)),
        None,
    );
    // In the same time: a UTC event and a local one (read as local = UTC here).
    add(
        calendar(&[
            &vevent(&["UID:s1", "DTSTART:20261112T230000Z", "SUMMARY:Tarde"]),
            &vevent(&["UID:s2", "DTSTART:20261112T000000Z", "SUMMARY:Medianoche"]),
            &vevent(&[
                "UID:s3",
                "DTSTART:20261112T235900",
                "SUMMARY:Casi medianoche",
            ]),
        ]),
        window,
        None,
    );
    // All-day events: the agenda of the plugin shows them, the core's
    // leaves them out (they have no moment to remind).
    add(
        calendar(&[
            &vevent(&[
                "UID:a",
                "DTSTART;VALUE=DATE:20261110",
                "DTEND;VALUE=DATE:20261111",
                "SUMMARY:Día entero",
            ]),
            &vevent(&["UID:b", "DTSTART:20261112T143000Z", "SUMMARY:Con hora"]),
        ]),
        window,
        Some("the plugin lists all-day events (as a day without a time); the core leaves them out"),
    );
    // Not a calendar.
    add(String::from("esto no es un calendario"), window, None);
    add(String::new(), window, None);

    out
}

// ---------------------------------------------------------------- settings

/// The keys both sides read with the same name, as the plugin names them.
fn shared_settings(raw: &str) -> Value {
    let s = Settings::parse(raw).expect("an object");
    let language = match s.language {
        island_core::settings::Language::System => "",
        island_core::settings::Language::En => "en",
        island_core::settings::Language::Es => "es",
        island_core::settings::Language::Pt => "pt",
    };
    let page = match s.default_page {
        island_core::pages::Page::Home => "home",
        island_core::pages::Page::Focus => "focus",
    };
    let align = match s.text_align {
        media::TextAlign::Left => "left",
        media::TextAlign::Center => "center",
    };
    let song = ["title-artist", "artist-title", "title", "artist"]
        .into_iter()
        .find(|m| SongText::parse(m) == Some(s.song_text))
        .unwrap();
    json!({
        "language": language,
        "defaultPage": page,
        "lyrics": s.lyrics,
        "openLyrics": s.open_lyrics,
        "textAlign": align,
        "songText": song,
        "focusMinutes": s.focus.focus,
        "breakMinutes": s.focus.short,
        "longBreakMinutes": s.focus.long,
        "longBreakEvery": s.focus.every,
        "reminderLead": s.reminder_lead,
    })
}

fn settings_cases() -> Cases {
    let mut out = Cases::new();
    let mut add = |raw: Value, skip: Option<&str>| {
        let text = raw.to_string();
        let c = case("normalize_shared", json!([raw]), shared_settings(&text));
        out.push(match skip {
            Some(r) => skip_js(c, r),
            None => c,
        });
    };
    add(json!({}), None);
    add(
        json!({
            "language": "es", "defaultPage": "focus", "lyrics": true, "openLyrics": false,
            "textAlign": "left", "songText": "artist-title", "focusMinutes": 50,
            "breakMinutes": 10, "longBreakMinutes": 30, "longBreakEvery": 3, "reminderLead": 15,
        }),
        None,
    );
    add(json!({ "language": "en" }), None);
    add(
        json!({ "language": "fr", "defaultPage": "nope", "textAlign": "right", "songText": "x" }),
        None,
    );
    add(
        json!({ "lyrics": "yes", "openLyrics": 0, "focusMinutes": [], "breakMinutes": null }),
        None,
    );
    add(
        json!({ "focusMinutes": 7, "breakMinutes": 1, "longBreakMinutes": 5, "longBreakEvery": 2 }),
        None,
    );
    add(
        json!({ "focusMinutes": 120, "breakMinutes": 30, "longBreakMinutes": 60, "longBreakEvery": 8, "reminderLead": 120 }),
        None,
    );
    add(json!({ "reminderLead": 0 }), None);
    add(json!({ "ghost": 1, "lyrics": true }), None);

    add(json!({ "language": "pt" }), None);
    add(json!({ "language": "auto" }), None);

    // Where the two read the same value differently.
    add(
        json!({ "defaultPage": "calendar" }),
        Some("the plugin has the calendar and world pages; the core only home and focus"),
    );
    add(
        json!({ "defaultPage": "world" }),
        Some("the plugin has the calendar and world pages; the core only home and focus"),
    );
    add(
        json!({ "focusMinutes": 500, "breakMinutes": 0, "longBreakMinutes": 1, "longBreakEvery": 99, "reminderLead": 999 }),
        Some(
            "a number out of range is clamped by the plugin and falls back to its default in the core",
        ),
    );
    add(
        json!({ "focusMinutes": -5 }),
        Some(
            "a number out of range is clamped by the plugin and falls back to its default in the core",
        ),
    );
    add(
        json!({ "lyrics": "true", "openLyrics": "false" }),
        Some("the plugin reads \"true\"/\"false\" text as booleans; the core only real booleans"),
    );
    add(
        json!({ "focusMinutes": "40", "breakMinutes": 3.7 }),
        Some(
            "the plugin reads numbers in text and cuts decimals (parseInt); the core only whole numbers",
        ),
    );
    out
}

// ------------------------------------------------------------------ files

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/vectors")
}

fn render(cases: &Cases) -> String {
    let mut s = serde_json::to_string_pretty(&Value::Array(cases.clone())).unwrap();
    s.push('\n');
    s
}

fn check(name: &str, cases: Cases) {
    assert!(!cases.is_empty());
    let path = vectors_dir().join(format!("{name}.json"));
    let want = render(&cases);
    if std::env::var_os("ISLAND_BLESS").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // LF on every platform.
        std::fs::write(&path, want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with ISLAND_BLESS=1", path.display()))
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "{} differs from what the core produces; if the change is meant, rerun with ISLAND_BLESS=1 and review the diff",
        path.display()
    );
}

#[test]
fn lyrics_vectors() {
    check("lyrics", lyrics_cases());
}

#[test]
fn focus_vectors() {
    check("focus", focus_cases());
}

#[test]
fn media_vectors() {
    check("media", media_cases());
}

#[test]
fn ics_vectors() {
    check("ics", ics_cases());
}

#[test]
fn settings_vectors() {
    check("settings", settings_cases());
}
