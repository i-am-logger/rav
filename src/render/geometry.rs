//! Where the bars, caps and backdrop go, in continuous coordinates.
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
//! were never eighths of anything - `value * height` is the answer.
//!
//! Origin is top-left with y increasing downward, which is what every raster
//! target expects. The bars still grow upward; that is what `y = height - h`
//! below is doing.

/// The area being drawn into, in device pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }
}

/// An axis-aligned rectangle in device pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    /// The lowest edge, which for a bar sitting on the floor is the viewport's.
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Whether this rectangle overlaps `other` at all.
    ///
    /// Touching edges do not count: a cap resting exactly on top of a bar shares
    /// an edge and is not overlapping it.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// Bar width and the gap beside it, in device pixels.
///
/// The same two numbers the terminal layout carries, in a unit that does not
/// have to be a whole cell. That is what lets one `+`/`-` setting drive both
/// surfaces: the terminal reads them as columns, a pixel surface as pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub bar: f32,
    pub gap: f32,
}

impl Layout {
    pub fn new(bar: f32, gap: f32) -> Self {
        Self {
            bar: bar.max(1.0),
            gap: gap.max(0.0),
        }
    }

    /// How many bars fit across `width`. The last one needs no trailing gap.
    pub fn count(&self, width: f32) -> usize {
        if width <= 0.0 {
            return 0;
        }
        (((width + self.gap) / (self.bar + self.gap)).floor() as usize).max(1)
    }

    /// Left edge of bar `i`.
    pub fn x_of(&self, i: usize) -> f32 {
        i as f32 * (self.bar + self.gap)
    }
}

/// The lit part of each bar, as rectangles.
///
/// `values` are 0..=1, already through the ballistics. A value of 0 produces a
/// rectangle of zero height rather than being skipped, so the caller can rely on
/// one rectangle per bar and index them alongside the colours.
pub fn bars(values: &[f32], layout: &Layout, view: &Viewport) -> Vec<Rect> {
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let h = v.clamp(0.0, 1.0) * view.height;
            Rect {
                x: layout.x_of(i),
                // Bars grow upward from the floor, so the top edge moves.
                y: view.height - h,
                w: layout.bar,
                h,
            }
        })
        .collect()
}

/// The peak caps, as rectangles.
///
/// `thickness` is in device pixels. A cap is drawn *above* the level it marks,
/// so it never covers the bar it belongs to, and it is held inside the viewport
/// at both ends: at full scale it would otherwise sit half off the top, and at
/// rest it would sit half below the floor.
pub fn caps(peaks: &[f32], thickness: f32, layout: &Layout, view: &Viewport) -> Vec<Rect> {
    let thickness = thickness.clamp(1.0, view.height.max(1.0));
    // A viewport shorter than the cap leaves nowhere to put it, and `clamp`
    // panics outright when its lower bound exceeds its upper - so the range is
    // collapsed to zero rather than inverted.
    let lowest = (view.height - thickness).max(0.0);
    peaks
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let level = p.clamp(0.0, 1.0) * view.height;
            let y = (view.height - level - thickness).clamp(0.0, lowest);
            Rect {
                x: layout.x_of(i),
                y,
                w: layout.bar,
                h: thickness,
            }
        })
        .collect()
}

