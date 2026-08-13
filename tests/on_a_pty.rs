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
//!
//! # It waits on frames, never on a clock
//!
//! Nothing here sleeps. The reader counts pictures as they arrive and says so
//! on a channel; every wait is a blocking receive with a deadline. So the run
//! takes as long as rav takes and no longer, a slow machine is slow rather than
//! flaky, and "the pixel path is running" is carried by the wait itself rather
//! than by a count compared against a guess about frame rates.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

const COLUMNS: u16 = 80;
const ROWS: u16 = 24;
const ACROSS: u16 = 2400;
const DOWN: u16 = 1440;

/// The escape that carries a picture: transmit, and display it here.
const PICTURE: &str = "\x1b_Ga=T";

/// Enough frames to tell a running loop from one that drew once and stalled.
const ENOUGH: usize = 8;

/// How long any one wait will hold before it is a hang. Generous, because it is
/// never reached when things work - a shared runner drawing at a tenth of the
/// speed still arrives in a fraction of it - and because a hung test in CI is a
/// job timeout with no output rather than a failure with a reason.
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

    /// Wait for [`ENOUGH`] pictures, quit, and hand back all the terminal saw.
    fn quit_once_it_is_drawing(self) -> String {
        self.watched(&[None])
    }

    /// The same, resizing the window once it is drawing and waiting again.
    fn quit_after_resizing_to(self, size: libc::winsize) -> String {
        self.watched(&[Some(size), None])
    }

    /// Each entry is a stretch of drawing to wait through, and what to do to
    /// the window at the end of it.
    fn watched(mut self, stages: &[Option<libc::winsize>]) -> String {
        let (tally, counted) = channel();
        let reading = {
            let fd = unsafe { libc::dup(self.master) };
            assert!(fd >= 0, "dup of the pty");
            std::thread::spawn(move || {
                let mut pty = unsafe { std::fs::File::from_raw_fd(fd) };
                let mut seen = Vec::new();
                let mut buf = [0u8; 1 << 16];
                loop {
                    match pty.read(&mut buf) {
                        Ok(0) => break,
                        Ok(read) => {
                            seen.extend_from_slice(&buf[..read]);
                            // Counted here rather than timed out there, so the
                            // waiting side never guesses at a frame rate. A
                            // recount of the whole capture each time, because a
                            // picture's escape can straddle two reads and this
                            // is tens of kilobytes.
                            let so_far = String::from_utf8_lossy(&seen).matches(PICTURE).count();
                            // The far side stops listening once it has quit,
                            // which is ordinary rather than a failure.
                            tally.send(so_far).ok();
                        }
                        // `EIO` rather than end-of-file is how a pty finishes
                        // when the far side closes, so that one is the end of
                        // the run. Any other error truncates the capture, and
                        // an assertion answered by half the bytes is worse
                        // than one that fails.
                        Err(no) if no.raw_os_error() == Some(libc::EIO) => break,
                        Err(no) => panic!("the pty stopped being readable: {no}"),
                    }
                }
                seen
            })
        };

        let mut wanted = 0;
        for stage in stages {
            wanted += ENOUGH;
            wait_for(&counted, wanted);
            if let Some(mut size) = *stage {
                // Setting the size on the master is what a window drag is: the
                // kernel tells the far side, and rav asks again.
                assert_eq!(
                    unsafe {
                        libc::ioctl(
                            self.master,
                            libc::TIOCSWINSZ as _,
                            std::ptr::from_mut(&mut size),
                        )
                    },
                    0,
                    "TIOCSWINSZ",
                );
            }
        }

        let mut keys = unsafe { std::fs::File::from_raw_fd(self.master) };
        keys.write_all(b"q").expect("send q");
        keys.flush().ok();

        // Drain until the reader's end of the channel goes with it, which is
        // the pty closing, which is rav gone. A blocking wait on the process
        // ending, with a deadline, and nothing polled.
        loop {
            match counted.recv_timeout(PATIENCE) {
                Ok(_) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    self.child.kill().ok();
                    panic!("rav was still holding the terminal {PATIENCE:?} after `q`");
                }
            }
        }
        let status = self.child.wait().expect("wait");
        assert!(status.success(), "rav left with {status}");
        String::from_utf8_lossy(&reading.join().expect("reader")).into_owned()
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

