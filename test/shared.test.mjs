import { test } from "node:test"
import assert from "node:assert/strict"
import {
  JS_OUT, RUST_OUT, LANGS, forDesktop, forPlugin, placeholders, readSource, render, upToDate, validate,
} from "../scripts/gen-shared.mjs"
import { load } from "./load.mjs"

const entries = readSource()

test("the generated files are what shared/texts.json says (node scripts/gen-shared.mjs)", () => {
  const { js, rust } = render(entries)
  assert.ok(upToDate(JS_OUT, js), "IslandStrings.js is out of date")
  assert.ok(upToDate(RUST_OUT, rust), "texts_gen.rs is out of date")
})

test("every entry has the three languages and the same placeholders in each", () => {
  for (const e of entries) {
    for (const l of LANGS) assert.ok(e[l].trim(), `${e.id} ${l}`)
    const [first, ...rest] = LANGS.map((l) => placeholders(e[l]).sort().join())
    for (const p of rest) assert.equal(p, first, e.id)
  }
})

test("the plugin's STRINGS carry each plugin entry, with %1 for {0}", () => {
  const { STRINGS } = load("IslandStrings.js")
  const list = entries.filter(forPlugin)
  for (const l of LANGS) {
    assert.deepEqual(Object.keys(STRINGS[l]), list.map((e) => e.id))
    for (const e of list) assert.equal(STRINGS[l][e.id], e[l].replace(/\{0\}/g, "%1"), `${e.id} ${l}`)
  }
})

test("the desktop's entries put their placeholders in order and use the Rust field once", () => {
  const list = entries.filter(forDesktop)
  assert.ok(list.length > 70)
  validate(entries)
})

test("a source that disagrees with itself is refused", () => {
  const ok = { id: "x", os: ["windows"], en: "a {0}", es: "b {0}", pt: "c {0}" }
  validate([ok])
  assert.throws(() => validate([{ ...ok, es: "b" }]), /placeholders/)
  assert.throws(() => validate([ok, { ...ok, id: "y", rust: "x" }]), /Rust field/)
  assert.throws(() => validate([{ ...ok, pt: " " }]), /no pt/)
  assert.throws(() => validate([{ ...ok, os: ["omarchy"], en: "a {1}", es: "b {1}", pt: "c {1}" }]), /one argument/)
})

test("the desktop takes the plugin's Spanish nothing-playing text", () => {
  const e = entries.find((x) => x.id === "nothingPlaying")
  assert.equal(e.es, "Nada sonando")
})
