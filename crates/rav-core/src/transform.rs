//! Where a shape ends up, when it is not simply where it was laid out.
//!
//! rav lays a bar out as an axis-aligned [`Rectangle`]: so far across, so far
//! down, standing on the floor. That is the right shape for a terminal and it
//! is the only shape a terminal can draw. Every other surface rav is heading
//! towards can do more - a panel raked away from a driver, a spectrum receding
//! into a corridor, a strip of lights wrapped round a cylinder - and all of
//! those are one matrix away from the layout that already exists.
//!
//! # Why a 4x4 of `f32`, and not a library
//!
//! `rav-core` runs on a microcontroller with no allocator, and a transform has
//! to be affordable per frame there as well as on a display. Sixteen floats is
//! nothing: composing two is 64 multiplies, and applying one to the four
//! corners of a bar is 64 more. A general tensor library is not, and would
//! bring an allocator with it.
//!
//! # The convention, since half of them are wrong somewhere
//!
//! Column vectors: a point is transformed as `M * p`, so
//! [`then`](Transform::then) composes left to right in the order things happen.
//! Storage is row-major, `m[row][column]`, which is how the entries are written
//! down in every reference this was checked against.
//!
//! Screen axes, so `x` runs right and `y` runs **down** - the same sense
//! [`Rectangle`] uses, because a transform that disagreed with the layout about
//! which way is down would be a sign error waiting in every skin. `z` runs away
//! from the viewer, so a larger `z` is farther off.
//!
//! # Nothing here decides anything
//!
//! A transform says where a shape goes, never what it looks like: no colour, no
//! opacity, no shape of its own. A surface that cannot honour one is free to
//! ignore it and draw the layout it was given, which is exactly what the glyph
//! renderer does - a cell grid has no room for a bar that leans.

use crate::geometry::Rectangle;
use crate::math::{cos, sin};
use crate::units::Length;

/// A place in the space a scene is laid out in.
///
/// Continuous, where `embedded_graphics::Point` is whole pixels. That is not a
/// preference: a bar edge landing where the arithmetic puts it rather than on a
/// pixel boundary is the whole argument for drawing pixels at all, and snapping
/// is the mechanism behind the uneven ladder in #63. The rounding happens once,
/// in a rasteriser, at the end.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: Length,
    pub y: Length,
    /// Depth, away from the viewer.
    ///
    /// Everything rav lays out today is at zero - a terminal is flat and every
    /// bar stands on the same plane. It is here because a transform that cannot
    /// move a point back cannot make one recede either.
    pub z: Length,
}

impl Point {
    /// On the plane everything is laid out on.
    pub const fn flat(x: Length, y: Length) -> Self {
        Self {
            x,
            y,
            z: Length::NONE,
        }
    }
}

/// Four corners, in the order a rectangle's are read: top-left, top-right,
/// bottom-right, bottom-left.
///
/// What a [`Rectangle`] becomes once it has been through a transform, because
/// almost nothing survives one as a rectangle - a turn alone leaves a
/// parallelogram, and a perspective leaves a trapezium. Surfaces that can only
/// draw rectangles never see one of these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub corners: [Point; 4],
}

impl Quad {
    /// The corners of a rectangle, untransformed.
    pub fn of(area: Rectangle) -> Self {
        let (left, top) = (area.left(), area.top());
        let (right, bottom) = (area.right(), area.bottom());
        Self {
            corners: [
                Point::flat(left, top),
                Point::flat(right, top),
                Point::flat(right, bottom),
                Point::flat(left, bottom),
            ],
        }
    }
}

