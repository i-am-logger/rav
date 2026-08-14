//! A window of rav's own.
//!
//! The frame is already a `Pixmap` on the CPU by the time it gets here, so this
//! is a blit target rather than a second renderer - which is the whole reason
//! the plan chose softbuffer over wgpu. What arrives is what the kitty surface
//! sends a terminal; only the delivery differs.
//!
//! # What a cell is here
//!
//! The bar width is in cells, because cells are what `+` and `-` move, and a
//! window has none. It declares [`CELL`] instead - a nominal character box, so
//! those keys go on meaning "this many characters wide" as they do everywhere
//! else. It becomes the bitmap font's cell once the overlay has one; until
//! then it is a stated number rather than a measured one.
//!
//! # Why the loop is here and not in `App`
//!
//! winit wants the main thread and runs its own event loop, where `App::run`
//! is async and driven from `tokio::select!`. Rather than bend one into the
//! other, this drives the same three steps that loop does - take in audio,
//! [`App::advance`], [`App::frame_pixels`] - from winit's callbacks. The
//! terminal path is untouched, so a window cannot break a terminal.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use crossterm::event::KeyCode;
use flume::Receiver;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use tiny_skia::Pixmap;

use crate::audio::AudioData;
use crate::ui::App;

/// One character box, in device pixels.
///
/// Only the geometry uses it: bar width, spacing and the height of one rung.
/// A terminal is asked for its own; a window has to say.
const CELL: (u16, u16) = (10, 20);

/// What a window opens at. The demo is recorded at 941x249, and a banner is the
/// shape the analyser is for - a tall box gives the bars nowhere to go.
const OPENS_AT: (u32, u32) = (960, 320);

