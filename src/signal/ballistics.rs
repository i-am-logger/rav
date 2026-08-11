//! Analyser bar and peak-cap motion.
//!
//! The recurrence, per frame, is:
//!
//! ```text
//! bar -= falloff / 16.0                  // falloff selectable; 12 by default
//! if bar <= value { bar = value }         // instant attack
//!
//! if peak <= bar { peak = bar; velocity = 3.0 }
//! peak -= velocity
//! velocity *= peak_falloff                // 1.1 by default
//! ```
//!
//! Two things about it are easy to get wrong.
//!
//! **There is no hold timer.** The cap looks like it hangs at the top, but that is
//! emergent: velocity starts at `3/4096` of full scale and only compounds into
//! visible motion after a few dozen frames. Coding an explicit hold followed by a
//! linear fall gives a visibly different feel.
//!
//! **The cap tracks the bar, not the input.** It is re-seeded from the
//! falloff-smoothed bar, so the two are chained rather than independent.
//!
//! The original works in pixels on a fixed 16px-tall display, with peaks in 1/256px
//! fixed point. rav resizes, so everything here is normalised to `0.0..=1.0` of
//! full scale and the constants are divided down accordingly. Rates are converted
//! from per-frame to per-second via [`Ballistics::reference_fps`], because a
//! geometric decay does not survive a frame-rate change otherwise: `v *= 1.1` at
//! 30fps is not `v *= 1.1` at 60fps, nor `v *= 1.21`.

/// The divisor in the original's `saFalloff -= falloff / 16.0`. This is part of
/// the rate expression, not the display height, and the two must not be
/// conflated - see [`FULL_SCALE_PIXELS`].
const FALLOFF_DIVISOR: f32 = 16.0;

/// Pixel height that counts as full scale.
///
/// Bars clip at 15, and the pipeline normalises so `1.0` means exactly
/// that (`spectrum::MAX_HEIGHT`). The analyser's row count is 16, and it is a
/// different number: converting the pixel-denominated constants with it makes
/// every bar and cap fall 6.7% slow.
const FULL_SCALE_PIXELS: f32 = 15.0;
/// Peaks are stored as 1/256 of a pixel.
const PEAK_FIXED_POINT: f32 = 256.0;
/// Velocity a cap is re-seeded with, in 1/256px per frame.
const PEAK_VELOCITY_SEED: f32 = 3.0;

/// The selectable fall speeds, in pixels-per-frame numerators over 16. Bound to
/// a key; the cap's own multiplier is not, so it stays at its default.
///
/// From `rav-core`, where the numbers belong: they are what a preset carries,
/// and a second copy here would be a second thing to change. This module keeps
/// the integrator - what a level *does* between frames - which is the part that
/// needs somewhere to hold state.
pub use rav_core::ballistics::FALL_SPEEDS as BAR_FALL_SPEEDS;

/// The defaults on a fresh install, read off the preset rav ships with.
pub const DEFAULT_BAR_FALL: f32 = rav_core::ballistics::WINAMP.bar_fall;
pub const DEFAULT_PEAK_FALL: f32 = rav_core::ballistics::WINAMP.peak_fall;

/// Frame rate the per-frame constants are interpreted against.
///
/// The original's vis timer is not documented in the material we have; Webamp drives
/// its painter from `requestAnimationFrame`, i.e. the display rate. 60 reproduces
/// Webamp's behaviour and drops a full-scale cap in ~0.86s; 30 stretches the same
/// curve to ~1.72s. Configurable because it is a genuine choice, not a fact.
pub const DEFAULT_REFERENCE_FPS: f32 = rav_core::ballistics::WINAMP.reference_fps;

