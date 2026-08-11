//! The terminal's own sixteen colours, and what a named ink resolves to.
//!
//! A theme may name a colour rather than spell it out, and the difference is
//! load-bearing: `#188408` is a promise about the hue, while `green` means
//! "whatever green is here". That deferral is the whole reason the `terminal`
//! and `mono` themes follow the palette a user already runs.
//!
//! Reading a terminal's palette is an OSC 4 conversation and belongs to a
//! surface. What lives here is everything that conversation produces and
//! everything done with it afterwards: the sixteen slots, the reply parser, and
//! the resolution a surface with no terminal to ask has to fall back on.

use crate::ink::{Colour, Ink};

/// What the terminal said its sixteen colours are.
///
/// Fixed size, `Copy`-able contents, no allocator: a target that compiles its
/// theme in can hold one in read-only memory and never write a byte to a
/// terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Palette {
    slots: [Option<(u8, u8, u8)>; 16],
}

impl Palette {
    /// A palette that answered, for tests and for a target that knows its own
    /// colours without asking anyone.
    ///
    /// `Palette::default()` is the terminal that said nothing, and a test built
    /// on it exercises the transform rather than what the running app feeds it -
    /// which is how a `mono` regression stayed invisible, since the case that
    /// breaks the theme is the one where the terminal *did* answer.
    pub fn answering(slots: [(u8, u8, u8); 16]) -> Self {
        Self {
            slots: slots.map(Some),
        }
    }

    /// Resolve a colour to RGB, if it is one this palette can speak for.
    ///
    /// `Rgb` passes through - the theme already said exactly what it wanted.
    /// A name resolves only when the terminal answered; `None` means the caller
    /// should leave the colour alone rather than invent one.
    pub fn rgb(&self, ink: Ink) -> Option<(u8, u8, u8)> {
        match ink {
            Ink::Rgb(r, g, b) => Some((r, g, b)),
            Ink::Ansi(i) => self.slots[usize::from(i) & 0x0f],
        }
    }

    /// Resolve a colour, falling back to the standard value for a name.
    ///
    /// For a surface with no terminal to ask, where `None` is not an answer - it
    /// would mean not drawing. Anything with a terminal should use
    /// [`rgb`](Self::rgb) and leave an unanswered name alone, so the theme still
    /// follows the palette the user runs.
    pub fn resolve(&self, ink: Ink) -> Colour {
        match self.rgb(ink) {
            Some((r, g, b)) => Colour::rgb(r, g, b),
            None => ink.resolved(),
        }
    }

    /// Whether every slot has been answered for.
    pub fn is_complete(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    /// Take what an OSC 4 reply said.
    ///
    /// The parser rather than the conversation: obtaining a reply needs a
    /// terminal, but reading one is arithmetic, and keeping it here is what lets
    /// it be tested without one.
    pub fn absorb(&mut self, bytes: &[u8]) {
        // Lossy rather than refusing: a reply split across two reads can cut a
        // multi-byte sequence, and the slots that did arrive whole are still
        // worth having.
        let text = alloc::string::String::from_utf8_lossy(bytes);
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

/// `rgb:rrrr/gggg/bbbb`, as a terminal answers it.
///
/// The channels may be any width - a terminal is free to answer `rgb:ff/00/00`
/// or `rgb:ffff/0000/0000` - so each is scaled by its own digit count rather
/// than assumed to be four.
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
    // A fourth channel is not a colour this understands, and guessing which
    // three of the four were meant is worse than declining.
    parts.next().is_none().then_some((r, g, b))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_read_back_as_rgb() {
        let mut palette = Palette::default();
        palette.absorb(b"\x1b]4;2;rgb:2929/cece/1010\x07");
        assert_eq!(
            palette.rgb(Ink::from_name("green").unwrap()),
            Some((41, 206, 16))
        );
    }

    #[test]
    fn replies_arrive_together_and_in_any_order() {
        // One write per index, but the terminal answers however it likes, and
        // the whole lot usually lands in a single read.
        let mut palette = Palette::default();
        palette.absorb(b"\x1b]4;9;rgb:ffff/0000/0000\x07\x1b]4;1;rgb:8080/0000/0000\x07");
        assert_eq!(
            palette.rgb(Ink::from_name("bright-red").unwrap()),
            Some((255, 0, 0))
        );
        assert_eq!(
            palette.rgb(Ink::from_name("red").unwrap()),
            Some((128, 0, 0))
        );
    }

    #[test]
    fn a_partial_read_is_absorbed_once_the_rest_arrives() {
        // `absorb` is called on the whole buffer each time, so a reply split
        // across two reads is picked up when it completes rather than lost.
        let mut palette = Palette::default();
        let full = b"\x1b]4;2;rgb:2929/cece/1010\x07";
        palette.absorb(&full[..12]);
        assert_eq!(
            palette.rgb(Ink::from_name("green").unwrap()),
            None,
            "not yet"
        );
        palette.absorb(full);
        assert_eq!(
            palette.rgb(Ink::from_name("green").unwrap()),
            Some((41, 206, 16))
        );
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
        let mut palette = Palette::default();
        palette.absorb(b"\x1b]4;99;rgb:ffff/ffff/ffff\x07");
        assert_eq!(
            palette,
            Palette::default(),
            "an out-of-range index is dropped"
        );
    }
}
