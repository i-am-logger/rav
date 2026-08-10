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

rav is capturing a device with no signal on it. `rav --list-devices` shows what
it can see, and `rav -d "<name>"` selects one. On Linux the device you want is
usually the one ending in `.monitor`.
