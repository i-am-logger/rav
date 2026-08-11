//! Sending a frame to the terminal as pixels.
//!
//! The escape sequences only, and the staging the file transport needs. Nothing
//! here decides what a frame contains or when to send one - it is the last mile,
//! and it is separate because every part of it is a rule the terminal imposes
//! rather than a choice rav makes.
//!
//! # The rules, each measured rather than read
//!
//! **`C=1`, and the caller homes the cursor.** An image lands wherever the
//! cursor is, and the terminal parks the cursor past the image's bottom-right
//! unless told otherwise - so each frame is placed from where the last one ended
//! and the picture walks off the screen.
//!
//! **`q=2`.** Every transmission is acknowledged by default. At 60fps that is
//! sixty replies a second arriving on the same descriptor rav reads keys from.
//!
//! **`S=` is not optional** for the file transport. Direct transmission carries
//! its length in the payload; a file does not, so without it the terminal reads
//! nothing, draws nothing, and reports success.
//!
//! **One image id, reused.** A new id per frame leaves the terminal holding
//! every frame ever sent.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The id every frame is sent under.
const IMAGE_ID: u32 = 1;

/// How long a frame may go unread before its staging file is reclaimed, in
/// frames. The terminal unlinks what it reads, so this normally finds nothing;
/// it matters when a frame was skipped, because in `/dev/shm` an abandoned
/// frame is resident memory rather than a file on a disk. Eight frames is 133ms
/// of tolerated lag at 60fps.
const LAG_TOLERATED: u64 = 8;

/// Where frames are staged for the terminal to read.
///
/// `/dev/shm` where it exists, which is tmpfs on Linux - so the handoff is
/// memory to memory and never reaches a disk. Falling back to the temp
/// directory elsewhere, which macOS backs with the page cache and which is
/// among the paths terminals will accept a frame from.
pub fn staging() -> &'static Path {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let shm = Path::new("/dev/shm");
        // Existing is not the same as writable: a container can mount it
        // read-only, and a frame that fails to write is worse than one written
        // somewhere slower.
        if shm.is_dir() {
            let probe = shm.join(format!("rav-probe-{}", std::process::id()));
            if std::fs::write(&probe, b"x").is_ok() {
                let _ = std::fs::remove_file(&probe);
                return shm.to_path_buf();
            }
        }
        std::env::temp_dir()
    })
    .as_path()
}

/// Sends frames, and cleans up after the ones the terminal did not read.
#[derive(Debug, Default)]
pub struct Pixels {
    sent: u64,
}

impl Pixels {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage `frame` and tell the terminal to draw it at the cursor.
    ///
    /// `frame` is RGBA, `width * height * 4` bytes. The caller homes the cursor
    /// first; see the module note on `C=1`.
    pub fn send(
        &mut self,
        out: &mut impl Write,
        frame: &[u8],
        width: u32,
        height: u32,
        dir: &Path,
    ) -> io::Result<()> {
        // A name per frame, never reused. Reusing one would truncate and rewrite
        // a path a still-pending escape sequence names, so the terminal would
        // read half a frame or find the file already gone - and it would happen
        // under exactly the lag this transport exists to tolerate.
        let path = dir.join(format!("rav-{}-{}.rgba", std::process::id(), self.sent));
        std::fs::write(&path, frame)?;
        out.write_all(&transmission(&path, frame.len(), width, height))?;

        if let Some(stale) = self.sent.checked_sub(LAG_TOLERATED) {
            let _ = std::fs::remove_file(dir.join(format!(
                "rav-{}-{}.rgba",
                std::process::id(),
                stale
            )));
        }
        self.sent += 1;
        out.flush()
    }

    /// Remove every frame this has staged and not yet reclaimed.
    ///
    /// For shutting down. In `/dev/shm` a leftover frame is resident memory
    /// that outlives the process.
    pub fn tidy(&self, dir: &Path) {
        let first = self.sent.saturating_sub(LAG_TOLERATED);
        for tick in first..self.sent {
            let _ =
                std::fs::remove_file(dir.join(format!("rav-{}-{}.rgba", std::process::id(), tick)));
        }
    }
}

/// The escape sequence that places a staged frame.
fn transmission(path: &Path, bytes: usize, width: u32, height: u32) -> Vec<u8> {
    let encoded = base64(path.to_string_lossy().as_bytes());
    // `z=0` is the default and is written anyway. Below zero puts the frame
    // under every cell's background, and a terminal running any theme gives
    // every cell one - so a negative z is a blank screen that still reports
    // every frame transmitted successfully.
    format!(
        "\x1b_Ga=T,q=2,i={IMAGE_ID},f=32,s={width},v={height},z=0,C=1,S={bytes},t=t;{encoded}\x1b\\"
    )
    .into_bytes()
}

