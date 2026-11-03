//! Rounded rects with Figma's corner smoothing, the same corners ErisDock's body has.

use alloc::vec::Vec;
use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, SQRT_2};

use crate::geometry::{Point, Rect};
use crate::math;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Element {
    Move(Point),
    Line(Point),
    Curve { control1: Point, control2: Point, end: Point },
    Close,
}

/// `rect` with every corner rounded to `radius` and smoothed by `smoothing` (0 is circular). A
/// corner takes (1 + smoothing)·radius of each edge beside it; when the rect is too small for
/// that, the corners shrink together to fit.
pub fn rounded_rect(rect: Rect, radius: f64, smoothing: f64) -> Vec<Element> {
    if rect.width <= 0.01 || rect.height <= 0.01 {
        return Vec::new();
    }
    let k = 1.0 + smoothing;
    let r = radius.min(rect.width / 2.0 / k).min(rect.height / 2.0 / k).max(0.0);
    let (left, right, bottom, top) = (rect.x, rect.max_x(), rect.y, rect.max_y());
    let (up, down, leftward, rightward) = ((0.0, 1.0), (0.0, -1.0), (-1.0, 0.0), (1.0, 0.0));
    let corners = [
        Corner::new(Point::new(right, bottom), rightward, up, r, smoothing),
        Corner::new(Point::new(right, top), up, leftward, r, smoothing),
        Corner::new(Point::new(left, top), leftward, down, r, smoothing),
        Corner::new(Point::new(left, bottom), down, rightward, r, smoothing),
    ];
    let mut elements = Vec::with_capacity(18);
    elements.push(Element::Move(Point::new(left + r * k, bottom)));
    for corner in &corners {
        elements.push(Element::Line(corner.start));
        corner.append_to(&mut elements);
    }
    elements.push(Element::Close);
    elements
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    control1: Point,
    control2: Point,
    end: Point,
}

/// A corner rounded with Figma's corner smoothing: a cubic easing into a circular arc and
/// another easing out of it, so the curvature builds up instead of jumping where the edges
/// end. It runs from `start`, (1 + smoothing)·radius before the vertex along the incoming
/// edge, to as far past it along the outgoing one.
#[derive(Clone, Debug, PartialEq)]
struct Corner {
    start: Point,
    segments: Vec<Segment>,
}

impl Corner {
    /// The corner at `vertex` between an edge heading along `u` into it and one heading along
    /// `v` out of it.
    fn new(vertex: Point, u: (f64, f64), v: (f64, f64), r: f64, s: f64) -> Self {
        let offset = |point: Point, along: f64, across: f64| {
            Point::new(point.x + along * u.0 + across * v.0, point.y + along * u.1 + across * v.1)
        };
        let p = (1.0 + s) * r;
        let start = offset(vertex, -p, 0.0);
        if r <= 0.001 {
            return Self { start, segments: Vec::new() };
        }
        let arc_angle = FRAC_PI_2 * (1.0 - s);
        let arc_side = math::sin(arc_angle / 2.0) * r * SQRT_2;
        let tangent = r * math::tan((FRAC_PI_2 - arc_angle) / 4.0);
        let beta = FRAC_PI_4 * s;
        let c = tangent * math::cos(beta);
        let d = c * math::tan(beta);
        let b = (p - arc_side - c - d) / 3.0;
        let a = 2.0 * b;

        let arc_start = offset(start, a + b + c, d);
        let arc_end = offset(arc_start, arc_side, arc_side);
        // The arc as one cubic: its unit tangents at either end, in (u, v), times
        // 4/3·tan(θ/4) of the radius.
        let handle = 4.0 / 3.0 * math::tan(arc_angle / 4.0);
        let segments = alloc::vec![
            Segment { control1: offset(start, a, 0.0), control2: offset(start, a + b, 0.0), end: arc_start },
            Segment {
                control1: offset(arc_start, (r - d) * handle, (r - arc_side - d) * handle),
                control2: offset(arc_end, -(r - arc_side - d) * handle, -(r - d) * handle),
                end: arc_end,
            },
            Segment {
                control1: offset(arc_end, d, c),
                control2: offset(arc_end, d, b + c),
                end: offset(arc_end, d, a + b + c),
            },
        ];
        Self { start, segments }
    }

    #[cfg(test)]
    fn end(&self) -> Point {
        self.segments.last().map_or(self.start, |segment| segment.end)
    }

    fn append_to(&self, elements: &mut Vec<Element>) {
        elements.extend(self.segments.iter().map(|s| Element::Curve {
            control1: s.control1,
            control2: s.control2,
            end: s.end,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    fn points(elements: &[Element]) -> Vec<Point> {
        elements
            .iter()
            .filter_map(|e| match *e {
                Element::Move(p) | Element::Line(p) | Element::Curve { end: p, .. } => Some(p),
                Element::Close => None,
            })
            .collect()
    }

    #[test]
    fn a_corner_runs_from_p_before_the_vertex_to_p_after_it() {
        for s in [0.0, 0.3, 0.6, 1.0] {
            let corner = Corner::new(Point::new(10.0, 10.0), (1.0, 0.0), (0.0, -1.0), 4.0, s);
            let p = (1.0 + s) * 4.0;
            assert!(close(corner.start, Point::new(10.0 - p, 10.0)), "smoothing {s}");
            assert!(close(corner.end(), Point::new(10.0, 10.0 - p)), "smoothing {s}");
        }
    }

    #[test]
    fn the_outline_stays_inside_its_rect_and_closes() {
        let rect = Rect::new(10.0, 20.0, 200.0, 120.0);
        let elements = rounded_rect(rect, 16.0, 0.6);
        let pts = points(&elements);
        assert!(pts.iter().all(|p| p.x >= 10.0 - 1e-9 && p.x <= 210.0 + 1e-9 && p.y >= 20.0 - 1e-9 && p.y <= 140.0 + 1e-9));
        assert!(close(*pts.last().unwrap(), pts[0]));
        assert_eq!(elements.last(), Some(&Element::Close));
        // Each edge's midpoint is on the outline.
        for mid in [Point::new(110.0, 20.0), Point::new(210.0, 80.0), Point::new(110.0, 140.0), Point::new(10.0, 80.0)] {
            let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
            let on_edge = pts.windows(2).any(|w| {
                (same(w[0].x, mid.x) && same(w[1].x, mid.x) && (w[0].y.min(w[1].y)..=w[0].y.max(w[1].y)).contains(&mid.y))
                    || (same(w[0].y, mid.y)
                        && same(w[1].y, mid.y)
                        && (w[0].x.min(w[1].x)..=w[0].x.max(w[1].x)).contains(&mid.x))
            });
            assert!(on_edge, "{mid:?}");
        }
    }

    #[test]
    fn small_rects_shrink_their_corners() {
        let pts = points(&rounded_rect(Rect::new(0.0, 0.0, 30.0, 6.0), 16.0, 0.6));
        assert!(pts.iter().all(|p| p.y >= -1e-9 && p.y <= 6.0 + 1e-9 && p.x >= -1e-9 && p.x <= 30.0 + 1e-9));
        assert!(rounded_rect(Rect::new(0.0, 0.0, 0.0, 10.0), 8.0, 0.6).is_empty());
    }
}
