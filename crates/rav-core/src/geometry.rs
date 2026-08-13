//! Where the bars, caps and backdrop go.
//!
//! The terminal renderer works in cells, so every measurement is rounded to one
//! before it is drawn: a bar is a whole number of rows plus an eighth, a cap
//! sits at one of two positions inside its row. That rounding is not a detail of
//! the drawing - it is visible. An eighth of a 60-pixel cell is 7.5 pixels, and
//! since a boundary has to land on a pixel the ladder comes out 7, 8, 7, 8,
//! which is the seam between bar levels in issue #63.
//!
//! Here a bar is a fraction of the height and nothing rounds until the
//! rasteriser antialiases it. The eighths are even at every size because they
//! were never eighths of anything - `height * level` is the answer.
//!
//! Everything is measured in [`Length`] and driven by [`Level`], so a caller
//! cannot pass a column count where a distance belongs, or an unnormalised
//! magnitude where a bar height belongs. Both were possible when these were bare
//! `f32`s, and both produce a wrong picture rather than a failed build.
//!
//! The placing rules live on [`Column`] rather than in the caller, so drawing a
//! frame is asking each column what its bar and cap are - not arithmetic spread
//! across whichever surface happens to be drawing.
//!
//! Origin is top-left with the vertical axis growing downward, which is what
//! every raster target expects. Bars still grow upward; that is what
//! `height - risen` below is doing.

use crate::units::{Length, Level};

/// The area being drawn into.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    width: Length,
    height: Length,
}

impl Screen {
    pub fn new(width: Length, height: Length) -> Self {
        Self {
            width: width.or_none(),
            height: height.or_none(),
        }
    }

    pub fn width(&self) -> Length {
        self.width
    }

    pub fn height(&self) -> Length {
        self.height
    }

    /// A screen with no area draws nothing - a terminal mid-resize, not a bug.
    pub fn is_empty(&self) -> bool {
        self.width <= Length::NONE || self.height <= Length::NONE
    }

    /// How far above the floor a level stands.
    pub fn risen_to(&self, level: Level) -> Length {
        self.height * level
    }

    /// The whole screen.
    pub fn bounds(&self) -> Rectangle {
        Rectangle::new(Length::NONE, Length::NONE, self.width, self.height)
    }
}

/// An axis-aligned rectangle on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectangle {
    left: Length,
    top: Length,
    width: Length,
    height: Length,
}

impl Rectangle {
    pub fn new(left: Length, top: Length, width: Length, height: Length) -> Self {
        Self {
            left,
            top,
            width: width.or_none(),
            height: height.or_none(),
        }
    }

    pub fn left(&self) -> Length {
        self.left
    }

    pub fn top(&self) -> Length {
        self.top
    }

    pub fn width(&self) -> Length {
        self.width
    }

    pub fn height(&self) -> Length {
        self.height
    }

    pub fn right(&self) -> Length {
        self.left + self.width
    }

    /// The lowest edge, which for a bar standing on the floor is the screen's.
    pub fn bottom(&self) -> Length {
        self.top + self.height
    }

    /// Nothing to draw. A bar at rest is this, every frame of a silent passage,
    /// so it is an ordinary state rather than an error.
    pub fn is_empty(&self) -> bool {
        self.width <= Length::NONE || self.height <= Length::NONE
    }

    /// Whether the two overlap at all.
    ///
    /// Touching edges do not count: a cap resting exactly on top of a bar shares
    /// an edge with it and is not overlapping it.
    pub fn overlaps(&self, other: &Rectangle) -> bool {
        self.left < other.right()
            && other.left < self.right()
            && self.top < other.bottom()
            && other.top < self.bottom()
    }

    /// Whether this sits entirely above `other`, sharing an edge at most.
    pub fn sits_above(&self, other: &Rectangle) -> bool {
        self.bottom() <= other.top
    }

    /// The part of this rectangle lying between two heights, if any.
    ///
    /// What clipping a bar to a colour band is, so neither the rasteriser nor a
    /// surface has to do it with `max` and `min` and get the empty case wrong.
    pub fn between(&self, top: Length, bottom: Length) -> Option<Rectangle> {
        let top = top.largest(self.top);
        let bottom = bottom.smallest(self.bottom());
        (bottom > top).then(|| Rectangle::new(self.left, top, self.width, bottom - top))
    }
}

/// How wide bars are and how far apart they sit.
///
/// The same two numbers the terminal layout carries, in a unit that does not
/// have to be a whole cell. That is what lets one `+`/`-` setting drive every
/// surface: the terminal reads them as columns, a pixel surface as distances.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarLayout {
    width: Length,
    gap: Length,
}

