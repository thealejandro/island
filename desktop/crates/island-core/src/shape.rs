//! The island's silhouette and where it sits on a screen, in pixels.
//!
//! Hanging from the top edge it has concave "ears" on both top corners, so
//! it reads as the edge bulging down (`IslandShape.qml` in the plugin).
//! Floating, it is a rounded rectangle.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// Top center, hanging from the screen edge.
    #[default]
    Top,
    /// Top center, a margin below the edge.
    Floating,
    TopLeft,
    TopRight,
}

impl Placement {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "top" => Some(Self::Top),
            "floating" => Some(Self::Floating),
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            _ => None,
        }
    }

    pub fn hangs(self) -> bool {
        self == Self::Top
    }
}

/// How the silhouette's corners are drawn (`shape` in the settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    /// Continuous corners that ease out of the edges, rounder the taller
    /// the body, like Apple's island.
    #[default]
    Smooth,
    /// Circular corners of a fixed radius.
    Classic,
}

impl Style {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "smooth" => Some(Self::Smooth),
            "classic" => Some(Self::Classic),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Classic => "classic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

const fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line(Point),
    /// A circular arc of `radius` to `to`; `clockwise` on screen (y down).
    Arc {
        to: Point,
        radius: f32,
        clockwise: bool,
    },
    /// A cubic Bézier to `to`.
    Cubic {
        c1: Point,
        c2: Point,
        to: Point,
    },
}

/// A closed outline: from `start` through `segments`, back to `start`.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub start: Point,
    pub segments: Vec<Segment>,
}

/// The body hanging from y = 0. It spans `width + 2 * ear` along x (the ears
/// stick out on both ends) and `height` down; `ear` and `radius` shrink to
/// fit a small body, like the plugin's.
pub fn hanging(width: f32, height: f32, radius: f32, ear: f32) -> Outline {
    let (w, h) = (width.max(0.0), height.max(0.0));
    let e = ear.min(h).min(w / 2.0).max(0.0);
    let r = radius.min(h - e).min(w / 2.0).max(0.0);
    let arc = |x, y, radius, clockwise| Segment::Arc {
        to: pt(x, y),
        radius,
        clockwise,
    };
    Outline {
        start: pt(0.0, 0.0),
        segments: vec![
            arc(e, e, e, true),
            Segment::Line(pt(e, h - r)),
            arc(e + r, h, r, false),
            Segment::Line(pt(e + w - r, h)),
            arc(e + w, h - r, r, false),
            Segment::Line(pt(e + w, e)),
            arc(w + 2.0 * e, 0.0, e, true),
        ],
    }
}

/// How far along each edge a smooth corner of `radius` reaches: it eases
/// in from farther than a circle's would.
const REACH: f32 = 1.5;
/// Where its control points sit, as a share of the reach from the corner.
/// Small: the curve leaves the edges with almost no bend (no visible
/// kink where a circle's would start) and turns in the middle.
const PULL: f32 = 1.0 / 6.0;

/// A smooth corner from where the last segment ended (`reach` before
/// `corner` along one edge) to `to` (`reach` after it along the other).
fn corner(from: Point, corner: Point, to: Point) -> Segment {
    let toward = |p: Point| {
        pt(
            corner.x + (p.x - corner.x) * PULL,
            corner.y + (p.y - corner.y) * PULL,
        )
    };
    Segment::Cubic {
        c1: toward(from),
        c2: toward(to),
        to,
    }
}

/// The smooth style's corner radius for a body `height` px tall at
/// `scale`: 14 DIPs closed, up to 28 open.
pub fn smooth_radius(height: f32, scale: f32) -> f32 {
    let s = scale.max(0.01);
    (0.44 * height / s).clamp(14.0, 28.0) * s
}

/// [`hanging`] with smooth corners and ears: each corner eases out of its
/// edges over `REACH` times `radius`.
pub fn hanging_smooth(width: f32, height: f32, radius: f32, ear: f32) -> Outline {
    let (w, h) = (width.max(0.0), height.max(0.0));
    let e = ear.min(h).min(w / 2.0).max(0.0);
    let l = (radius * REACH).min(h - e).min(w / 2.0).max(0.0);
    Outline {
        start: pt(0.0, 0.0),
        segments: vec![
            corner(pt(0.0, 0.0), pt(e, 0.0), pt(e, e)),
            Segment::Line(pt(e, h - l)),
            corner(pt(e, h - l), pt(e, h), pt(e + l, h)),
            Segment::Line(pt(e + w - l, h)),
            corner(pt(e + w - l, h), pt(e + w, h), pt(e + w, h - l)),
            Segment::Line(pt(e + w, e)),
            corner(pt(e + w, e), pt(e + w, 0.0), pt(w + 2.0 * e, 0.0)),
        ],
    }
}