/// Repack a frame for softbuffer, resolving what is behind it.
///
/// softbuffer wants one `u32` per pixel as `0x00RRGGBB`, and
/// [`Canvas::to_rgba`](crate::render::Canvas::to_rgba) hands back four straight
/// bytes - demultiplied, alpha kept.
///
/// **The alpha has to be resolved here, and against black.** A terminal
/// composites rav's frame over whatever the cell behind it was, so the kitty
/// surface passes partial alpha on and lets it. A window has nothing behind it
/// at all, so an unresolved edge would arrive as whatever the last contents of
/// that buffer happened to be. Multiplying onto black is what "nothing behind
/// it" means, and it keeps an antialiased bar edge reading as a soft edge
/// rather than a bright one.
///
/// Writes into `into` rather than returning, because this runs on every frame
/// and a 2400x1440 window is 3.5 million pixels - a fresh allocation sixty
/// times a second is the trap the canvas already avoids being rebuilt for.
pub fn onto_black(rgba: &[u8], into: &mut Vec<u32>) {
    into.clear();
    into.reserve(rgba.len() / 4);
    into.extend(rgba.chunks_exact(4).map(|pixel| {
        let [red, green, blue, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        // `u16` for the product, then back down: `r * a` overflows a `u8` for
        // anything above the very darkest, and dividing by 255 rather than
        // shifting by 8 keeps full scale exactly full scale.
        let over_black = |channel: u8| (u16::from(channel) * u16::from(alpha) / 255) as u32;
        (over_black(red) << 16) | (over_black(green) << 8) | over_black(blue)
    }));
}

/// What a window key is, said in the terminal's vocabulary.
///
/// Translated rather than mapped straight to an [`Action`](crate::ui::Action),
/// so [`map_key`](crate::ui::map_key) stays the one place that decides what a
/// key does. A second table here would be a window whose `f` cycled something
/// else six months from now, and nothing would notice until someone used both.
///
/// `None` for a key rav has no use for - a modifier on its own, a function key
/// past the one - which is left to fall through rather than reported.
fn pressed(key: &Key) -> Option<KeyCode> {
    Some(match key {
        // `to_text` rather than the character variant alone: it is what gives
        // `+` and `-` on a keyboard that needs a modifier to reach them.
        Key::Named(NamedKey::Escape) => KeyCode::Esc,
        Key::Named(NamedKey::Tab) => KeyCode::Tab,
        Key::Named(NamedKey::Space) => KeyCode::Char(' '),
        Key::Named(NamedKey::ArrowUp) => KeyCode::Up,
        Key::Named(NamedKey::ArrowDown) => KeyCode::Down,
        Key::Named(NamedKey::F1) => KeyCode::F(1),
        other => KeyCode::Char(other.to_text()?.chars().next()?),
    })
}

/// What the title bar says: rav, and whatever rav has to say.
///
/// The status line has to go *somewhere*, and a window has no text of its own
/// until the overlay has a font. The desktop already draws a title, so a
/// setting that changed is visible - pressing `t` and seeing nothing happen to
/// the words is how a key looks broken.
///
/// Only the status. The help panel is fourteen rows and does not fit in a
/// title bar, which is why the font is still owed.
fn titled(status: Option<&str>) -> String {
    match status {
        Some(saying) => format!("rav - {saying}"),
        None => "rav".to_string(),
    }
}

/// The size `App` asks for, built from a window rather than from a terminal.
///
/// `WindowSize` is crossterm's, and carries pixels and cells together. A window
/// knows the pixels and decides the cells, which is the one thing the two pixel
/// surfaces cannot share.
fn sized(width: u32, height: u32) -> crossterm::terminal::WindowSize {
    let (across, down) = CELL;
    crossterm::terminal::WindowSize {
        width: width.min(u32::from(u16::MAX)) as u16,
        height: height.min(u32::from(u16::MAX)) as u16,
        columns: (width.min(u32::from(u16::MAX)) as u16 / across).max(1),
        rows: (height.min(u32::from(u16::MAX)) as u16 / down).max(1),
    }
}

struct Showing {
    app: App,
    audio: Receiver<AudioData>,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    /// Kept between frames for the same reason the canvas is: a screenful of
    /// `u32` per frame is an allocation the frame budget has no room for.
    packed: Vec<u32>,
    /// Where the help panel is drawn before it is composited over the frame.
    ///
    /// Its own surface because `frame_pixels` hands back bytes rather than a
    /// `Pixmap`, and kept between frames for the reason the canvas is: this
    /// is a screenful, and a screenful allocated per frame is the trap the
    /// rest of this loop avoids. Rebuilt only when the window resizes.
    overlay: Option<Pixmap>,
    /// What the title already says, so a frame that changed nothing does not
    /// ask the window server to set it again.
    titled: String,
    /// The frame ceiling, kept here because winit drives this loop and
    /// `App::run` - which has the same one - does not.
    min_frame: std::time::Duration,
    next_frame: Instant,
    /// Frames, spectra and wakes over the last second - the terminal loop
    /// reports the first and last of those, and this adds the middle one
    /// because winit wakes far more often than it is asked to and the count
    /// of FFTs is the thing that costs.
    ///
    /// Also the only evidence from outside that this surface draws rather than
    /// merely runs: a window that opened and paints nothing looks exactly like
    /// one that works, from here.
    drawn: u32,
    shown: u32,
    measured: u32,
    woke: u32,
    counted_from: Instant,
    failed: Option<anyhow::Error>,
}

impl ApplicationHandler for Showing {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        // Called again on platforms that suspend, so this must not open a
        // second window on the way back.
        if self.window.is_some() {
            return;
        }
        match self.open(events) {
            Ok(()) => {}
            Err(why) => {
                self.failed = Some(why);
                events.exit();
            }
        }
    }

    fn window_event(&mut self, events: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(code) = pressed(&event.logical_key) {
                    self.app.apply(crate::ui::map_key(code));
                    if self.app.wants_to_quit() {
                        events.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(why) = self.draw() {
                    self.failed = Some(why);
                    events.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        // The audio arrives on a channel winit knows nothing about, so there is
        // no event to be woken by and the loop has to come back on its own. It
        // comes back *at a time*, not immediately: `Poll` with a redraw every
        // pass spins the event loop as fast as the machine will go and pegs a
        // core, which is what the rest of rav is built not to do.
        //
        // The ceiling is `App`'s, not one of this surface's own. `App::run` has
        // the same deadline for the same reason; what it does not have is any
        // way to lend it, since it does not drive this loop.
        // On every wake, above the ceiling, exactly as `App::run` does it: only
        // the drawing is capped, and the analysis runs on the signal. Putting
        // these below the deadline would make the interval between spectra a
        // property of the frame rate, which is what `App::advance` says not to
        // do - the ballistics integrate `dt` and would survive it, but anything
        // measuring across time rather than within one frame would inherit the
        // jitter into its measurement.
        //
        // What a window still cannot do is wake *because* audio arrived: winit
        // knows nothing about the channel, so a wake is the frame deadline or a
        // user event. Waking on a buffer would need an `EventLoopProxy` fed
        // from the receiving side, which is a thread and a second event type -
        // worth it when something here measures across frames, and not before.
        let now = Instant::now();
        self.woke += 1;
        if self.counted_from.elapsed() >= std::time::Duration::from_secs(1) {
            tracing::debug!(
                "{} of {} frames shown, {} spectra, from {} wakes",
                self.shown,
                self.drawn,
                self.measured,
                self.woke
            );
            self.drawn = 0;
            self.shown = 0;
            self.measured = 0;
            self.woke = 0;
            self.counted_from = now;
        }
        let mut arrived = false;
        while let Ok(data) = self.audio.try_recv() {
            self.app.push_samples(&data.samples);
            arrived = true;
        }
        // When there is something new to measure, or when a frame is due
        // anyway. Not on every wake: winit returns here far more often than it
        // was asked to - measured at 180 to 376 times a second against a 60Hz
        // deadline - and `advance` is a 1024-point FFT. Analysing the same
        // unchanged window three times over is work nobody sees.
        //
        // The frame-due half is what keeps the bars falling when the audio
        // stops entirely, which is the case `App::run` keeps a watchdog for.
        if arrived || now >= self.next_frame {
            self.app.advance();
            self.measured += 1;
        }
        if now >= self.next_frame {
            // A moving deadline, carrying the remainder - the form that asks
            // "has min_frame passed since the last frame" quantises to the wake
            // interval and loses whatever does not divide it.
            self.next_frame += self.min_frame;
            // And no debt. A stall would otherwise leave the deadline in the
            // past owing frames it would take back to back, and the next frame
            // shows the current state either way.
            if self.next_frame < now {
                self.next_frame = now + self.min_frame;
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        events.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

impl Showing {
    fn open(&mut self, events: &ActiveEventLoop) -> Result<()> {
        let attributes = Window::default_attributes()
            .with_title(titled(None))
            .with_inner_size(winit::dpi::LogicalSize::new(OPENS_AT.0, OPENS_AT.1));
        let window = Rc::new(
            events
                .create_window(attributes)
                .context("opening a window")?,
        );
        let context = softbuffer::Context::new(window.clone())
            .map_err(|why| anyhow::anyhow!("no drawing context for the window: {why}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|why| anyhow::anyhow!("nothing to draw on: {why}"))?;
        self.window = Some(window);
        self.surface = Some(surface);
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        // The size and the title first, and the window borrow released before
        // anything else: the frame and the panel below both want `&mut self`,
        // and the surface is only needed for the blit at the end.
        let Some(size) = self.window.as_ref().map(|w| w.inner_size()) else {
            return Ok(());
        };
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // A window dragged to nothing, or minimised. Not a failure, and
            // drawing zero pixels into it would be.
            return Ok(());
        };

        self.drawn += 1;

        // The audio and the analysis happened in `about_to_wait`, on every wake
        // rather than on every frame. This is only the drawing.

        // Only when it changes. Setting a title is a round trip to the window
        // server, and this runs on every frame.
        let saying = titled(self.app.status_line().as_deref());
        if saying != self.titled {
            if let Some(window) = &self.window {
                window.set_title(&saying);
            }
            self.titled = saying;
        }

        let Some(pixels) = self.app.frame_pixels(&sized(size.width, size.height)) else {
            return Ok(());
        };
        onto_black(&pixels, &mut self.packed);

        // Over the frame rather than into it: the panel is opaque and the bars
        // behind it are not meant to show through, which is what makes it
        // readable over a moving picture.
        if self.app.showing_help() {
            self.draw_help(size.width, size.height);
        }

        let Some(surface) = &mut self.surface else {
            return Ok(());
        };
        surface
            .resize(width, height)
            .map_err(|why| anyhow::anyhow!("the window would not take that size: {why}"))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|why| anyhow::anyhow!("no buffer to draw into: {why}"))?;
        // A frame that does not fill the buffer is a resize caught mid-flight;
        // showing it stretched is worse than showing the last one again.
        //
        // Counted rather than passed over in silence. If the two ever stopped
        // agreeing for good, this would be a window that draws nothing and
        // says nothing about it - and the count sits in the same line as the
        // frames, where a window drawing 0 of 60 is impossible to miss.
        if buffer.len() == self.packed.len() {
            buffer.copy_from_slice(&self.packed);
            buffer
                .present()
                .map_err(|why| anyhow::anyhow!("the frame would not go up: {why}"))?;
            self.shown += 1;
        }
        Ok(())
    }
    /// Draw the help panel over the packed frame.
    ///
    /// Onto a surface of its own and then composited, because `frame_pixels`
    /// hands back bytes and the text drawing works on a `Pixmap`.
    ///
    /// That surface is the size of the **panel**, not the window. A
    /// window-sized one would mean clearing and then scanning a few million
    /// pixels every frame the panel is open, to find the few hundred thousand
    /// it covers.
    fn draw_help(&mut self, across: u32, down: u32) {
        use crate::surface::text;

        let rows = self.app.help_rows();
        let Some((x, y, wide, tall)) =
            text::help_area(&text::Font8x8, &rows, "rav", (across, down))
        else {
            return;
        };

        let fits = self
            .overlay
            .as_ref()
            .is_some_and(|had| had.width() == wide && had.height() == tall);
        if !fits {
            self.overlay = Pixmap::new(wide, tall);
        }
        let Some(overlay) = &mut self.overlay else {
            return;
        };

        let colours = self.app.overlay_colours();
        text::help(overlay, &text::Font8x8, &rows, "rav", colours);
        over(&mut self.packed, across, overlay, (x, y));
    }
}

/// Show rav in a window until it is closed.
///
/// Takes the thread, because winit does. The audio is already arriving on a
/// channel fed from somewhere else, which is what makes that survivable.
pub fn show(app: App, audio: Receiver<AudioData>) -> Result<()> {
    let events = EventLoop::new().context("starting an event loop")?;
    let min_frame = app.min_frame();
    let mut showing = Showing {
        app,
        audio,
        window: None,
        surface: None,
        packed: Vec::new(),
        titled: String::new(),
        overlay: None,
        min_frame,
        next_frame: Instant::now(),
        drawn: 0,
        shown: 0,
        measured: 0,
        woke: 0,
        counted_from: Instant::now(),
        failed: None,
    };
    events.run_app(&mut showing).context("running the window")?;
    match showing.failed {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

/// Composite an overlay onto a packed frame at `(x, y)`, in place.
///
/// Touches only the rows and columns the overlay occupies - it is the panel,
/// not a screenful - and within those, only pixels it actually painted. A
/// transparent pixel leaves the frame showing, which is what makes this a panel
/// over a picture rather than a second picture.
///
/// Anything partly transparent is taken at full strength rather than blended:
/// the panel is drawn opaque on purpose, because a half-transparent help row
/// over moving bars is unreadable.
fn over(packed: &mut [u32], frame_width: u32, overlay: &Pixmap, (x, y): (u32, u32)) {
    for row in 0..overlay.height() {
        let start = ((y + row) * frame_width + x) as usize;
        let Some(line) = packed.get_mut(start..start + overlay.width() as usize) else {
            // The frame changed size between being packed and being covered.
            // The next one is drawn at the new size, so this one is skipped
            // rather than written past its end.
            return;
        };
        for (column, under) in line.iter_mut().enumerate() {
            let Some(pixel) = overlay.pixel(column as u32, row) else {
                continue;
            };
            if pixel.alpha() == 0 {
                continue;
            }
            let straight = pixel.demultiply();
            *under = (u32::from(straight.red()) << 16)
                | (u32::from(straight.green()) << 8)
                | u32::from(straight.blue());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(rgba: [u8; 4]) -> u32 {
        let mut out = Vec::new();
        onto_black(&rgba, &mut out);
        out[0]
    }

    #[test]
    fn an_opaque_pixel_keeps_its_colour() {
        assert_eq!(one([0xff, 0xff, 0xff, 0xff]), 0x00ff_ffff, "white");
        assert_eq!(one([0xff, 0x00, 0x00, 0xff]), 0x00ff_0000, "red");
        assert_eq!(one([0x00, 0xff, 0x00, 0xff]), 0x0000_ff00, "green");
        assert_eq!(one([0x00, 0x00, 0xff, 0xff]), 0x0000_00ff, "blue");
    }

    #[test]
    fn full_scale_survives_the_divide() {
        // The reason this is `/ 255` and not `>> 8`: shifting turns 255 into
        // 254, so a white bar in a window would be a shade under white and the
        // two pixel surfaces would not draw the same picture.
        assert_eq!(one([0xff, 0xff, 0xff, 0xff]) & 0xff, 0xff);
    }

    #[test]
    fn a_soft_edge_reads_as_dark_rather_than_as_bright() {
        // Half-covered white is mid grey, not white. Getting this backwards -
        // ignoring alpha - is what turns every antialiased bar edge into a
        // bright fringe.
        let half = one([0xff, 0xff, 0xff, 0x80]);
        assert_eq!(half, 0x0080_8080, "got {half:#08x}");
        assert_eq!(one([0xff, 0xff, 0xff, 0x00]), 0, "nothing there is black");
    }

    #[test]
    fn every_pixel_arrives_and_none_is_invented() {
        let frame: Vec<u8> = (0..64u8).collect();
        let mut out = Vec::new();
        onto_black(&frame, &mut out);
        assert_eq!(out.len(), 16, "16 pixels of 4 bytes");

        // Reused across frames, so it must not accumulate. A window that grew
        // its buffer by a screenful per frame would look fine and then run out
        // of memory somewhere in the second minute.
        onto_black(&frame, &mut out);
        assert_eq!(out.len(), 16, "the buffer was not cleared");
    }

    #[test]
    fn the_top_byte_is_left_alone() {
        // softbuffer reads `0x00RRGGBB` and ignores the top byte on some
        // platforms and not others. Leaving it zero is the portable answer.
        for alpha in [0x00, 0x7f, 0xff] {
            assert_eq!(one([0xff, 0xff, 0xff, alpha]) >> 24, 0, "alpha {alpha:#x}");
        }
    }

    fn typed(text: &str) -> Option<crate::ui::Action> {
        pressed(&Key::Character(text.into())).map(crate::ui::map_key)
    }

    #[test]
    fn a_window_key_does_what_the_same_key_does_in_a_terminal() {
        use crate::ui::Action;

        // Not a second key table: these go through `map_key`, so what is
        // asserted is the translation, and what each key *means* stays decided
        // in one place. A window whose `f` cycled something else would be a
        // defect nobody found until they used both.
        assert_eq!(typed("q"), Some(Action::Quit));
        assert_eq!(typed("t"), Some(Action::CycleTheme));
        assert_eq!(typed("v"), Some(Action::CycleView));
        assert_eq!(typed("b"), Some(Action::CycleBarStyle));
        assert_eq!(typed("+"), Some(Action::BarSize(1)));
        assert_eq!(typed("-"), Some(Action::BarSize(-1)));

        // The named ones, which have no character to fall back on.
        let named = |key| pressed(&Key::Named(key)).map(crate::ui::map_key);
        assert_eq!(named(NamedKey::Escape), Some(Action::Quit));
        assert_eq!(named(NamedKey::Space), Some(Action::CycleVisualisation));
        assert_eq!(named(NamedKey::Tab), Some(Action::CycleVisualisation));
        assert_eq!(named(NamedKey::ArrowUp), Some(Action::Gain(1)));
        assert_eq!(named(NamedKey::ArrowDown), Some(Action::Gain(-1)));
        assert_eq!(named(NamedKey::F1), Some(Action::ToggleHelp));
    }

    #[test]
    fn a_key_rav_has_no_use_for_falls_through() {
        // `None` rather than `Action::None`, so a modifier held on its own does
        // not count as a press that did nothing - the window would redraw for
        // every shift key on the way to a capital.
        assert_eq!(pressed(&Key::Named(NamedKey::Shift)), None);
        assert_eq!(pressed(&Key::Named(NamedKey::Control)), None);
        // And one rav does not bind still translates - `map_key` is what says
        // it does nothing, which keeps that decision in one place too.
        assert_eq!(typed("z"), Some(crate::ui::Action::None));
    }

    #[test]
    fn h_and_f1_both_reach_the_panel() {
        // Through `map_key` rather than against the character, so rebinding
        // help cannot leave the window opening something else.
        assert_eq!(typed("h"), Some(crate::ui::Action::ToggleHelp));
        assert_eq!(
            pressed(&Key::Named(NamedKey::F1)).map(crate::ui::map_key),
            Some(crate::ui::Action::ToggleHelp),
            "F1 is the same key and gets the same answer",
        );
    }

    #[test]
    fn an_overlay_covers_only_where_it_painted() {
        // The whole of what makes this a panel over a picture: a transparent
        // pixel leaves the frame alone. Getting it backwards paints a black
        // rectangle over the bars and looks like the window stopped drawing.
        // A 4x2 frame with a 2x1 panel dropped at (1, 1): the offset is half
        // of what this checks, since a panel almost never sits at the origin.
        let mut frame = vec![0x00ff_0000u32; 8];
        let mut overlay = Pixmap::new(2, 1).unwrap();
        overlay.fill(tiny_skia::Color::TRANSPARENT);
        // One opaque white pixel, top left.
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0xff, 0xff, 0xff, 0xff);
        paint.anti_alias = false;
        overlay.fill_rect(
            tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap(),
            &paint,
            tiny_skia::Transform::identity(),
            None,
        );

        over(&mut frame, 4, &overlay, (1, 1));
        assert_eq!(
            frame[5], 0x00ff_ffff,
            "the painted pixel did not land at the offset"
        );
        assert_eq!(
            frame[6], 0x00ff_0000,
            "a transparent pixel overwrote the frame"
        );
        assert_eq!(frame[0], 0x00ff_0000, "it painted outside its rectangle");
        assert_eq!(frame[1], 0x00ff_0000, "it painted above itself");
    }

    #[test]
    fn the_panel_fits_the_window_rav_opens_at() {
        // Three things have to agree: the size a window opens at, the font's
        // cell, and how many rows the panel has. `Help::panel` clamps to the
        // area and `text::help` stops at the bottom edge, so a row that does
        // not fit is not reported - it is simply not drawn.
        //
        // Twenty rows against the fourteen the panel has today, so this fails
        // while there is still room to fix it rather than on the row that
        // first disappears.
        use crate::surface::text;
        let rows: Vec<crate::ui::help::HelpRow<'static>> = (0..20)
            .map(|_| crate::ui::help::HelpRow {
                key: "up/down",
                description: "grid behind the bars",
                value: Some("up to 16 kHz".to_string()),
            })
            .collect();

        let (_, _, wide, tall) = text::help_area(&text::Font8x8, &rows, "rav", OPENS_AT)
            .expect("no room for a panel at the size a window opens at");
        let (_, down) = text::Bitmap::cell(&text::Font8x8);
        assert!(
            tall >= (rows.len() as u32 + 4) * down,
            "rows are being cut: {tall}px for {} rows of {down}px",
            rows.len()
        );
        assert!(wide <= OPENS_AT.0 && tall <= OPENS_AT.1);
    }

    #[test]
    fn the_title_carries_what_rav_has_to_say() {
        // With nothing to say it is just the name - not "rav - " with an empty
        // tail, which reads as a window that lost something.
        assert_eq!(titled(None), "rav");
        assert_eq!(titled(Some("theme winamp")), "rav - theme winamp");
    }

    #[test]
    fn a_window_says_how_many_cells_it_is_counting() {
        // The bar width is in cells and a window has none, so it declares one.
        // Without this the layout would divide by zero cells on a small window
        // - and `max(1)` is what stops that being a crash on a drag.
        let (across, down) = CELL;
        let size = sized(u32::from(across) * 8, u32::from(down) * 3);
        assert_eq!((size.columns, size.rows), (8, 3));
        assert_eq!((size.width, size.height), (across * 8, down * 3));

        let tiny = sized(1, 1);
        assert_eq!((tiny.columns, tiny.rows), (1, 1), "never zero cells");
    }
}
