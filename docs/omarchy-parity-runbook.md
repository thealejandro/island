# Omarchy desktop additions

Port to the existing plugin, tracked in #81. The common-core proposal
remains in #80. Desktop-specific placement, autostart, packaging and volume
APIs do not replace Omarchy's bar, theme, OSD or notification integrations.

## Validate

```bash
node --test 'test/*.test.mjs'
omarchy plugin validate .
qmllint LyricsColumn.qml FocusPage.qml HomePage.qml IslandWindow.qml Service.qml
QT_QPA_PLATFORM=offscreen qmltestrunner -input test/qml/tst_focus.qml
```

The plugin is interpreted QML: there is no executable build step. CI runs
model, translation, manifest and Qt timer tests on every pull request. On Omarchy,
also load the views with Quickshell and check logs: syntax checks alone
cannot catch layout or binding errors.

Check Home with lyrics on/off, `openLyrics` off, a pause, missing lyrics
and a seek backwards. Check 420/560/960 widths, small/scaled screens and
each bar edge. The lyrics column builds at most seven lines. Choice
controls should wrap at narrow widths. Check focus, pause, resume, skip,
long break, reset and duration changes during a running phase. The timer
continues while closed. Shell restart resets the session but not settings.

## Local activation and rollback

For local testing, point the installed plugin symlink at the tested
development checkout and run `omarchy restart shell`. Keep its previous
target; rollback restores that symlink and restarts the shell. Do not
change `shell.json` to enable optional features just for automated tests;
use fixtures instead.

After restart, verify `omarchy-shell island ping`, `omarchy-shell island
state`, and shell logs for QML errors. A failed load must restore the
previous target immediately. No database or agenda migrations are involved.
New settings are additive, unknown keys remain and missing keys default.

Publishing to main is separate and requires the user's confirmation.

## Golden vectors shared with the desktop core

`island-core` (Rust, `desktop/`) and the plugin's models (`LyricsModel.js`,
`FocusModel.js`, `MediaModel.js`, `IcsModel.js`, `IslandSettings.js`) hold the
same rules twice. `test/vectors/*.json` keeps them from drifting: arrays of
`{"fn", "args", "expect"}` cases that the core generates from its real
functions (lyrics, focus, media, ics, settings).

- `desktop/crates/island-core/tests/vectors.rs` fails when a stored file
  differs from what the core produces now. CI runs it on Linux
  (`cargo test -p island-core`).
- `test/vectors.test.mjs` runs the same cases through the JS models. `fn` is
  a neutral name; the mapping to each JS function is in that file.
- A case with `"skip": {"js": "reason"}` is a known difference between the
  two sides. It is not run in JS, and every one is written down in the
  case itself. Fix the difference (then remove the `skip`) or keep the
  reason honest.

To change a rule: edit the core and the JS, add or change cases in
`vectors.rs`, then regenerate and review the diff of `test/vectors/`:

```bash
cd desktop
ISLAND_BLESS=1 cargo test -p island-core --test vectors
cd .. && node --test 'test/*.test.mjs'
```

Floats are stored with 6 decimals and compared with that tolerance in JS.
The files are LF (`.gitattributes`). All text in the cases is made up.
