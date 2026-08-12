//! What rav actually sends a terminal, read from outside the process.
//!
//! Every other test of the pixel surface calls [`App::painted`] and inspects
//! the bytes it returns. This one runs the binary and reads what comes back up
//! a pty, so it covers the part those cannot: that the loop reaches the pixel
//! path at all, keeps reaching it, and leaves the terminal as it found it.
//!
//! # Why a pty has to be built by hand
//!
//! `script` and the usual harnesses give a pty whose `winsize` is `0x0`. rav
//! reads that as a terminal that does not know how big it is - deriving a cell
//! size from the font is how #63 began, so it declines to guess - and draws
//! block characters. The pixel path never runs, and every check here would
//! pass vacuously against a picture nobody drew.
//!
//! `openpty` takes a `winsize`, so one can be made with a real size in it. The
//! subtlety is that crossterm asks `/dev/tty` rather than stdout, so the child
//! needs the pty as its **controlling terminal**: `setsid`, then `TIOCSCTTY`.
//! Without that the size comes from whatever ran the tests, and the failure
//! looks exactly like a rav that cannot draw - so the harness says which it is.
//!
//! 80x24 at 2400x1440 is what `TIOCGWINSZ` reported in WezTerm on the machine
//! this was written on: 30x60 per cell, and 60 is not a multiple of eight,
//! which is the ladder spacing #63 is about.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const COLUMNS: u16 = 80;
const ROWS: u16 = 24;
const ACROSS: u16 = 2400;
const DOWN: u16 = 1440;

/// Long enough for the loop to settle into a rhythm, short enough for a test.
const WATCH: Duration = Duration::from_millis(1500);

/// After `q`. A process still up past this is a hang, and a hung test in CI is
/// a job timeout with no output rather than a failure with a reason.
const PATIENCE: Duration = Duration::from_secs(10);

struct OnAPty {
    child: Child,
    master: i32,
}

impl OnAPty {
    /// Run rav on a pty that reports a size in pixels.
    fn running(args: &[&str]) -> Self {
        let mut master = 0;
        let mut slave = 0;
        let mut want = libc::winsize {
            ws_row: ROWS,
            ws_col: COLUMNS,
            ws_xpixel: ACROSS,
            ws_ypixel: DOWN,
        };
        assert_eq!(
            unsafe {
                // A raw pointer rather than a reference: this argument is
                // `*mut winsize` on Darwin and `*const winsize` on Linux, and
                // only a `*mut` satisfies both - it coerces one way and not the
                // other.
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::from_mut(&mut want),
                )
            },
            0,
            "openpty",
        );

        // Before anything is spawned on it, so a harness that failed here is
        // never mistaken for a rav that would not draw.
        let mut carries = unsafe { std::mem::zeroed::<libc::winsize>() };
        assert_eq!(
            unsafe { libc::ioctl(slave, libc::TIOCGWINSZ as _, &mut carries) },
            0,
            "TIOCGWINSZ on the pty",
        );
        assert_eq!(
            (carries.ws_xpixel, carries.ws_ypixel),
            (ACROSS, DOWN),
            "the pty reports no size in pixels, so rav would draw glyphs",
        );