/// [`floating`] with smooth corners; a short body is a soft pill.
pub fn floating_smooth(width: f32, height: f32, radius: f32) -> Outline {
    let (w, h) = (width.max(0.0), height.max(0.0));
    let l = (radius * REACH).min(w / 2.0).min(h / 2.0).max(0.0);
    Outline {
        start: pt(l, 0.0),
        segments: vec![
            Segment::Line(pt(w - l, 0.0)),
            corner(pt(w - l, 0.0), pt(w, 0.0), pt(w, l)),
            Segment::Line(pt(w, h - l)),
            corner(pt(w, h - l), pt(w, h), pt(w - l, h)),
            Segment::Line(pt(l, h)),
            corner(pt(l, h), pt(0.0, h), pt(0.0, h - l)),
            Segment::Line(pt(0.0, l)),
            corner(pt(0.0, l), pt(0.0, 0.0), pt(l, 0.0)),
        ],
    }
}

/// The body's outline in `style`, `width` x `height` px at `scale`,
/// hanging from the edge (with `ear`) or not. `classic_radius` is the
/// classic style's, in px.
pub fn outline(
    style: Style,
    hangs: bool,
    width: f32,
    height: f32,
    classic_radius: f32,
    ear: f32,
    scale: f32,
) -> Outline {
    match (style, hangs) {
        (Style::Classic, true) => hanging(width, height, classic_radius, ear),
        (Style::Classic, false) => floating(width, height, classic_radius),
        (Style::Smooth, true) => hanging_smooth(width, height, smooth_radius(height, scale), ear),
        (Style::Smooth, false) => floating_smooth(width, height, smooth_radius(height, scale)),
    }
}

