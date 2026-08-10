//! Reading the terminal's own colour palette.
//!
//! A skin that names `green` is saying "whatever this theme calls green". That
//! is exactly what makes those skins follow your setup — and exactly what makes
//! them impossible to adjust, because rav has no number to work with. There is
//! no ANSI slot for "darker than green", so a skin that wants a dimmer backdrop
//! has nothing to ask for.
//!
//! Terminals will tell you, though. `OSC 4 ; n ; ?` asks what index `n` actually
//! resolves to and the reply carries the RGB. With that in hand a named colour
//! can be scaled like any other, and the result is still the user's theme —
//! their green, darkened — rather than a colour rav invented.
//!
//! Terminals that do not answer are the normal case, not an error: rav waits a
//! moment, gives up, and leaves those colours alone.

use ratatui::style::Color;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

/// How long to wait for the whole reply. Long enough for a terminal over ssh,
/// short enough not to be a visible pause at startup.
const TIMEOUT: Duration = Duration::from_millis(120);

/// The sixteen ANSI colours as this terminal actually paints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Theme {
    slots: [Option<(u8, u8, u8)>; 16],
}

impl Theme {
    /// Ask the terminal for its palette, if `needed`.
    ///
    /// Asking is not free and it is not invisible: it writes escape sequences and
    /// reads the replies straight off the terminal. Skins that spell their
    /// colours out in hex never need it, so the common case says nothing at all.
    ///
    /// Must run before the alternate screen is entered and while nothing else is
    /// reading stdin.
    #[cfg(not(test))]
    pub fn query(needed: bool) -> Self {
        if !needed {
            return Self::default();
        }
        // Raw mode, or the terminal echoes every reply back as visible text and
        // line buffering holds them until the user presses Enter. Restored to
        // however it was found - the caller may already have set it up.
        let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
        if !was_raw && crossterm::terminal::enable_raw_mode().is_err() {
            return Self::default();
        }
        let theme = Self::query_via(&mut std::io::stdout(), &mut std::io::stdin());
        if !was_raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        theme
    }

    /// The palette without asking anything.
    #[cfg(test)]
    pub fn query(_needed: bool) -> Self {
        Self::default()
    }

    /// Resolve a colour to RGB, if it is one this theme can speak for.
    ///
    /// `Rgb` passes through - the skin already said exactly what it wanted.
    pub fn rgb(&self, color: Color) -> Option<(u8, u8, u8)> {
        match color {
            Color::Rgb(r, g, b) => Some((r, g, b)),
            other => index_of(other).and_then(|i| self.slots[i]),
        }
    }

    fn query_via<W: Write, R: Read + AsRawFd>(out: &mut W, input: &mut R) -> Self {
        let mut theme = Self::default();
        for i in 0..16 {
            if write!(out, "\x1b]4;{i};?\x07").is_err() {
                return theme;
            }
        }
        if out.flush().is_err() {
            return theme;
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
                    theme.absorb(&buf);
                    if theme.is_complete() {
                        break;
                    }
                }
            }
        }
        theme
    }

    fn is_complete(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    /// Pull every `4;n;rgb:…` reply out of whatever has arrived so far.
    fn absorb(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for reply in text.split('\x1b') {
            let Some(body) = reply.strip_prefix("]4;") else {
                continue;
            };
            let body = body
                .trim_end_matches(['\x07', '\\'])
                .trim_end_matches('\x1b');
            let Some((index, spec)) = body.split_once(';') else {
                continue;
            };
            let Ok(index) = index.trim().parse::<usize>() else {
                continue;
            };
            if index < 16
                && let Some(rgb) = parse_rgb(spec)
            {
                self.slots[index] = Some(rgb);
            }
        }
    }
}

/// Whether `fd` has something to read within `within`.
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

