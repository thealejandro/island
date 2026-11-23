// Golden vectors shared with the Rust core (desktop/crates/island-core).
// The core writes test/vectors/*.json (ISLAND_BLESS=1 cargo test -p
// island-core --test vectors); here the same cases go through the plugin's
// models. A case's "fn" is a neutral name, mapped below to the JS function.
// A case with "skip": { "js": reason } is a known, written-down difference
// between the two sides: it is not run, and the reason is printed.
process.env.TZ = "UTC"

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync, readdirSync } from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { load, plain } from "./load.mjs"

const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), "vectors")
const L = load("LyricsModel.js")
const F = load("FocusModel.js")
const M = load("MediaModel.js")
const I = load("IcsModel.js")
const S = load("IslandSettings.js")

// Rust rounds f32 results to 6 decimals; numbers are compared with this slack.
const EPS = 1e-6

const lrc = r => ({ lines: r.lines, offset: r.offset })
const found = r => ({ kind: r.kind, lines: r.lines, offset: r.offset, id: r.id })

const impl = {
  lyrics: {
    parse_time: t => L.parseTime(t),
    parse_lrc: text => lrc(L.parseLrc(text)),
    clean_line: (raw, max) => L.cleanLine(raw, max),
    line_at: (lines, pos) => L.lineAt(lines, pos),
    window: (lines, pos) => L.window(lines, pos),
    normalize_title: raw => L.normalizeTitle(raw),
    normalize_artist: raw => L.normalizeArtist(raw),
    cache_key: (a, t, al, d) => L.cacheKey(a, t, al, d),
    validate: (rec, d) => found(L.validate(rec, d)),
    pick_search_id: (list, d) => { const r = L.pickSearch(list, d); return r ? r.id : null }
  },
  focus: {
    run: (lengths, ops) => {
      let l = lengths
      let s = F.create(l)
      return ops.map(op => {
        if (op.op === "toggle") s = F.toggle(s, op.now)
        else if (op.op === "skip") s = F.advance(s, op.now, l)
        else if (op.op === "finish") s = F.finish(s, op.now, l)
        else if (op.op === "reset") s = F.create(l)
        else if (op.op === "lengths") { l = op.lengths; s = F.resize(s, l) }
        else throw new Error("unknown op " + op.op)
        return { phase: s.phase, rounds: s.rounds, running: s.running,
          left: F.remaining(s, op.now), active: F.active(s) }
      })
    }
  },
  media: {
    fit_width: (setting, screen) => M.fitWidth(setting, screen, 1, 0),
    song_line: (title, artist, mode) => M.songLine(title, artist, mode),
    lyric_alpha: d => M.lyricAlpha(d)
  },
  ics: {
    // Timed events only, as date, time and title; the window is in seconds.
    events: (text, from, to) => I.parse(text, { from: new Date(from * 1000), to: new Date(to * 1000) })
      .map(e => ({ date: e.date, time: e.time, title: e.title }))
  },
  settings: {
    normalize_shared: raw => {
      const n = S.normalize(raw)
      const out = {}
      for (const k of ["language", "defaultPage", "lyrics", "openLyrics", "textAlign", "songText",
        "focusMinutes", "breakMinutes", "longBreakMinutes", "longBreakEvery", "reminderLead"]) out[k] = n[k]
      return out
    }
  }
}

// deepStrictEqual, except that numbers differ by up to EPS.
function same(actual, expected, where) {
  if (typeof expected === "number" && typeof actual === "number") {
    assert.ok(Math.abs(actual - expected) <= EPS, `${where}: ${actual} != ${expected}`)
  } else if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${where}: not an array`)
    assert.equal(actual.length, expected.length, `${where}: length`)
    expected.forEach((e, i) => same(actual[i], e, `${where}[${i}]`))
  } else if (expected && typeof expected === "object") {
    assert.deepEqual(Object.keys(actual || {}).sort(), Object.keys(expected).sort(), `${where}: keys`)
    for (const k of Object.keys(expected)) same(actual[k], expected[k], `${where}.${k}`)
  } else {
    assert.deepStrictEqual(actual, expected, where)
  }
}

const files = readdirSync(dir).filter(f => f.endsWith(".json")).sort()

test("every vectors file has a JS mapping, and the other way round", () => {
  assert.deepEqual(files.map(f => f.replace(/\.json$/, "")), Object.keys(impl).sort())
})

for (const file of files) {
  const name = file.replace(/\.json$/, "")
  const cases = JSON.parse(readFileSync(path.join(dir, file), "utf8"))
  test(`vectors: ${name} (${cases.length} cases)`, t => {
    assert.ok(cases.length > 0)
    let skipped = 0
    const failures = []
    cases.forEach((c, i) => {
      const fn = impl[name][c.fn]
      assert.ok(fn, `${name}.json #${i}: no JS function for "${c.fn}"`)
      if (c.skip && c.skip.js) {
        assert.ok(String(c.skip.js).length > 10, `${name}.json #${i}: a skip needs a reason`)
        skipped++
        return
      }
      try {
        const got = plain(fn(...JSON.parse(JSON.stringify(c.args))))
        same(got, c.expect, `${name}.json #${i} ${c.fn}(${JSON.stringify(c.args).slice(0, 120)})`)
      } catch (e) {
        failures.push(e.message.split("\n")[0])
      }
    })
    assert.equal(failures.length, 0, `${failures.length} case(s) differ:\n` + failures.join("\n"))
    if (skipped) t.diagnostic(`${skipped} case(s) skipped as known differences`)
  })
}