impl BarLayout {
    pub fn new(width: Length, gap: Length) -> Self {
        Self {
            width: width.largest(Length(1.0)),
            gap: gap.or_none(),
        }
    }

    pub fn bar_width(&self) -> Length {
        self.width
    }

    pub fn gap(&self) -> Length {
        self.gap
    }

    /// How many bars fit across a screen. The last one needs no trailing gap.
    pub fn fitting_across(&self, screen: &Screen) -> usize {
        if screen.width() <= Length::NONE {
            return 0;
        }
        let stride = self.width + self.gap;
        (crate::math::floor((screen.width() + self.gap).get() / stride.get()) as usize).max(1)
    }

    /// The slot the band at `index` occupies.
    pub fn column(&self, index: usize) -> Column {
        Column {
            left: (self.width + self.gap) * index as f32,
            width: self.width,
            anchor: Anchor::default(),
        }
    }

    /// Every slot that fits, in order.
    pub fn columns(&self, screen: &Screen) -> impl Iterator<Item = Column> + '_ {
        (0..self.fitting_across(screen)).map(|index| self.column(index))
    }
}

/// Which edge a bar grows from.
///
/// One knob, and the classic arrangements fall out of it: a stereo pair with the
/// left channel on `Floor` and the right on `Ceiling` is the mirrored look on a
/// car dashboard or a speaker grille, and `Middle` is the symmetric butterfly
/// that reads fastest at a glance because the eye catches a symmetric shape
/// sooner than a one-sided one.
///
/// Stacked and interleaved arrangements need nothing here: stacked is two
/// panels, and interleaved is which column a band is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    /// Grows upward from the bottom. What a spectrum analyser has always done.
    #[default]
    Floor,
    /// Grows downward from the top - the other half of a mirrored pair.
    Ceiling,
    /// Grows both ways from the centre line, half in each direction.
    Middle,
}

/// The vertical slot one frequency band occupies, and the rules for placing
/// things in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    left: Length,
    width: Length,
    anchor: Anchor,
}

impl Column {
    pub fn left(&self) -> Length {
        self.left
    }

    pub fn width(&self) -> Length {
        self.width
    }

    /// The far edge, so asking whether a column fits is asking the column.
    pub fn right(&self) -> Length {
        self.left + self.width
    }

    pub fn anchor(&self) -> Anchor {
        self.anchor
    }

    /// The same slot, growing from a different edge.
    pub fn anchored(self, anchor: Anchor) -> Self {
        Self { anchor, ..self }
    }

    /// The lit part of the bar.
    ///
    /// A silent band gives a rectangle of no height rather than nothing at all,
    /// so a caller can rely on one per band and index them alongside colours.
    pub fn bar(&self, level: Level, screen: &Screen) -> Rectangle {
        let risen = screen.risen_to(level);
        let top = match self.anchor {
            // Growing upward from the floor, so it is the top edge that moves.
            Anchor::Floor => screen.height() - risen,
            // Growing downward, so the top edge is fixed and the bottom moves.
            Anchor::Ceiling => Length::NONE,
            // Half each way from the centre line, so both edges move together.
            Anchor::Middle => (screen.height() - risen) / 2.0,
        };
        Rectangle::new(self.left, top, self.width, risen)
    }

    /// The peak cap.
    ///
    /// Drawn just *beyond* the level it marks, so it never covers the bar it
    /// belongs to, and held inside the screen at both ends: at full scale it
    /// would otherwise sit half off the far edge, and at rest half outside the
    /// near one.
    ///
    /// "Beyond" follows the anchor - above a bar growing up, below one growing
    /// down. A `Middle` bar grows both ways and this marks its upper extent; a
    /// surface wanting both mirrors it, rather than this returning two things
    /// most callers would throw one of away.
    pub fn cap(&self, peak: Level, thickness: Length, screen: &Screen) -> Rectangle {
        let thickness = thickness.held_between(Length(1.0), screen.height().largest(Length(1.0)));
        // A screen shorter than the cap leaves nowhere to put it; `held_between`
        // collapses rather than panicking as `f32::clamp` would.
        let furthest = (screen.height() - thickness).or_none();
        let risen = screen.risen_to(peak);
        let top = match self.anchor {
            Anchor::Floor => screen.height() - risen - thickness,
            Anchor::Ceiling => risen,
            Anchor::Middle => (screen.height() - risen) / 2.0 - thickness,
        }
        .held_between(Length::NONE, furthest);
        // Clamping the top is not enough to keep the cap on the screen, because
        // `(height - thickness) + thickness` is not always `height` in `f32`.
        // A property test found it: a cap of 7.7606773 on a display 73.27264
        // tall landed three millionths of a pixel below the floor. Nothing
        // anyone could see, and still a rectangle that is not where this method
        // promises every rectangle it returns will be - so the thickness gives
        // way rather than the promise.
        let thickness = thickness.smallest(screen.height() - top);
        Rectangle::new(self.left, top, self.width, thickness)
    }

