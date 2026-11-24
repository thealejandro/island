//! The desktop island's settings, as `settings.json` keeps them: pure
//! parsing and editing, so the file can be read on any machine's tests.
//!
//! Like `IslandSettings.js` in the plugin, anything missing or invalid
//! falls back to its default, key by key: a typo never loses the rest.
//! Keys shared with the plugin keep its names (`language`, `lyrics`,
//! `openWidth`).

use serde_json::{Map, Value};

use crate::color;
use crate::dodge::{self, DodgeStyle};
use crate::focus::Lengths;
use crate::media::{self, SongText, TextAlign};
use crate::monitor::{FullScreen, GlanceScreen, MonitorChoice, Pick};
use crate::pages::Page;
use crate::shape::{Form, Placement, Style};

/// Bigger than any real settings file: anything past it is not read.
pub const MAX_FILE: usize = 64 * 1024;

/// The texts' language: the system's unless chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    System,
    En,
    Es,
    Pt,
}

impl Language {
    /// `""` (what the file said before) and `"auto"` are both the system's.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "" | "auto" => Some(Self::System),
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            "pt" => Some(Self::Pt),
            _ => None,
        }
    }

    /// How the file spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "auto",
            Self::En => "en",
            Self::Es => "es",
            Self::Pt => "pt",
        }
    }

    /// English, Spanish or Portuguese, given the system's language name
    /// (`es-MX`, `pt-BR`); anything else is English.
    pub fn resolve(self, locale: &str) -> Self {
        let locale = locale.to_ascii_lowercase();
        let base = locale.split(['-', '_']).next().unwrap_or_default();
        match self {
            Self::System if base == "es" => Self::Es,
            Self::System if base == "pt" => Self::Pt,
            Self::System => Self::En,
            chosen => chosen,
        }
    }
}

/// The next language for the menu's row: the system's, Spanish, English,
/// Portuguese, and the system's again.
pub fn next_language(now: Language) -> Language {
    match now {
        Language::System => Language::Es,
        Language::Es => Language::En,
        Language::En => Language::Pt,
        Language::Pt => Language::System,
    }
}

/// The color of the glances' levels (volume and the like) and of a
/// notification with no logo of its own.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Accent {
    /// Windows' accent, followed when it changes.
    #[default]
    System,
    White,
    /// The accent of the Windows theme applied with `tema` (its
    /// `actual.json`); the system's while there is none.
    Theme,
    /// `#rrggbb` or `#rgb`, as 0 to 1.
    Color([f32; 3]),
}

impl Accent {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Self::System),
            "white" => Some(Self::White),
            "theme" => Some(Self::Theme),
            _ => color::parse_hex(s).map(Self::Color),
        }
    }
}

/// The island's own color: black like a notch by default.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum IslandColor {
    #[default]
    Black,
    /// The background of the Windows theme applied with `tema`; black
    /// while there is none.
    Theme,
    Color([f32; 3]),
}

impl IslandColor {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "black" => Some(Self::Black),
            "theme" => Some(Self::Theme),
            _ => color::parse_hex(s).map(Self::Color),
        }
    }
}

/// Whether the island moves with springs and flourishes, or goes straight
/// to where it's headed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Animations {
    /// As Windows' "Animation effects" (Accessibility › Visual effects).
    #[default]
    System,
    On,
    Off,
}

impl Animations {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Self::System),
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// On or off, given what Windows says for `System`.
    pub fn on(self, system: bool) -> bool {
        match self {
            Self::System => system,
            Self::On => true,
            Self::Off => false,
        }
    }
}

/// A text that never prints: a private link.
#[derive(Clone, PartialEq, Default)]
pub struct Hidden(pub String);

impl std::fmt::Debug for Hidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<hidden>")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub language: Language,
    pub placement: Placement,
    /// The silhouette's corners: smooth (Apple's) or classic.
    pub shape: Style,
    /// The island's look: a notch (as it was) or a pill.
    pub island_style: Form,
    pub monitor: MonitorChoice,
    /// Where the glances that tell something show (see [`GlanceScreen`]).
    pub glance_screen: GlanceScreen,
    pub text_align: TextAlign,
    pub song_text: SongText,
    /// Synced lyrics from lrclib.net (off: it sends the song's name there).
    pub lyrics: bool,
    /// The open island shows them in a column beside the player.
    pub open_lyrics: bool,
    /// The open island's width; `None`: by whether there is a column.
    pub open_width: Option<f32>,
    /// The page the island opens on, until another is chosen.
    pub default_page: Page,
    /// The focus timer's lengths.
    pub focus: Lengths,
    /// A glance on each change of the system's volume.
    pub volume_glance: bool,
    /// With the volume glance on, the island takes the volume and media keys
    /// itself, so Windows' own flyout doesn't show over it.
    pub hide_windows_flyout: bool,
    /// A glance when an app starts or stops using the microphone or camera.
    pub privacy_glance: bool,
    /// A glance when a paired Bluetooth device connects or disconnects.
    pub bluetooth_glance: bool,
    /// A glance when charging starts or stops, at full charge, and when
    /// the battery runs low (20 % and 10 %).
    pub battery_glance: bool,
    /// A glance when the built-in screen's brightness changes.
    pub brightness_glance: bool,
    /// Reminders of the agenda's events (see [`crate::agenda`]).
    pub agenda_glance: bool,
    /// The calendar's private iCalendar link; empty: none. Never shown.
    pub agenda_url: Hidden,
    /// How many minutes before an event its reminder shows.
    pub reminder_lead: u32,
    /// A glance when a notification shows: its app and title.
    pub notification_glance: bool,
    /// Where those glances go while an app is in full screen on a screen
    /// (never while presenting or in a game): over it, on another screen,
    /// or nowhere.
    pub notifications_in_full_screen: FullScreen,
    /// Windows' notification sound when a glance shows while the banners
    /// are off ("Notifications only in the island").
    pub notification_sound: bool,
    /// "Check for updates" in the tray's menu (GitHub, only when chosen).
    pub update_check: bool,
    /// How late the lyrics follow the position the player tells, in ms:
    /// the sound comes out later than that (more so over Bluetooth).
    /// `None` ("auto"): by the speakers in use, see [`Settings::lyrics_delay`].
    pub lyrics_delay: Option<u32>,
    /// The closed island's and the glance's sizes in DIPs, before
    /// `islandScale` (see [`Settings::sizes`] for how they fit together).
    pub closed_width: f32,
    pub closed_height: f32,
    pub glance_width: f32,
    pub glance_height: f32,
    /// How big the closed island and its glances are, times this; the open
    /// island keeps its size.
    pub island_scale: f32,
    pub accent: Accent,
    pub island_color: IslandColor,
    pub animations: Animations,
    /// The island hides on a screen while an app is in full screen there.
    pub hide_in_full_screen: bool,
    /// While a song is loaded and the focus timer is on, the closed island
    /// shows the timer in a small bubble beside it.
    pub dual_activity: bool,
    /// The closed island makes way while a window's top edge is under it
    /// (see `dodge`), so the controls beneath stay reachable.
    pub dodge_windows: bool,
    /// How the island looks while it makes way (global, no per-screen value).
    pub dodge_style: DodgeStyle,
    /// How opaque the island is while it makes way in the faded style.
    pub dodge_opacity: f32,
    /// What each screen has of its own over the settings above, by the
    /// screen's device id or its number (see [`Settings::screen`]).
    pub screens: Vec<(String, ScreenOverride)>,
}