/// Hold until that many pictures have gone out, or say how far it got.
///
/// This is the assertion that the pixel path is running, rather than anything
/// counted afterwards: a terminal that will not report a size in pixels draws
/// none of these, and a loop that drew one and stalled never reaches the total.
fn wait_for(counted: &Receiver<usize>, pictures: usize) {
    let mut so_far = 0;
    while so_far < pictures {
        match counted.recv_timeout(PATIENCE) {
            Ok(count) => so_far = count,
            Err(_) => panic!("{so_far} of {pictures} pictures arrived, then {PATIENCE:?} of none"),
        }
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
    let seen = run.quit_once_it_is_drawing();
    let frames = seen.matches(PICTURE).count();

    // A kitty image lands wherever the cursor is, and the terminal parks it
    // past the bottom-right of what it just drew - so a frame that did not home
    // first lands below the last one and the picture walks off the screen.
    assert_eq!(
        seen.matches(&format!("\x1b[H{PICTURE}")).count(),
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
        hidden < seen.find(PICTURE).expect("no frame at all"),
        "the cursor was hidden after the first picture was already up",
    );

    // Leaving on good terms: images down, out of the alternate screen, cursor
    // back. Without the first of those a quit leaves pictures over the shell.
    assert!(seen.contains("\x1b_Ga=d,d=A"), "the images were left up");
    assert!(seen.contains("\x1b[?1049l"), "left in the alternate screen");
    // The last word on the cursor has to be "show" - not the last bytes on the
    // wire, because a panic message or a stray line on stderr would land after
    // it and says nothing about the cursor either way.
    assert!(
        seen.rfind("\x1b[?25h") > seen.rfind("\x1b[?25l"),
        "the cursor was left hidden",
    );

    // A terminal that fell behind leaves frames staged, and in `/dev/shm` those
    // are resident memory - megabytes each - outliving the process that made
    // them.
    assert_eq!(frames_left_by(pid), 0, "staged frames were not reclaimed");
}

#[test]
fn a_window_drag_redraws_at_the_size_the_terminal_now_is() {
    // A cell size worked out once and kept is how #63 began. rav asks the
    // terminal on every frame instead, so a resize needs no special case - but
    // "needs no special case" is a claim about what goes out on the wire, and
    // the wire is out here. The plan calls re-placing on a resize a
    // non-negotiable; this is the only thing that checks it end to end.
    let narrower = libc::winsize {
        ws_row: 40,
        ws_col: 100,
        ws_xpixel: 1400,
        ws_ypixel: 1000,
    };
    let run = OnAPty::running(&["--surface", "kitty", "--test-audio"]);
    let seen = run.quit_after_resizing_to(narrower);

    // Reaching the second stretch of pictures at all is most of this: the wait
    // would have run out if the resize had stopped the drawing.
    let was = format!("s={ACROSS},v={DOWN},");
    let now = format!("s={},v={},", narrower.ws_xpixel, narrower.ws_ypixel);
    assert!(
        seen.matches(&was).count() >= ENOUGH,
        "nothing drawn at first"
    );
    assert!(
        seen.matches(&now).count() >= ENOUGH,
        "the window changed size and rav kept drawing at the old one",
    );

    // In that order, and with the byte count following the geometry: a frame
    // that promised the old size at the new one would be read as far as the
    // terminal believed and torn.
    assert!(
        seen.rfind(&was) < seen.find(&now),
        "a frame at the old size went out after the new size was known",
    );
    let bytes = u32::from(narrower.ws_xpixel) * u32::from(narrower.ws_ypixel) * 4;
    assert_eq!(
        seen.matches(&now).count(),
        seen.matches(&format!("S={bytes},")).count(),
        "a frame at the new size promised the wrong number of bytes",
    );
}
