//! Geometry between the core's outline and AppKit's paths. No OS calls, so
//! it is tested on any machine.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use island_core::shape::Point;

/// Where the tangents of a quarter arc from `from` to `to` meet: what
/// AppKit's `appendBezierPathWithArcFromPoint:toPoint:radius:` asks for.
/// `clockwise` as seen on screen, y down (the core's outline).
pub fn arc_corner(from: Point, to: Point, clockwise: bool) -> Point {
    // The center is half the chord away from its middle, to the right of
    // the way when turning clockwise; the corner is opposite to it.
    let side = if clockwise { 1.0 } else { -1.0 };
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let center = Point {
        x: (from.x + to.x) / 2.0 - side * dy / 2.0,
        y: (from.y + to.y) / 2.0 + side * dx / 2.0,
    };
    Point {
        x: from.x + to.x - center.x,
        y: from.y + to.y - center.y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn pt(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    #[test]
    fn the_corner_of_a_quarter_arc_is_where_its_tangents_meet() {
        // The island's left ear: from the edge down into the body, clockwise.
        assert_eq!(
            arc_corner(pt(0.0, 0.0), pt(10.0, 10.0), true),
            pt(10.0, 0.0)
        );
        // Its bottom left corner: down, then right, counterclockwise.
        assert_eq!(
            arc_corner(pt(10.0, 14.0), pt(28.0, 32.0), false),
            pt(10.0, 32.0)
        );
        // A rounded rectangle's top right corner, clockwise.
        assert_eq!(
            arc_corner(pt(90.0, 0.0), pt(100.0, 10.0), true),
            pt(100.0, 0.0)
        );
    }
}
