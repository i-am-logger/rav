//! Reading the terminal's own colour palette.
//!
//! A theme that names `green` is saying "whatever this palette calls green". That
//! is exactly what makes those themes follow your setup — and exactly what makes
//! them impossible to adjust, because rav has no number to work with. There is
//! no ANSI slot for "darker than green", so a theme that wants a dimmer backdrop
//! has nothing to ask for.
//!
//! Terminals will tell you, though. `OSC 4 ; n ; ?` asks what index `n` actually
//! resolves to and the reply carries the RGB. With that in hand a named colour
//! can be scaled like any other, and the result is still the user's palette —
//! their green, darkened — rather than a colour rav invented.
//!
//! Terminals that do not answer are the normal case, not an error: rav waits a
//! moment, gives up, and leaves those colours alone.

use rav_appearance::Palette;

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

/// How long to wait for the whole reply. Long enough for a terminal over ssh,
/// short enough not to be a visible pause at startup.
const TIMEOUT: Duration = Duration::from_millis(120);

/// Ask the terminal for its palette, if `needed`.
///
/// Asking is not free and it is not invisible: it writes escape sequences and
/// reads the replies straight off the terminal. Themes that spell their colours
/// out in hex never need it, so the common case says nothing at all.
///
/// Must run before the alternate screen is entered and while nothing else is
/// reading stdin.
///
/// A free function rather than a constructor, because [`Palette`] itself is
/// portable and this is the one part that requires a terminal to exist.
#[cfg(not(test))]
pub fn query(needed: bool) -> Palette {
    if !needed {
        return Palette::default();
    }
    // Raw mode, or the terminal echoes every reply back as visible text and
    // line buffering holds them until the user presses Enter. Restored to
    // however it was found - the caller may already have set it up.
    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && crossterm::terminal::enable_raw_mode().is_err() {
        return Palette::default();
    }
    let palette = query_via(&mut std::io::stdout(), &mut std::io::stdin());
    if !was_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    palette
}

/// No terminal under test, so nothing answers.
#[cfg(test)]
pub fn query(_needed: bool) -> Palette {
    Palette::default()
}

fn query_via<W: Write, R: Read + AsRawFd>(out: &mut W, input: &mut R) -> Palette {
    let mut palette = Palette::default();
    for i in 0..16 {
        if write!(out, "\x1b]4;{i};?\x07").is_err() {
            return palette;
        }
    }
    if out.flush().is_err() {
        return palette;
    }

    let deadline = Instant::now() + TIMEOUT;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        // Wait with a deadline rather than calling read() straight away: a
        // terminal that does not implement this says nothing at all, and a
        // blocking read would hang rav at startup rather than give up.
        if !readable(input.as_raw_fd(), left) {
            break;
        }
        match input.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                palette.absorb(&buf);
                if palette.is_complete() {
                    break;
                }
            }
        }
    }
    palette
}
fn readable(fd: i32, within: Duration) -> bool {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = within.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: one initialised pollfd, and the count matches.
    unsafe { libc::poll(&mut poll, 1, ms) > 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rav_appearance::Ink;

    #[test]
    fn an_unanswered_query_leaves_named_colours_alone() {
        // The normal case on a terminal that does not implement OSC 4. Nothing
        // is invented: the caller is told it has no RGB for that colour.
        let palette = Palette::default();
        assert_eq!(palette.rgb(Ink::from_name("green").unwrap()), None);
        // A theme that named an exact colour never needed the terminal's help.
        assert_eq!(palette.rgb(Ink::Rgb(1, 2, 3)), Some((1, 2, 3)));
    }

    #[test]
    fn the_query_gives_up_rather_than_blocking() {
        // A terminal that accepts the request and answers nothing must not hang
        // rav at startup. /dev/null is readable-at-EOF, which is the closest
        // stand-in for a terminal that will never reply.
        let mut sink = Vec::new();
        let mut null = std::fs::File::open("/dev/null").unwrap();
        let start = Instant::now();
        let palette = query_via(&mut sink, &mut null);
        assert!(start.elapsed() < TIMEOUT * 2, "took too long to give up");
        assert_eq!(palette, Palette::default());
        assert!(!sink.is_empty(), "it should still have asked");
    }
}
