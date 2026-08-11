//! Which surface rav would draw on, and asking the terminal whether it can.
//!
//! Reporting only. Nothing here changes what is drawn - the choice is made,
//! logged and shown in the help overlay, and the glyph renderer draws every
//! frame regardless. A probe that hangs or garbles a terminal is the worst bug
//! class in this area, so it ships first with its only possible symptom being a
//! wrong label.

pub mod frame;
pub mod kitty;
pub mod overlay;
pub mod pixels;

use crate::render::Capabilities;
use rav_core::units::Cells;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

/// How long to wait before giving up on a terminal that answers nothing at all.
///
/// A backstop, not the mechanism. The graphics query is chained with a device
/// attributes request that every terminal answers, so the usual "no" arrives in
/// a millisecond rather than by running this out. Reaching it means the terminal
/// ignored *both* questions, which is rare enough that a stall is the right
/// price for not guessing.
const PROBE_TIMEOUT: Duration = Duration::from_millis(120);

/// Where a frame is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Surface {
    /// Unicode block glyphs in a cell grid. Works everywhere, and is what every
    /// terminal that cannot do better gets.
    #[default]
    Glyphs,
    /// Pixels, over the kitty graphics protocol.
    Kitty,
    /// A window of rav's own.
    Window,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Self::Glyphs => "glyphs",
            Self::Kitty => "pixels",
            Self::Window => "window",
        }
    }

    /// What this surface can show, for deciding which visualisations to offer.
    ///
    /// A glyph grid cannot layer: one cell holds one symbol and one background,
    /// which is why a cap drawn over the backdrop replaces it (#65) and why a
    /// written cell blanks a kitty image.
    pub fn capabilities(self, columns: Cells, rows: Cells) -> Capabilities {
        let grid = Capabilities::terminal(columns, rows);
        match self {
            Self::Glyphs => grid,
            // Sub-cell resolution and real compositing, at the same extent.
            Self::Kitty | Self::Window => Capabilities {
                shades: 256,
                layers: true,
                ..grid
            },
        }
    }
}

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    /// Ask the terminal, and take the best it admits to.
    #[default]
    Auto,
    Glyphs,
    Kitty,
    Window,
}

impl Choice {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Self::Auto),
            "glyphs" => Some(Self::Glyphs),
            "kitty" | "pixels" => Some(Self::Kitty),
            "window" | "gui" => Some(Self::Window),
            _ => None,
        }
    }
}

/// Why a surface was chosen, so a wrong choice can be argued with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub surface: Surface,
    pub because: &'static str,
}

impl Chosen {
    /// What to hold before anything has been decided.
    ///
    /// Glyphs, because they work everywhere - which is what makes them the
    /// answer when there is no answer.
    pub const UNASKED: Self = Self {
        surface: Surface::Glyphs,
        because: "not asked yet",
    };
}

/// Decide which surface to draw on.
///
/// `asked` is what the user typed; `multiplexed` is whether rav is running under
/// tmux or screen; `answers` asks the terminal whether it admits to the kitty
/// protocol. Split from the probe so the decision is testable without a terminal
/// - the probe needs one and this does not.
///
/// `answers` is a closure rather than a `bool` so the arms that ignore it never
/// ask. That is not a micro-optimisation: asking writes escape sequences to the
/// terminal and swallows what comes back off stdin, so it is a thing that
/// happens rather than a value that is read. Passing a `bool` would do it on
/// every startup for an answer three of the five arms discard.
pub fn choose(asked: Choice, multiplexed: bool, answers: impl FnOnce() -> bool) -> Chosen {
    let glyphs = |because| Chosen {
        surface: Surface::Glyphs,
        because,
    };
    match asked {
        // A window of rav's own is not built. Saying so beats reporting a
        // surface nothing draws on and leaving the user to wonder why the
        // picture looks exactly as it did before.
        Choice::Window => glyphs("a window of rav's own is not built yet"),
        // An explicit request is honoured even where auto would decline, so a
        // terminal that does not answer the query but does draw images is still
        // reachable. It is the user's terminal and they can see the result.
        Choice::Kitty => Chosen {
            surface: Surface::Kitty,
            because: "asked for",
        },
        Choice::Glyphs => glyphs("asked for"),
        // A multiplexer sits between rav and the terminal and rewrites what
        // passes through. Image escapes do not survive that intact, and the
        // failure is a garbled screen rather than a missing picture - so auto
        // never picks pixels there, however the terminal answers.
        Choice::Auto if multiplexed => glyphs("tmux or screen is in the way"),
        // Pixels are asked for, never assumed. A terminal that says it can draw
        // images is told so and left on block characters, because "it looked
        // fine on the one terminal that was tried" is not something a default
        // can rest on - and a default that turns out wrong is a broken display
        // for everybody rather than for whoever opted in.
        //
        // The line to change when that is no longer true is the one below.
        //
        // Not a match guard, because calling `answers` consumes it and a guard
        // only gets a borrow.
        Choice::Auto => {
            if answers() {
                glyphs("your terminal can draw images - try --surface kitty")
            } else {
                glyphs("your terminal cannot draw images")
            }
        }
    }
}

