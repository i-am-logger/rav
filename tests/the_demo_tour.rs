//! The demo tour walks every theme and every angle, and comes back.
//!
//! `record-demo` drives the GIF on the front page by sending keys on a timer,
//! and the tour is a string in `devenv.nix`. One press per theme and one per
//! angle, because the last press of each is what returns to the first - so a
//! setting added to rav without a press added here leaves the recording ending
//! somewhere other than it started, and the GIF stops looping cleanly.
//!
//! That is not hypothetical: the fifth viewing angle arrived after the tour was
//! written and the tour went on pressing `v` four times. The plan predicted
//! this exact coupling before either existed. Nothing else checks it, because
//! the failure is only visible to someone watching a finished recording.

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
