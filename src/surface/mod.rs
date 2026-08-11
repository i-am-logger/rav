//! Which surface rav would draw on, and asking the terminal whether it can.
//!
//! Reporting only. Nothing here changes what is drawn - the choice is made,
//! logged and shown in the help overlay, and the glyph renderer draws every
//! frame regardless. A probe that hangs or garbles a terminal is the worst bug
//! class in this area, so it ships first with its only possible symptom being a
//! wrong label.

use crate::render::Capabilities;
use rav_core::units::Cells;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

/// How long to wait for a terminal to answer the capability query.
///
/// The same budget the palette query allows, for the same reason: a terminal
/// that does not implement this says nothing at all, so the deadline is the only
/// thing that ends the wait.
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

/// Decide which surface to draw on.
///
/// `asked` is what the user typed; `multiplexed` is whether rav is running under
/// tmux or screen; `answers` is whether the terminal admitted to the kitty
/// protocol. Split from the probe so the decision is testable without a
/// terminal - the probe needs one and this does not.
pub fn choose(asked: Choice, multiplexed: bool, answers: bool) -> Chosen {
    let glyphs = |because| Chosen {
        surface: Surface::Glyphs,
        because,
    };
    match asked {
        Choice::Window => Chosen {
            surface: Surface::Window,
            because: "asked for",
        },
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
        Choice::Auto if multiplexed => glyphs("running under a multiplexer"),
        Choice::Auto if answers => Chosen {
            surface: Surface::Kitty,
            because: "the terminal answered the graphics query",
        },
        Choice::Auto => glyphs("the terminal did not answer the graphics query"),
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
/// reading stdin. Costs one round trip, bounded by [`PROBE_TIMEOUT`].
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
    let pixel = "AAAA"; // three zero bytes, base64
    if write!(out, "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;{pixel}\x1b\\").is_err() || out.flush().is_err()
    {
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
    let reply = text.split("\x1b_G").nth(1)?;
    // The terminator has to have arrived, or the payload may still be growing.
    let body = reply.split("\x1b\\").next()?;
    if !reply.contains("\x1b\\") {
        return None;
    }
    Some(body.contains(";OK"))
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

    #[test]
    fn a_multiplexer_declines_pixels_however_the_terminal_answers() {
        // tmux and screen rewrite what passes through, and an image escape does
        // not survive that intact. The failure is a garbled screen rather than a
        // missing picture, which is worse than not trying.
        let under_tmux = choose(Choice::Auto, true, true);
        assert_eq!(under_tmux.surface, Surface::Glyphs);
        assert_eq!(under_tmux.because, "running under a multiplexer");
    }

    #[test]
    fn auto_follows_the_terminals_answer() {
        assert_eq!(choose(Choice::Auto, false, true).surface, Surface::Kitty);
        assert_eq!(choose(Choice::Auto, false, false).surface, Surface::Glyphs);
    }

    #[test]
    fn an_explicit_request_is_honoured_even_where_auto_would_decline() {
        // It is the user's terminal and they can see the result. A terminal that
        // draws images without answering the query is reachable this way.
        assert_eq!(choose(Choice::Kitty, false, false).surface, Surface::Kitty);
        assert_eq!(choose(Choice::Kitty, true, false).surface, Surface::Kitty);
        assert_eq!(choose(Choice::Glyphs, false, true).surface, Surface::Glyphs);
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