/// The unlit backdrop behind each bar: the full column.
///
/// A separate layer rather than a hole left in the bars. In a terminal the cap
/// and the backdrop compete for one cell, so drawing the cap destroys the grid
/// glyph under it; here they are two rectangles and the cap simply sits on top.
pub fn backdrop(count: usize, layout: &Layout, view: &Viewport) -> Vec<Rect> {
    (0..count)
        .map(|i| Rect {
            x: layout.x_of(i),
            y: 0.0,
            w: layout.bar,
            h: view.height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(w: f32, h: f32) -> Viewport {
        Viewport::new(w, h)
    }

    #[test]
    fn a_bar_is_its_value_times_the_height() {
        let v = view(100.0, 60.0);
        let l = Layout::new(3.0, 1.0);
        let rects = bars(&[0.0, 0.5, 1.0], &l, &v);
        assert_eq!(rects[0].h, 0.0);
        assert_eq!(rects[1].h, 30.0);
        assert_eq!(rects[2].h, 60.0);
        // Growing upward: a full bar starts at the top, an empty one at the floor.
        assert_eq!(rects[2].y, 0.0);
        assert_eq!(rects[0].y, 60.0);
    }

    #[test]
    fn the_eighth_ladder_is_even_at_every_height() {
        // The defect in #63, asserted dead. WezTerm fills a block from
        // `(8-n)*h/8` and snaps to whole pixels, so a 60px cell steps 7,8,7,8.
        // Nothing rounds here, so the steps are equal at every height - including
        // the ones that are not divisible by eight.
        let l = Layout::new(3.0, 1.0);
        for height in [7.0, 15.0, 20.0, 29.0, 40.0, 59.0, 60.0, 149.0] {
            let v = view(10.0, height);
            let eighths: Vec<f32> = (0..=8).map(|n| n as f32 / 8.0).collect();
            let rects = bars(&eighths, &l, &v);
            let steps: Vec<f32> = rects.windows(2).map(|w| w[1].h - w[0].h).collect();
            let first = steps[0];
            for (i, step) in steps.iter().enumerate() {
                assert!(
                    (step - first).abs() < 1e-4,
                    "height {height}: step {i} was {step}, expected {first}"
                );
            }
        }
    }

    #[test]
    fn a_cap_never_sinks_below_the_bar_it_marks() {
        // In a cell grid the cap and the bar contend for the same row, and the
        // fix is a fudge that lifts the cap when the top cell is partly filled.
        // Here the cap simply starts at or above the bar's top edge.
        //
        // At full scale it rests *on* the bar rather than above it - there is no
        // room above - which is the terminal renderer's behaviour too, and the
        // reason this asserts the top edge rather than the bottom.
        let v = view(100.0, 60.0);
        let l = Layout::new(3.0, 1.0);
        for value in [0.0f32, 0.13, 0.5, 0.87, 1.0] {
            let bar = bars(&[value], &l, &v)[0];
            let cap = caps(&[value], 2.0, &l, &v)[0];
            assert!(
                cap.y <= bar.y + 1e-4,
                "value {value}: cap top {} sank below bar top {}",
                cap.y,
                bar.y
            );
        }
        // Below full scale it clears the bar entirely, so the two never merge
        // into one block.
        let bar = bars(&[0.5], &l, &v)[0];
        let cap = caps(&[0.5], 2.0, &l, &v)[0];
        assert!(cap.bottom() <= bar.y + 1e-4, "a mid bar must show its cap");
    }

    #[test]
    fn a_cap_stays_inside_the_viewport_at_both_ends() {
        let v = view(100.0, 60.0);
        let l = Layout::new(3.0, 1.0);
        // At rest it sits on the floor rather than half below it, which is how
        // a silent band still shows where it would rise from.
        let resting = caps(&[0.0], 3.0, &l, &v)[0];
        assert_eq!(resting.bottom(), 60.0);
        // At full scale it sits under the ceiling rather than half above it.
        let full = caps(&[1.0], 3.0, &l, &v)[0];
        assert_eq!(full.y, 0.0);
    }

    #[test]
    fn a_cap_composites_over_the_backdrop_instead_of_replacing_it() {
        // The defect that cannot be fixed in a terminal: a cell holds one glyph,
        // so writing the cap destroys the backdrop under it. As geometry they
        // are two rectangles in the same column, and the cap overlaps the
        // backdrop without consuming it.
        let v = view(100.0, 60.0);
        let l = Layout::new(3.0, 1.0);
        let grid = backdrop(1, &l, &v)[0];
        let cap = caps(&[0.5], 2.0, &l, &v)[0];
        assert!(
            cap.intersects(&grid),
            "the cap should sit over the backdrop"
        );
        assert_eq!(grid.h, 60.0, "the backdrop is still the whole column");
        assert_eq!(grid.y, 0.0);
    }

    #[test]
    fn bar_height_never_decreases_as_the_value_rises() {
        let v = view(100.0, 37.0);
        let l = Layout::new(3.0, 1.0);
        let values: Vec<f32> = (0..=100).map(|i| i as f32 / 100.0).collect();
        let rects = bars(&values, &l, &v);
        for pair in rects.windows(2) {
            assert!(pair[1].h >= pair[0].h, "{} then {}", pair[0].h, pair[1].h);
        }
    }

    #[test]
    fn values_outside_the_range_are_held_at_the_edges() {
        // The ballistics clamp, but a bar that drew past the viewport would
        // corrupt neighbouring rows rather than merely look wrong.
        let v = view(100.0, 60.0);
        let l = Layout::new(3.0, 1.0);
        let rects = bars(&[-0.5, 1.5], &l, &v);
        assert_eq!(rects[0].h, 0.0);
        assert_eq!(rects[1].h, 60.0);
        let capped = caps(&[-0.5, 1.5], 2.0, &l, &v);
        assert!(capped.iter().all(|c| c.y >= 0.0 && c.bottom() <= 60.0));
    }

    #[test]
    fn bars_are_laid_out_edge_to_edge_with_their_gap() {
        let l = Layout::new(3.0, 1.0);
        assert_eq!(l.x_of(0), 0.0);
        assert_eq!(l.x_of(1), 4.0);
        assert_eq!(l.x_of(2), 8.0);
        // The last bar needs no trailing gap, so 100 wide holds 25 of them.
        assert_eq!(l.count(100.0), 25);
        assert_eq!(l.count(0.0), 0);
    }

    #[test]
    fn a_wider_bar_means_fewer_of_them() {
        // The property `+`/`-` relies on, in pixels rather than cells.
        let narrow = Layout::new(2.0, 1.0).count(100.0);
        let wide = Layout::new(8.0, 1.0).count(100.0);
        assert!(wide < narrow, "{narrow} then {wide}");
    }

    #[test]
    fn a_zero_height_viewport_produces_nothing_that_panics() {
        let v = view(0.0, 0.0);
        let l = Layout::new(3.0, 1.0);
        assert!(bars(&[0.5], &l, &v).iter().all(|r| r.h == 0.0));
        assert!(caps(&[0.5], 2.0, &l, &v).iter().all(|r| r.y >= 0.0));
    }
}