    /// The unlit backdrop: the whole column.
    ///
    /// Its own layer rather than a hole left in the bars. In a terminal the cap
    /// and the backdrop compete for one cell, so drawing the cap destroys the
    /// grid glyph beneath it; here they are two rectangles and the cap sits on
    /// top without consuming what is under it.
    pub fn backdrop(&self, screen: &Screen) -> Rectangle {
        Rectangle::new(self.left, Length::NONE, self.width, screen.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `no_std` drops the prelude, so a test that collects needs this said.
    use std::vec::Vec;

    fn screen(width: f32, height: f32) -> Screen {
        Screen::new(Length(width), Length(height))
    }

    fn layout(width: f32, gap: f32) -> BarLayout {
        BarLayout::new(Length(width), Length(gap))
    }

    /// The first column, which every placement test uses.
    fn first(width: f32, gap: f32) -> Column {
        layout(width, gap).column(0)
    }

    #[test]
    fn a_bar_stands_as_high_as_its_level() {
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        assert_eq!(column.bar(Level::SILENT, &screen).height(), Length::NONE);
        assert_eq!(column.bar(Level::new(0.5), &screen).height(), Length(30.0));
        assert_eq!(column.bar(Level::FULL, &screen).height(), Length(60.0));
        // Growing upward: a full bar starts at the ceiling, a silent one at the floor.
        assert_eq!(column.bar(Level::FULL, &screen).top(), Length::NONE);
        assert_eq!(column.bar(Level::SILENT, &screen).top(), Length(60.0));
    }

    #[test]
    fn the_eighth_ladder_is_even_at_every_height() {
        // The defect in #63, asserted dead. WezTerm fills a block from
        // `(8-n)*h/8` and snaps to whole pixels, so a 60px cell steps 7,8,7,8.
        // Nothing rounds here, so the steps are equal at every height -
        // including the ones not divisible by eight.
        let column = first(3.0, 1.0);
        for height in [7.0, 15.0, 20.0, 29.0, 40.0, 59.0, 60.0, 149.0] {
            let screen = screen(10.0, height);
            let heights: Vec<f32> = (0..=8)
                .map(|n| {
                    column
                        .bar(Level::new(n as f32 / 8.0), &screen)
                        .height()
                        .get()
                })
                .collect();
            let steps: Vec<f32> = heights.windows(2).map(|pair| pair[1] - pair[0]).collect();
            let first_step = steps[0];
            for (i, step) in steps.iter().enumerate() {
                assert!(
                    (step - first_step).abs() < 1e-4,
                    "height {height}: step {i} was {step}, expected {first_step}"
                );
            }
        }
    }

    #[test]
    fn a_cap_never_sinks_below_the_bar_it_marks() {
        // In a cell grid the cap and the bar contend for one row, and the fix is
        // a fudge that lifts the cap when the top cell is partly filled. Here
        // the cap simply starts at or above the bar's top edge.
        //
        // At full scale it rests *on* the bar rather than above it - there is no
        // room above - which is the terminal renderer's behaviour too, and the
        // reason this asserts the top edge rather than the bottom.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        for value in [0.0f32, 0.13, 0.5, 0.87, 1.0] {
            let level = Level::new(value);
            let bar = column.bar(level, &screen);
            let cap = column.cap(level, Length(2.0), &screen);
            assert!(
                cap.top() <= bar.top() + Length(1e-4),
                "level {value}: cap top {:?} sank below bar top {:?}",
                cap.top(),
                bar.top()
            );
        }
        // Below full scale it clears the bar entirely, so the two never merge
        // into one block.
        let level = Level::new(0.5);
        assert!(
            column
                .cap(level, Length(2.0), &screen)
                .sits_above(&column.bar(level, &screen)),
            "a mid bar must show its cap"
        );
    }

    #[test]
    fn a_cap_stays_inside_the_screen_at_both_ends() {
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        // At rest it sits on the floor rather than half below it, which is how a
        // silent band still shows where it would rise from.
        assert_eq!(
            column.cap(Level::SILENT, Length(3.0), &screen).bottom(),
            Length(60.0)
        );
        // At full scale it sits under the ceiling rather than half above it.
        assert_eq!(
            column.cap(Level::FULL, Length(3.0), &screen).top(),
            Length::NONE
        );
    }

    #[test]
    fn a_cap_stays_inside_the_screen_at_sizes_that_do_not_divide() {
        // The numbers a property test found, kept because nothing anyone would
        // choose by hand goes near this: clamping the top is not enough, since
        // `(height - thickness) + thickness` is not always `height` in `f32`.
        // This one overshot by three millionths of a pixel - invisible, and
        // still a rectangle outside the screen it was asked to be inside.
        let screen = screen(256.0, 73.27264);
        let cap = BarLayout::new(Length(8.0), Length(1.0))
            .column(0)
            .anchored(Anchor::Ceiling)
            .cap(Level::new(0.958_762_6), Length(7.760_677_3), &screen);
        assert!(
            cap.bottom() <= screen.height(),
            "{:?} hangs below a screen {:?} tall",
            cap.bottom(),
            screen.height(),
        );
        assert!(
            cap.top() >= Length::NONE,
            "{:?} is above the top",
            cap.top()
        );
    }

    #[test]
    fn a_cap_lies_over_the_backdrop_instead_of_replacing_it() {
        // The defect that cannot be fixed in a terminal: a cell holds one glyph,
        // so writing the cap destroys the backdrop under it. As geometry they
        // are two rectangles in one column, and the cap overlaps the backdrop
        // without consuming it.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        let backdrop = column.backdrop(&screen);
        let cap = column.cap(Level::new(0.5), Length(2.0), &screen);
        assert!(
            cap.overlaps(&backdrop),
            "the cap should lie over the backdrop"
        );
        assert_eq!(backdrop.height(), Length(60.0), "still the whole column");
        assert_eq!(backdrop.top(), Length::NONE);
    }

    #[test]
    fn an_anchor_moves_a_bar_without_resizing_it() {
        // The property every arrangement rests on: how tall a bar is says how
        // loud the band is, and that must not change with which edge it grows
        // from. Otherwise a mirrored pair would read as two different loudnesses
        // for one signal.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        for value in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let level = Level::new(value);
            let floor = column.bar(level, &screen);
            for anchor in [Anchor::Ceiling, Anchor::Middle] {
                let moved = column.anchored(anchor).bar(level, &screen);
                assert_eq!(
                    moved.height(),
                    floor.height(),
                    "level {value} changed height under {anchor:?}"
                );
                assert_eq!(moved.left(), floor.left(), "and must not move sideways");
            }
        }
    }

