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
//!    growing? Bandwidth is not the risk - `t=s` is a memcpy into a mapped
//!    region - the terminal's image pipeline is.
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

    match probe() {
        Support::Yes => println!("kitty graphics: supported"),
        Support::No => {
            println!("kitty graphics: NOT supported (no reply within {PROBE_TIMEOUT:?})");
            return std::process::ExitCode::FAILURE;
        }
    }

    report_window_pixels();

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
}

impl Args {
    fn parse() -> Self {
        let mut args = Self {
            // rav's committed demo geometry, so the number means something.
            width: 941,
            height: 249,
            seconds: 10,
            transport: Transport::SharedMemory,
            occlusion: false,
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
                        _ => Transport::SharedMemory,
                    }
                }
                "--occlusion" => args.occlusion = true,
                _ => {}
            }
        }
        args
    }
}

/// How the pixels reach the terminal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Transport {
    /// Base64 in the escape sequence itself. Every byte crosses the pty, and
    /// base64 adds a third on top, so this is the arm that should lose.
    Direct,
    /// A temp file the terminal reads and unlinks. Portable everywhere.
    File,
    /// POSIX shared memory. The bytes never cross the pty at all.
    SharedMemory,
}

impl Transport {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct (t=d)",
            Self::File => "file (t=f)",
            Self::SharedMemory => "shared memory (t=s)",
        }
    }
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

    let mut frame = vec![0u8; frame_bytes];
    let mut out = std::io::stdout();
    let budget = Duration::from_secs(args.seconds);
    let target = Duration::from_micros(16_667);

    let started = Instant::now();
    let mut sent = 0u64;
    let mut late = 0u64;
    let mut worst = Duration::ZERO;

    while started.elapsed() < budget {
        let at = Instant::now();
        // Repaint every frame so nothing downstream can cache it and flatter
        // the result. A moving band is also visible, so a stalled pipeline is
        // obvious rather than silent.
        paint(&mut frame, args.width, sent);

        if transmit(&mut out, &frame, args, sent).is_err() {
            eprintln!("transmit failed after {sent} frames");
            break;
        }

        let took = at.elapsed();
        worst = worst.max(took);
        if took > target {
            late += 1;
        } else {
            std::thread::sleep(target - took);
        }
        sent += 1;
    }

    let elapsed = started.elapsed().as_secs_f64();
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

/// A moving band, so a stalled pipeline is visible rather than silent.
fn paint(frame: &mut [u8], width: u32, tick: u64) {
    let band = (tick % u64::from(width)) as usize;
    for (i, px) in frame.chunks_exact_mut(4).enumerate() {
        let x = i % width as usize;
        let lit = x.abs_diff(band) < 24;
        px[0] = if lit { 0x29 } else { 0x03 };
        px[1] = if lit { 0xce } else { 0x10 };
        px[2] = if lit { 0x10 } else { 0x01 };
        px[3] = 0xff;
    }
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
        "a=T,q=2,i={IMAGE_ID},f=32,s={},v={},z=-1",
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
            let path = std::env::temp_dir().join(format!("rav-spike-{tick}.rgba"));
            std::fs::write(&path, frame)?;
            let encoded = base64(path.to_string_lossy().as_bytes());
            write!(out, "\x1b_G{head},t=t;{encoded}\x1b\\")?;
        }
        Transport::SharedMemory => {
            let name = shm_write(frame, tick)?;
            let encoded = base64(name.as_bytes());
            write!(out, "\x1b_G{head},t=s;{encoded}\x1b\\")?;
        }
    }
    out.flush()
}

/// Publish `frame` in POSIX shared memory and hand back the name.
///
/// `/dev/shm` does not exist on macOS, so this is `shm_open(3)` rather than a
/// path. The name has a leading slash and must fit in 31 bytes there, which is
/// why it is short.
fn shm_write(frame: &[u8], tick: u64) -> std::io::Result<String> {
    let name = format!("/rav{}", tick % 4);
    let c_name = std::ffi::CString::new(name.clone())?;
    // SAFETY: a valid NUL-terminated name; the fd is checked before use and
    // closed on every path below.
    let fd = unsafe {
        libc::shm_open(
            c_name.as_ptr(),
            libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC,
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

// ── the occlusion question ──────────────────────────────────────────────────

/// Whether text drawn over an image at `z=-1` hides it.
///
/// This is the one that can invalidate the design. rav wants bars as pixels with
/// the status line and help overlay still real text on top. That works only if a
/// cell whose background is the terminal default leaves the image visible.
fn occlusion_check() -> std::process::ExitCode {
    let (w, h) = (400u32, 120u32);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    paint(&mut frame, w, 0);

    let mut out = std::io::stdout();
    println!("\nAn image with two lines of text over it.\n");
    let name = match shm_write(&frame, 0) {
        Ok(name) => name,
        Err(e) => {
            eprintln!("shm failed: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let encoded = base64(name.as_bytes());
    let _ = write!(
        out,
        "\x1b_Ga=T,q=2,i={IMAGE_ID},f=32,s={w},v={h},z=-1,t=s;{encoded}\x1b\\"
    );
    // Line one inherits the terminal's default background; line two sets one
    // explicitly. If only the second hides the image, ratatui's default-styled
    // cells are safe and the overlay plan holds.
    let _ = write!(
        out,
        "\x1b[2;3Hdefault background - image should show through"
    );
    let _ = write!(
        out,
        "\x1b[4;3H\x1b[48;2;0;0;0mexplicit background - expected to hide it\x1b[0m"
    );
    let _ = write!(out, "\x1b[8;1H");
    let _ = out.flush();

    println!("\n\nIf line one is readable AND the bars still show behind it, the");
    println!("overlay plan works. If line one blanks the image, status and help");
    println!("have to become scene geometry instead.\n");
    println!("Press Enter to clear.");
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
    fn a_painted_frame_is_opaque_everywhere() {
        // The kitty payload is sent as f32 RGBA and rav paints its own backdrop
        // across the whole area, so any transparent pixel is a bug in `paint`
        // rather than a design choice.
        let (w, h) = (16u32, 4u32);
        let mut frame = vec![0u8; (w * h * 4) as usize];
        paint(&mut frame, w, 3);
        assert!(frame.chunks_exact(4).all(|px| px[3] == 0xff));
    }

    #[test]
    fn the_band_moves_with_the_tick() {
        let (w, h) = (64u32, 2u32);
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