/// What a screen may set for itself (`screens` in the settings): the keys
/// that shape the island, each one apart. Anything not set follows the
/// settings above, so a later change to those shows here too.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScreenOverride {
    pub placement: Option<Placement>,
    pub island_style: Option<Form>,
    pub shape: Option<Style>,
    pub island_scale: Option<f32>,
    pub closed_width: Option<f32>,
    pub closed_height: Option<f32>,
    pub glance_width: Option<f32>,
    pub glance_height: Option<f32>,
    /// `Some(None)` is "auto": its own, even over a fixed global width.
    pub open_width: Option<Option<f32>>,
}

/// An island's look on one screen: the overrides over the settings, all
/// resolved (`Look` in `menu` is the tray menu's own).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenLook {
    pub placement: Placement,
    pub form: Form,
    pub shape: Style,
    pub scale: f32,
    pub sizes: media::Sizes,
    pub open_width: f32,
}

/// The sizes the tray menu steps through (`islandScale`).
pub const SCALE_STEPS: [f32; 7] = [0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4];

/// The next of [`SCALE_STEPS`] past `now`; the first after the last.
pub fn next_scale(now: f32) -> f32 {
    SCALE_STEPS
        .iter()
        .copied()
        .find(|s| *s > now + 0.001)
        .unwrap_or(SCALE_STEPS[0])
}

impl ScreenOverride {
    /// Reads one screen's entry with the globals' rules (numbers clamped,
    /// widths in steps of 10); whatever is not usable is not set.
    pub fn parse(o: &Map<String, Value>) -> Self {
        let text = |k: &str| o.get(k).and_then(Value::as_str);
        let num = |k: &str| o.get(k).and_then(Value::as_f64).map(|v| v as f32);
        Self {
            placement: text("placement").and_then(Placement::parse),
            island_style: text("islandStyle").and_then(Form::parse),
            shape: text("shape").and_then(Style::parse),
            island_scale: num("islandScale").map(|v| v.clamp(ISLAND_SCALE.0, ISLAND_SCALE.1)),
            closed_width: num("closedWidth").map(|v| media::size_width(v, media::CLOSED_WIDTH)),
            closed_height: num("closedHeight")
                .map(|v| v.clamp(media::CLOSED_HEIGHT.0, media::CLOSED_HEIGHT.1)),
            glance_width: num("glanceWidth").map(|v| media::size_width(v, media::GLANCE_WIDTH)),
            glance_height: num("glanceHeight")
                .map(|v| v.clamp(media::GLANCE_HEIGHT.0, media::GLANCE_HEIGHT.1)),
            open_width: match o.get("openWidth") {
                Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("auto") => Some(None),
                Some(Value::Number(n)) => media::parse_open_width(&n.to_string()).map(Some),
                _ => None,
            },
        }
    }

    /// Nothing set: the screen is as the settings say.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Each field of `self`, else `base`'s.
    fn over(self, base: Self) -> Self {
        Self {
            placement: self.placement.or(base.placement),
            island_style: self.island_style.or(base.island_style),
            shape: self.shape.or(base.shape),
            island_scale: self.island_scale.or(base.island_scale),
            closed_width: self.closed_width.or(base.closed_width),
            closed_height: self.closed_height.or(base.closed_height),
            glance_width: self.glance_width.or(base.glance_width),
            glance_height: self.glance_height.or(base.glance_height),
            open_width: self.open_width.or(base.open_width),
        }
    }
}

/// `lyricsDelay`'s range, in ms.
pub const LYRICS_DELAY: (u32, u32) = (0, 2000);
/// `reminderLead`'s range, in minutes.
pub const REMINDER_LEAD: (u32, u32) = (0, 120);
/// `islandScale`'s range: the closed island and its glances, times this.
pub const ISLAND_SCALE: (f32, f32) = (0.8, 1.4);
/// "auto" over Bluetooth speakers or headphones: what they add, about.
/// Wired ones add next to nothing.
pub const BLUETOOTH_DELAY: u32 = 200;

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            placement: Placement::default(),
            shape: Style::default(),
            island_style: Form::default(),
            monitor: MonitorChoice::default(),
            glance_screen: GlanceScreen::All,
            text_align: TextAlign::default(),
            song_text: SongText::default(),
            lyrics: false,
            open_lyrics: true,
            open_width: None,
            default_page: Page::default(),
            focus: Lengths::default(),
            volume_glance: true,
            hide_windows_flyout: true,
            privacy_glance: true,
            bluetooth_glance: true,
            battery_glance: true,
            brightness_glance: true,
            agenda_glance: true,
            agenda_url: Hidden::default(),
            reminder_lead: 10,
            notification_glance: true,
            notifications_in_full_screen: FullScreen::Here,
            notification_sound: true,
            update_check: true,
            lyrics_delay: None,
            closed_width: media::CLOSED.0,
            closed_height: media::CLOSED.1,
            glance_width: media::PEEK.0,
            glance_height: media::PEEK.1,
            island_scale: 1.0,
            accent: Accent::System,
            island_color: IslandColor::Black,
            animations: Animations::System,
            hide_in_full_screen: true,
            dual_activity: true,
            dodge_windows: true,
            dodge_style: DodgeStyle::Mini,
            dodge_opacity: dodge::OPACITY_DEFAULT,
            screens: Vec::new(),
        }
    }
}

/// What a new `settings.json` holds: every key, at its default, so they
/// can be found and changed by hand.
pub const TEMPLATE: &str = r#"{
  "language": "auto",
  "placement": "top",
  "shape": "smooth",
  "islandStyle": "notch",
  "monitor": "primary",
  "glanceScreen": "all",
  "textAlign": "center",
  "songText": "title-artist",
  "lyrics": false,
  "openLyrics": true,
  "openWidth": "auto",
  "defaultPage": "home",
  "closedWidth": 180,
  "closedHeight": 32,
  "glanceWidth": 400,
  "glanceHeight": 44,
  "islandScale": 1.0,
  "accent": "system",
  "islandColor": "black",
  "animations": "system",
  "hideInFullScreen": true,
  "dualActivity": true,
  "dodgeWindows": true,
  "dodgeStyle": "mini",
  "dodgeOpacity": 0.35,
  "focusMinutes": 25,
  "breakMinutes": 5,
  "longBreakMinutes": 15,
  "longBreakEvery": 4,
  "volumeGlance": true,
  "hideWindowsFlyout": true,
  "privacyGlance": true,
  "bluetoothGlance": true,
  "batteryGlance": true,
  "brightnessGlance": true,
  "agendaGlance": true,
  "agendaUrl": "",
  "reminderLead": 10,
  "notificationGlance": true,
  "notificationsInFullScreen": "here",
  "notificationSound": true,
  "updateCheck": true,
  "lyricsDelay": "auto",
  "screens": {}
}
"#;