/// Where a shape goes: a 4x4 of `f32`, row-major, applied as `M * p`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    rows: [[f32; 4]; 4],
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Leaves everything where the layout put it.
    pub const IDENTITY: Self = Self {
        rows: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Build one from its entries, row by row.
    pub const fn from_rows(rows: [[f32; 4]; 4]) -> Self {
        Self { rows }
    }

    pub const fn rows(&self) -> &[[f32; 4]; 4] {
        &self.rows
    }

    /// Whether this would move anything at all.
    ///
    /// Exactly, not nearly: the question a surface asks is "did anyone set one
    /// of these", so that it can take the path it has always taken and produce
    /// the picture it has always produced. A tolerance here would mean a nearly
    /// flat skin silently drawing as a flat one.
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// Shift by a distance on each axis.
    pub const fn moving(x: f32, y: f32, z: f32) -> Self {
        Self::from_rows([
            [1.0, 0.0, 0.0, x],
            [0.0, 1.0, 0.0, y],
            [0.0, 0.0, 1.0, z],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Stretch about the origin.
    pub const fn scaling(x: f32, y: f32, z: f32) -> Self {
        Self::from_rows([
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Turn about the vertical, so the field swings to face away.
    ///
    /// A panel bolted flat to a speaker grille, seen from off to one side.
    pub fn turning(radians: f32) -> Self {
        let (s, c) = (sin(radians), cos(radians));
        Self::from_rows([
            [c, 0.0, s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Tip about the horizontal, so the far edge falls away.
    ///
    /// A dashboard raked back from the driver.
    pub fn leaning(radians: f32) -> Self {
        let (s, c) = (sin(radians), cos(radians));
        Self::from_rows([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, -s, 0.0],
            [0.0, s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Spin in the plane of the screen.
    pub fn rolling(radians: f32) -> Self {
        let (s, c) = (sin(radians), cos(radians));
        Self::from_rows([
            [c, -s, 0.0, 0.0],
            [s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Make depth read as depth: the farther off, the smaller.
    ///
    /// `eye` is how far in front of the plane the viewer stands, in the same
    /// unit everything else is in. Large values approach a flat drawing; small
    /// ones exaggerate. Zero or less is refused, because an eye on the plane it
    /// is looking at has nothing to look at - it comes back as
    /// [`IDENTITY`](Self::IDENTITY), so a skin with a nonsense number draws flat
    /// rather than draws nothing.
    ///
    /// Shrinking is toward the origin, so a scene wanting it toward the middle
    /// of the picture composes a move there and back - which is what a stack is
    /// for, and is not assumed here because only the caller knows where the
    /// middle of its picture is.
    pub fn receding(eye: f32) -> Self {
        // NaN spelled out rather than left to `!(eye > 0.0)`: it is the case a
        // skin file reaches by arithmetic rather than by typing, and the
        // negated comparison that catches it reads like a mistake.
        if eye.is_nan() || eye <= 0.0 {
            return Self::IDENTITY;
        }
        Self::from_rows([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0 / eye, 1.0],
        ])
    }

    /// This one, and then the other.
    ///
    /// Reads in the order things happen - `turning(a).then(receding(d))` turns
    /// the field and then looks at it from a distance - which is the opposite
    /// order to how the multiplication is written, and the reason this exists
    /// rather than an operator.
    pub fn then(self, next: Self) -> Self {
        let mut rows = [[0.0f32; 4]; 4];
        for (r, row) in rows.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| next.rows[r][k] * self.rows[k][c]).sum();
            }
        }
        Self { rows }
    }

    /// Where a point ends up.
    ///
    /// `None` when the perspective divide has nothing to say: a point level
    /// with the eye or behind it has no place on the picture, and inventing one
    /// puts a bar in the reflection of where it should be. A caller that gets
    /// `None` should draw nothing rather than draw something wrong.
    pub fn apply(&self, point: Point) -> Option<Point> {
        let p = [point.x.get(), point.y.get(), point.z.get(), 1.0];
        let out = |row: usize| (0..4).map(|k| self.rows[row][k] * p[k]).sum::<f32>();
        let (x, y, z, w) = (out(0), out(1), out(2), out(3));
        if !w.is_finite() || w <= 0.0 {
            return None;
        }
        let point = Point {
            x: Length(x / w),
            y: Length(y / w),
            z: Length(z / w),
        };
        point
            .x
            .get()
            .is_finite()
            .then_some(point)
            .filter(|p| p.y.get().is_finite() && p.z.get().is_finite())
    }

    /// Where a laid-out rectangle ends up.
    ///
    /// `None` if any corner does, because three corners of a bar is not a bar.
    pub fn quad(&self, area: Rectangle) -> Option<Quad> {
        let mut corners = [Point::default(); 4];
        for (out, corner) in corners.iter_mut().zip(Quad::of(area).corners) {
            *out = self.apply(corner)?;
        }
        Some(Quad { corners })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOSE: f32 = 1e-4;

    fn near(a: Length, b: f32) -> bool {
        (a.get() - b).abs() < CLOSE
    }

    fn area() -> Rectangle {
        Rectangle::new(Length(10.0), Length(20.0), Length(30.0), Length(40.0))
    }

    #[test]
    fn identity_is_the_only_thing_a_surface_has_to_recognise() {
        // A surface takes the path it has always taken when nothing set a
        // transform, which is what keeps today's picture exactly today's
        // picture. Exact, so a nearly-flat skin cannot be mistaken for a flat
        // one and silently lose its lean.
        assert!(Transform::IDENTITY.is_identity());
        assert!(Transform::default().is_identity());
        assert!(!Transform::moving(0.0, 0.001, 0.0).is_identity());
        assert!(!Transform::rolling(0.001).is_identity());
    }

    #[test]
    fn a_point_comes_back_where_it_started_through_identity() {
        let there = Transform::IDENTITY
            .apply(Point::flat(Length(3.0), Length(4.0)))
            .unwrap();
        assert_eq!(there, Point::flat(Length(3.0), Length(4.0)));
    }

    #[test]
    fn then_reads_in_the_order_things_happen() {
        // The reason this is a method and not an operator: the multiplication
        // is written the other way round, and a skin author thinks in the order
        // the moves are made.
        let moved_then_doubled =
            Transform::moving(1.0, 0.0, 0.0).then(Transform::scaling(2.0, 2.0, 2.0));
        let there = moved_then_doubled
            .apply(Point::flat(Length::NONE, Length::NONE))
            .unwrap();
        assert!(near(there.x, 2.0), "moved by one, then doubled: {there:?}");

        let doubled_then_moved =
            Transform::scaling(2.0, 2.0, 2.0).then(Transform::moving(1.0, 0.0, 0.0));
        let there = doubled_then_moved
            .apply(Point::flat(Length::NONE, Length::NONE))
            .unwrap();
        assert!(near(there.x, 1.0), "doubled, then moved by one: {there:?}");
    }

    #[test]
    fn a_roll_turns_the_picture_in_its_own_plane() {
        // Screen sense: `y` is down, so a quarter turn takes the point on the
        // positive `x` axis to the one below the origin, not above it. A sign
        // error here leans every skin the wrong way and looks deliberate.
        let there = Transform::rolling(core::f32::consts::FRAC_PI_2)
            .apply(Point::flat(Length(1.0), Length::NONE))
            .unwrap();
        assert!(near(there.x, 0.0) && near(there.y, 1.0), "{there:?}");
    }

    #[test]
    fn receding_makes_the_far_edge_smaller_and_the_near_one_alone() {
        // What "depth" has to come to on a flat surface. The plane everything
        // is laid out on is untouched, so a scene that sets a perspective and
        // leaves every bar at zero depth draws exactly what it drew before.
        let eye = Transform::receding(100.0);
        let near_edge = eye.apply(Point::flat(Length(50.0), Length(50.0))).unwrap();
        assert!(near(near_edge.x, 50.0) && near(near_edge.y, 50.0));

        let far = eye
            .apply(Point {
                x: Length(50.0),
                y: Length(50.0),
                z: Length(100.0),
            })
            .unwrap();
        assert!(near(far.x, 25.0) && near(far.y, 25.0), "{far:?}");
    }

    #[test]
    fn an_eye_on_the_plane_it_looks_at_draws_flat_rather_than_nothing() {
        // A skin carrying a nonsense number is a picture in the wrong shape,
        // not a reason to have no picture. Division by zero is the alternative.
        assert!(Transform::receding(0.0).is_identity());
        assert!(Transform::receding(-1.0).is_identity());
        assert!(Transform::receding(f32::NAN).is_identity());
    }

    #[test]
    fn a_point_at_or_behind_the_eye_has_no_place_on_the_picture() {
        // Inventing one puts the bar in the reflection of where it belongs,
        // which reads as a glitch rather than as depth.
        let eye = Transform::receding(10.0);
        let behind = Point {
            x: Length(1.0),
            y: Length(1.0),
            z: Length(-10.0),
        };
        assert!(eye.apply(behind).is_none());
        assert!(
            eye.quad(Rectangle::new(
                Length(1.0),
                Length(1.0),
                Length(1.0),
                Length(1.0)
            ))
            .is_some(),
            "a bar on the plane is always drawable",
        );
    }

    #[test]
    fn a_rectangle_keeps_its_corner_order_through_a_transform() {
        // Top-left, top-right, bottom-right, bottom-left - the order a
        // rasteriser walks to close a path. Scrambling it draws a bow tie.
        let plain = Quad::of(area());
        assert_eq!(plain.corners[0], Point::flat(Length(10.0), Length(20.0)));
        assert_eq!(plain.corners[1], Point::flat(Length(40.0), Length(20.0)));
        assert_eq!(plain.corners[2], Point::flat(Length(40.0), Length(60.0)));
        assert_eq!(plain.corners[3], Point::flat(Length(10.0), Length(60.0)));

        let same = Transform::IDENTITY.quad(area()).unwrap();
        assert_eq!(same, plain, "identity moved a corner");
    }

    #[test]
    fn a_turn_leaves_a_shape_no_rectangle_can_hold() {
        // The reason a transformed rectangle is a `Quad` and not a `Rectangle`:
        // once the field turns, the two vertical edges are at different depths
        // and a perspective makes them different lengths. There is no
        // axis-aligned box that is this shape.
        let turned = Transform::turning(0.6).then(Transform::receding(200.0));
        let quad = turned.quad(area()).unwrap();
        let left_edge = (quad.corners[3].y - quad.corners[0].y).get();
        let right_edge = (quad.corners[2].y - quad.corners[1].y).get();
        assert!(
            (left_edge - right_edge).abs() > 1.0,
            "the two sides came out the same length: {left_edge} and {right_edge}",
        );
    }
}
