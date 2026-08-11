//! What the signal chain has to do for every input, not just musical ones.
//!
//! The ballistics and the frequency mapping are where rav stops being arithmetic
//! and starts being a picture, and both have a failure mode that a fixed example
//! misses: they carry state across frames, driven by a `dt` that whatever the
//! machine was doing decides. A test at 60fps says nothing about the frame after
//! a garbage collection, a window drag, or a laptop waking up.
//!
//! An integration test, so this sees the crate as `cargo install rav` leaves it.
//! That rules out anything reaching `App`: the render loop swaps its terminal
//! for a `TestBackend` under `#[cfg(test)]`, which an integration test does not
//! get, so end-to-end frames stay inline where the backend is swappable. What
//! lives here is the part with no terminal in it at all.

use proptest::prelude::*;
use rav::signal::ballistics::Ballistics;
use rav::signal::mapping::BarMap;

/// A frame interval, from a 240Hz display to a machine that stalled for a
/// quarter of a second - which is where `step` stops believing the clock.
const FRAME: core::ops::RangeInclusive<f32> = 0.004..=0.25;

/// How far a linear interpolation may land outside the pair it interpolates,
/// relative to the larger of them.
///
/// Four epsilons: `a * (1 - f) + b * f` rounds four times, and each rounding
/// costs at most half an ULP of its own result. Measured rather than reasoned
/// at by [`sampling_overshoots_by_at_most_a_rounding_step`], which sweeps the
/// arithmetic and reports what it actually reaches.
const ROUNDING_SLACK: f32 = 4.0 * f32::EPSILON;

prop_compose! {
    /// A run of frames: what each band read, and how long the frame took.
    fn a_run(bands: usize)(
        frames in prop::collection::vec(
            (prop::collection::vec(0.0f32..=1.0, bands), FRAME),
            1..=120,
        ),
    ) -> Vec<(Vec<f32>, f32)> {
        frames
    }
}