impl Settings {
    /// Reads `settings.json`'s text; whatever can't be used is the default.
    /// `None` when it isn't a JSON object at all (half saved, a missing
    /// comma): better to keep the settings in use than to drop them all.
    /// Empty text is a file with nothing set yet.
    pub fn parse(text: &str) -> Option<Self> {
        let d = Self::default();
        let text = text.trim_start_matches('\u{feff}');
        if text.trim().is_empty() {
            return Some(d);
        }
        if text.len() > MAX_FILE {
            return None;
        }
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(text) else {
            return None;
        };
        let text = |k: &str| o.get(k).and_then(Value::as_str);
        let flag = |k: &str, def: bool| o.get(k).and_then(Value::as_bool).unwrap_or(def);
        // Whole minutes in range; anything else is the default.
        let minutes = |k: &str, (lo, hi): (u32, u32), def: u32| {
            o.get(k)
                .and_then(Value::as_u64)
                .filter(|m| (u64::from(lo)..=u64::from(hi)).contains(m))
                .map_or(def, |m| m as u32)
        };
        // A number is clamped into range (widths in steps of 10); anything
        // else: the default.
        let size = |k: &str, range: (f32, f32), step: bool, def: f32| {
            o.get(k).and_then(Value::as_f64).map_or(def, |v| {
                let v = v as f32;
                if step {
                    media::size_width(v, range)
                } else {
                    v.clamp(range.0, range.1)
                }
            })
        };
        let df = d.focus;
        Some(Self {
            language: text("language")
                .and_then(Language::parse)
                .unwrap_or(d.language),
            placement: text("placement")
                .and_then(Placement::parse)
                .unwrap_or(d.placement),
            shape: text("shape").and_then(Style::parse).unwrap_or(d.shape),
            island_style: text("islandStyle")
                .and_then(Form::parse)
                .unwrap_or(d.island_style),
            // A screen's number may come as a number or as text; "all" or
            // a list ([1, 3] in JSON, "1,3" as text) puts an island on each.
            // A list may name screens by their device id too.
            monitor: match o.get("monitor") {
                Some(Value::String(s)) => MonitorChoice::parse(s),
                Some(Value::Number(n)) => MonitorChoice::parse(&n.to_string()),
                Some(Value::Array(a)) => {
                    let ps: Option<Vec<Pick>> = a
                        .iter()
                        .map(|v| match v {
                            Value::Number(n) => n
                                .as_u64()
                                .and_then(|n| usize::try_from(n).ok())
                                .filter(|n| *n >= 1)
                                .map(Pick::Number),
                            Value::String(s) => Pick::parse(s),
                            _ => None,
                        })
                        .collect();
                    ps.filter(|ps| !ps.is_empty()).map(MonitorChoice::List)
                }
                _ => None,
            }
            .unwrap_or(d.monitor),
            glance_screen: text("glanceScreen")
                .and_then(GlanceScreen::parse)
                .unwrap_or(d.glance_screen),
            text_align: text("textAlign")
                .and_then(TextAlign::parse)
                .unwrap_or(d.text_align),
            song_text: text("songText")
                .and_then(SongText::parse)
                .unwrap_or(d.song_text),
            lyrics: flag("lyrics", d.lyrics),
            open_lyrics: flag("openLyrics", d.open_lyrics),
            open_width: o
                .get("openWidth")
                .and_then(Value::as_f64)
                .and_then(|w| media::parse_open_width(&w.to_string())),
            default_page: text("defaultPage")
                .and_then(Page::parse)
                .unwrap_or(d.default_page),
            focus: Lengths {
                focus: minutes("focusMinutes", Lengths::FOCUS, df.focus),
                short: minutes("breakMinutes", Lengths::SHORT, df.short),
                long: minutes("longBreakMinutes", Lengths::LONG, df.long),
                every: minutes("longBreakEvery", Lengths::EVERY, df.every),
            },
            volume_glance: flag("volumeGlance", d.volume_glance),
            hide_windows_flyout: flag("hideWindowsFlyout", d.hide_windows_flyout),
            privacy_glance: flag("privacyGlance", d.privacy_glance),
            bluetooth_glance: flag("bluetoothGlance", d.bluetooth_glance),
            battery_glance: flag("batteryGlance", d.battery_glance),
            brightness_glance: flag("brightnessGlance", d.brightness_glance),
            agenda_glance: flag("agendaGlance", d.agenda_glance),
            agenda_url: Hidden(
                text("agendaUrl")
                    .map(str::trim)
                    .filter(|u| u.len() <= crate::agenda::MAX_URL)
                    .unwrap_or("")
                    .to_string(),
            ),
            reminder_lead: minutes("reminderLead", REMINDER_LEAD, d.reminder_lead),
            notification_glance: flag("notificationGlance", d.notification_glance),
            notifications_in_full_screen: o
                .get("notificationsInFullScreen")
                .and_then(FullScreen::parse)
                .unwrap_or(d.notifications_in_full_screen),
            notification_sound: flag("notificationSound", d.notification_sound),
            update_check: flag("updateCheck", d.update_check),
            // A number in range; "auto" or anything else: by the speakers.
            lyrics_delay: o
                .get("lyricsDelay")
                .and_then(Value::as_u64)
                .filter(|m| (u64::from(LYRICS_DELAY.0)..=u64::from(LYRICS_DELAY.1)).contains(m))
                .map(|m| m as u32),
            closed_width: size("closedWidth", media::CLOSED_WIDTH, true, d.closed_width),
            closed_height: size("closedHeight", media::CLOSED_HEIGHT, false, d.closed_height),
            glance_width: size("glanceWidth", media::GLANCE_WIDTH, true, d.glance_width),
            glance_height: size("glanceHeight", media::GLANCE_HEIGHT, false, d.glance_height),
            // A number is clamped into range; anything else: 1.0 (as made).
            island_scale: o
                .get("islandScale")
                .and_then(Value::as_f64)
                .map_or(d.island_scale, |v| {
                    (v as f32).clamp(ISLAND_SCALE.0, ISLAND_SCALE.1)
                }),
            accent: text("accent").and_then(Accent::parse).unwrap_or(d.accent),
            island_color: text("islandColor")
                .and_then(IslandColor::parse)
                .unwrap_or(d.island_color),
            animations: text("animations")
                .and_then(Animations::parse)
                .unwrap_or(d.animations),
            hide_in_full_screen: flag("hideInFullScreen", d.hide_in_full_screen),
            dual_activity: flag("dualActivity", d.dual_activity),
            dodge_windows: flag("dodgeWindows", d.dodge_windows),
            dodge_style: text("dodgeStyle")
                .and_then(DodgeStyle::parse)
                .unwrap_or(d.dodge_style),
            dodge_opacity: size("dodgeOpacity", dodge::OPACITY, false, d.dodge_opacity),
            screens: Self::parse_screens(o.get("screens")),
        })
    }

    /// The `screens` object: a screen per key, kept in the file's order.
    /// Entries that are not objects or set nothing usable, repeated keys
    /// (ignoring case) are left out.
    fn parse_screens(v: Option<&Value>) -> Vec<(String, ScreenOverride)> {
        let Some(Value::Object(m)) = v else {
            return Vec::new();
        };
        let mut out: Vec<(String, ScreenOverride)> = Vec::new();
        for (key, entry) in m {
            let key = key.trim();
            let Value::Object(entry) = entry else {
                continue;
            };
            let o = ScreenOverride::parse(entry);
            if key.is_empty()
                || o.is_empty()
                || out.iter().any(|(k, _)| k.eq_ignore_ascii_case(key))
            {
                continue;
            }
            out.push((key.to_owned(), o));
        }
        out
    }

    /// The lyrics' delay in ms: the one chosen, or by whether the sound goes
    /// out over Bluetooth.
    pub fn lyrics_delay(&self, bluetooth: bool) -> u32 {
        self.lyrics_delay
            .unwrap_or(if bluetooth { BLUETOOTH_DELAY } else { 0 })
    }

    /// The closed island's and the glance's sizes: the settings, with the
    /// glance kept a little taller than the closed island (see [`media::Sizes::new`]).
    pub fn sizes(&self) -> media::Sizes {
        media::Sizes::new(
            self.closed_width,
            self.closed_height,
            self.glance_width,
            self.glance_height,
        )
    }

