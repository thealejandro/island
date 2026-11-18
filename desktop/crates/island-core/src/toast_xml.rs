//! What the island reads from a toast's XML (kept in Windows' notification
//! database): the picture it carries and whether it asks for silence. A
//! small scan, element by element with quotes respected, not a full XML
//! parser: a toast is short and only these few things are looked for.

/// The `src` of the first `<image placement="appLogoOverride">`; `None`
/// when there is none. Still escaped (see [`unescape`]).
pub fn app_logo_override(xml: &str) -> Option<&str> {
    tags(xml, "image")
        .find(|tag| attr(tag, "placement") == Some("appLogoOverride") && attr(tag, "src").is_some())
        .and_then(|tag| attr(tag, "src"))
}

/// The toast asks to make no sound: `<audio silent="true"/>`.
pub fn silent(xml: &str) -> bool {
    tags(xml, "audio")
        .next()
        .and_then(|tag| attr(tag, "silent"))
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

/// Each `<name ...` element's opening tag (up to its `>`), in order; not
/// `<nameSomething`.
fn tags<'a>(xml: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> {
    let open = format!("<{name}");
    let mut rest = xml;
    std::iter::from_fn(move || {
        loop {
            let at = rest.find(&open)?;
            rest = &rest[at..];
            let after = &rest[open.len()..];
            if !after.starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>') {
                rest = after;
                continue;
            }
            let end = rest.find('>')?;
            let tag = &rest[..end];
            rest = &rest[end..];
            return Some(tag);
        }
    })
}

/// An attribute's value in a tag's text (`<image a="b" ...`); quotes single
/// or double.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest.as_bytes().get(at.wrapping_sub(1)).copied();
        let after = &rest[at + name.len()..];
        // A whole attribute name, not the tail of a longer one.
        if before.is_some_and(|b| b == b'_' || b.is_ascii_alphanumeric()) {
            rest = &rest[at + 1..];
            continue;
        }
        let after = after.trim_start();
        let Some(eq) = after.strip_prefix('=') else {
            rest = &rest[at + 1..];
            continue;
        };
        let value = eq.trim_start();
        let quote = value.as_bytes().first().copied()?;
        if quote != b'"' && quote != b'\'' {
            rest = &rest[at + 1..];
            continue;
        }
        let inner = &value[1..];
        let end = inner.find(quote as char)?;
        return Some(&inner[..end]);
    }
    None
}

/// An attribute's value with XML's entities (`&amp;`, `&#39;`…) decoded;
/// `None` if one is malformed.
pub fn unescape(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find(';')? + at;
        let name = &rest[at + 1..end];
        let c = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let n = if let Some(h) = name.strip_prefix("#x").or(name.strip_prefix("#X")) {
                    u32::from_str_radix(h, 16).ok()?
                } else {
                    name.strip_prefix('#')?.parse().ok()?
                };
                char::from_u32(n)?
            }
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logo_of(xml: &str) -> Option<String> {
        app_logo_override(xml).map(|s| s.to_string())
    }

    #[test]
    fn takes_the_app_logo_override() {
        let xml = "<toast><visual><binding template=\"ToastGeneric\">\
            <image placement=\"appLogoOverride\" src=\"C:\\a\\p.png\"/>\
            <text>hi</text></binding></visual></toast>";
        assert_eq!(logo_of(xml).as_deref(), Some("C:\\a\\p.png"));
    }

    #[test]
    fn none_without_app_logo_override() {
        let xml = "<toast><image src=\"C:\\a\\p.png\"/>\
            <image placement=\"hero\" src=\"C:\\a\\h.jpg\"/></toast>";
        assert_eq!(logo_of(xml), None);
        assert_eq!(logo_of("<toast><text>hi</text></toast>"), None);
        assert_eq!(logo_of(""), None);
    }

    #[test]
    fn first_of_several_wins() {
        let xml = "<toast>\
            <image placement=\"appLogoOverride\" src=\"C:\\a\\1.png\"/>\
            <image placement=\"appLogoOverride\" src=\"C:\\a\\2.png\"/></toast>";
        assert_eq!(logo_of(xml).as_deref(), Some("C:\\a\\1.png"));
    }

    #[test]
    fn attributes_in_any_order_and_single_quotes() {
        let xml = "<toast><image src='C:\\a\\p.png' placement='appLogoOverride'/></toast>";
        assert_eq!(logo_of(xml).as_deref(), Some("C:\\a\\p.png"));
    }

    #[test]
    fn src_on_a_later_image_is_found() {
        let xml = "<toast><image src=\"C:\\a\\big.png\"/>\
            <image placement=\"appLogoOverride\" src=\"C:\\a\\p.png\"/></toast>";
        assert_eq!(logo_of(xml).as_deref(), Some("C:\\a\\p.png"));
    }

    #[test]
    fn not_an_image_element() {
        let xml = "<toast><imagex placement=\"appLogoOverride\" src=\"C:\\a\\p.png\"/></toast>";
        assert_eq!(logo_of(xml), None);
    }

    #[test]
    fn entities() {
        assert_eq!(
            unescape("C:\\A &amp; B\\it&apos;s&#33;&#x41;.png").as_deref(),
            Some("C:\\A & B\\it's!A.png")
        );
        assert_eq!(unescape("a &bogus; b"), None);
    }

    #[test]
    fn silence_is_asked_for_by_the_audio_element() {
        assert!(silent("<toast><audio silent=\"true\"/></toast>"));
        assert!(silent(
            "<toast><audio src='ms-winsoundevent:x' silent='True' /></toast>"
        ));
        assert!(silent(
            "<toast>\n<audio\n  silent = \"true\"></audio></toast>"
        ));
        assert!(!silent("<toast><audio silent=\"false\"/></toast>"));
        assert!(!silent(
            "<toast><audio src=\"ms-winsoundevent:Notification.IM\"/></toast>"
        ));
        assert!(!silent("<toast><text>silent=\"true\"</text></toast>"));
        assert!(!silent("<toast><audiox silent=\"true\"/></toast>"));
        assert!(!silent("<toast><audio nonsilent=\"true\"/></toast>"));
        assert!(!silent(""));
    }
}
