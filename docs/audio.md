# Capturing system audio

rav visualises whatever its input device hears. To see the music you are actually
playing — rather than the room — that playback has to be looped back to an input.

## Linux

Nothing to install, but rav cannot pick the source on its own, and it is worth
knowing why before reaching for `--list-devices`.

rav captures through cpal, which on Linux uses **ALSA**. ALSA enumerates PCM
names — `default`, `pipewire`, `front:CARD=…`. A PulseAudio or PipeWire monitor
is a **source**, not an ALSA PCM, so it has no name in that list: rav cannot find
one by searching, and `-d "<sink>.monitor"` cannot select one either, because it
searches the same list. rav says so in the log on every run:

```
No monitor/loopback device found - capturing from default input 'default'
```

What rav opens is the ALSA `default` PCM, and on a PipeWire system that is a
PipeWire stream like any other. So the routing is set from the PipeWire side,
not from rav:

```
PIPEWIRE_NODE=<sink>.monitor rav    # point this capture at a sink's monitor
wpctl status                        # the sink names to choose from
```

A patchbay does the same thing by hand and survives a restart: run `qpwgraph` or
`helvum`, find rav's capture node, and connect its inputs to the monitor ports of
the sink you are playing to. `pw-link -l` shows what it ended up connected to.

`rav --list-devices` and `-d` still work, and are the right tools for a real
capture device — an `snd-aloop` loopback (`…CARD=Loopback`) is found
automatically, as is a USB interface you name explicitly.

The log says which device was opened, every run, and reports the level of the
first audio to arrive:

```
Capture delivering audio (peak 0.500)
```

A peak near zero means the routing is wrong; that number is the quickest way to
tell "rav is broken" from "rav is looking at a silent source".

## macOS

**14.2 and later** need nothing installed. rav opens a CoreAudio process tap,
which captures the mixed output of every application directly. The first run
raises a permission prompt for "System Audio Recording Only".

Because a tap sits *ahead* of the system volume, the volume knob does not change
the display.

**Older than 14.2** has no tap API. Install
[Background Music](https://github.com/kyleneideck/BackgroundMusic), set it as
your output device, and rav finds it automatically. A loopback device sits
*after* the system volume, so playback level does ride into the display — that is
what the gain trim is for.

### If the display is flat and nothing moves

A tap that is running but delivering silence is what a refused recording
permission looks like from inside the process: the tap is created, its callback
fires, and every sample is zero. rav notices and says so after a few seconds:

```
no audio - grant this terminal System Audio Recording
```

The permission belongs to the **terminal**, not to rav — rav is just a process it
launched. So granting it to one terminal and then running rav from another gives
exactly this.

There is a catch worth knowing about. macOS only raises the consent dialog for an
application that declares `NSAudioCaptureUsageDescription` in its `Info.plist`.
An app that does not declare it is never prompted, so the permission can never be
granted, and `tccutil reset` does not help — there is no decision to reset. At the
time of writing, **WezTerm does not declare it**; Terminal.app works because
Apple's own applications are exempt.

To check a terminal before blaming rav:

```
/usr/libexec/PlistBuddy -c "Print" \
  /Applications/YourTerminal.app/Contents/Info.plist | grep AudioCapture
```

No output means no prompt is possible. Adding the key to that app's bundle fixes
it, at the cost of re-signing the bundle.

## When there is no loopback at all

rav falls back to the default input — your microphone — and says so in the log.
That is usually not what you wanted, so it is worth reading the first few lines
of `rav.log` if the display looks like room noise.
