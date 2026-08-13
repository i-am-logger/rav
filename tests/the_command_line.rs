//! What the binary does with a file it was handed, before it takes the screen.
//!
//! A startup error is the last thing rav can say where anyone will read it.
//! Once the alternate screen is up, a message goes onto a surface that is about
//! to be painted over sixty times a second - so the checks that stop a run have
//! to happen first, and this is where that ordering is asserted.
//!
//! No pty here: nothing in this file gets as far as needing one, and a test
//! that built one would be asserting that the error came *before* the terminal
//! by relying on a terminal.

use std::process::Command;

/// A skin someone is part-way through drawing: well-formed enough to look
/// finished, and not parseable.
const HALF_WRITTEN: &str =
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M 0 0"#;

/// A directory of this run's own, so two test binaries do not read each other's
/// files or delete them mid-run.
fn a_file_holding(svg: &str, named: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rav-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("somewhere to write a skin");
    let path = dir.join(named);
    std::fs::write(&path, svg).expect("write the skin");
    path
}

/// No pty and no sound device: the skin is checked before the log file, the
/// device and the screen, so a run that has none of them still gets this far.
/// If that ordering ever slips, these fail with whatever rav died of instead,
/// which is how the ordering stays asserted rather than assumed.
///
/// `--list-devices` returns early, and it returns *after* the skin check - so
/// a good skin reaches an exit rather than a terminal it cannot have, and this
/// cannot hang waiting on a TUI that will never start. A skin check that moved
/// below it would let the failing cases through, and they would say so.
///
/// Returns what rav said on **stdout**, which is where it reports a failure on
/// purpose: stderr is pointed at the log file early, so the usual channel would
/// make a mistyped flag look like rav dying without a word. `src/main.rs` says
/// so at the point it decides.
fn rav_wearing(skin: &std::path::Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_rav"))
        .arg("--list-devices")
        .arg("--skin")
        .arg(skin)
        .output()
        .expect("run rav");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn a_skin_that_will_not_parse_stops_the_run_and_names_the_file() {
    let path = a_file_holding(HALF_WRITTEN, "half-written.svg");
    let (started, said) = rav_wearing(&path);
    std::fs::remove_file(&path).ok();

    // The alternative is what this replaces: rav starts, draws the plain block
    // ladder, and says nothing - so a typo in someone's own file is
    // indistinguishable from rav ignoring the flag.
    assert!(!started, "rav started wearing a skin it cannot draw");
    assert!(
        said.contains(&*path.to_string_lossy()),
        "the error does not say which file: {said}",
    );
}

#[test]
fn a_skin_that_is_not_there_stops_the_run_and_names_the_file() {
    let path = std::env::temp_dir().join(format!("rav-cli-{}-absent.svg", std::process::id()));
    // A pid comes round again, and so does a temp directory that was never
    // cleared. Without this, the one run where the name is already taken turns
    // this into the case two tests down and passes for the wrong reason.
    std::fs::remove_file(&path).ok();
    let (started, said) = rav_wearing(&path);

    assert!(!started, "rav started with no skin to wear");
    assert!(
        said.contains(&*path.to_string_lossy()),
        "the error does not say which file: {said}",
    );
}

#[test]
fn a_skin_that_parses_is_not_what_stops_the_run() {
    // The other half of the pair. Without this the two above would pass just as
    // well against a rav that refused every skin, and the check would be
    // measuring nothing but its own presence.
    let path = a_file_holding(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#ffffff"/></svg>"##,
        "plain.svg",
    );
    let (_started, said) = rav_wearing(&path);
    std::fs::remove_file(&path).ok();

    // Not "it succeeded": with no terminal to draw on, rav has every right to
    // stop for that instead. What must not appear is a complaint about the
    // skin, which is the thing this file is about.
    assert!(
        !said.contains("skin"),
        "a skin that parses was refused anyway: {said}",
    );
}
