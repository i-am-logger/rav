//! The demo tour walks every theme, every bar style and every angle, and comes
//! back.
//!
//! `record-demo` drives the GIF on the front page by sending keys on a timer,
//! and the tour is a string in `devenv.nix`. One press per theme, one per style
//! and one per angle, because the last press of each is what returns to the
//! first - so a setting added to rav without a press added here leaves the
//! recording ending somewhere other than it started, and the GIF stops looping
//! cleanly.
//!
//! That is not hypothetical: the fifth viewing angle arrived after the tour was
//! written and the tour went on pressing `v` four times. The plan predicted
//! this exact coupling before either existed. Nothing else checks it, because
//! the failure is only visible to someone watching a finished recording.

use rav::ui::analyzer::BarStyle;
use rav_appearance::scene::View;
use rav_appearance::theme::Theme;

/// The tour as `record-demo` defaults it, or `None` where `devenv.nix` is not
/// beside us - a crate unpacked from the registry has the tests and not the
/// development environment, and that is not a failure.
///
/// Absent and unreadable are told apart on purpose. A file that is there and
/// does not parse means `record-demo` has been rewritten, and returning `None`
/// for it would leave both of these skipping in the one repository that has the
/// script - green, and guarding nothing. That is the failure they exist to
/// catch in the recording, so they had better not be it themselves.
fn default_tour() -> Option<String> {
    let nix = std::fs::read_to_string("devenv.nix").ok()?;
    let line = nix
        .lines()
        .find(|line| line.contains("RAV_DEMO_TOUR:-"))
        .expect("devenv.nix is here but names no RAV_DEMO_TOUR - has record-demo moved?");
    let after = line
        .split("RAV_DEMO_TOUR:-")
        .nth(1)
        .expect("RAV_DEMO_TOUR is named but has no default after `:-`");
    let tour = after
        .split('}')
        .next()
        .expect("the default is never closed")
        .trim()
        .to_string();
    assert!(
        tour.split_whitespace().all(|step| step.contains(':')),
        "the tour is not `seconds:key` steps any more: {tour}",
    );
    Some(tour)
}

/// How many times the tour sends that key.
fn presses_of(tour: &str, key: char) -> usize {
    tour.split_whitespace()
        .filter_map(|step| step.split(':').nth(1))
        .filter(|sent| sent.chars().eq(std::iter::once(key)))
        .count()
}

#[test]
fn one_press_per_viewing_angle() {
    let Some(tour) = default_tour() else {
        return;
    };
    let mut angles = 1;
    let mut view = View::default().next();
    while view != View::default() {
        angles += 1;
        view = view.next();
    }

    // Where the tour actually leaves the field, by pressing `v` as it does.
    // Naming it is the whole value of the message: "four presses, five angles"
    // is arithmetic, and "it ends on swaying" is the thing to go and look at.
    let pressed = presses_of(&tour, 'v');
    let ends_on = (0..pressed).fold(View::default(), |view, _| view.next());

    assert_eq!(
        pressed,
        angles,
        "the tour presses `v` {pressed} times for {angles} angles, so the \
         recording ends on {} rather than back at {}\ntour: {tour}",
        ends_on.label(),
        View::default().label(),
    );
}

#[test]
fn one_press_per_bar_style() {
    let Some(tour) = default_tour() else {
        return;
    };

    // `segment` is artwork rather than a ladder of fractions, and it is the
    // release this tour was extended for, so a style dropped from the cycle
    // takes it out of the GIF as readily as one added without a press here.
    let pressed = presses_of(&tour, 'b');
    let ends_on = (0..pressed).fold(BarStyle::default(), |style, _| style.next());

    assert_eq!(
        pressed,
        BarStyle::COUNT,
        "the tour presses `b` {pressed} times for {} styles, so the recording \
         ends on {} rather than back at {}\ntour: {tour}",
        BarStyle::COUNT,
        ends_on.label(),
        BarStyle::default().label(),
    );
}

/// Where in the tour a key is sent, counting steps from the start.
fn presses_at(tour: &str, key: char) -> Vec<usize> {
    tour.split_whitespace()
        .enumerate()
        .filter(|(_, step)| {
            step.split(':')
                .nth(1)
                .is_some_and(|sent| sent.chars().eq(std::iter::once(key)))
        })
        .map(|(at, _)| at)
        .collect()
}

#[test]
fn the_styles_are_done_with_before_the_field_leans() {
    let Some(tour) = default_tour() else {
        return;
    };

    // `segment` is a mask, and a mask cannot follow a leaning bar - `App`'s
    // `drawing_is_missing` says `needs a flat view` and the plain ladder is
    // drawn instead. So the styles have to finish while the view is still
    // flat, or the one style that is artwork is filmed as the one thing it is
    // not. Reordering the tour is what would break this, and the GIF is the
    // only place it would show.
    let last_style = presses_at(&tour, 'b').into_iter().max();
    let first_angle = presses_at(&tour, 'v').into_iter().min();

    if let (Some(last_style), Some(first_angle)) = (last_style, first_angle) {
        assert!(
            last_style < first_angle,
            "the tour is still pressing `b` at step {last_style} after `v` at \
             step {first_angle}, so `segment` is filmed at an angle - where it \
             is drawn as the plain ladder\ntour: {tour}",
        );
    }
}

#[test]
fn one_press_per_built_in_theme() {
    let Some(tour) = default_tour() else {
        return;
    };
    let themes = Theme::built_in_names().count();

    assert_eq!(
        presses_of(&tour, 't'),
        themes,
        "the tour presses `t` {} times for {themes} themes, so the recording \
         does not end on the theme it opened with\ntour: {tour}",
        presses_of(&tour, 't'),
    );
}
