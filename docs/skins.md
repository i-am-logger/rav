# Writing a skin

A skin is a TOML file. Every skin rav ships is one — none of the palette lives in
code — so adding one means writing a file, not touching Rust.

```
rav --skin ./sunset.toml     # a path
rav --skin sunset            # a name, looked up in ./skins then ~/.config/rav/skins
```

Press `s` in rav to cycle the built-ins; a skin loaded with `--skin` joins the
rotation.

| built-in | |
|---|---|
| `rav` *(default)* | the classic ramp, over a backdrop of its own colours dimmed |
| `winamp` | the faithful reproduction: one flat dim backdrop |
| `terminal` | ANSI names throughout, so it follows the reader's theme |
| `mono` | greys only |

`rav.toml` and `winamp.toml` share a ramp and differ only in the backdrop, which
is the shortest possible demonstration of what a skin file changes.

## The file

```toml
name = "sunset"
description = "Dusk over a city with no nightlife to speak of."

[colors]
# 16 stops, foot of the bar first.
bars = [
  "#2a1b3d", "#44318d", "#8265a7", "#a4b3b6",
  "#d83f87", "#e05780", "#e8687a", "#f07873",
  "#f4886c", "#f89865", "#fba85e", "#fdb857",
  "#fec850", "#fed849", "#ffe842", "#fff83b",
]

# The unlit column behind each bar. One colour, or 16 to match `bars` stop for
# stop.
grid = "#1a1024"

# The peak caps.
peak = "#ffffff"

# The oscilloscope, 5 levels, centre of the trace first.
scope = ["#fff83b", "#fdb857", "#f07873", "#8265a7", "#44318d"]
```

Only `name`, `darken` and `[colors]` are read. Anything else — `description`, an
`[about]` block, comments — is for whoever reads the file next.

## `darken`

```toml
darken = true    # or a number in 0.0..=1.0 for how much brightness the
                 # backdrop keeps; `true` means 0.25
```

Optional, off by default, and it applies to `grid` alone. The bars are the thing
being looked at and are always drawn at full strength; what tends to be too
bright is the large area behind them, which was a hairline of dots on the panel
this look comes from and is a full-height column here.

All three channels scale together, which keeps the hue — scaling per channel
drains a saturated colour towards black by way of brown.

It works on named colours too, which takes a moment to explain. A name has no
number to scale, so rav asks the terminal what the name resolves to (`OSC 4`) and
scales *that*. The result is still your theme — your green, taken down — rather
than a green rav chose. A terminal that does not answer leaves the colour alone,
because inventing one would replace the theme instead of dimming it.

## Colours

Two kinds, and you can mix them:

| | |
|---|---|
| `"#rrggbb"` | an exact colour, identical on every terminal |
| an ANSI name | resolved from the reader's own theme |

The sixteen names are `black`, `red`, `green`, `yellow`, `blue`, `magenta`,
`cyan`, `white`, and each with a `bright-` prefix.

Which you choose is the whole difference between a skin that reproduces a
specific look and one that follows whatever the user already runs. `winamp.toml`
is all hex, because the point of it is to reproduce one exact look.
`terminal.toml` is all names, because the point of it is to look like *your*
terminal.

## Ramps

`bars`, `grid` and `scope` each take either a single colour — used for every stop
— or a full ladder of the exact length. A ladder of the wrong length is an error
rather than being padded: a silently invented colour at the top of the ramp reads
as a rendering bug, not as a broken file.

`bars` and `grid` are 16 stops, `scope` is 5.

The ramp is **stretched** to the window, so the foot of a bar is always stop 0
and a full-height bar is always stop 15 — a skin does not have to think about how
tall the terminal is. Below 16 rows the selection is skewed towards the top of
the ramp, so a short window still sweeps the hues instead of showing only the
first few.

## Two things worth knowing

**The backdrop should never equal the bar colour at the same stop.** `grid` is
what an unlit row looks like; if it matches `bars` at that stop the column is
invisible and `g` looks like a key that does nothing. A test enforces this for
the built-ins.

**`grid` can be a ladder, and that is often better than one colour.** Every ANSI
colour has a dark twin — `green` is the dim version of `bright-green` — so a
theme-following skin can make an unlit row the dark version of the colour it
lights up in. That is what `terminal.toml` does: dark green low down, dark yellow
through the middle, dark red at the top.

## Contributing one

Drop it in `skins/`, add it to `BUILT_IN` in `src/visual/skin.rs` if it should
ship, and open a PR. `cargo test` checks every built-in skin parses, has ramps of
the right length, and keeps its backdrop distinguishable from its bars.

If the colours come from someone else's work, put an `[about]` block in the file
naming the source and its licence — `winamp.toml` is the worked example.
