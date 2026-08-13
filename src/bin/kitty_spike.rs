//! Measures whether a terminal can carry rav's frames as pixels.
//!
//! rav's bars are Unicode block glyphs, and the terminal decides how those are
//! drawn - which is why the same build looks different on Linux and macOS
//! (see issues #63 and #64). Drawing pixels instead removes that decision from
//! the terminal, but only if the terminal can actually keep up with a frame per
//! vblank. That is what this measures.
//!
//! Four questions, in order of how badly a wrong answer would hurt:
//!
//! 1. Does the terminal support the kitty graphics protocol at all?
//! 2. Can it retire 60 new images a second, sustained, without its own memory
//!    growing? Bandwidth is not the risk - a frame staged in tmpfs never reaches
//!    a disk - the terminal's image pipeline is.
//! 3. Does text drawn over an image at `z=-1` occlude it? rav wants bars as
//!    pixels with the help overlay and status line still real text. If a cell
//!    with a default background paints over the image, that plan does not work.
//! 4. Does `TIOCGWINSZ` report pixel dimensions, so a frame can be sized to the
//!    window rather than guessed from the cell count?
//!
//! Run it in the terminal being measured; it writes to stdout and needs a tty.
//!
//! ```text
//! cargo run --bin kitty_spike -- --width 941 --height 249 --seconds 10
//! cargo run --bin kitty_spike -- --transport file --seconds 10
//! cargo run --bin kitty_spike -- --occlusion
//! ```

use rav::render::{Band, BarLayout, Canvas, CapStyle, Colour, Draw, Ramp, Scene, Screen, Style};
use rav::units::{Length, Level};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

/// How long to wait for a terminal to answer the capability query.
///
/// The same budget `visual::theme` allows the palette query, for the same
/// reason: a terminal that does not implement this says nothing at all, so the
/// deadline is the only thing that ends the wait.
const PROBE_TIMEOUT: Duration = Duration::from_millis(120);

/// Payload bytes per escape chunk. The protocol's own limit.
const CHUNK: usize = 4096;

/// Rows the occlusion image covers. Enough that a written row can be surrounded
/// by clear ones on both sides.
const ROWS_COVERED: u32 = 8;

/// The image slot every frame is transmitted into.
///
/// One id reused rather than a new one per frame: a fresh id per frame at 60fps
/// is 216,000 images an hour for the terminal to track, which is the shape of
/// leak this is looking for.
const IMAGE_ID: u32 = 1;

fn main() -> std::process::ExitCode {
    let args = Args::parse();

    if !std::io::stdout().is_terminal_like() {
        eprintln!("kitty_spike needs a terminal on stdout");
        return std::process::ExitCode::FAILURE;
    }

    // A panic between entering the alternate screen and leaving it would hand
    // back a terminal with no cursor and rav's images still painted over the
    // shell. `panic = "abort"` still runs the hook, so this holds in release.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\\x1b[?25h\x1b[?1049l");
        let _ = out.flush();
        previous(info);
    }));

    match probe() {
        Support::Yes => println!("kitty graphics: supported"),
        Support::No => {
            println!("kitty graphics: NOT supported (no reply within {PROBE_TIMEOUT:?})");
            return std::process::ExitCode::FAILURE;
        }
    }

    report_window_pixels();

    if args.diagnose {
        return diagnose();
    }

    if args.occlusion {
        return occlusion_check();
    }

    measure(&args)
}

// ── arguments ───────────────────────────────────────────────────────────────

struct Args {
    width: u32,
    height: u32,
    seconds: u64,
    transport: Transport,
    occlusion: bool,
    diagnose: bool,
}

impl Args {
    fn parse() -> Self {
        let mut args = Self {
            // rav's committed demo geometry, so the number means something.
            width: 941,
            height: 249,
            seconds: 10,
            // Measured: the only transport that both works and stays cheap.
            transport: Transport::File,
            occlusion: false,
            diagnose: false,
        };
        let mut argv = std::env::args().skip(1);
        while let Some(flag) = argv.next() {
            let mut value = || argv.next().unwrap_or_default();
            match flag.as_str() {
                "--width" => args.width = value().parse().unwrap_or(args.width),
                "--height" => args.height = value().parse().unwrap_or(args.height),
                "--seconds" => args.seconds = value().parse().unwrap_or(args.seconds),
                "--transport" => {
                    args.transport = match value().as_str() {
                        "direct" => Transport::Direct,
                        "file" => Transport::File,
                        "shm" => Transport::SharedMemory,
                        _ => Transport::File,
                    }
                }
                "--occlusion" => args.occlusion = true,
                "--diagnose" => args.diagnose = true,
                _ => {}
            }
        }
        args
    }
}