    #[test]
    fn a_mirrored_pair_grows_away_from_each_other() {
        // The car-dashboard and speaker-grille look: left on the floor, right on
        // the ceiling. They must meet only when both are at full scale.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        let level = Level::new(0.25);
        let up = column.bar(level, &screen);
        let down = column.anchored(Anchor::Ceiling).bar(level, &screen);

        assert_eq!(
            up.bottom(),
            Length(60.0),
            "the upward bar stands on the floor"
        );
        assert_eq!(
            down.top(),
            Length::NONE,
            "the downward bar hangs from the top"
        );
        assert!(
            down.sits_above(&up),
            "at a quarter scale they must not touch"
        );

        // At full scale each fills the screen, so they coincide rather than
        // overflowing it - the only case where they meet.
        let full_up = column.bar(Level::FULL, &screen);
        let full_down = column.anchored(Anchor::Ceiling).bar(Level::FULL, &screen);
        assert_eq!(full_up, full_down);
        assert_eq!(full_up.height(), Length(60.0));
    }

    #[test]
    fn a_middle_anchored_bar_is_symmetric_about_the_centre_line() {
        // The butterfly look. Equal margins above and below at every level, so
        // the shape stays centred as it grows.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0).anchored(Anchor::Middle);
        for value in [0.0f32, 0.3, 0.5, 0.8, 1.0] {
            let bar = column.bar(Level::new(value), &screen);
            let above = bar.top();
            let below = Length(60.0) - bar.bottom();
            assert!(
                (above.get() - below.get()).abs() < 1e-4,
                "level {value}: {above:?} above but {below:?} below"
            );
        }
    }

    #[test]
    fn a_cap_marks_the_edge_its_bar_grows_towards() {
        // A cap that stayed at the top of a downward bar would mark where the
        // signal is not.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        let level = Level::new(0.5);

        let up = column.anchored(Anchor::Floor);
        assert!(
            up.cap(level, Length(2.0), &screen)
                .sits_above(&up.bar(level, &screen)),
            "an upward bar wears its cap above"
        );

        let down = column.anchored(Anchor::Ceiling);
        assert!(
            down.bar(level, &screen)
                .sits_above(&down.cap(level, Length(2.0), &screen)),
            "a downward bar wears its cap below"
        );
    }

