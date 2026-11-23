//! The desktop apps' words, in English, Spanish and Portuguese.
//!
//! The texts themselves are in `shared/texts.json` (the Omarchy plugin reads the
//! same file); `scripts/gen-shared.mjs` writes `texts_gen.rs` from it.

use crate::settings::Language;

#[path = "texts_gen.rs"]
mod texts_gen;
pub use texts_gen::*;

/// `text` with each `{}` replaced by the next of `args`.
pub fn fill(text: &str, args: &[u32]) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    let mut args = args.iter();
    let mut rest = text;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// The texts for `language`, the system's being `locale` (`es-MX`).
pub fn texts(language: Language, locale: &str) -> &'static Texts {
    match language.resolve(locale) {
        Language::Es => &ES,
        Language::Pt => &PT,
        _ => &EN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_by_setting_then_system() {
        assert_eq!(texts(Language::System, "es-ES"), &ES);
        assert_eq!(texts(Language::System, "pt-BR"), &PT);
        assert_eq!(texts(Language::System, "de-DE"), &EN);
        assert_eq!(texts(Language::En, "es-ES"), &EN);
        assert_eq!(texts(Language::Es, "en-US"), &ES);
        assert_eq!(texts(Language::Pt, "en-US"), &PT);
    }

    #[test]
    fn numbers_fill_in() {
        assert_eq!(fill(ES.round, &[2, 4]), "ronda 2 de 4");
        assert_eq!(fill(PT.round, &[2, 4]), "rodada 2 de 4");
        assert_eq!(fill(EN.take_break, &[5]), "5 min break");
        assert_eq!(fill("{} y {}", &[1]), "1 y ");
        assert_eq!(fill("sin nada", &[3]), "sin nada");
        for t in [&EN, &ES, &PT] {
            assert_eq!(t.round.matches("{}").count(), 2);
            assert_eq!(t.then_break.matches("{}").count(), 1);
            assert_eq!(t.take_break.matches("{}").count(), 1);
            assert_eq!(t.display_n.matches("{}").count(), 1);
            assert_eq!(t.event_in.matches("{}").count(), 1);
        }
    }

    #[test]
    fn every_text_is_there() {
        for t in [&EN, &ES, &PT] {
            for s in all(t) {
                assert!(!s.trim().is_empty());
                // The tray's tooltip and menu take up to 127 characters.
                assert!(s.chars().count() < 64);
            }
        }
    }

    #[test]
    fn every_language_has_the_same_numbers() {
        for t in [&ES, &PT] {
            for (s, e) in all(t).into_iter().zip(all(&EN)) {
                assert_eq!(s.matches("{}").count(), e.matches("{}").count(), "{s}");
            }
        }
    }
}