/// `rgb:RRRR/GGGG/BBBB`, with each channel 1-4 hex digits.
fn parse_rgb(spec: &str) -> Option<(u8, u8, u8)> {
    let spec = spec.trim().strip_prefix("rgb:")?;
    let mut parts = spec.split('/');
    let mut channel = || -> Option<u8> {
        let hex = parts.next()?.trim();
        if hex.is_empty() || hex.len() > 4 {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        // Terminals answer in whatever width they please: `ff`, `ffff`, even
        // `f`. Scale by the width rather than truncating, or `f` would read as 15
        // instead of white.
        let max = (1u32 << (4 * hex.len())) - 1;
        Some((value * 255 / max) as u8)
    };
    let (r, g, b) = (channel()?, channel()?, channel()?);
    parts.next().is_none().then_some((r, g, b))
}

/// The ANSI slot a named colour occupies.
fn index_of(color: Color) -> Option<usize> {
    Some(match color {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_read_back_as_rgb() {
        let mut theme = Theme::default();
        theme.absorb(b"\x1b]4;2;rgb:2929/cece/1010\x07");
        assert_eq!(theme.rgb(Color::Green), Some((41, 206, 16)));
    }

    #[test]
    fn replies_arrive_together_and_in_any_order() {
        // One write per index, but the terminal answers however it likes, and
        // the whole lot usually lands in a single read.
        let mut theme = Theme::default();
        theme.absorb(b"\x1b]4;9;rgb:ffff/0000/0000\x07\x1b]4;1;rgb:8080/0000/0000\x07");
        assert_eq!(theme.rgb(Color::LightRed), Some((255, 0, 0)));
        assert_eq!(theme.rgb(Color::Red), Some((128, 0, 0)));
    }

    #[test]
    fn a_partial_read_is_absorbed_once_the_rest_arrives() {
        // `absorb` is called on the whole buffer each time, so a reply split
        // across two reads is picked up when it completes rather than lost.
        let mut theme = Theme::default();
        let full = b"\x1b]4;2;rgb:2929/cece/1010\x07";
        theme.absorb(&full[..12]);
        assert_eq!(theme.rgb(Color::Green), None, "not yet");
        theme.absorb(full);
        assert_eq!(theme.rgb(Color::Green), Some((41, 206, 16)));
    }

    #[test]
    fn channels_scale_by_their_width() {
        // Terminals answer in 1 to 4 hex digits; `f` is white, not 15.
        assert_eq!(parse_rgb("rgb:f/f/f"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("rgb:ff/ff/ff"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("rgb:ffff/ffff/ffff"), Some((255, 255, 255)));
        // 0x8000/0xffff is a hair under a half, so this floors to 127.
        assert_eq!(parse_rgb("rgb:0000/8000/ffff"), Some((0, 127, 255)));
    }

    #[test]
    fn nonsense_is_ignored_rather_than_guessed_at() {
        for bad in [
            "",
            "rgb:",
            "rgb:zz/00/00",
            "rgb:00/00",
            "rgb:0/0/0/0",
            "#ff0000",
        ] {
            assert_eq!(parse_rgb(bad), None, "{bad:?} should not parse");
        }
        let mut theme = Theme::default();
        theme.absorb(b"\x1b]4;99;rgb:ffff/ffff/ffff\x07");
        assert_eq!(theme, Theme::default(), "an out-of-range index is dropped");
    }

    #[test]
    fn an_unanswered_query_leaves_named_colours_alone() {
        // The normal case on a terminal that does not implement OSC 4. Nothing
        // is invented: the caller is told it has no RGB for that colour.
        let theme = Theme::default();
        assert_eq!(theme.rgb(Color::Green), None);
        // A skin that named an exact colour never needed the terminal's help.
        assert_eq!(theme.rgb(Color::Rgb(1, 2, 3)), Some((1, 2, 3)));
    }

    #[test]
    fn the_query_gives_up_rather_than_blocking() {
        // A terminal that accepts the request and answers nothing must not hang
        // rav at startup. /dev/null is readable-at-EOF, which is the closest
        // stand-in for a terminal that will never reply.
        let mut sink = Vec::new();
        let mut null = std::fs::File::open("/dev/null").unwrap();
        let start = Instant::now();
        let theme = Theme::query_via(&mut sink, &mut null);
        assert!(start.elapsed() < TIMEOUT * 2, "took too long to give up");
        assert_eq!(theme, Theme::default());
        assert!(!sink.is_empty(), "it should still have asked");
    }
}