    #[test]
    fn every_anchor_stays_inside_the_screen_at_both_ends() {
        let screen = screen(100.0, 60.0);
        for anchor in [Anchor::Floor, Anchor::Ceiling, Anchor::Middle] {
            let column = first(3.0, 1.0).anchored(anchor);
            for value in [0.0f32, 0.5, 1.0] {
                let level = Level::new(value);
                let bar = column.bar(level, &screen);
                assert!(
                    bar.top() >= Length::NONE && bar.bottom() <= Length(60.0),
                    "{anchor:?}"
                );
                let cap = column.cap(level, Length(3.0), &screen);
                assert!(
                    cap.top() >= Length::NONE && cap.bottom() <= Length(60.0),
                    "{anchor:?}"
                );
            }
        }
    }

    #[test]
    fn a_bar_never_shrinks_as_its_level_rises() {
        let screen = screen(100.0, 37.0);
        let column = first(3.0, 1.0);
        let heights: Vec<Length> = (0..=100)
            .map(|i| column.bar(Level::new(i as f32 / 100.0), &screen).height())
            .collect();
        for pair in heights.windows(2) {
            assert!(pair[1] >= pair[0], "{:?} then {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn a_level_past_full_scale_cannot_draw_past_the_screen() {
        // `Level` clamps on construction, so a magnitude that escaped the
        // ballistics cannot corrupt the rows around it. The type enforces this
        // now, rather than every call site remembering to.
        let screen = screen(100.0, 60.0);
        let column = first(3.0, 1.0);
        assert_eq!(column.bar(Level::new(-0.5), &screen).height(), Length::NONE);
        assert_eq!(column.bar(Level::new(1.5), &screen).height(), Length(60.0));
        let cap = column.cap(Level::new(1.5), Length(2.0), &screen);
        assert!(cap.top() >= Length::NONE && cap.bottom() <= Length(60.0));
    }

    #[test]
    fn columns_sit_edge_to_edge_with_their_gap_between() {
        let layout = layout(3.0, 1.0);
        assert_eq!(layout.column(0).left(), Length::NONE);
        assert_eq!(layout.column(1).left(), Length(4.0));
        assert_eq!(layout.column(2).left(), Length(8.0));
        // The last bar needs no trailing gap, so 100 across holds 25 of them.
        assert_eq!(layout.fitting_across(&screen(100.0, 10.0)), 25);
        assert_eq!(layout.fitting_across(&screen(0.0, 10.0)), 0);
        assert_eq!(layout.columns(&screen(100.0, 10.0)).count(), 25);
    }

    #[test]
    fn a_wider_bar_means_fewer_of_them() {
        // The property `+`/`-` relies on, in distances rather than cells.
        let screen = screen(100.0, 10.0);
        let narrow = layout(2.0, 1.0).fitting_across(&screen);
        let wide = layout(8.0, 1.0).fitting_across(&screen);
        assert!(wide < narrow, "{narrow} then {wide}");
    }

    #[test]
    fn slicing_a_rectangle_between_two_heights_keeps_only_the_overlap() {
        // What clipping a bar to a colour band is. Doing it with `max` and `min`
        // at the call site is how the empty case gets forgotten.
        let bar = Rectangle::new(Length::NONE, Length(20.0), Length(10.0), Length(40.0));
        let middle = bar.between(Length(30.0), Length(50.0)).expect("overlaps");
        assert_eq!(middle.top(), Length(30.0));
        assert_eq!(middle.bottom(), Length(50.0));
        // Clipped to the rectangle's own bounds, never beyond them.
        let all = bar.between(Length::NONE, Length(100.0)).expect("overlaps");
        assert_eq!(all.top(), Length(20.0));
        assert_eq!(all.bottom(), Length(60.0));
        // No overlap is None rather than a rectangle of negative height.
        assert_eq!(bar.between(Length(70.0), Length(80.0)), None);
        assert_eq!(bar.between(Length(20.0), Length(20.0)), None);
    }

    #[test]
    fn a_screen_with_no_area_produces_nothing_that_panics() {
        let screen = screen(0.0, 0.0);
        let column = first(3.0, 1.0);
        assert!(screen.is_empty());
        assert!(column.bar(Level::new(0.5), &screen).is_empty());
        assert!(column.cap(Level::new(0.5), Length(2.0), &screen).top() >= Length::NONE);
    }
}