/// Take every image off the screen.
///
/// Belongs in a panic hook as much as in a clean exit: `panic = "abort"` still
/// runs one, and without this a panic leaves pixels painted over the shell the
/// user comes back to.
pub fn clear(out: &mut impl Write) -> io::Result<()> {
    out.write_all(b"\x1b_Ga=d,d=A\x1b\\")?;
    out.flush()
}

fn base64(bytes: &[u8]) -> String {
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

    fn sequence(path: &str, bytes: usize, width: u32, height: u32) -> String {
        String::from_utf8(transmission(Path::new(path), bytes, width, height)).unwrap()
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        // The terminal decodes this; a wrong table sends it a path that does not
        // exist and it draws nothing, silently.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xef, 0xfe]), "/+/+", "the last two symbols");
    }

    #[test]
    fn a_transmission_carries_everything_the_terminal_needs() {
        let escape = sequence("/dev/shm/rav-1-0.rgba", 40_000, 100, 100);

        assert!(escape.starts_with("\x1b_G"), "not an APC string");
        assert!(escape.ends_with("\x1b\\"), "unterminated");
        assert!(escape.contains("S=40000"), "no size: it would draw nothing");
        assert!(escape.contains("C=1"), "the picture would walk off-screen");
        assert!(escape.contains("q=2"), "replies would flood the key reader");
        assert!(escape.contains("s=100,v=100"), "wrong extent");
        assert!(escape.contains("f=32"), "RGBA");
        assert!(escape.contains("t=t"), "a file the terminal unlinks");
        assert!(escape.contains(&format!("i={IMAGE_ID}")), "one id, reused");

        // Byte for byte the sequence measured at 60fps in WezTerm, so any
        // difference here is a difference from the only form known to draw.
        assert_eq!(
            escape,
            format!(
                "\x1b_Ga=T,q=2,i=1,f=32,s=100,v=100,z=0,C=1,S=40000,t=t;{}\x1b\\",
                base64(b"/dev/shm/rav-1-0.rgba"),
            ),
        );
    }

    #[test]
    fn the_path_travels_encoded_rather_than_raw() {
        // A raw path with a `;` or a `,` in it would be read as more of the
        // header, and the frame would be dropped or misread.
        let escape = sequence("/tmp/a,b;c/rav-0.rgba", 4, 1, 1);
        assert!(!escape.contains("/tmp/a,b;c"), "the path went out raw");
        assert!(escape.contains(&base64(b"/tmp/a,b;c/rav-0.rgba")));
    }

    #[test]
    fn clearing_removes_every_image_rather_than_one() {
        // `d=A` is all of them. A panic hook has no idea which ids are live, and
        // leaving one behind paints it over the user's shell.
        let mut out = Vec::new();
        clear(&mut out).unwrap();
        assert_eq!(out, b"\x1b_Ga=d,d=A\x1b\\");
    }

    #[test]
    fn every_frame_is_staged_under_a_name_of_its_own() {
        // Reusing a name rewrites a file a pending escape sequence still points
        // at, so the terminal reads half a frame - under exactly the lag this
        // transport exists to tolerate.
        let dir = std::env::temp_dir().join(format!("rav-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut pixels = Pixels::new();
        let mut out = Vec::new();
        for _ in 0..3 {
            pixels.send(&mut out, &[0u8; 16], 2, 2, &dir).unwrap();
        }
        let staged: std::collections::BTreeSet<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(staged.len(), 3, "frames shared a name: {staged:?}");

        pixels.tidy(&dir);
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "left frames behind"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_frame_older_than_the_tolerated_lag_is_reclaimed() {
        // In /dev/shm an abandoned frame is resident memory, so a terminal that
        // skips frames must not cost a megabyte a second.
        let dir = std::env::temp_dir().join(format!("rav-lag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut pixels = Pixels::new();
        let mut out = Vec::new();
        for _ in 0..(LAG_TOLERATED + 5) {
            pixels.send(&mut out, &[0u8; 16], 2, 2, &dir).unwrap();
        }
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            LAG_TOLERATED as usize,
            "the staging directory is growing without bound",
        );

        pixels.tidy(&dir);
        std::fs::remove_dir_all(&dir).ok();
    }
}
