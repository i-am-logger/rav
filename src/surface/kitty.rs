//! The kitty graphics protocol, as the three things rav says in it.
//!
//! An enum rather than a builder, so the combinations that mean nothing cannot
//! be written down: there is no way to ask for a delete with a width, or to
//! transmit a frame and forget its size. Every field the terminal needs for a
//! given command is a field of that variant.
//!
//! # Pixels do not go through base64
//!
//! A command is an APC string - `ESC _ G` … `ESC \` - and everything after the
//! `;` is base64 by the protocol's rules, because the payload shares a channel
//! with the escape that ends it. That would be a 33% tax on every frame, which
//! at 2400x1440 is 13.8MB becoming 18.4MB, sixty times a second, through a pty.
//!
//! So the frame is not the payload. It is written to a file and the *path* is
//! the payload: about thirty bytes encoded, while the pixels reach the terminal
//! as a plain write to shared memory. That is the whole reason rav uses `t=t`
//! rather than the direct transport, and it is why [`Payload`] exists as its own
//! type - the encoding is a property of the channel, not of a frame, and having
//! a name makes it obvious that no frame is passing through it.

use std::fmt::Write as _;
use std::path::Path;

/// The image id every frame is sent under.
///
/// One, reused. A fresh id per frame leaves the terminal holding every frame it
/// has ever been sent.
const IMAGE_ID: u32 = 1;

/// Something small enough to travel inside an escape sequence.
///
/// Base64 because the protocol says so - the payload shares a channel with the
/// escape that terminates it, so raw bytes could end the command early. **No
/// frame is ever one of these**; see the module note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload(String);

impl Payload {
    /// A path to a staged frame.
    ///
    /// Encoded rather than written out, because a path may hold a `,` or a `;`
    /// and the terminal would read those as more of the command.
    pub fn path(path: &Path) -> Self {
        Self(encode(path.to_string_lossy().as_bytes()))
    }

    /// Raw bytes - only ever a handful, for the query below.
    pub fn bytes(raw: &[u8]) -> Self {
        Self(encode(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One thing rav says to the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command<'a> {
    /// Draw a frame that has been staged where the terminal can read it.
    ///
    /// `bytes` is how much to read: a direct payload carries its own length and
    /// a file does not, so without it the terminal reads nothing, draws
    /// nothing, and reports success.
    Draw {
        frame: &'a Path,
        bytes: usize,
        width: u32,
        height: u32,
    },
    /// Ask whether images are supported at all. Draws nothing either way.
    Ask,
    /// Take every image off the screen.
    ///
    /// Every one, not one by one: a panic hook has no idea which ids are live,
    /// and a frame left behind is painted over the shell the user returns to.
    Erase,
}

impl Command<'_> {
    /// The escape sequence, ready to write.
    pub fn escape(&self) -> String {
        let mut out = String::from("\x1b_G");
        match self {
            Self::Draw {
                frame,
                bytes,
                width,
                height,
            } => {
                // `q=2` silences the acknowledgement. Sixty a second would
                // arrive on the descriptor rav reads keypresses from.
                //
                // `z=0` is the default and is written anyway: below zero the
                // frame sits under every cell's background, and a terminal
                // running any theme gives every cell one - so a negative z is a
                // blank screen with every frame transmitted successfully.
                //
                // `C=1` leaves the cursor alone. Without it the terminal parks
                // it past the frame's bottom-right, so the next frame is placed
                // from there and the picture walks off the screen.
                let _ = write!(
                    out,
                    "a=T,q=2,i={IMAGE_ID},f=32,s={width},v={height},z=0,C=1,S={bytes},t=t;{}",
                    Payload::path(frame).as_str(),
                );
            }
            Self::Ask => {
                // One opaque pixel, transmitted directly and only queried, so
                // there is nothing to stage and nothing appears.
                let _ = write!(
                    out,
                    "i={IMAGE_ID},s=1,v=1,a=q,t=d,f=24;{}",
                    Payload::bytes(&[0, 0, 0]).as_str(),
                );
            }
            Self::Erase => out.push_str("a=d,d=A"),
        }
        out.push_str("\x1b\\");
        out
    }
}

fn encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let b = [
            group[0],
            group.get(1).copied().unwrap_or(0),
            group.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= group.len() {
                out.push(ALPHABET[(n >> (18 - i * 6)) as usize & 0x3f] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        // The terminal decodes this. A wrong table hands it a path that does not
        // exist, and it draws nothing without saying so.
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(&[0xff, 0xef, 0xfe]), "/+/+", "the last two symbols");
    }

    #[test]
    fn drawing_says_everything_the_terminal_needs() {
        let escape = Command::Draw {
            frame: Path::new("/dev/shm/rav-1-0.rgba"),
            bytes: 40_000,
            width: 100,
            height: 100,
        }
        .escape();

        // Byte for byte the sequence measured at 60fps in WezTerm, so drifting
        // from the only form known to draw is a failure here rather than a
        // blank screen there.
        assert_eq!(
            escape,
            format!(
                "\x1b_Ga=T,q=2,i=1,f=32,s=100,v=100,z=0,C=1,S=40000,t=t;{}\x1b\\",
                encode(b"/dev/shm/rav-1-0.rgba"),
            ),
        );
    }

    #[test]
    fn a_path_travels_encoded_rather_than_raw() {
        // A `,` or a `;` in a path would otherwise be read as more of the
        // command, and the frame would be misread or dropped.
        let escape = Command::Draw {
            frame: Path::new("/tmp/a,b;c/rav-0.rgba"),
            bytes: 4,
            width: 1,
            height: 1,
        }
        .escape();
        assert!(!escape.contains("/tmp/a,b;c"), "the path went out raw");
        assert!(escape.contains(&encode(b"/tmp/a,b;c/rav-0.rgba")));
    }

    #[test]
    fn asking_draws_nothing_and_erasing_takes_everything() {
        let ask = Command::Ask.escape();
        assert!(ask.contains("a=q"), "a query, so nothing appears");
        assert!(!ask.contains("a=T"), "this would put a pixel on the screen");
        assert!(ask.contains("t=d"), "three bytes need no file");

        assert_eq!(Command::Erase.escape(), "\x1b_Ga=d,d=A\x1b\\");
    }

    #[test]
    fn every_command_is_a_complete_apc_string() {
        // An unterminated escape leaves the terminal consuming whatever follows
        // - including the next frame - as though it were part of this one.
        for command in [
            Command::Draw {
                frame: Path::new("/tmp/f"),
                bytes: 1,
                width: 1,
                height: 1,
            },
            Command::Ask,
            Command::Erase,
        ] {
            let escape = command.escape();
            assert!(escape.starts_with("\x1b_G"), "{command:?} is not an APC");
            assert!(escape.ends_with("\x1b\\"), "{command:?} is unterminated");
        }
    }

    #[test]
    fn a_frame_is_never_a_payload() {
        // The point of the file transport, and the thing a refactor would undo
        // first. A megabyte through `Payload` is a megabyte and a third through
        // the pty, sixty times a second.
        let frame = vec![0u8; 4096];
        let escape = Command::Draw {
            frame: Path::new("/dev/shm/f"),
            bytes: frame.len(),
            width: 32,
            height: 32,
        }
        .escape();
        assert!(
            escape.len() < 120,
            "the command grew with the frame: {} bytes",
            escape.len(),
        );
    }
}