        // Close-on-exec, so the copies rav is given as 0/1/2 are the only ones
        // it gets. `dup2` clears the flag on the descriptor it writes, so the
        // three standard streams still work; what closes is the spare. An
        // ordinary `dup` leaks these into the child, and a child holding a
        // descriptor cargo is piping is a test run that finishes and hangs.
        let inherited = || unsafe {
            let fd = libc::fcntl(slave, libc::F_DUPFD_CLOEXEC, 0);
            assert!(fd >= 0, "F_DUPFD_CLOEXEC");
            Stdio::from_raw_fd(fd)
        };
        let child = unsafe {
            Command::new(env!("CARGO_BIN_EXE_rav"))
                .args(args)
                .stdin(inherited())
                .stdout(inherited())
                .stderr(inherited())
                .pre_exec(|| {
                    // A session of its own, then claim fd 0 as the controlling
                    // terminal - which is what `/dev/tty` resolves to, and what
                    // crossterm reads the size from.
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
                .spawn()
                .expect("rav")
        };
        unsafe { libc::close(slave) };
        Self { child, master }
    }

    /// Watch for a while, quit, and hand back everything the terminal saw.
    fn quit_after(mut self, watching: Duration) -> String {
        let reading = {
            let fd = unsafe { libc::dup(self.master) };
            std::thread::spawn(move || {
                let mut pty = unsafe { std::fs::File::from_raw_fd(fd) };
                let mut seen = Vec::new();
                let mut buf = [0u8; 1 << 16];
                // `EIO` rather than `Ok(0)` is how a pty ends when the far side
                // closes, so either one is the end of the run.
                while let Ok(read) = pty.read(&mut buf) {
                    if read == 0 {
                        break;
                    }
                    seen.extend_from_slice(&buf[..read]);
                }
                seen
            })
        };

        std::thread::sleep(watching);
        let mut keys = unsafe { std::fs::File::from_raw_fd(self.master) };
        keys.write_all(b"q").expect("send q");
        keys.flush().ok();

        let deadline = Instant::now() + PATIENCE;
        loop {
            match self.child.try_wait().expect("wait") {
                Some(status) => {
                    assert!(status.success(), "rav left with {status}");
                    break;
                }
                None if Instant::now() >= deadline => {
                    self.child.kill().ok();
                    panic!("rav was still running {PATIENCE:?} after `q`");
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        String::from_utf8_lossy(&reading.join().expect("reader")).into_owned()
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

/// The staged frames this run left behind, which should be none of them.
///
/// By pid, because two ravs can share a staging directory and a test that
/// counted every file would fail on someone else's.
fn frames_left_by(pid: u32) -> usize {
    ["/dev/shm", &std::env::temp_dir().to_string_lossy()]
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("rav-{pid}-"))
        })
        .count()
}

#[test]
fn the_pixel_surface_draws_and_gives_the_terminal_back() {
    // `--test-audio` opens no device, so this holds on a machine with no sound
    // card and no recording permission - which is every CI runner.
    let run = OnAPty::running(&["--surface", "kitty", "--test-audio"]);
    let pid = run.pid();
    let seen = run.quit_after(WATCH);

    let frames = seen.matches("\x1b_Ga=T").count();
    // Six rather than sixty: the bar is there to tell "drawing" from "drew one
    // and stopped", not to measure the frame rate, and a shared CI runner is
    // not the machine this was written on. The unoptimised build measured 84 in
    // this window here, so the margin is fourteen-fold before it is a flake.
    assert!(
        frames > 5,
        "the pixel path drew {frames} frames in {WATCH:?} - a terminal that \
         will not report a size in pixels draws none, and one frame is a loop \
         that ran once and stalled",
    );

    // A kitty image lands wherever the cursor is, and the terminal parks it
    // past the bottom-right of what it just drew - so a frame that did not home
    // first lands below the last one and the picture walks off the screen.
    assert_eq!(
        seen.matches("\x1b[H\x1b_Ga=T").count(),
        frames,
        "a frame was placed without homing the cursor first",
    );

    // The byte count is not advisory: without a correct `S` the terminal reads
    // nothing, draws nothing, and reports success.
    let bytes = u32::from(ACROSS) * u32::from(DOWN) * 4;
    assert_eq!(
        seen.matches(&format!("S={bytes},")).count(),
        frames,
        "a frame promised a size the picture is not",
    );

    // Every frame homes the cursor, so an unhidden one blinks on the bars.
    let hidden = seen.find("\x1b[?25l").expect("the cursor was left showing");
    assert!(
        hidden < seen.find("\x1b_Ga=T").expect("no frame at all"),
        "the cursor was hidden after the first picture was already up",
    );

    // Leaving on good terms: images down, out of the alternate screen, cursor
    // back. Without the first of those a quit leaves pictures over the shell.
    assert!(seen.contains("\x1b_Ga=d,d=A"), "the images were left up");
    assert!(seen.contains("\x1b[?1049l"), "left in the alternate screen");
    assert!(seen.ends_with("\x1b[?25h"), "the cursor was left hidden");

    // A terminal that fell behind leaves frames staged, and in `/dev/shm` those
    // are resident memory - megabytes each - outliving the process that made
    // them.
    assert_eq!(frames_left_by(pid), 0, "staged frames were not reclaimed");
}