/// Whether rav is running under a terminal multiplexer.
pub fn multiplexed() -> bool {
    std::env::var_os("TMUX").is_some()
        || std::env::var("TERM").is_ok_and(|term| term.starts_with("screen"))
}

/// Ask the terminal whether it speaks the kitty graphics protocol.
///
/// Must run before the alternate screen is entered and while nothing else is
/// reading stdin. Costs one round trip, bounded by `PROBE_TIMEOUT`.
///
/// **No test reaches this body** - it needs a pty, so the raw-mode handling and
/// its restore are exercised by running rav and by nothing else. [`answered`] is
/// the testable half.
#[cfg(not(test))]
pub fn probe() -> bool {
    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && crossterm::terminal::enable_raw_mode().is_err() {
        return false;
    }
    let answer = probe_via(&mut std::io::stdout(), &mut std::io::stdin());
    if !was_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    answer
}

/// No terminal under test, so nothing answers.
#[cfg(test)]
pub fn probe() -> bool {
    false
}

fn probe_via<W: Write, R: Read + AsRawFd>(out: &mut W, input: &mut R) -> bool {
    // One opaque pixel, transmitted and queried rather than displayed: `a=q`
    // asks whether the terminal *could* take it and draws nothing either way.
    //
    // Chained with a device attributes request in the same write, because a
    // terminal that does not implement graphics answers the first question with
    // silence and there is nothing to wait for. Terminals answer queries in the
    // order they arrive, so a `CSI c` reply with no graphics reply ahead of it
    // is a definite no, available immediately.
    if write!(out, "{}\x1b[c", kitty::Command::Ask.escape()).is_err() || out.flush().is_err() {
        return false;
    }

    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut seen = Vec::new();
    let mut chunk = [0u8; 256];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if !crate::visual::palette::readable(input.as_raw_fd(), left) {
            break;
        }
        match input.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                seen.extend_from_slice(&chunk[..n]);
                if let Some(verdict) = answered(&seen) {
                    return verdict;
                }
            }
        }
    }
    false
}

/// Read a reply, if it is complete enough to be one.
///
/// `None` means keep waiting: a reply split across two reads is ordinary, and
/// treating a partial one as a refusal would decline terminals that do support
/// this.
pub fn answered(bytes: &[u8]) -> Option<bool> {
    let text = String::from_utf8_lossy(bytes);
    if let Some(reply) = text.split("\x1b_G").nth(1) {
        // The terminator has to have arrived, or the payload may still be
        // growing - and a graphics reply outranks whatever follows it.
        return reply
            .split_once("\x1b\\")
            .map(|(body, _)| body.contains(";OK"));
    }
    // No graphics reply, but the terminal has answered the question chained
    // behind it - so it read the first one, understood it well enough to ignore
    // it, and moved on. That is the answer.
    device_attributes(&text).then_some(false)
}