    /// The open island's width setting: the chosen one, or 560 (with a
    /// column of lyrics or without, so turning them on doesn't resize it).
    pub fn open_width(&self) -> f32 {
        self.open_width.unwrap_or_else(|| self.auto_open_width())
    }

    /// The open island's width when it is "auto".
    fn auto_open_width(&self) -> f32 {
        if self.lyrics && self.open_lyrics {
            media::OPEN_LYRICS_WIDTH
        } else {
            media::OPEN_WIDTH
        }
    }

    /// What screen `id` (its device id, empty if Windows gives none) has of
    /// its own, `number` being its place left to right, from 1: the entry
    /// by id, field by field over the one by number.
    pub fn screen(&self, id: &str, number: Option<usize>) -> ScreenOverride {
        let by_id = (!id.is_empty())
            .then(|| {
                self.screens
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(id))
            })
            .flatten()
            .map(|(_, o)| *o)
            .unwrap_or_default();
        let by_number = number
            .and_then(|n| {
                self.screens
                    .iter()
                    .find(|(k, _)| Pick::parse(k) == Some(Pick::Number(n)))
            })
            .map(|(_, o)| *o)
            .unwrap_or_default();
        by_id.over(by_number)
    }

    /// Whether the screen has anything of its own.
    pub fn has_override(&self, id: &str, number: Option<usize>) -> bool {
        !self.screen(id, number).is_empty()
    }

    /// The island's look on a screen: its own, else the settings'.
    pub fn for_screen(&self, id: &str, number: Option<usize>) -> ScreenLook {
        let o = self.screen(id, number);
        ScreenLook {
            placement: o.placement.unwrap_or(self.placement),
            form: o.island_style.unwrap_or(self.island_style),
            shape: o.shape.unwrap_or(self.shape),
            scale: o.island_scale.unwrap_or(self.island_scale),
            sizes: media::Sizes::new(
                o.closed_width.unwrap_or(self.closed_width),
                o.closed_height.unwrap_or(self.closed_height),
                o.glance_width.unwrap_or(self.glance_width),
                o.glance_height.unwrap_or(self.glance_height),
            ),
            open_width: match o.open_width {
                Some(w) => w.unwrap_or_else(|| self.auto_open_width()),
                None => self.open_width(),
            },
        }
    }

    /// The look no screen's own changes: the settings'.
    pub fn look(&self) -> ScreenLook {
        self.for_screen("", None)
    }

    /// The entry for `key` (a device id or a number), added if missing.
    pub fn screen_entry(&mut self, key: &str) -> &mut ScreenOverride {
        let i = self
            .screens
            .iter()
            .position(|(k, _)| k.eq_ignore_ascii_case(key))
            .unwrap_or_else(|| {
                self.screens
                    .push((key.to_owned(), ScreenOverride::default()));
                self.screens.len() - 1
            });
        &mut self.screens[i].1
    }

    /// Drops the entries under these keys: those screens follow the settings.
    pub fn reset_screen(&mut self, keys: &[&str]) {
        self.screens
            .retain(|(k, _)| !keys.iter().any(|d| k.eq_ignore_ascii_case(d)));
    }
}

/// The `monitor` setting as the file holds it; read back, the same choice.
pub fn monitor_value(choice: &MonitorChoice) -> Value {
    match choice {
        MonitorChoice::Primary => Value::from("primary"),
        MonitorChoice::All => Value::from("all"),
        MonitorChoice::Number(n) => Value::from(*n),
        MonitorChoice::List(ps) => ps
            .iter()
            .map(|p| match p {
                Pick::Number(n) => Value::from(*n),
                Pick::Id(id) => Value::from(id.as_str()),
            })
            .collect(),
    }
}

/// `text` with one key set to `value`, everything else kept as it was. Text
/// that isn't a JSON object starts over as one.
pub fn set(text: &str, key: &str, value: Value) -> String {
    let mut o = match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(o)) if text.len() <= MAX_FILE => o,
        _ => Map::new(),
    };
    o.insert(key.to_string(), value);
    let mut out = serde_json::to_string_pretty(&Value::Object(o)).unwrap_or_default();
    out.push('\n');
    out
}

/// `text`'s object, or a new one when it isn't a JSON object.
fn object_of(text: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(o)) if text.len() <= MAX_FILE => o,
        _ => Map::new(),
    }
}

fn pretty(o: Map<String, Value>) -> String {
    let mut out = serde_json::to_string_pretty(&Value::Object(o)).unwrap_or_default();
    out.push('\n');
    out
}

/// `text` with `field` set to `value` in the `screens` entry for `key`
/// (found ignoring case, else added), everything else kept as it was.
/// Text that isn't a JSON object starts over as one.
pub fn set_screen(text: &str, key: &str, field: &str, value: Value) -> String {
    let mut o = object_of(text);
    if !matches!(o.get("screens"), Some(Value::Object(_))) {
        o.insert("screens".to_owned(), Value::Object(Map::new()));
    }
    if let Some(Value::Object(screens)) = o.get_mut("screens") {
        let name = screens
            .keys()
            .find(|k| k.trim().eq_ignore_ascii_case(key))
            .cloned()
            .unwrap_or_else(|| key.to_owned());
        let entry = screens
            .entry(name)
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Value::Object(e) = entry {
            e.insert(field.to_owned(), value);
        }
    }
    pretty(o)
}