/// How the pixels reach the terminal.
///
/// Measured against WezTerm: `Direct` and `File` draw, `SharedMemory` draws on
/// Linux and cannot draw on macOS. WezTerm reads an shm segment with
/// `File::read_exact` on the descriptor `shm_open` returns, which is a tmpfs file
/// on Linux and mmap-only on macOS - there `read(2)` gives `ENXIO` and `lseek`
/// gives `ESPIPE`. No sequence of bytes from this end changes that.
///
/// `File` closes the gap anyway: the path it sends may point into `/dev/shm`,
/// which WezTerm's `looks_like_temp_path` accepts explicitly. That is tmpfs, so
/// the frame is a shared-memory handoff that happens to be named by a path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Transport {
    /// Base64 in the escape sequence itself. Every byte crosses the pty, and
    /// base64 adds a third on top, so this is the arm that should lose.
    Direct,
    /// A file the terminal reads and unlinks, staged in `/dev/shm` where that
    /// exists. Portable, and the one that works everywhere it has been tried.
    File,
    /// POSIX shared memory by name. The bytes never cross the pty at all -
    /// cheapest where the terminal can read it, and macOS is not such a place.
    SharedMemory,
}

impl Transport {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct (t=d)",
            Self::File => "file (t=t)",
            Self::SharedMemory => "shared memory (t=s)",
        }
    }
}

/// Where a frame is staged for the terminal to read.
///
/// `/dev/shm` is tmpfs, so the bytes are written into RAM and never queued for a
/// disk - the same memory the `t=s` transport would have mapped, reached through
/// the transport that terminals actually implement. WezTerm names the directory
/// in `looks_like_temp_path`, so it still unlinks the frame after reading it.
///
/// macOS has no tmpfs and falls back to `$TMPDIR`. A frame there is created,
/// read and unlinked inside one frame interval, so it lives and dies in the page
/// cache and no write-back is ever issued for it.
fn frame_dir() -> &'static std::path::Path {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let shm = std::path::Path::new("/dev/shm");
        // Existence is not permission: a container can mount it read-only, and a
        // frame that fails to write is worse than one written to $TMPDIR.
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

/// Stage `frame` where the terminal can read it, and hand back the base64 of the
/// path that `t=t` carries.
///
/// The terminal unlinks the file as it reads it, so a name is good for exactly
/// one placement.
fn stage_frame(frame: &[u8], name: &str) -> std::io::Result<String> {
    let path = frame_dir().join(name);
    std::fs::write(&path, frame)?;
    Ok(base64(path.to_string_lossy().as_bytes()))
}

// ── capability probe ────────────────────────────────────────────────────────

enum Support {
    Yes,
    No,
}

/// Ask the terminal whether it speaks the kitty graphics protocol.
///
/// Transmits a 1×1 image with `a=q`, which asks for a reply without displaying
/// anything. A terminal that understands answers `OK`; one that does not says
/// nothing, and the deadline is what ends the wait.
///
/// Raw mode for the same reason `visual::theme` needs it: otherwise the reply is
/// echoed as visible text and line buffering holds it until Enter.
fn probe() -> Support {
    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && crossterm::terminal::enable_raw_mode().is_err() {
        return Support::No;
    }
    let answer = probe_via(&mut std::io::stdout(), &mut std::io::stdin());
    if !was_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    answer
}

fn probe_via<W: Write, R: Read + AsRawFd>(out: &mut W, input: &mut R) -> Support {
    // One opaque pixel, so the query is about capability rather than content.
    let pixel = base64(&[0u8, 0, 0]);
    if write!(out, "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;{pixel}\x1b\\").is_err() || out.flush().is_err()
    {
        return Support::No;
    }

    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut seen = Vec::new();
    let mut chunk = [0u8; 256];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if !readable(input.as_raw_fd(), left) {
            break;
        }
        match input.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                seen.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&seen);
                // Any `_G…;OK` is an answer. The id is echoed back, but a
                // terminal that replies at all has told us what we asked.
                if text.contains("_G") && text.contains(";OK") {
                    return Support::Yes;
                }
            }
        }
    }
    Support::No
}

