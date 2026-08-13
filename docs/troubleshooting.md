# Troubleshooting

## No audio in WezTerm on macOS

rav captures system audio through a CoreAudio process tap, which needs no
virtual device and no install. macOS gates that behind the **System Audio
Recording Only** permission, and it only ever offers that permission to an
application whose bundle declares `NSAudioCaptureUsageDescription` in its
`Info.plist`.

WezTerm does not declare it. Without the key macOS never shows the prompt, so
there is nothing to grant and no way to grant it — `tccutil reset` does not help,
because the permission was never requestable in the first place. rav shows
`no audio - grant this terminal System Audio Recording` and the bars stay flat.

This needs an upstream change to WezTerm's bundle. Until then, run rav in a
terminal that declares the key — Terminal.app, iTerm2 and Ghostty all do.

Unaffected on Linux: capture there is a PipeWire or PulseAudio monitor source,
with no per-application permission model in the way. See
[audio.md](audio.md).

## The demo GIF looks choppier than rav actually is

`assets/demo.gif` is **25 fps**. That is not rav's frame rate — it is what
macOS Terminal.app repaints at. Measured with per-frame checksums, Terminal.app
redraws this content about 23 times a second, so 25 captures all of it and the
committed GIF holds 633 distinct frames out of 650.

The same build measures 52 fps in WezTerm, and higher again on Linux. A Linux
re-record at 50 or 100 fps is pending; `record-demo` takes `RAV_DEMO_FPS`.

GIF stores frame delay in whole centiseconds, so only rates dividing 100 evenly
are smooth: 25 (4cs), 50 (2cs), 100 (1cs). 60 fps lands on 1.67cs and plays back
as an uneven 2/1 cadence.

## Bars stay flat with no warning

rav is capturing a device with no signal on it. `rav.log`, beside wherever you
started rav, settles it in one line — rav reports the level of the first audio
that arrives:

```
Capture delivering audio (peak 0.500)        # a capture device
Process tap delivering audio (peak 0.500)    # macOS, capturing system output
```

Which of the two depends on where the sound is coming from, and the line above
it says which rav chose. Neither line at all means nothing is arriving; a peak
near zero means the source is silent rather than absent.

Read it from the bottom. The file is appended to, so it holds every run since
you last deleted it, and two ravs going at once interleave their lines.
`🚀 RAV Audio Visualizer starting up...` marks where a run begins, and the
**pid** after it is what tells two of them apart — so that is the prefix to
search for, and the number on the end is the answer:

```
🚀 RAV Audio Visualizer starting up... (pid 41337)
```

`ps` will say whether that pid is still going. A run you thought had finished
and has not is the reason to check: its lines go on arriving under yours, and
they look exactly like yours.

`rav --list-devices` shows what rav can see and `rav -d "<name>"` selects one.
On Linux that list is ALSA PCMs only — a PipeWire/PulseAudio `.monitor` source
is not in it and cannot be selected with `-d`. Route the capture from the
PipeWire side instead; see [audio.md](audio.md).

## rav draws block characters on a terminal that can do better

Press `h`. The last row of the panel names the surface and the reason, and it
reports rather than forecasts: **drawing on** while the pixels are going,
**would draw on** while they are not.

| what it says | |
|---|---|
| `pixels (your terminal can draw images)` | pixels are going, and nothing needs fixing. `asked for` in place of the reason means `--surface kitty` on the command line |
| `glyphs (your terminal will not say how big it is in pixels)` | rav asked the terminal how big it is and got no answer. Deriving a cell size from the font is the rounding the pixel surface exists to remove, so rav declines to guess |
| `glyphs (tmux or screen is in the way)` | image escapes do not pass through a multiplexer intact. An explicit `--surface kitty` is still honoured — asking always wins — but it is the terminal underneath that has to answer |

**Pixels are the bars.** The oscilloscope is block characters on every surface,
so `Space` takes the picture down and the panel goes back to saying *would*.
Switching back brings it up again - and `v`, the viewing angle, reads `needs
pixels` for as long as either of those is true, rather than moving a setting
nothing is drawing.