proptest! {
    /// A cap is never drawn below the bar it belongs to.
    ///
    /// The two are chained rather than independent - the cap is re-seeded from
    /// the falloff-smoothed bar, and its motion is applied before the bar's - so
    /// this is the invariant that ordering exists to keep. A cap that dipped
    /// under its bar would read as a second, wrong bar.
    #[test]
    fn a_cap_is_never_below_its_bar(run in a_run(8)) {
        let mut ballistics = Ballistics::new(8);
        for (values, dt) in run {
            ballistics.step(&values, dt);
            for (bar, peak) in ballistics.bars().iter().zip(ballistics.peaks()) {
                prop_assert!(peak >= bar, "cap {peak} below bar {bar}");
            }
        }
    }

    /// Nothing ever leaves the display.
    ///
    /// Levels are a fraction of full scale, and a surface multiplies them by a
    /// height. One above 1.0 draws off the top of the screen; one below zero
    /// draws a negative rectangle, which tiny-skia discards without a word.
    #[test]
    fn bars_and_caps_stay_between_silence_and_full_scale(run in a_run(8)) {
        let mut ballistics = Ballistics::new(8);
        for (values, dt) in run {
            ballistics.step(&values, dt);
            for value in ballistics.bars().iter().chain(ballistics.peaks()) {
                prop_assert!((0.0..=1.0).contains(value), "{value} is off the display");
            }
        }
    }

    /// Attack is instantaneous: a bar is never left below what it just read.
    ///
    /// The decay is smoothed and the attack is not, which is what makes a
    /// transient look like one. A bar that lagged its input would round the
    /// front of every drum hit.
    #[test]
    fn a_bar_is_never_left_below_what_it_just_read(run in a_run(8)) {
        let mut ballistics = Ballistics::new(8);
        for (values, dt) in run {
            ballistics.step(&values, dt);
            for (bar, value) in ballistics.bars().iter().zip(&values) {
                prop_assert!(bar >= value, "bar {bar} lagging input {value}");
            }
        }
    }

    /// A fall takes the same time however the frames are cut.
    ///
    /// The reason the constants are per-second rather than per-frame. The cap is
    /// the hard half: its velocity compounds geometrically, so the distance it
    /// covers is the sum of a series and not velocity times frames - and getting
    /// that wrong gives motion that is *nearly* right at 60fps and visibly wrong
    /// anywhere else, which is the kind of bug that ships.
    ///
    /// Half a second, because a cap takes about 0.86s to fall from full scale
    /// and this has to compare two caps that are both still moving. One resting
    /// on the floor matches any other one resting on the floor.
    #[test]
    fn a_fall_takes_the_same_time_however_the_frames_are_cut(
        coarse in 2usize..=40,
        fine in 41usize..=400,
    ) {
        const HALF_A_SECOND: f32 = 0.5;
        let after = |steps: usize| {
            let mut ballistics = Ballistics::new(1);
            ballistics.step(&[1.0], 1.0 / 60.0);
            for _ in 0..steps {
                ballistics.step(&[0.0], HALF_A_SECOND / steps as f32);
            }
            (ballistics.bars()[0], ballistics.peaks()[0])
        };

        let (coarse_bar, coarse_cap) = after(coarse);
        let (fine_bar, fine_cap) = after(fine);
        prop_assert!(coarse_cap > 0.0, "the cap had already landed - nothing compared");
        prop_assert!(
            (coarse_bar - fine_bar).abs() < 1e-3,
            "bar {coarse_bar} in {coarse} frames, {fine_bar} in {fine}",
        );
        prop_assert!(
            (coarse_cap - fine_cap).abs() < 1e-3,
            "cap {coarse_cap} in {coarse} frames, {fine_cap} in {fine}",
        );
    }

    /// The frequency axis never goes backwards.
    ///
    /// Each bar's position is a blend of a linear axis and a logarithmic one,
    /// mixed by the `scale` knob. Both rise, so the blend must - and a display
    /// whose axis doubled back would show the same band twice with a gap
    /// somewhere else, which reads as a broken analyser rather than a broken
    /// axis.
    #[test]
    fn the_frequency_axis_only_ever_climbs(
        bars in 1usize..=512,
        bins in 1usize..=4096,
        scale in 0.0f32..=1.0,
    ) {
        let map = BarMap::new(bars, bins, scale);
        prop_assert_eq!(map.len(), bars);
        for pair in map.positions().windows(2) {
            prop_assert!(pair[1] >= pair[0], "the axis doubled back: {:?}", pair);
        }
        for &position in map.positions() {
            prop_assert!(
                position >= 0.0 && position <= (bins - 1) as f32,
                "{position} is not a bin of {bins}",
            );
        }
    }

    /// Sampling invents nothing.
    ///
    /// Interpolating between two bins cannot land outside them, so no bar can
    /// read louder than the loudest thing in the spectrum. Deliberately run with
    /// the magnitude count *disagreeing* with the bin count the mapping was
    /// built for, which is the resize path: the layout is rebuilt from the new
    /// width while a buffer sized for the old one is still in flight.
    ///
    /// Within a rounding step, because `v * (1 - f) + v * f` is not exactly `v`
    /// - three rounded operations, so a single bin holding `0.030617567` reads
    /// back as `0.030617569`. The bound is *relative*: an FFT magnitude is not a
    /// fraction of full scale and is routinely well above 1.0
    /// (`Spectrum::analyse` returns `norm() * equalize`, normalised by nothing),
    /// so an absolute epsilon holds only for inputs that happen to be small. See
    /// [`sampling_overshoots_by_at_most_a_rounding_step`] for the measurement
    /// behind the multiplier.
    ///
    /// The claim that matters is not float exactness - nothing downstream cares,
    /// since `step` clamps its input. It is that a bar cannot read *materially*
    /// louder than the spectrum, which would be a gain bug.
    #[test]
    fn sampling_never_reads_louder_than_the_spectrum(
        bars in 1usize..=256,
        bins in 1usize..=2048,
        magnitudes in prop::collection::vec(0.0f32..=64.0, 1..=512),
        scale in 0.0f32..=1.0,
    ) {
        let map = BarMap::new(bars, bins, scale);
        let mut out = Vec::new();
        map.sample(&magnitudes, &mut out);

        prop_assert_eq!(out.len(), bars, "one reading per bar, whatever came in");
        let loudest = magnitudes.iter().copied().fold(f32::MIN, f32::max);
        let quietest = magnitudes.iter().copied().fold(f32::MAX, f32::min);
        let slack = loudest * ROUNDING_SLACK;
        for value in out {
            prop_assert!(
                value <= loudest + slack && value >= quietest - slack,
                "{value} is outside {quietest}..={loudest} by more than a rounding step",
            );
        }
    }

    /// A spectrum that has not arrived yet reads as silence, not as nothing.
    ///
    /// The first frames of a run, and every frame of a device that stopped. A
    /// caller indexes the output alongside its colours, so a short answer is a
    /// panic somewhere further on rather than a quiet display.
    #[test]
    fn an_empty_spectrum_still_gives_a_reading_per_bar(bars in 1usize..=256) {
        let mut out = Vec::new();
        BarMap::new(bars, 1024, 0.5).sample(&[], &mut out);
        prop_assert_eq!(out.len(), bars);
        prop_assert!(out.iter().all(|&value| value == 0.0), "it invented a signal");
    }
}