/// Whatever the terminal said back, if it said anything.
///
/// Without `q=2` a transmit is acknowledged or refused, and the refusal carries
/// the reason. Suppressing it is right in the measurement loop - a reply per
/// frame at 60fps floods stdin - and wrong here, where the reason is the point.
fn read_reply() -> Option<String> {
    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && crossterm::terminal::enable_raw_mode().is_err() {
        return None;
    }
    let mut input = std::io::stdin();
    let mut seen = Vec::new();
    let mut chunk = [0u8; 256];
    let deadline = Instant::now() + PROBE_TIMEOUT;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if !readable(input.as_raw_fd(), left) {
            break;
        }
        match input.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => seen.extend_from_slice(&chunk[..n]),
        }
    }
    if !was_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    if seen.is_empty() {
        return None;
    }
    // The escape framing is noise in a printed message; the body is the answer.
    Some(
        String::from_utf8_lossy(&seen)
            .replace(['\x1b', '\\'], "")
            .trim()
            .to_string(),
    )
}

/// Whether `fd` has something to read within `within`.
fn readable(fd: i32, within: Duration) -> bool {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = i32::try_from(within.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: one initialised pollfd, a count matching it, and a timeout that
    // cannot be negative.
    unsafe { libc::poll(&raw mut poll, 1, ms) > 0 }
}

// ── the measurement ─────────────────────────────────────────────────────────

fn measure(args: &Args) -> std::process::ExitCode {
    let pixels = (args.width as usize) * (args.height as usize);
    let frame_bytes = pixels * 4;
    println!(
        "\nframe {}x{} RGBA = {:.2} MiB, transport {}, {}s",
        args.width,
        args.height,
        frame_bytes as f64 / (1024.0 * 1024.0),
        args.transport.name(),
        args.seconds,
    );
    if args.transport == Transport::File {
        let dir = frame_dir();
        println!(
            "staged in {} ({})",
            dir.display(),
            if dir == std::path::Path::new("/dev/shm") {
                "tmpfs - the frame never leaves RAM"
            } else {
                "no tmpfs here; page cache, unlinked within the frame"
            }
        );
    }

    let mut frame = vec![0u8; frame_bytes];
    let mut out = std::io::stdout();
    let budget = Duration::from_secs(args.seconds);
    let target = Duration::from_micros(16_667);

    // Draw on the alternate screen. The measurement scrolls the shell otherwise,
    // and - the reason it matters here - a scrolled screen moves the cursor,
    // which is where the terminal places the image.
    let _ = write!(out, "\x1b[?1049h\x1b[?25l\x1b[2J");
    let _ = out.flush();

    let started = Instant::now();
    let mut sent = 0u64;
    let mut late = 0u64;
    let mut worst = Duration::ZERO;
    let mut failure = None;

    while started.elapsed() < budget {
        let at = Instant::now();
        // Repaint every frame so nothing downstream can cache it and flatter
        // the result. A moving band is also visible, so a stalled pipeline is
        // obvious rather than silent.
        paint(&mut frame, args.width, sent);

        // Home the cursor first. A kitty image is placed at the cursor, so
        // without this the frame lands wherever the last one left it.
        let _ = write!(out, "\x1b[H");
        if let Err(e) = transmit(&mut out, &frame, args, sent) {
            failure = Some(format!("transmit failed after {sent} frames: {e}"));
            break;
        }

        let took = at.elapsed();
        worst = worst.max(took);
        if took > target {
            late += 1;
        } else {
            // Sleep most of it, then spin the last millisecond. `sleep` on macOS
            // overshoots by milliseconds, which reported 50fps for a loop that
            // was idle 90% of each frame - the granularity of the wait, not a
            // ceiling of the terminal. A visualiser pacing itself for real would
            // have the same problem.
            let left = target - took;
            if let Some(coarse) = left.checked_sub(Duration::from_millis(1)) {
                std::thread::sleep(coarse);
            }
            while at.elapsed() < target {
                std::hint::spin_loop();
            }
        }
        sent += 1;
    }

    let elapsed = started.elapsed().as_secs_f64();

    // Take the images down and give the shell back before reporting, or the
    // numbers are printed onto a screen that is about to be discarded.
    let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    if let Some(e) = failure {
        eprintln!("{e}");
    }

    let fps = sent as f64 / elapsed;
    println!(
        "\n  frames      {sent}\n  \
           sustained   {fps:.1} fps\n  \
           bytes       {:.1} MB/s\n  \
           over budget {late} of {sent} ({:.1}%)\n  \
           worst frame {:.2} ms",
        (sent as f64 * frame_bytes as f64) / elapsed / 1_000_000.0,
        if sent == 0 {
            0.0
        } else {
            late as f64 / sent as f64 * 100.0
        },
        worst.as_secs_f64() * 1000.0,
    );
    println!(
        "\n  Watch the terminal's own RSS alongside this. Bandwidth is not the\n  \
           risk; an image pipeline that grows is."
    );

    let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\");
    let _ = out.flush();
    std::process::ExitCode::SUCCESS
}

/// A moving wave, so a stalled pipeline is visible rather than silent.
///
/// Drawn through the real renderer rather than a copy of it: a spike painting
/// its own rectangles would measure a transport against pixels rav does not
/// actually produce.
fn paint(frame: &mut [u8], width: u32, tick: u64) {
    let height = (frame.len() / 4) as u32 / width.max(1);
    let screen = Screen::new(Length(width as f32), Length(height as f32));
    let layout = BarLayout::new(Length(12.0), Length(4.0));

    // A travelling wave rather than a still frame, so a stalled pipeline is
    // obvious - and it moves slowly enough to read.
    let phase = tick as f32 * 0.08;
    let bands: Vec<Band> = (0..layout.fitting_across(&screen))
        .map(|i| {
            let level = Level::new(((phase + i as f32 * 0.4).sin() * 0.5 + 0.5).max(0.05));
            // The cap rides above its bar, which is what makes the compositing
            // visible: in a terminal it would have to erase the backdrop to be
            // drawn at all.
            Band::new(level, Level::new(level.fraction() + 0.08))
        })
        .collect();

    let mut canvas = match Canvas::for_screen(&screen) {
        Some(canvas) => canvas,
        None => return,
    };
    let bars = Ramp::new(vec![Colour::rgb(0xe0, 0x22, 0x18)]);
    let grid = Ramp::new(vec![Colour::rgb(0x22, 0x04, 0x04)]);
    let styles = [Style {
        bars: &bars,
        grid: Some(&grid),
        cap: Some(CapStyle {
            colour: Colour::rgb(0xff, 0xc0, 0xb0),
            thickness: Length(3.0),
        }),
        ladder: None,
    }];
    Scene {
        bands: &bands,
        view: rav_appearance::View::Flat,
        layout,
        screen,
        styles: &styles,
    }
    .draw(&mut canvas);

    frame.copy_from_slice(&canvas.to_rgba());
}

fn transmit(
    out: &mut std::io::Stdout,
    frame: &[u8],
    args: &Args,
    tick: u64,
) -> std::io::Result<()> {
    // `q=2` suppresses both the OK and any error. Without it a reply per frame
    // at 60fps floods stdin and collides with key handling - and rav reads keys
    // off the same descriptor.
    let head = format!(
        // z=0, above the text, so the measurement is visible. A frame at z=-1
        // sits below every cell's background, and a terminal running any theme
        // gives every cell one - so the whole run showed a blank screen while
        // reporting 502 successful transmits. Whether text can be laid *over*
        // the image is the occlusion check's question, not this one's.
        // `C=1` leaves the cursor where it was. Without it the terminal parks the
        // cursor past the bottom-right of the image, so the next frame is placed
        // from there and the picture walks off the screen - which is what put the
        // bars in the bottom-right corner, clipped, rather than where they were
        // sent. The caller homes the cursor each frame; `C=1` is what keeps that
        // from being undone.
        "a=T,q=2,i={IMAGE_ID},f=32,s={},v={},z=0,C=1",
        args.width, args.height
    );
    match args.transport {
        Transport::Direct => {
            let payload = base64(frame);
            let mut rest = payload.as_bytes();
            let mut first = true;
            while !rest.is_empty() {
                let take = rest.len().min(CHUNK);
                let (now, later) = rest.split_at(take);
                let more = u8::from(!later.is_empty());
                if first {
                    write!(out, "\x1b_G{head},t=d,m={more};")?;
                } else {
                    write!(out, "\x1b_Gm={more};")?;
                }
                out.write_all(now)?;
                write!(out, "\x1b\\")?;
                rest = later;
                first = false;
            }
        }
        Transport::File => {
            // A name per frame, never reused. Reusing a slot would let this
            // truncate and rewrite a path a still-pending escape sequence names,
            // so the terminal would read a half-written frame or find the file
            // already unlinked - the one transport that works would start
            // dropping frames under exactly the lag it is meant to tolerate.
            let path = frame_dir().join(format!("rav-spike-{tick}.rgba"));
            std::fs::write(&path, frame)?;
            let encoded = base64(path.to_string_lossy().as_bytes());
            // `S` is how many bytes to read. Direct transmission carries its own
            // length in the payload; a file or a shared-memory object does not,
            // so without this the terminal has nothing to read and quietly
            // draws nothing.
            write!(out, "\x1b_G{head},S={},t=t;{encoded}\x1b\\", frame.len())?;
            // Reclaim the frame from eight ticks back. The terminal unlinks what
            // it reads, so this normally finds nothing and the error is dropped;
            // it matters when the terminal skipped that frame, because in
            // `/dev/shm` an abandoned frame is resident memory rather than a file
            // on a disk. Eight is 133ms of tolerated lag at 60fps.
            if let Some(stale) = tick.checked_sub(8) {
                let _ = std::fs::remove_file(frame_dir().join(format!("rav-spike-{stale}.rgba")));
            }
        }
        Transport::SharedMemory => {
            let name = shm_write(frame, tick)?;
            let encoded = base64(name.as_bytes());
            write!(out, "\x1b_G{head},S={},t=s;{encoded}\x1b\\", frame.len())?;
        }
    }
    out.flush()
}

/// Publish `frame` in POSIX shared memory and hand back the name.
///
/// `shm_open(3)` rather than a path, because macOS has no `/dev/shm` to write
/// into. The name has a leading slash and must fit in 31 bytes there, which is
/// why it is short.
///
/// The segment this creates is readable only by `mmap` on macOS, and WezTerm
/// reads it with `read(2)` - so this succeeds and the frame is still never drawn.
/// It stays because the question is the terminal's to answer, and on Linux the
/// same call produces a tmpfs file that WezTerm reads without complaint.
fn shm_write(frame: &[u8], tick: u64) -> std::io::Result<String> {
    let name = format!("/rav{}", tick % 4);
    let c_name = std::ffi::CString::new(name.clone())?;
    // Unlink first. macOS refuses a second `ftruncate` on an shm object that has
    // already been sized, so reusing a name fails once the rotation wraps -
    // which it does on the fifth frame. The terminal is supposed to unlink after
    // reading, but a dropped frame leaves the name behind and nothing else ever
    // clears it. Ignoring the result is deliberate: not existing is the common
    // case and is not an error.
    // SAFETY: a valid NUL-terminated name.
    unsafe { libc::shm_unlink(c_name.as_ptr()) };
    // SAFETY: a valid NUL-terminated name; the fd is checked before use and
    // closed on every path below.
    let fd = unsafe {
        libc::shm_open(
            c_name.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let len = frame.len();
    // SAFETY: fd is open and len is the size just computed from the slice.
    let sized = unsafe { libc::ftruncate(fd, len as libc::off_t) };
    if sized < 0 {
        // SAFETY: fd is open.
        unsafe { libc::close(fd) };
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: fd is open and sized to len, so the mapping is in bounds.
    let map = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if map == libc::MAP_FAILED {
        // SAFETY: fd is open.
        unsafe { libc::close(fd) };
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: map is a valid writable mapping of exactly len bytes, and frame
    // is len bytes; the regions cannot overlap.
    unsafe {
        std::ptr::copy_nonoverlapping(frame.as_ptr(), map.cast::<u8>(), len);
        libc::munmap(map, len);
        libc::close(fd);
    }
    // The terminal unlinks it once read; the rotation above keeps a dropped
    // frame from leaving a name behind forever.
    Ok(name)
}

// ── which transport actually works ──────────────────────────────────────────

/// Try each transport in turn and report what the terminal says about it.
///
/// Every arm draws the same small image at `z=0`, one at a time, and asks for a
/// reply rather than suppressing it. A terminal can accept a transmit and still
/// display nothing - the reply is what separates "refused, and here is why" from
/// "accepted but invisible", and those have completely different fixes.
fn diagnose() -> std::process::ExitCode {
    let (w, h) = (200u32, 60u32);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    paint(&mut frame, w, 0);
    let mut out = std::io::stdout();

    println!("\nEach transport draws the same 200x60 band. Watch for it.\n");

    for (label, arm) in [
        ("t=d  direct, base64 through the pty", Transport::Direct),
        ("t=t  temporary file", Transport::File),
        ("t=s  POSIX shared memory", Transport::SharedMemory),
    ] {
        let _ = write!(out, "\r\n{label}\r\n");
        let _ = out.flush();

        let head = format!("a=T,i={IMAGE_ID},f=32,s={w},v={h},z=0");
        let sent = match arm {
            Transport::Direct => {
                let payload = base64(&frame);
                let mut rest = payload.as_bytes();
                let mut first = true;
                let mut result = Ok(());
                while !rest.is_empty() && result.is_ok() {
                    let take = rest.len().min(CHUNK);
                    let (now, later) = rest.split_at(take);
                    let more = u8::from(!later.is_empty());
                    result = if first {
                        write!(out, "\x1b_G{head},t=d,m={more};")
                    } else {
                        write!(out, "\x1b_Gm={more};")
                    }
                    .and_then(|()| out.write_all(now))
                    .and_then(|()| write!(out, "\x1b\\"));
                    rest = later;
                    first = false;
                }
                result
            }
            Transport::File => match frame_dir()
                .join("rav-diagnose.rgba")
                .to_str()
                .map(str::to_owned)
            {
                Some(path) => std::fs::write(&path, &frame).and_then(|()| {
                    write!(
                        out,
                        "\x1b_G{head},S={},t=t;{}\x1b\\",
                        frame.len(),
                        base64(path.as_bytes())
                    )
                }),
                None => Ok(()),
            },
            // Both name forms are sent because the protocol never says whether
            // the leading slash POSIX wants at creation belongs in the name the
            // terminal receives, and a terminal looking for `//rav0` finds
            // nothing in a way that looks exactly like a silent failure.
            //
            // On macOS neither form can work: WezTerm reads the segment with
            // `read(2)`, which a Darwin shm object answers with `ENXIO`. Kept as
            // a probe because the answer is the terminal's, not this platform's,
            // and Linux stages the same object on tmpfs where reading it is fine.
            Transport::SharedMemory => shm_write(&frame, 0).and_then(|name| {
                let bare = name.trim_start_matches('/');
                write!(
                    out,
                    "\x1b_G{head},S={},t=s;{}\x1b\\",
                    frame.len(),
                    base64(name.as_bytes())
                )
                .and_then(|()| {
                    write!(out, "\r\n  and again without the leading slash\r\n")?;
                    write!(
                        out,
                        "\x1b_G{head},S={},t=s;{}\x1b\\",
                        frame.len(),
                        base64(bare.as_bytes())
                    )
                })
            }),
        };
        let _ = out.flush();

        match sent {
            Err(e) => println!("  local error: {e}"),
            Ok(()) => match read_reply() {
                Some(reply) if reply.contains("OK") => println!("  terminal: {reply}"),
                Some(reply) => println!("  terminal REFUSED: {reply}"),
                None => println!("  terminal said nothing (it may still have drawn it)"),
            },
        }
        println!("  Press Enter for the next transport.");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\");
        let _ = out.flush();
    }

    println!("\nWhichever arm showed a green band is the transport to build on.");
    std::process::ExitCode::SUCCESS
}

// ── the occlusion question ──────────────────────────────────────────────────

/// Whether a cell written over an image hides it.
///
/// The check that decides how the pixel surface draws. rav wants bars as pixels
/// with the status line and help overlay still real text on top, which needs a
/// cell at the terminal's default background to leave the image showing.
///
/// Measured in WezTerm, it does not: a row of nothing but spaces blanks the
/// image, and `z=1`, `z=0` and `z=-1` all give the same picture. A row with an
/// explicit background comes back in *that* colour rather than black, so cell
/// backgrounds are painted over the image rather than the image being dropped.
/// The consequence is that the surface must write only the cells it actually
/// wants text in - anything that paints the whole grid erases the frame.
fn occlusion_check() -> std::process::ExitCode {
    // A solid slab, not the bar pattern. Bars leave most of the frame
    // transparent, so a hidden region and a region that simply had no bar in it
    // look identical - the picture cannot distinguish the two answers it is here
    // to tell apart.
    //
    // Sized in whole cells so every row below is either wholly inside the image
    // or wholly outside it. Guessed dimensions put the bottom edge inside a row,
    // and a row showing the terminal's background is then ambiguous between
    // "occluded" and "past the end of the image".
    let (cell_w, cell_h) = cell_pixels().unwrap_or((30, 60));
    let (w, h) = (cell_w * 30, cell_h * ROWS_COVERED);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    for (i, px) in frame.chunks_exact_mut(4).enumerate() {
        // One band per covered row, so which row is which can be counted off
        // the screen instead of measured.
        let row = i as u32 / w;
        let light = (row / cell_h).is_multiple_of(2);
        px[0] = if light { 0x20 } else { 0x10 };
        px[1] = if light { 0xd0 } else { 0xa0 };
        px[2] = if light { 0x40 } else { 0x30 };
        px[3] = 0xff;
    }

    let mut out = std::io::stdout();

    // Two phases, because "nothing appeared" has two very different causes and
    // only one of them is about occlusion. Phase one puts the image above the
    // text: if that is invisible the transmit failed and z has nothing to do
    // with it. Errors are reported rather than suppressed - `q=2` would hide
    // exactly the message that explains a blank screen.
    for (phase, z, blurb) in [
        (
            1,
            1i32,
            "ABOVE text (z=1) - proves the image arrives, and that z works",
        ),
        (
            2,
            0i32,
            "z=0, the default - kitty puts only *positive* z above the text, so \
             this is expected to sit below it",
        ),
        (
            3,
            -1i32,
            "BELOW text (z=-1) - the question this is here to answer",
        ),
    ] {
        // The file transport, not `t=s`. This check asked its question through
        // the one transport that cannot answer on macOS, so both phases came
        // back blank for the reason phase one exists to rule out - it was
        // measuring the transport rather than occlusion.
        //
        // Staged per phase because the terminal unlinks a `t=t` file as it reads
        // it, so one staging cannot serve two placements.
        let encoded = match stage_frame(&frame, &format!("rav-occlusion-{phase}.rgba")) {
            Ok(encoded) => encoded,
            Err(e) => {
                eprintln!("could not stage the frame: {e}");
                return std::process::ExitCode::FAILURE;
            }
        };

        // Absolute positioning assumes a known screen, so clear it first.
        let _ = write!(out, "\x1b[2J\x1b[H");
        let _ = write!(out, "phase {phase}: {blurb}\r\n");
        let _ = out.flush();

        // Place it from a known row rather than wherever the cursor drifted to.
        let _ = write!(out, "\x1b[3;1H");
        let _ = write!(
            out,
            "\x1b_Ga=T,i={IMAGE_ID},f=32,s={w},v={h},z={z},C=1,S={},t=t;{encoded}\x1b\\",
            frame.len()
        );
        let _ = out.flush();

        // Rows 3..=10 carry the image. Rows 3, 5, 9 and 10 are left clear, so
        // the image's presence is never in doubt while reading the rest.
        //
        // Row 4 decides the design - ratatui writes default-styled cells, and if
        // those hide the image the overlay plan is dead whatever else happens.
        // On its own it cannot say why, though: "the image was blanked" and "the
        // terminal's default background was painted over it" are the same
        // picture. So two rows disambiguate it.
        //
        // - Row 8 sets an explicit *red* background. Red on screen means cell
        //   backgrounds paint over the image, which makes row 4's black the
        //   default background doing exactly that rather than an empty frame.
        // - Row 6 writes only spaces, at the default background, so no glyph is
        //   involved. If it hides the image too then it is the cell that
        //   occludes, not the character in it.
        let _ = write!(out, "\x1b[4;3H default background with text - THE QUESTION");
        let _ = write!(out, "\x1b[6;3H                              ");
        let _ = write!(
            out,
            "\x1b[8;3H\x1b[48;2;200;0;0m explicit RED background \x1b[0m"
        );
        let _ = write!(out, "\x1b[13;1H");
        let _ = out.flush();

        if let Some(reply) = read_reply() {
            println!("terminal replied: {reply}");
        }
        println!("Press Enter for the next phase.");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\");
        let _ = out.flush();
    }

    let _ = write!(out, "\x1b[2J\x1b[H");
    let _ = out.flush();
    println!("The image covers rows 3 to 10. Rows 3, 5, 9 and 10 carry nothing and");
    println!("must be green in every phase - if they are not, the image did not");
    println!("arrive and nothing else on the screen means anything.\n");
    println!("If all three phases look alike, the terminal gives a written cell");
    println!("precedence over the image whatever z says. That is an answer, not a");
    println!("failed run - it means z cannot be used to buy transparency.\n");
    println!("Then, in phase 3, read the three written rows together:");
    println!("  row 8, explicit red   -> red means cell backgrounds paint over the");
    println!("                           image. Black would mean the image is gone.");
    println!("  row 6, spaces only    -> if this hides it too, the cell occludes");
    println!("                           rather than the glyph in it.");
    println!("  row 4, THE QUESTION   -> green around the letters means ratatui's");
    println!("                           default cells let the image through, and");
    println!("                           bars as pixels with help and status as");
    println!("                           real text works. Black means every cell");
    println!("                           rav writes blanks the image, so it must");
    println!("                           write only where text actually goes.");
    println!("\nPress Enter to finish.");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\");
    let _ = out.flush();
    std::process::ExitCode::SUCCESS
}

// ── window geometry ─────────────────────────────────────────────────────────

/// Report the window size in pixels, if the terminal fills it in.
///
/// A pixel renderer needs device pixels, not cells. `ws_xpixel`/`ws_ypixel` are
/// the only way to ask, and plenty of terminals leave them zero - in which case
/// the size has to be inferred from the cell count, which is approximate.
/// Device pixels per cell, if the terminal reports them.
///
/// The occlusion check sizes its image in whole rows from this. Guessing a cell
/// height instead puts the image edge somewhere inside a row, and then a row
/// that shows the terminal's background is ambiguous - it could be occlusion, or
/// it could simply be past the bottom of the image.
fn cell_pixels() -> Option<(u32, u32)> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: stdout is a tty here (checked in main) and size is a valid,
    // initialised winsize for the duration of the call.
    let ok = unsafe {
        libc::ioctl(
            std::io::stdout().as_raw_fd(),
            libc::TIOCGWINSZ,
            &raw mut size,
        )
    };
    if ok < 0 || size.ws_xpixel == 0 || size.ws_ypixel == 0 || size.ws_col == 0 || size.ws_row == 0
    {
        return None;
    }
    Some((
        u32::from(size.ws_xpixel / size.ws_col),
        u32::from(size.ws_ypixel / size.ws_row),
    ))
}

fn report_window_pixels() {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: stdout is a tty here (checked in main) and size is a valid,
    // initialised winsize for the duration of the call.
    let ok = unsafe {
        libc::ioctl(
            std::io::stdout().as_raw_fd(),
            libc::TIOCGWINSZ,
            &raw mut size,
        )
    };
    if ok < 0 {
        println!("window pixels: TIOCGWINSZ failed");
        return;
    }
    if size.ws_xpixel == 0 || size.ws_ypixel == 0 {
        println!(
            "window pixels: not reported ({}x{} cells) - size must be inferred",
            size.ws_col, size.ws_row
        );
        return;
    }
    println!(
        "window pixels: {}x{} for {}x{} cells ({}x{} per cell)",
        size.ws_xpixel,
        size.ws_ypixel,
        size.ws_col,
        size.ws_row,
        size.ws_xpixel / size.ws_col.max(1),
        size.ws_ypixel / size.ws_row.max(1),
    );
}

// ── base64 ──────────────────────────────────────────────────────────────────

/// Standard base64, which is what the protocol asks for.
///
/// Hand-rolled rather than pulling a crate in for a measurement binary.
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

/// Whether stdout is a terminal.
///
/// A trait so the check reads as a question about the handle rather than a bare
/// libc call in the middle of `main`.
trait TerminalLike {
    fn is_terminal_like(&self) -> bool;
}

impl TerminalLike for std::io::Stdout {
    fn is_terminal_like(&self) -> bool {
        // SAFETY: isatty only reads the descriptor's type.
        unsafe { libc::isatty(self.as_raw_fd()) == 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_round_trips_arbitrary_bytes() {
        // Padding is what a hand-rolled encoder gets wrong, so check every
        // length modulo 3 against the decoded length rather than the bytes.
        for len in 0..64usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 7) as u8).collect();
            let encoded = base64(&bytes);
            assert_eq!(encoded.len(), len.div_ceil(3) * 4, "length for {len} bytes");
            assert_eq!(
                encoded.trim_end_matches('=').len(),
                (len * 8).div_ceil(6),
                "payload characters for {len} bytes"
            );
        }
    }

    #[test]
    fn a_painted_frame_is_opaque_where_it_draws_and_clear_where_it_does_not() {
        // The gaps between columns are deliberately transparent, so the
        // terminal's own background shows through them rather than rav painting
        // a slab over whatever theme the user runs. That makes the alpha channel
        // part of what the transport has to carry correctly, not padding.
        let (width, height) = (200u32, 60u32);
        let mut frame = vec![0u8; (width * height * 4) as usize];
        paint(&mut frame, width, 3);

        let alphas: Vec<u8> = frame.chunks_exact(4).map(|pixel| pixel[3]).collect();
        assert!(alphas.contains(&0xff), "the bars must be opaque");
        assert!(
            alphas.contains(&0x00),
            "the gaps between columns must stay clear"
        );
    }

    #[test]
    fn the_bars_move_with_the_tick() {
        // Tall enough for the bars to differ. At two pixels the caps alone fill
        // the frame and every tick looks the same, which says nothing about the
        // animation and everything about the viewport.
        let (w, h) = (200u32, 60u32);
        let mut first = vec![0u8; (w * h * 4) as usize];
        let mut later = first.clone();
        paint(&mut first, w, 0);
        paint(&mut later, w, 30);
        assert_ne!(first, later, "a still frame would flatter the measurement");
    }

    #[test]
    fn a_terminal_that_says_nothing_is_not_supported() {
        // The deadline is the only thing that ends the wait, so this is the
        // case that must not hang.
        let mut out = Vec::new();
        let mut input = std::fs::File::open("/dev/null").expect("/dev/null");
        let at = Instant::now();
        assert!(matches!(probe_via(&mut out, &mut input), Support::No));
        assert!(at.elapsed() < PROBE_TIMEOUT * 4, "gave up too slowly");
    }
}