/// Per-bar bar and cap motion.
#[derive(Debug, Clone)]
pub struct Ballistics {
    /// Falloff-smoothed bar heights, 0.0..=1.0.
    bars: Vec<f32>,
    /// Cap positions, 0.0..=1.0.
    peaks: Vec<f32>,
    /// Cap velocities in full-scale units per reference frame.
    velocities: Vec<f32>,
    bar_fall: f32,
    peak_fall: f32,
    reference_fps: f32,
}

impl Default for Ballistics {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Ballistics {
    pub fn new(bars: usize) -> Self {
        Self {
            bars: vec![0.0; bars],
            peaks: vec![0.0; bars],
            velocities: vec![0.0; bars],
            bar_fall: DEFAULT_BAR_FALL,
            peak_fall: DEFAULT_PEAK_FALL,
            reference_fps: DEFAULT_REFERENCE_FPS,
        }
    }

    pub fn with_speeds(mut self, bar_fall: f32, peak_fall: f32) -> Self {
        self.bar_fall = bar_fall;
        self.peak_fall = peak_fall;
        self
    }

    pub fn with_reference_fps(mut self, fps: f32) -> Self {
        self.reference_fps = fps.max(1.0);
        self
    }

    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    pub fn bars(&self) -> &[f32] {
        &self.bars
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// Bar decrement per reference frame, as a fraction of full scale.
    fn bar_step(&self) -> f32 {
        // `falloff / 16.0` is the original's pixels-per-frame; dividing by the clip
        // converts that to a fraction of full scale.
        (self.bar_fall / FALLOFF_DIVISOR) / FULL_SCALE_PIXELS
    }

    /// Cap velocity seed, as a fraction of full scale per reference frame.
    fn velocity_seed(&self) -> f32 {
        PEAK_VELOCITY_SEED / (PEAK_FIXED_POINT * FULL_SCALE_PIXELS)
    }

    /// Resize, remapping existing state onto the new bar count.
    ///
    /// Caps are carried across by nearest neighbour so they do not visibly jump
    /// while a window is being dragged; bars re-attack within a frame or two
    /// anyway, so they are simply reset.
    pub fn resize(&mut self, bars: usize) {
        if bars == self.bars.len() {
            return;
        }
        let old_peaks = std::mem::take(&mut self.peaks);
        let old_vels = std::mem::take(&mut self.velocities);
        let remap = |src: &[f32], i: usize| src[(i * src.len() / bars).min(src.len() - 1)];
        // Velocities travel with the peaks they belong to. Zeroing them stranded
        // every carried-over cap: nothing re-arms a velocity except a bar
        // catching up to the cap, so a cap remapped onto a quiet band hung in
        // place indefinitely.
        if old_peaks.is_empty() || bars == 0 {
            self.peaks = vec![0.0; bars];
            self.velocities = vec![0.0; bars];
        } else {
            self.peaks = (0..bars).map(|i| remap(&old_peaks, i)).collect();
            self.velocities = (0..bars).map(|i| remap(&old_vels, i)).collect();
        }
        self.bars = vec![0.0; bars];
    }

    /// Advance by `dt` seconds against new band values.
    ///
    /// `dt` is clamped so a stall cannot teleport the caps to the floor.
    pub fn step(&mut self, values: &[f32], dt: f32) {
        if values.len() != self.bars.len() {
            self.resize(values.len());
        }

        let frames = dt.clamp(0.0, 0.25) * self.reference_fps;
        // NaN is checked explicitly: `NaN.clamp()` is NaN and `NaN <= 0.0` is
        // false, so a bare `frames <= 0.0` let it through and permanently
        // poisoned both bars and peaks - once a bar is NaN the instant-attack
        // branch never fires again.
        if frames.is_nan() || frames <= 0.0 {
            return;
        }

        let bar_drop = self.bar_step() * frames;
        let seed = self.velocity_seed();
        let k = self.peak_fall;
        // Distance covered by a geometric velocity over `frames` frames is the sum
        // of the series, not velocity * frames. k == 1 degenerates to linear.
        let growth = k.powf(frames);
        let distance_factor = if (k - 1.0).abs() < f32::EPSILON {
            frames
        } else {
            (growth - 1.0) / (k - 1.0)
        };

        // Indexing rather than zipping: four parallel arrays, and three of them
        // are mutated, so the nested zip needed to satisfy the lint is markedly
        // harder to read than the index.
        #[allow(clippy::needless_range_loop)]
        for i in 0..self.bars.len() {
            let value = values[i].clamp(0.0, 1.0);

            // Apply the previous frame's cap motion first. The original captures the
            // drawn cap position *before* decrementing it, so the value a caller
            // observes is always the post-catch-up one - which is what keeps the
            // cap from ever being drawn below its own bar.
            self.peaks[i] -= self.velocities[i] * distance_factor;
            self.velocities[i] *= growth;
            if self.peaks[i] < 0.0 {
                self.peaks[i] = 0.0;
            }

            // Bar: fall, then snap up to the input. Attack is instantaneous.
            self.bars[i] -= bar_drop;
            if self.bars[i] <= value {
                self.bars[i] = value;
            }

            // Cap: re-seeded from the *bar*, not the raw input.
            if self.peaks[i] <= self.bars[i] {
                self.peaks[i] = self.bars[i];
                self.velocities[i] = seed;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Step a single bar from full scale with no further input, returning the time
    /// for the named element to reach the floor.
    fn fall_seconds(fps: f32, peak: bool) -> f32 {
        let mut b = Ballistics::new(1).with_reference_fps(fps);
        b.step(&[1.0], 1.0 / fps);
        let dt = 1.0 / 240.0;
        let mut t = 0.0;
        for _ in 0..100_000 {
            b.step(&[0.0], dt);
            t += dt;
            let v = if peak { b.peaks()[0] } else { b.bars()[0] };
            if v <= 0.0 {
                break;
            }
        }
        t
    }

    #[test]
    fn attack_is_instantaneous() {
        let mut b = Ballistics::new(3);
        b.step(&[0.8, 0.2, 1.0], 1.0 / 60.0);
        assert_eq!(b.bars(), &[0.8, 0.2, 1.0]);
    }

    #[test]
    fn cap_never_sits_below_its_bar() {
        let mut b = Ballistics::new(1);
        for _ in 0..600 {
            b.step(&[0.7], 1.0 / 60.0);
            assert!(b.peaks()[0] >= b.bars()[0] - 1e-6);
        }
    }

    #[test]
    fn cap_falls_far_slower_than_its_bar() {
        // The whole point of the effect: the cap must detach and linger.
        let bar = fall_seconds(60.0, false);
        let cap = fall_seconds(60.0, true);
        assert!(cap > bar * 2.0, "bar {bar:.3}s vs cap {cap:.3}s");
    }

    #[test]
    fn bar_fall_matches_the_original_rate() {
        // 15px clip at 12/16 px per frame = 20 frames = 0.3333s at 60fps.
        // Spelled as literals on purpose - deriving it from the same constants
        // the implementation uses made this test self-confirming, and it passed
        // happily while the conversion divided by 16 instead of 15.
        let actual = fall_seconds(60.0, false);
        assert!(
            (actual - 0.3333).abs() < 0.005,
            "expected 0.3333s (20 frames), got {actual:.4}s"
        );
    }

    #[test]
    fn cap_fall_matches_the_geometric_series() {
        // velocity 3/4096 per frame growing by 1.1 reaches full scale after N
        // frames where 30*(1.1^N - 1) = 4096, i.e. N = 51.66.
        let expected = 51.66 / 60.0;
        let actual = fall_seconds(60.0, true);
        assert!(
            (actual - expected).abs() < 0.05,
            "expected ~{expected:.3}s, got {actual:.3}s"
        );
    }

    #[test]
    fn motion_is_framerate_independent() {
        // Sample at 0.4s, while the cap is still airborne. Run past the 0.86s
        // floor and both sides read 0.0, so the comparison holds even for a
        // naive `distance_factor = frames`.
        let mut slow = Ballistics::new(1);
        let mut fast = Ballistics::new(1);
        slow.step(&[1.0], 1.0 / 30.0);
        fast.step(&[1.0], 1.0 / 240.0);
        for _ in 0..12 {
            slow.step(&[0.0], 1.0 / 30.0);
            for _ in 0..8 {
                fast.step(&[0.0], 1.0 / 240.0);
            }
        }
        let (a, b) = (slow.peaks()[0], fast.peaks()[0]);
        assert!(
            a > 0.05,
            "cap already grounded at 0.4s ({a:.4}); test proves nothing"
        );
        // The closed form composes exactly, so there is no reason to be loose.
        assert!((a - b).abs() < 1e-3, "30fps {a:.6} vs 240fps {b:.6}");
    }

    #[test]
    fn cap_decay_composes_exactly() {
        // The property that makes the closed form correct rather than
        // approximate: one big step equals two small ones covering the same time.
        let mut one = Ballistics::new(1);
        let mut two = Ballistics::new(1);
        one.step(&[1.0], 1.0 / 60.0);
        two.step(&[1.0], 1.0 / 60.0);
        one.step(&[0.0], 0.2);
        two.step(&[0.0], 0.1);
        two.step(&[0.0], 0.1);
        assert!(
            (one.peaks()[0] - two.peaks()[0]).abs() < 1e-6,
            "0.2 gave {:.9}, 0.1+0.1 gave {:.9}",
            one.peaks()[0],
            two.peaks()[0]
        );
    }

    #[test]
    fn a_carried_over_cap_still_falls_after_a_resize() {
        // Resize carries velocities across with the peaks they belong to. Zeroing
        // them would strand any cap remapped onto a quiet band at full scale,
        // since nothing re-arms a velocity except a bar catching up.
        let mut b = Ballistics::new(4);
        b.step(&[1.0; 4], 1.0 / 60.0);
        b.resize(8);
        for _ in 0..120 {
            b.step(&[0.0; 8], 1.0 / 60.0);
        }
        assert!(
            b.peaks().iter().all(|&p| p == 0.0),
            "caps froze after resize: {:?}",
            b.peaks()
        );
    }

    #[test]
    fn a_nan_dt_cannot_poison_the_state() {
        let mut b = Ballistics::new(1);
        b.step(&[1.0], f32::NAN);
        b.step(&[0.5], 1.0 / 60.0);
        assert!(b.bars()[0].is_finite() && b.peaks()[0].is_finite());
        assert_eq!(b.bars()[0], 0.5, "attack must still work");
    }

    #[test]
    fn reference_fps_scales_the_whole_curve() {
        // Halving the reference rate should roughly double the fall time.
        let at60 = fall_seconds(60.0, true);
        let at30 = fall_seconds(30.0, true);
        assert!(
            (at30 / at60 - 2.0).abs() < 0.15,
            "60fps {at60:.3}s vs 30fps {at30:.3}s"
        );
    }

    #[test]
    fn resize_carries_caps_but_resets_bars() {
        let mut b = Ballistics::new(4);
        b.step(&[1.0, 1.0, 1.0, 1.0], 1.0 / 60.0);
        b.resize(8);
        assert_eq!(b.len(), 8);
        assert!(b.peaks().iter().all(|&p| p > 0.5), "caps should survive");
        assert!(b.bars().iter().all(|&v| v == 0.0), "bars should reset");
    }

    #[test]
    fn a_stall_cannot_teleport_the_cap_to_the_floor() {
        let mut b = Ballistics::new(1);
        b.step(&[1.0], 1.0 / 60.0);
        b.step(&[0.0], 30.0); // a 30-second freeze
        assert!(b.peaks()[0] > 0.0, "dt must be clamped");
    }
}
