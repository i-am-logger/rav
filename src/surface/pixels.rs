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

use super::kitty::Command;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
        // The frame goes to the file exactly as it is. Nothing encodes it: the
        // command below carries the path, not the pixels.
        std::fs::write(&path, frame)?;
        let command = Command::Draw {
            frame: &path,
            bytes: frame.len(),
            width,
            height,
        };
        out.write_all(command.escape().as_bytes())?;

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

/// Take every image off the screen.
///
/// Belongs in a panic hook as much as in a clean exit: `panic = "abort"` still
/// runs one, and without this a panic leaves pixels painted over the shell the
/// user comes back to.
pub fn clear(out: &mut impl Write) -> io::Result<()> {
    out.write_all(Command::Erase.escape().as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_staged_frame_reaches_the_file_byte_for_byte() {
        // The reason `t=t` is the transport at all: what the terminal reads is
        // the frame itself, not an encoding of it. Anything that put the pixels
        // through the escape sequence would cost a third again in bytes and a
        // pass over the whole frame, sixty times a second.
        let dir = std::env::temp_dir().join(format!("rav-raw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let frame: Vec<u8> = (0..=255u8).cycle().take(4096).collect();

        let mut pixels = Pixels::new();
        let mut small = Vec::new();
        pixels.send(&mut small, &frame, 32, 32, &dir).unwrap();

        let staged = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap();
        assert_eq!(
            std::fs::read(staged.path()).unwrap(),
            frame,
            "not the frame"
        );

        // A hundredfold larger frame, and the same command but for the digits of
        // the byte count. Anything else means the pixels are being carried
        // rather than pointed at.
        let big = vec![0u8; frame.len() * 100];
        let mut large = Vec::new();
        pixels.send(&mut large, &big, 320, 320, &dir).unwrap();
        assert!(
            large.len().abs_diff(small.len()) <= 4,
            "{} bytes for a 4KB frame, {} for a 400KB one",
            small.len(),
            large.len(),
        );
        assert!(large.len() * 1000 < big.len(), "the command carried it");

        pixels.tidy(&dir);
        std::fs::remove_dir_all(&dir).ok();
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
