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
    /// What the title already says, so a frame that changed nothing does not
    /// ask the window server to set it again.
    titled: String,
    /// The frame ceiling, kept here because winit drives this loop and
    /// `App::run` - which has the same one - does not.
    min_frame: std::time::Duration,
    next_frame: Instant,
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
                    let action = crate::ui::map_key(code);
                    self.app.apply(action);
                    // The panel is fourteen rows of text and a window has no
                    // font for them yet, so `h` toggles something nothing
                    // draws. Said out loud through the same note a setting
                    // uses, which reaches the title bar - a key that silently
                    // does nothing is the defect this whole surface keeps
                    // being careful about.
                    if action == crate::ui::Action::ToggleHelp {
                        self.app
                            .note("help is a terminal thing for now".to_string());
                    }
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
        while let Ok(data) = self.audio.try_recv() {
            self.app.push_samples(&data.samples);
        }
        self.app.advance();
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
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return Ok(());
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // A window dragged to nothing, or minimised. Not a failure, and
            // drawing zero pixels into it would be.
            return Ok(());
        };

        // The audio and the analysis happened in `about_to_wait`, on every wake
        // rather than on every frame. This is only the drawing.

        // Only when it changes. Setting a title is a round trip to the window
        // server, and this runs on every frame.
        let saying = titled(self.app.status_line().as_deref());
        if saying != self.titled {
            window.set_title(&saying);
            self.titled = saying;
        }

        let Some(pixels) = self.app.frame_pixels(&sized(size.width, size.height)) else {
            return Ok(());
        };
        onto_black(&pixels, &mut self.packed);

        surface
            .resize(width, height)
            .map_err(|why| anyhow::anyhow!("the window would not take that size: {why}"))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|why| anyhow::anyhow!("no buffer to draw into: {why}"))?;
        // A frame that does not fill the buffer is a resize caught mid-flight;
        // showing it stretched is worse than showing the last one again.
        if buffer.len() == self.packed.len() {
            buffer.copy_from_slice(&self.packed);
            buffer
                .present()
                .map_err(|why| anyhow::anyhow!("the frame would not go up: {why}"))?;
        }
        Ok(())
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
        min_frame,
        next_frame: Instant::now(),
        failed: None,
    };
    events.run_app(&mut showing).context("running the window")?;
    match showing.failed {
        Some(why) => Err(why),
        None => Ok(()),
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
    fn h_is_a_key_that_says_why_nothing_happened() {
        // The one key a window cannot honour yet. It is `ToggleHelp` that has
        // to be recognised here, so this asserts through `map_key` rather than
        // against the character - rebinding help would otherwise leave the
        // note attached to whatever `h` became.
        assert_eq!(typed("h"), Some(crate::ui::Action::ToggleHelp));
        assert_eq!(
            pressed(&Key::Named(NamedKey::F1)).map(crate::ui::map_key),
            Some(crate::ui::Action::ToggleHelp),
            "F1 is the same key and gets the same answer",
        );
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