/// A rounded rectangle at (0, 0).
pub fn floating(width: f32, height: f32, radius: f32) -> Outline {
    let (w, h) = (width.max(0.0), height.max(0.0));
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let arc = |x, y| Segment::Arc {
        to: pt(x, y),
        radius: r,
        clockwise: true,
    };
    Outline {
        start: pt(r, 0.0),
        segments: vec![
            Segment::Line(pt(w - r, 0.0)),
            arc(w, r),
            Segment::Line(pt(w, h - r)),
            arc(w - r, h),
            Segment::Line(pt(r, h)),
            arc(0.0, h - r),
            Segment::Line(pt(0.0, r)),
            arc(r, 0.0),
        ],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The extra width the ears add on each side.
pub fn ear_room(p: Placement, ear: f32) -> f32 {
    if p.hangs() { ear } else { 0.0 }
}

/// Where the window goes on a `monitor`, sized for the biggest body
/// (`max_width` x `max_height`, without ears).
pub fn window_rect(
    p: Placement,
    monitor: Rect,
    max_width: f32,
    max_height: f32,
    ear: f32,
    margin: f32,
) -> Rect {
    let width = max_width + 2.0 * ear_room(p, ear);
    let centered = monitor.x + ((monitor.width - width) / 2.0).round();
    let (x, y) = match p {
        Placement::Top => (centered, monitor.y),
        Placement::Floating => (centered, monitor.y + margin),
        Placement::TopLeft => (monitor.x + margin, monitor.y + margin),
        Placement::TopRight => (
            monitor.x + monitor.width - margin - width,
            monitor.y + margin,
        ),
    };
    Rect {
        x,
        y,
        width,
        height: max_height,
    }
}

/// Where a body of `width` x `height` (without ears) sits in a window of
/// `window_width`, ears included: centered, or against its corner. A
/// centered body at rest (`snap`) sits on whole pixels, for crisp edges;
/// while it moves it doesn't, so its center never wobbles by half a pixel.
pub fn body_rect(
    p: Placement,
    window_width: f32,
    width: f32,
    height: f32,
    ear: f32,
    snap: bool,
) -> Rect {
    let full = width + 2.0 * ear_room(p, ear);
    let x = match p {
        Placement::TopLeft => 0.0,
        Placement::TopRight => window_width - full,
        _ if snap => ((window_width - full) / 2.0).round(),
        _ => (window_width - full) / 2.0,
    };
    Rect {
        x,
        y: 0.0,
        width: full,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end(o: &Outline) -> Point {
        match o.segments.last() {
            Some(Segment::Line(p))
            | Some(Segment::Arc { to: p, .. })
            | Some(Segment::Cubic { to: p, .. }) => *p,
            None => o.start,
        }
    }

    #[test]
    fn hanging_spans_the_ears_and_ends_on_the_edge() {
        let o = hanging(200.0, 32.0, 18.0, 10.0);
        assert_eq!(end(&o), pt(220.0, 0.0));
        // The bottom corners leave room for the ears: r <= h - e.
        assert_eq!(
            o.segments[2],
            Segment::Arc {
                to: pt(28.0, 32.0),
                radius: 18.0,
                clockwise: false
            }
        );
    }

    #[test]
    fn hanging_shrinks_ears_and_corners_on_a_tiny_body() {
        let o = hanging(6.0, 4.0, 18.0, 10.0);
        // ear <= w / 2 = 3, radius <= h - ear = 1
        assert_eq!(
            o.segments[0],
            Segment::Arc {
                to: pt(3.0, 3.0),
                radius: 3.0,
                clockwise: true
            }
        );
        assert_eq!(
            o.segments[2],
            Segment::Arc {
                to: pt(4.0, 4.0),
                radius: 1.0,
                clockwise: false
            }
        );
        // A spring that dips below zero gives an empty shape, not a broken one.
        assert_eq!(end(&hanging(-3.0, -1.0, 18.0, 10.0)), pt(0.0, 0.0));
    }

    #[test]
    fn floating_is_a_closed_pill_when_short() {
        let o = floating(180.0, 32.0, 18.0);
        assert_eq!(o.start, pt(16.0, 0.0));
        assert_eq!(end(&o), o.start);
    }

    #[test]
    fn windows_and_bodies_by_placement() {
        let mon = Rect {
            x: -1920.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let top = window_rect(Placement::Top, mon, 560.0, 180.0, 10.0, 8.0);
        assert_eq!(
            top,
            Rect {
                x: -1920.0 + 670.0,
                y: 0.0,
                width: 580.0,
                height: 180.0
            }
        );
        assert_eq!(
            body_rect(Placement::Top, 580.0, 180.0, 32.0, 10.0, true),
            Rect {
                x: 190.0,
                y: 0.0,
                width: 200.0,
                height: 32.0
            }
        );
        let right = window_rect(Placement::TopRight, mon, 560.0, 180.0, 10.0, 8.0);
        assert_eq!((right.x, right.y, right.width), (-8.0 - 560.0, 8.0, 560.0));
        assert_eq!(
            body_rect(Placement::TopRight, 560.0, 180.0, 32.0, 10.0, true).x,
            380.0
        );
        assert_eq!(
            body_rect(Placement::TopLeft, 560.0, 180.0, 32.0, 10.0, true).x,
            0.0
        );
        // Moving, a centered body keeps its center on the window's.
        let b = body_rect(Placement::Floating, 581.0, 180.3, 32.0, 10.0, false);
        assert_eq!(b.x + b.width / 2.0, 290.5);
        assert_eq!(
            window_rect(Placement::Floating, mon, 560.0, 180.0, 10.0, 8.0).y,
            8.0
        );
    }

    /// Every point the outline passes through or pulls toward.
    fn points(o: &Outline) -> Vec<Point> {
        let mut v = vec![o.start];
        for s in &o.segments {
            match *s {
                Segment::Line(p) | Segment::Arc { to: p, .. } => v.push(p),
                Segment::Cubic { c1, c2, to } => v.extend([c1, c2, to]),
            }
        }
        v
    }

    #[test]
    fn smooth_outlines_stay_in_their_box_and_close() {
        for (w, h) in [(180.0, 32.0), (560.0, 180.0), (6.0, 4.0), (-3.0, -1.0)] {
            let o = hanging_smooth(w, h, smooth_radius(h, 1.0), 10.0);
            assert_eq!(
                end(&o),
                pt(
                    w.max(0.0) + 2.0 * 10f32.min(h.max(0.0)).min(w.max(0.0) / 2.0),
                    0.0
                )
            );
            let f = floating_smooth(w, h, smooth_radius(h, 1.0));
            assert_eq!(end(&f), f.start);
            for (o, wide) in [(&o, w.max(0.0) + 20.0), (&f, w.max(0.0))] {
                for p in points(o) {
                    assert!(
                        p.x >= 0.0 && p.x <= wide && p.y >= 0.0 && p.y <= h.max(0.0),
                        "{w}x{h}: {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn smooth_corners_leave_the_edges_straight() {
        // The bottom left corner: its first control point is on the side,
        // its second on the bottom, so the curve meets both edges flat.
        let o = hanging_smooth(560.0, 180.0, 28.0, 10.0);
        let Segment::Cubic { c1, c2, to } = o.segments[2] else {
            panic!("{:?}", o.segments[2]);
        };
        assert_eq!(c1.x, 10.0);
        assert_eq!(c2.y, 180.0);
        assert_eq!(to, pt(10.0 + 42.0, 180.0));
    }

    #[test]
    fn the_smooth_radius_grows_with_the_body() {
        assert_eq!(smooth_radius(30.0, 1.0), 14.0);
        assert_eq!(smooth_radius(180.0, 1.0), 28.0);
        assert!((smooth_radius(50.0, 1.0) - 22.0).abs() < 1e-4);
        // In DIPs: twice the pixels at twice the scale.
        assert_eq!(smooth_radius(60.0, 2.0), 28.0);
    }

    #[test]
    fn style_names() {
        for s in [Style::Smooth, Style::Classic] {
            assert_eq!(Style::parse(s.name()), Some(s));
        }
        assert_eq!(Style::parse("round"), None);
        assert_eq!(Style::default(), Style::Smooth);
    }

    #[test]
    fn placement_names() {
        for (s, p) in [
            ("top", Placement::Top),
            ("floating", Placement::Floating),
            ("top-left", Placement::TopLeft),
            ("top-right", Placement::TopRight),
        ] {
            assert_eq!(Placement::parse(s), Some(p));
        }
        assert_eq!(Placement::parse("bottom"), None);
    }
}