/// How far outside its two bins an interpolated reading can actually land.
///
/// The number behind [`ROUNDING_SLACK`], measured rather than asserted from an
/// error analysis. Sweeping magnitudes across the range an FFT produces - which
/// is not `0..=1`, since nothing normalises it - the worst overshoot is **0.55
/// epsilon of the larger bin, and it is never material**. Four is the bound the
/// property uses, seven times the headroom: enough for a rounding mode this
/// machine does not have, nowhere near enough to hide a gain bug.
#[test]
fn sampling_overshoots_by_at_most_a_rounding_step() {
    // The overshoot needs both ends of the interpolation to be the *same* bin,
    // which is where `v * (1 - f) + v * f` has to reproduce `v` exactly and does
    // not. A long spectrum of differing magnitudes never reaches it, so the
    // shapes below are short buffers, which force `hi` to clamp onto `lo`, and
    // flat ones, where the two bins hold the same value.
    let shapes: [Vec<f32>; 4] = [
        std::vec![0.030617567],
        std::vec![7.25; 3],
        (0..9).map(|i| 1.0 + i as f32 * 8.0).collect(),
        (0..512)
            .map(|i| (i as f32 * 0.37).sin().abs() * 64.0)
            .collect(),
    ];

    let mut worst: f32 = 0.0;
    let mut reached = false;
    for magnitudes in &shapes {
        let loudest = magnitudes.iter().copied().fold(f32::MIN, f32::max);
        let quietest = magnitudes.iter().copied().fold(f32::MAX, f32::min);
        for bars in [1usize, 2, 3, 6, 7, 16, 64, 129, 256] {
            for bins in [1usize, 2, 17, 256, 512, 763, 1024, 2048] {
                for scale in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
                    let mut out = Vec::new();
                    BarMap::new(bars, bins, scale).sample(magnitudes, &mut out);
                    for value in out {
                        let over = (value - loudest).max(quietest - value).max(0.0);
                        reached |= over > 0.0;
                        worst = worst.max(over / loudest);
                    }
                }
            }
        }
    }

    println!("worst overshoot {:.2} epsilon", worst / f32::EPSILON);
    assert!(reached, "no overshoot at all - this is measuring nothing");
    assert!(
        worst <= ROUNDING_SLACK,
        "overshot by {:.2} epsilon, past the {:.0} the property allows",
        worst / f32::EPSILON,
        ROUNDING_SLACK / f32::EPSILON,
    );
}