/// `text` without the `screens` entries under these keys (found ignoring
/// case); `screens` stays, empty if need be. Text that isn't a JSON object
/// starts over as one.
pub fn reset_screen(text: &str, keys: &[&str]) -> String {
    let mut o = object_of(text);
    if let Some(Value::Object(screens)) = o.get_mut("screens") {
        screens.retain(|k, _| !keys.iter().any(|d| k.trim().eq_ignore_ascii_case(d)));
    }
    pretty(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(text: &str) -> Settings {
        Settings::parse(text).unwrap()
    }

    #[test]
    fn the_accent_is_the_system_white_or_a_color() {
        assert_eq!(Settings::default().accent, Accent::System);
        assert_eq!(parse(r#"{"accent": "white"}"#).accent, Accent::White);
        assert_eq!(parse(r#"{"accent": "theme"}"#).accent, Accent::Theme);
        assert_eq!(Settings::default().island_color, IslandColor::Black);
        assert_eq!(
            parse(r#"{"islandColor": "Theme"}"#).island_color,
            IslandColor::Theme
        );
        assert_eq!(
            parse(r##"{"islandColor": "#102030"}"##).island_color,
            IslandColor::Color([16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0])
        );
        assert_eq!(
            parse(r#"{"islandColor": "white"}"#).island_color,
            IslandColor::Black
        );
        assert_eq!(parse(r#"{"accent": " System "}"#).accent, Accent::System);
        assert_eq!(
            parse(r##"{"accent": "#ff8000"}"##).accent,
            Accent::Color([1.0, 128.0 / 255.0, 0.0])
        );
        assert_eq!(
            parse(r##"{"accent": "#0F0"}"##).accent,
            Accent::Color([0.0, 1.0, 0.0])
        );
        for bad in [
            r#""red""#,
            r##""#12345""##,
            r##""#gggggg""##,
            "3",
            r##""#+1+2+3""##,
        ] {
            let s = parse(&format!(r#"{{"accent": {bad}}}"#));
            assert_eq!(s.accent, Accent::System, "{bad}");
        }
    }

    #[test]
    fn empty_files_give_the_defaults_and_broken_ones_nothing() {
        for text in ["", "  \n", "\u{feff}", "{}", TEMPLATE] {
            assert_eq!(Settings::parse(text), Some(Settings::default()), "{text:?}");
        }
        for text in [
            "nope",
            "[1, 2]",
            "null",
            "{\"lyrics\": ",
            "{\"lyrics\": true,}",
        ] {
            assert_eq!(Settings::parse(text), None, "{text:?}");
        }
        let d = Settings::default();
        assert!(!d.lyrics && d.open_lyrics && d.open_width.is_none());
        assert_eq!(d.open_width(), media::OPEN_WIDTH);
    }

    #[test]
    fn each_key_is_read_on_its_own() {
        let s = parse(
            r#"{
                "language": "es", "placement": "floating", "monitor": 2,
                "textAlign": "left", "songText": "title", "lyrics": true,
                "openLyrics": false, "openWidth": 544, "other": 1
            }"#,
        );
        assert_eq!(s.language, Language::Es);
        assert_eq!(s.placement, Placement::Floating);
        assert_eq!(s.monitor, MonitorChoice::parse("2").unwrap());
        assert_eq!(s.text_align, TextAlign::Left);
        assert_eq!(s.song_text, SongText::Title);
        assert!(s.lyrics && !s.open_lyrics);
        assert_eq!(s.open_width, Some(540.0));
        // A bad value loses only itself.
        let s = parse(r#"{"language": "fr", "lyrics": "yes", "placement": "top-left"}"#);
        assert_eq!(s.language, Language::System);
        assert!(!s.lyrics);
        assert_eq!(s.placement, Placement::TopLeft);
        // "auto", or anything that isn't a number: by the lyrics.
        assert_eq!(parse(r#"{"openWidth": "auto"}"#).open_width, None);
        assert_eq!(
            parse(r#"{"monitor": "1"}"#).monitor,
            MonitorChoice::Number(1)
        );
        // The focus timer: whole minutes in range, each on its own.
        let s = parse(
            r#"{"defaultPage": "focus", "focusMinutes": 50, "breakMinutes": 0,
                "longBreakMinutes": 20.5, "longBreakEvery": 3}"#,
        );
        assert_eq!(s.default_page, Page::Focus);
        assert_eq!(
            s.focus,
            Lengths {
                focus: 50,
                short: 5,
                long: 15,
                every: 3
            }
        );
        assert_eq!(parse(r#"{"defaultPage": "nope"}"#).default_page, Page::Home);
        assert!(!parse(r#"{"volumeGlance": false}"#).volume_glance);
        assert!(parse(r#"{"volumeGlance": 0}"#).volume_glance);
        assert!(!parse(r#"{"hideWindowsFlyout": false}"#).hide_windows_flyout);
        assert!(parse("{}").hide_windows_flyout);
        assert!(!parse(r#"{"updateCheck": false}"#).update_check);
        assert!(parse("{}").update_check);
        assert!(!parse(r#"{"privacyGlance": false}"#).privacy_glance);
        assert!(!parse(r#"{"bluetoothGlance": false}"#).bluetooth_glance);
        assert!(!parse(r#"{"batteryGlance": false}"#).battery_glance);
        assert!(parse("{}").battery_glance);
        assert!(!parse(r#"{"brightnessGlance": false}"#).brightness_glance);
        assert!(parse("{}").brightness_glance);
        assert!(!parse(r#"{"agendaGlance": false}"#).agenda_glance);
        assert!(parse("{}").agenda_glance);
        let lead = |t: &str| parse(t).reminder_lead;
        assert_eq!(lead("{}"), 10);
        assert_eq!(lead(r#"{"reminderLead": 0}"#), 0);
        assert_eq!(lead(r#"{"reminderLead": 120}"#), 120);
        assert_eq!(lead(r#"{"reminderLead": 121}"#), 10);
        assert_eq!(lead(r#"{"reminderLead": "x"}"#), 10);
        let url = |t: &str| parse(t).agenda_url.0;
        let shown = format!("{:?}", parse(r#"{"agendaUrl": "https://a.b/secret.ics"}"#));
        assert!(!shown.contains("secret") && shown.contains("<hidden>"));
        assert_eq!(url("{}"), "");
        assert_eq!(
            url(r#"{"agendaUrl": "  https://a.b/c.ics "}"#),
            "https://a.b/c.ics"
        );
        assert_eq!(url(r#"{"agendaUrl": 5}"#), "");
        let long = format!(
            r#"{{"agendaUrl": "{}"}}"#,
            "a".repeat(crate::agenda::MAX_URL + 1)
        );
        assert_eq!(url(&long), "");
        assert!(!parse(r#"{"notificationGlance": false}"#).notification_glance);
        let fs = |t: &str| parse(t).notifications_in_full_screen;
        assert_eq!(
            fs(r#"{"notificationsInFullScreen": false}"#),
            FullScreen::Hide
        );
        assert_eq!(
            fs(r#"{"notificationsInFullScreen": true}"#),
            FullScreen::Here
        );
        assert_eq!(
            fs(r#"{"notificationsInFullScreen": "other"}"#),
            FullScreen::Other
        );
        assert_eq!(
            fs(r#"{"notificationsInFullScreen": "x"}"#),
            FullScreen::Here
        );
        assert_eq!(fs(TEMPLATE), FullScreen::Here);
        assert!(!parse(r#"{"notificationSound": false}"#).notification_sound);
        assert!(parse(r#"{"notificationSound": "no"}"#).notification_sound);
        // The lyrics' delay: a number wins over the speakers; else by them.
        let s = parse(r#"{"lyricsDelay": 350}"#);
        assert_eq!((s.lyrics_delay(true), s.lyrics_delay(false)), (350, 350));
        assert_eq!(parse(r#"{"lyricsDelay": 0}"#).lyrics_delay(true), 0);
        for auto in [r#""auto""#, "-5", "9000", r#""fast""#] {
            let s = parse(&format!(r#"{{"lyricsDelay": {auto}}}"#));
            assert_eq!(s.lyrics_delay(true), BLUETOOTH_DELAY, "{auto}");
            assert_eq!(s.lyrics_delay(false), 0, "{auto}");
        }
        // The island's scale: a number is clamped into range; else 1.0.
        assert_eq!(parse(r#"{"islandScale": 1.2}"#).island_scale, 1.2);
        assert_eq!(
            parse(r#"{"islandScale": 2.5}"#).island_scale,
            ISLAND_SCALE.1
        );
        assert_eq!(
            parse(r#"{"islandScale": 0.5}"#).island_scale,
            ISLAND_SCALE.0
        );
        assert_eq!(parse(r#"{"islandScale": "big"}"#).island_scale, 1.0);
        assert_eq!(parse(r#"{"islandScale": true}"#).island_scale, 1.0);
        assert_eq!(Settings::default().island_scale, 1.0);
        // The sizes: numbers are clamped (widths in steps of 10); else as made.
        let d = Settings::default();
        assert_eq!(d.sizes(), media::Sizes::default());
        assert_eq!(parse(TEMPLATE).sizes(), media::Sizes::default());
        let s = parse(
            r#"{"closedWidth": 237, "closedHeight": 40.5, "glanceWidth": 512, "glanceHeight": 50}"#,
        );
        assert_eq!((s.closed_width, s.closed_height), (240.0, 40.5));
        assert_eq!((s.glance_width, s.glance_height), (510.0, 50.0));
        let s =
            parse(r#"{"closedWidth": 1, "closedHeight": 1, "glanceWidth": 1, "glanceHeight": 1}"#);
        assert_eq!((s.closed_width, s.closed_height), (120.0, 24.0));
        assert_eq!((s.glance_width, s.glance_height), (280.0, 32.0));
        let s = parse(
            r#"{"closedWidth": 9999, "closedHeight": 9999, "glanceWidth": 9999, "glanceHeight": 9999}"#,
        );
        assert_eq!((s.closed_width, s.closed_height), (320.0, 48.0));
        assert_eq!((s.glance_width, s.glance_height), (600.0, 64.0));
        for bad in [r#""wide""#, "true", "null", "[1]"] {
            let s = parse(&format!(
                r#"{{"closedWidth": {bad}, "closedHeight": {bad}, "glanceWidth": {bad}, "glanceHeight": {bad}}}"#
            ));
            assert_eq!(s.sizes(), media::Sizes::default(), "{bad}");
        }
        // The glance stays taller than the closed island, whatever is set.
        let s = parse(r#"{"closedHeight": 48, "glanceHeight": 32}"#);
        assert_eq!(s.sizes().closed.1, 48.0);
        assert_eq!(s.sizes().glance.1, 52.0);
        assert_eq!(parse(r#"{"closedHeight": 36}"#).sizes().glance.1, 44.0);
        // A BOM (Notepad's UTF-8) is fine.
        assert!(parse("\u{feff}{\"lyrics\": true}").lyrics);
        // Too big to be a settings file.
        let huge = format!("{{\"lyrics\": true, \"x\": \"{}\"}}", "a".repeat(MAX_FILE));
        assert_eq!(Settings::parse(&huge), None);
    }

    #[test]
    fn the_monitor_written_reads_back_the_same() {
        for c in [
            MonitorChoice::Primary,
            MonitorChoice::All,
            MonitorChoice::Number(2),
            MonitorChoice::List(vec![
                Pick::Id("\\\\?\\DISPLAY#DEL41B8#5&1".into()),
                Pick::Number(3),
            ]),
        ] {
            let text = set("{}", "monitor", monitor_value(&c));
            assert_eq!(parse(&text).monitor, c);
        }
    }

    #[test]
    fn the_monitor_takes_a_number_all_or_a_list() {
        assert_eq!(
            parse(r#"{"monitor": "primary"}"#).monitor,
            MonitorChoice::Primary
        );
        assert_eq!(parse(r#"{"monitor": 2}"#).monitor, MonitorChoice::Number(2));
        assert_eq!(parse(r#"{"monitor": "all"}"#).monitor, MonitorChoice::All);
        // A list: an array in JSON, comma-separated as text.
        assert_eq!(
            parse(r#"{"monitor": [1, 3]}"#).monitor,
            MonitorChoice::List(vec![Pick::Number(1), Pick::Number(3)])
        );
        assert_eq!(
            parse(r#"{"monitor": "1,3"}"#).monitor,
            MonitorChoice::List(vec![Pick::Number(1), Pick::Number(3)])
        );
        assert_eq!(
            parse(r#"{"monitor": "2, 1"}"#).monitor,
            MonitorChoice::List(vec![Pick::Number(2), Pick::Number(1)])
        );
        // Screens by their device id, mixed with numbers.
        assert_eq!(
            parse(r#"{"monitor": ["\\\\?\\DISPLAY#DEL41B8#5&1", 2]}"#).monitor,
            MonitorChoice::List(vec![
                Pick::Id("\\\\?\\DISPLAY#DEL41B8#5&1".into()),
                Pick::Number(2)
            ])
        );
        // A bad value is the default alone.
        for bad in [
            r#""0""#,
            r#""nope""#,
            r#""1,0""#,
            r#""1,x""#,
            "-1",
            "1.5",
            "[]",
            "[0]",
            r#"[1, ""]"#,
            "[1, true]",
            "true",
        ] {
            let s = parse(&format!(r#"{{"monitor": {bad}}}"#));
            assert_eq!(s.monitor, MonitorChoice::Primary, "{bad}");
        }
    }

    #[test]
    fn the_width_follows_the_lyrics_unless_chosen() {
        let s = parse(r#"{"lyrics": true}"#);
        assert_eq!(s.open_width(), media::OPEN_LYRICS_WIDTH);
        let s = parse(r#"{"lyrics": true, "openLyrics": false}"#);
        assert_eq!(s.open_width(), media::OPEN_WIDTH);
        let s = parse(r#"{"lyrics": true, "openWidth": 5000}"#);
        assert_eq!(s.open_width(), media::OPEN_WIDTH_MAX);
    }

    #[test]
    fn setting_a_key_keeps_the_rest() {
        let text = set(r#"{"lyrics": false, "mine": [1]}"#, "lyrics", json!(true));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["lyrics"], json!(true));
        assert_eq!(v["mine"], json!([1]));
        assert!(parse(&text).lyrics);
        let text = set(TEMPLATE, "openLyrics", json!(false));
        assert!(!parse(&text).open_lyrics);
        // The keys stay where they were.
        let keys: Vec<String> = serde_json::from_str::<Map<String, Value>>(&text)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let template: Vec<String> = serde_json::from_str::<Map<String, Value>>(TEMPLATE)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, template);
        assert_eq!(keys[0], "language");
        // Nothing usable there: a new object.
        let text = set("nope", "openLyrics", json!(false));
        assert!(!parse(&text).open_lyrics);
    }

    #[test]
    fn shape_by_name() {
        assert_eq!(parse(r#"{"shape": "classic"}"#).shape, Style::Classic);
        assert_eq!(parse(r#"{"shape": "wavy"}"#).shape, Style::Smooth);
        assert_eq!(parse(TEMPLATE).shape, Style::Smooth);
    }

    #[test]
    fn island_style_by_name() {
        assert_eq!(parse(r#"{"islandStyle": "pill"}"#).island_style, Form::Pill);
        assert_eq!(
            parse(r#"{"islandStyle": "glass"}"#).island_style,
            Form::Notch
        );
        assert_eq!(parse(TEMPLATE).island_style, Form::Notch);
        // Apart from the placement.
        let s = parse(r#"{"placement": "floating"}"#);
        assert_eq!(
            (s.placement, s.island_style),
            (Placement::Floating, Form::Notch)
        );
    }

    #[test]
    fn animations_and_full_screen() {
        assert_eq!(parse(TEMPLATE).animations, Animations::System);
        assert_eq!(
            parse(r#"{"animations": "OFF"}"#).animations,
            Animations::Off
        );
        assert_eq!(parse(r#"{"animations": "on"}"#).animations, Animations::On);
        assert_eq!(
            parse(r#"{"animations": false}"#).animations,
            Animations::System
        );
        assert_eq!(
            parse(r#"{"animations": "slow"}"#).animations,
            Animations::System
        );
        assert!(Animations::System.on(true) && !Animations::System.on(false));
        assert!(Animations::On.on(false) && !Animations::Off.on(true));
        assert!(parse(TEMPLATE).hide_in_full_screen);
        assert!(!parse(r#"{"hideInFullScreen": false}"#).hide_in_full_screen);
        assert!(parse(TEMPLATE).dual_activity);
        assert!(parse("{}").dual_activity);
        assert!(!parse(r#"{"dualActivity": false}"#).dual_activity);
        assert!(parse(r#"{"dualActivity": "no"}"#).dual_activity);
        assert!(parse(TEMPLATE).dodge_windows);
        assert!(parse("{}").dodge_windows);
        assert!(!parse(r#"{"dodgeWindows": false}"#).dodge_windows);
        assert!(parse(r#"{"dodgeWindows": "no"}"#).dodge_windows);
        assert_eq!(parse(TEMPLATE).dodge_style, DodgeStyle::Mini);
        assert_eq!(parse(TEMPLATE).dodge_opacity, 0.35);
        assert_eq!(parse("{}").dodge_style, DodgeStyle::Mini);
        assert_eq!(parse("{}").dodge_opacity, 0.35);
        for v in DodgeStyle::ALL {
            let s = parse(&format!(r#"{{"dodgeStyle": "{}"}}"#, v.name()));
            assert_eq!(s.dodge_style, v);
        }
        for bad in [r#""notch""#, r#""bogus""#, "7"] {
            let s = parse(&format!(r#"{{"dodgeStyle": {bad}}}"#));
            assert_eq!(s.dodge_style, DodgeStyle::Mini);
        }
        assert_eq!(parse(r#"{"dodgeOpacity": 0.05}"#).dodge_opacity, 0.1);
        assert_eq!(parse(r#"{"dodgeOpacity": 5}"#).dodge_opacity, 0.9);
        assert_eq!(parse(r#"{"dodgeOpacity": 0.5}"#).dodge_opacity, 0.5);
        assert_eq!(parse(r#"{"dodgeOpacity": "x"}"#).dodge_opacity, 0.35);
        assert_eq!(parse(TEMPLATE).glance_screen, GlanceScreen::All);
        let s = parse(r#"{"glanceScreen": "active"}"#);
        assert_eq!(s.glance_screen, GlanceScreen::Active);
        assert_eq!(
            parse(r#"{"glanceScreen": 2}"#).glance_screen,
            GlanceScreen::All
        );
    }

    #[test]
    fn language_resolves_from_the_system() {
        assert_eq!(Language::System.resolve("es-MX"), Language::Es);
        assert_eq!(Language::System.resolve("ES-es"), Language::Es);
        assert_eq!(Language::System.resolve("en-US"), Language::En);
        assert_eq!(Language::System.resolve(""), Language::En);
        assert_eq!(Language::En.resolve("es-MX"), Language::En);
        assert_eq!(Language::System.resolve("pt-BR"), Language::Pt);
        assert_eq!(Language::System.resolve("pt-PT"), Language::Pt);
        assert_eq!(Language::System.resolve("pt"), Language::Pt);
        assert_eq!(Language::System.resolve("PT_br"), Language::Pt);
        // Only the language counts, not what a longer name starts with.
        assert_eq!(Language::System.resolve("est-EE"), Language::En);
        assert_eq!(Language::System.resolve("pl-PL"), Language::En);
        assert_eq!(Language::Es.resolve("pt-BR"), Language::Es);
        assert_eq!(Language::Pt.resolve("en-US"), Language::Pt);
        // The file's "" and "auto" are the same; each name reads back.
        assert_eq!(Language::parse(""), Some(Language::System));
        assert_eq!(Language::parse("auto"), Some(Language::System));
        assert_eq!(Language::parse("pt"), Some(Language::Pt));
        assert_eq!(Language::parse("pt-BR"), None);
        for l in [Language::System, Language::En, Language::Es, Language::Pt] {
            assert_eq!(Language::parse(l.name()), Some(l));
        }
    }

    #[test]
    fn next_language_goes_around() {
        let mut l = Language::System;
        let mut seen = vec![l];
        for _ in 0..4 {
            l = next_language(l);
            seen.push(l);
        }
        assert_eq!(
            seen,
            [
                Language::System,
                Language::Es,
                Language::En,
                Language::Pt,
                Language::System
            ]
        );
    }

    #[test]
    fn a_new_file_has_the_language_on_auto() {
        let v: Value = serde_json::from_str(TEMPLATE).unwrap();
        assert_eq!(v["language"], json!("auto"));
        assert_eq!(parse(TEMPLATE).language, Language::System);
        assert_eq!(parse(r#"{"language": "pt"}"#).language, Language::Pt);
        assert_eq!(parse(r#"{"language": ""}"#).language, Language::System);
    }

    const ID: &str = r"\\?\DISPLAY#GSM5B09#5&2a6b5e3&0&UID4352#{e6f07b5f}";

    #[test]
    fn no_screens_is_the_global_look() {
        let s = parse(TEMPLATE);
        assert!(s.screens.is_empty());
        assert_eq!(s, Settings::default());
        let l = s.for_screen(ID, Some(1));
        assert_eq!(l, s.look());
        assert_eq!(l.placement, s.placement);
        assert_eq!(l.form, s.island_style);
        assert_eq!(l.shape, s.shape);
        assert_eq!(l.scale, s.island_scale);
        assert_eq!(l.sizes, s.sizes());
        assert_eq!(l.open_width, s.open_width());
        assert!(!s.has_override(ID, Some(1)));
    }

    #[test]
    fn a_screen_overrides_only_what_it_names() {
        let text = serde_json::to_string(&json!({
            "placement": "top",
            "islandScale": 1.0,
            "screens": { ID: { "placement": "floating", "islandStyle": "pill", "islandScale": 1.2 } }
        }))
        .unwrap();
        let s = parse(&text);
        let l = s.for_screen(ID, Some(1));
        assert_eq!(l.placement, Placement::Floating);
        assert_eq!(l.form, Form::Pill);
        assert_eq!(l.scale, 1.2);
        assert_eq!(l.sizes, s.sizes());
        assert_eq!(l.shape, Style::Smooth);
        assert!(s.has_override(ID, Some(1)));
        // Another screen keeps the global look.
        let other = s.for_screen("other", Some(2));
        assert_eq!(other, s.look());
        // What it does not name follows a later global change.
        let text = text.replacen(r#""placement":"top""#, r#""shape":"classic""#, 1);
        let l = parse(&text).for_screen(ID, Some(1));
        assert_eq!(l.shape, Style::Classic);
        assert_eq!(l.placement, Placement::Floating);
    }

    #[test]
    fn ids_match_ignoring_case_and_unknown_ones_are_ignored() {
        let s = parse(r#"{"screens": {"\\\\?\\Display#ABC": {"placement": "floating"}}}"#);
        assert_eq!(s.screens.len(), 1);
        let l = s.for_screen(r"\\?\DISPLAY#abc", Some(1));
        assert_eq!(l.placement, Placement::Floating);
        assert_eq!(s.for_screen(r"\\?\DISPLAY#xyz", Some(1)), s.look());
        // No id: only a number can name it.
        assert_eq!(s.for_screen("", None), s.look());
        // Repeated keys (ignoring case): the first one.
        let s = parse(
            r#"{"screens": {"A": {"placement": "floating"}, "a": {"placement": "top-left"}}}"#,
        );
        assert_eq!(s.screens.len(), 1);
        assert_eq!(s.for_screen("a", None).placement, Placement::Floating);
    }

    #[test]
    fn a_number_names_a_screen_by_position_and_the_id_wins() {
        let text = serde_json::to_string(&json!({
            "screens": {
                "2": { "placement": "top-left", "closedHeight": 28 },
                ID: { "placement": "floating" }
            }
        }))
        .unwrap();
        let s = parse(&text);
        // By number alone.
        let l = s.for_screen("", Some(2));
        assert_eq!(l.placement, Placement::TopLeft);
        assert_eq!(l.sizes.closed.1, 28.0);
        assert_eq!(s.for_screen("", Some(1)), s.look());
        // Field by field: the id's placement, the number's height.
        let l = s.for_screen(ID, Some(2));
        assert_eq!(l.placement, Placement::Floating);
        assert_eq!(l.sizes.closed.1, 28.0);
        // The id's entry on another position does not get the number's.
        let l = s.for_screen(ID, Some(1));
        assert_eq!(l.placement, Placement::Floating);
        assert_eq!(l.sizes.closed.1, 32.0);
        // Numbers below 1 name nothing.
        let s = parse(r#"{"screens": {"0": {"placement": "floating"}}}"#);
        assert_eq!(s.for_screen("", Some(1)), s.look());
    }

    #[test]
    fn bad_values_fall_back_to_the_global() {
        let s = parse(
            r#"{"screens": {"1": {"placement": "nope", "islandScale": "big",
                "closedHeight": true, "islandStyle": 3, "openWidth": "wide"},
                "2": 5, "3": [], "4": {}}}"#,
        );
        assert!(s.screens.is_empty());
        let s = parse(
            r#"{"screens": {"1": {"placement": "nope", "islandScale": "big",
                "closedHeight": true, "shape": "classic"}, "2": 5}}"#,
        );
        assert_eq!(s.screens.len(), 1);
        let l = s.for_screen("", Some(1));
        assert_eq!(l.placement, Placement::Top);
        assert_eq!(l.scale, 1.0);
        assert_eq!(l.sizes, s.sizes());
        assert_eq!(l.shape, Style::Classic);
        for bad in [r#""screens": 5"#, r#""screens": []"#, r#""screens": "x""#] {
            assert!(parse(&format!("{{{bad}}}")).screens.is_empty());
        }
        // An empty key names nothing.
        assert!(
            parse(r#"{"screens": {" ": {"placement": "floating"}}}"#)
                .screens
                .is_empty()
        );
    }

    #[test]
    fn numbers_are_clamped_like_the_globals() {
        let s = parse(
            r#"{"screens": {"1": {"islandScale": 9, "closedWidth": 123, "closedHeight": 90,
                "glanceWidth": 10, "glanceHeight": 1, "openWidth": 544}}}"#,
        );
        let l = s.for_screen("", Some(1));
        assert_eq!(l.scale, ISLAND_SCALE.1);
        assert_eq!(l.sizes.closed, (120.0, media::CLOSED_HEIGHT.1));
        assert_eq!(l.sizes.glance.0, media::GLANCE_WIDTH.0);
        assert_eq!(l.open_width, 540.0);
        let s = parse(r#"{"screens": {"1": {"islandScale": 0.1, "openWidth": 5000}}}"#);
        let l = s.for_screen("", Some(1));
        assert_eq!(l.scale, ISLAND_SCALE.0);
        assert_eq!(l.open_width, media::OPEN_WIDTH_MAX);
    }

    #[test]
    fn the_glance_stays_taller_per_screen() {
        let s = parse(r#"{"glanceHeight": 44, "screens": {"1": {"closedHeight": 48}}}"#);
        let l = s.for_screen("", Some(1));
        assert_eq!(l.sizes.closed.1, 48.0);
        assert!(l.sizes.glance.1 >= 52.0);
        assert_eq!(s.look().sizes.glance.1, 44.0);
    }

    #[test]
    fn open_width_follows_the_global_auto() {
        let s = parse(r#"{"lyrics": true, "screens": {"1": {"placement": "floating"}}}"#);
        assert_eq!(
            s.for_screen("", Some(1)).open_width,
            media::OPEN_LYRICS_WIDTH
        );
        let s = parse(r#"{"openWidth": 700, "screens": {"1": {"openWidth": 600}}}"#);
        assert_eq!(s.for_screen("", Some(1)).open_width, 600.0);
        assert_eq!(s.for_screen("", Some(2)).open_width, 700.0);
    }

    #[test]
    fn a_screen_can_be_auto_over_a_fixed_width() {
        let s = parse(r#"{"openWidth": 700, "screens": {"1": {"openWidth": "auto"}}}"#);
        assert_eq!(s.screens.len(), 1);
        assert_eq!(s.for_screen("", Some(1)).open_width, media::OPEN_WIDTH);
        assert_eq!(s.for_screen("", Some(2)).open_width, 700.0);
        let s =
            parse(r#"{"openWidth": 700, "lyrics": true, "screens": {"1": {"openWidth": "Auto"}}}"#);
        assert_eq!(
            s.for_screen("", Some(1)).open_width,
            media::OPEN_LYRICS_WIDTH
        );
    }

    #[test]
    fn every_screen_in_the_file_is_read() {
        let mut m = Map::new();
        for n in 1..=40 {
            m.insert(n.to_string(), json!({ "placement": "floating" }));
        }
        let text = serde_json::to_string(&json!({ "screens": m })).unwrap();
        assert_eq!(parse(&text).screens.len(), 40);
    }

    #[test]
    fn set_screen_keeps_the_rest() {
        let text = set_screen(TEMPLATE, ID, "placement", json!("floating"));
        let s = parse(&text);
        assert_eq!(s.for_screen(ID, None).placement, Placement::Floating);
        // The other keys are where they were.
        let keys: Vec<String> = serde_json::from_str::<Map<String, Value>>(&text)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys.first().map(String::as_str), Some("language"));
        assert_eq!(keys.last().map(String::as_str), Some("screens"));
        // The key is found ignoring case; its other fields are kept.
        let text = set_screen(&text, &ID.to_lowercase(), "islandScale", json!(1.2));
        let s = parse(&text);
        assert_eq!(s.screens.len(), 1);
        assert_eq!(s.screens[0].0, ID);
        let l = s.for_screen(ID, None);
        assert_eq!((l.placement, l.scale), (Placement::Floating, 1.2));
        // Another screen is added beside it.
        let text = set_screen(&text, "2", "islandStyle", json!("pill"));
        let s = parse(&text);
        assert_eq!(s.screens.len(), 2);
        assert_eq!(s.for_screen("", Some(2)).form, Form::Pill);
        // Text of someone's own is kept; broken text starts over.
        let text = set_screen(
            r#"{"mine": [1], "screens": 5}"#,
            "1",
            "shape",
            json!("classic"),
        );
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mine"], json!([1]));
        assert_eq!(parse(&text).for_screen("", Some(1)).shape, Style::Classic);
        let text = set_screen("nope", "1", "shape", json!("classic"));
        assert_eq!(parse(&text).screens.len(), 1);
    }

    #[test]
    fn reset_screen_removes_the_entry() {
        let text = set_screen(TEMPLATE, ID, "placement", json!("floating"));
        let text = set_screen(&text, "2", "placement", json!("top-left"));
        let text = reset_screen(&text, &[&ID.to_uppercase(), "7"]);
        let s = parse(&text);
        assert_eq!(s.screens.len(), 1);
        assert_eq!(s.screens[0].0, "2");
        let text = reset_screen(&text, &["2"]);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["screens"], json!({}));
        assert_eq!(v["language"], json!("auto"));
        assert_eq!(parse(&text), Settings::default());
    }

    #[test]
    fn the_settings_in_memory_edit_like_the_text() {
        let mut s = Settings::default();
        s.screen_entry(ID).placement = Some(Placement::Floating);
        s.screen_entry(&ID.to_lowercase()).island_scale = Some(1.1);
        assert_eq!(s.screens.len(), 1);
        assert!(s.has_override(ID, Some(1)));
        s.reset_screen(&[ID, "1"]);
        assert!(s.screens.is_empty());
    }

    #[test]
    fn next_scale_steps_and_wraps() {
        assert_eq!(next_scale(1.0), 1.1);
        assert_eq!(next_scale(0.8), 0.9);
        assert_eq!(next_scale(1.4), 0.8);
        assert_eq!(next_scale(1.25), 1.3);
        assert_eq!(next_scale(9.0), 0.8);
        // A step does not stay put on float noise.
        assert_eq!(next_scale(1.1 + 0.0001), 1.2);
    }
}