/// Whether a Primary Device Attributes reply - `CSI ? … c` - has arrived.
///
/// The parameters are digits and semicolons and say which VT the terminal claims
/// to be, which rav does not care about. What matters is that one arrived.
fn device_attributes(text: &str) -> bool {
    text.split("\x1b[?").skip(1).any(|rest| {
        rest.split_once('c')
            .is_some_and(|(params, _)| params.bytes().all(|b| b.is_ascii_digit() || b == b';'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_gives_up_rather_than_blocking() {
        // A terminal that takes the query and never answers must not hang rav at
        // startup - and most terminals are exactly that, since not implementing
        // the protocol means saying nothing at all. /dev/null is readable-at-EOF,
        // the closest stand-in for a terminal that will never reply.
        let mut sink = Vec::new();
        let mut null = std::fs::File::open("/dev/null").unwrap();
        let start = Instant::now();
        let answered = probe_via(&mut sink, &mut null);
        assert!(
            start.elapsed() < PROBE_TIMEOUT * 2,
            "took too long to give up"
        );
        assert!(!answered, "silence is not support");
        assert!(!sink.is_empty(), "it should still have asked");
    }

    /// Say `yes`, and record whether anyone asked.
    fn asking(asked: &std::cell::Cell<bool>, yes: bool) -> impl FnOnce() -> bool + '_ {
        move || {
            asked.set(true);
            yes
        }
    }

    /// Run the probe against a socket playing the part of a terminal that has
    /// already replied `reply`, and report the verdict and how long it took.
    ///
    /// A socket rather than a pty because `probe_via` only needs something
    /// readable with a descriptor to poll, which is all a terminal is to it. The
    /// reply is waiting before the query goes out, which a real terminal cannot
    /// do - the point here is what happens once bytes arrive, not the round trip.
    fn against_a_terminal_saying(reply: &[u8]) -> (bool, Duration) {
        use std::os::unix::net::UnixStream;
        let (mut terminal, mut rav) = UnixStream::pair().unwrap();
        terminal.write_all(reply).unwrap();
        terminal.flush().unwrap();
        let mut sink = Vec::new();
        let start = Instant::now();
        let verdict = probe_via(&mut sink, &mut rav);
        (verdict, start.elapsed())
    }

    #[test]
    fn a_terminal_without_graphics_is_taken_at_its_word_immediately() {
        // The one path every user of Terminal.app, Alacritty and xterm takes. A
        // graphics query alone is answered with silence there, so believing
        // silence would put the whole timeout on the common case - a visible
        // stall at startup, shipped under a change that draws nothing.
        let (verdict, took) = against_a_terminal_saying(b"\x1b[?62;1;6c");
        assert!(!verdict, "it declined");
        assert!(took < PROBE_TIMEOUT / 4, "waited it out instead: {took:?}");

        let (verdict, took) = against_a_terminal_saying(b"\x1b_Gi=31;OK\x1b\\\x1b[?62c");
        assert!(verdict, "it accepted");
        assert!(took < PROBE_TIMEOUT / 4, "waited it out instead: {took:?}");
    }

    #[test]
    fn a_multiplexer_declines_pixels_however_the_terminal_answers() {
        // tmux and screen rewrite what passes through, and an image escape does
        // not survive that intact. The failure is a garbled screen rather than a
        // missing picture, which is worse than not trying.
        let under_tmux = choose(Choice::Auto, true, || true);
        assert_eq!(under_tmux.surface, Surface::Glyphs);
        assert_eq!(under_tmux.because, "tmux or screen is in the way");
    }

    #[test]
    fn nobody_gets_pixels_without_asking_for_them() {
        // Both ways round, auto draws block characters. A terminal that can do
        // better is told how, rather than being switched over on its behalf:
        // the pixel surface has been seen working on one terminal, and a
        // default that turns out wrong breaks the display for everyone instead
        // of for whoever chose it.
        let capable = choose(Choice::Auto, false, || true);
        assert_eq!(capable.surface, Surface::Glyphs);
        assert!(
            capable.because.contains("--surface kitty"),
            "a capable terminal was not told how to use it: {}",
            capable.because,
        );
        assert_eq!(
            choose(Choice::Auto, false, || false).surface,
            Surface::Glyphs
        );

        // Asking is still honoured, or there would be no way in at all.
        assert_eq!(
            choose(Choice::Kitty, false, || false).surface,
            Surface::Kitty
        );
    }

    #[test]
    fn asking_for_a_window_says_there_is_not_one_yet() {
        // Reporting a surface nothing draws on leaves the user looking at a
        // picture identical to the one before, wondering what the flag did.
        let asked = choose(Choice::Window, false, || false);
        assert_eq!(asked.surface, Surface::Glyphs);
        assert!(asked.because.contains("not built"), "{}", asked.because);
    }

    #[test]
    fn an_explicit_request_is_honoured_even_where_auto_would_decline() {
        // It is the user's terminal and they can see the result. A terminal that
        // draws images without answering the query is reachable this way.
        assert_eq!(
            choose(Choice::Kitty, false, || false).surface,
            Surface::Kitty
        );
        assert_eq!(
            choose(Choice::Kitty, true, || false).surface,
            Surface::Kitty
        );
        assert_eq!(
            choose(Choice::Glyphs, false, || true).surface,
            Surface::Glyphs
        );
    }

    #[test]
    fn the_terminal_is_only_asked_when_the_answer_could_change_the_outcome() {
        // Asking is not free and not invisible: it writes escape sequences,
        // reads stdin, and waits out the timeout against a terminal that will
        // never reply. Every arm that ignores the answer must cost nothing.
        for (asked, multiplexed) in [
            (Choice::Glyphs, false),
            (Choice::Kitty, false),
            (Choice::Window, false),
            (Choice::Auto, true),
        ] {
            let probed = std::cell::Cell::new(false);
            choose(asked, multiplexed, asking(&probed, true));
            assert!(!probed.get(), "{asked:?} asked and did not need to");
        }

        let probed = std::cell::Cell::new(false);
        choose(Choice::Auto, false, asking(&probed, true));
        assert!(probed.get(), "auto has nothing else to go on");
    }

    #[test]
    fn a_partial_reply_is_waited_out_rather_than_read_as_refusal() {
        // A reply split across two reads is ordinary. Calling the first half a
        // refusal would decline terminals that do support this.
        assert_eq!(answered(b"\x1b_Gi=31;O"), None, "still arriving");
        assert_eq!(answered(b""), None, "nothing yet");
        assert_eq!(answered(b"\x1b_Gi=31;OK\x1b\\"), Some(true));
        assert_eq!(answered(b"\x1b_Gi=31;ENOTSUPPORTED\x1b\\"), Some(false));
    }

    #[test]
    fn the_no_arrives_rather_than_being_waited_out() {
        // The whole reason the graphics query is chained with `CSI c`. Silence
        // is what a terminal without graphics says, so waiting for it would put
        // the full timeout on the one path every user of every other terminal
        // takes - a startup stall on Terminal.app, Alacritty and xterm.
        assert_eq!(
            answered(b"\x1b[?62;1;6c"),
            Some(false),
            "answered, not mute"
        );
        assert_eq!(answered(b"\x1b[?6c"), Some(false), "a short one counts");

        // Order settles it: terminals answer queries as they arrive, so a
        // graphics reply is always ahead of the attributes reply behind it.
        assert_eq!(answered(b"\x1b_Gi=31;OK\x1b\\\x1b[?62c"), Some(true));

        // Half an attributes reply is not one, or a terminal that is mid-word
        // gets read as a refusal it has not made.
        assert_eq!(answered(b"\x1b[?62;1"), None, "no final byte yet");
        assert_eq!(answered(b"\x1b[?"), None);
        // Not every CSI is this one - a mouse or bracketed-paste report shares
        // the prefix and says nothing about graphics.
        assert_eq!(answered(b"\x1b[?1000;1006$y"), None, "a different report");
    }

    #[test]
    fn a_glyph_grid_cannot_layer_and_a_pixel_surface_can() {
        // The measured behaviour behind #65 and the kitty occlusion result,
        // carried into what each surface reports it can do.
        let grid = Surface::Glyphs.capabilities(Cells(80), Cells(24));
        let pixels = Surface::Kitty.capabilities(Cells(80), Cells(24));
        assert!(!grid.layers);
        assert!(pixels.layers);
        assert!(pixels.shades > grid.shades, "sub-cell resolution");
        assert_eq!(pixels.columns, grid.columns, "the same extent either way");
    }

    #[test]
    fn every_spelling_a_user_might_type_is_understood() {
        assert_eq!(Choice::parse("auto"), Some(Choice::Auto));
        assert_eq!(Choice::parse("kitty"), Some(Choice::Kitty));
        assert_eq!(Choice::parse("pixels"), Some(Choice::Kitty));
        assert_eq!(Choice::parse("gui"), Some(Choice::Window));
        assert_eq!(Choice::parse("sixel"), None, "deliberately not offered");
    }
}
