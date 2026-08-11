# The block glyphs, and why they are not the skin

![The eight eighth-blocks and the three shades, as the font draws them](glyphs.svg)

Traced from the font's own outlines: the eight eighth-height blocks `▁▂▃▄▅▆▇█`,
then `░▒▓`. Each sits in a dashed box showing the cell it is drawn into.

## What it is for

Two things draw these glyphs, and they disagree.

**The font** draws `░▒▓` as genuine dithers — the figure above shows `U+2591` as
a grid of small separated squares, because that is what the outline contains.

**WezTerm does not consult the font at all.** `custom_block_glyphs` defaults to
true, so it synthesises the block characters itself, and it renders `░▒▓` as the
whole cell at flat 25/50/75% alpha. Verified with `wezterm ls-fonts --text`.

So the picture above is what the *font* would draw, and on the terminal most rav
users run it is not what appears.

## Why that matters for skins

A skin has a shape per level, and the obvious way to author one is to trace this
figure. **Do not.** A skin traced from the font would make the first pixel
release differ from the glyph release it is meant to reproduce — dithered shades
against flat ones — and the difference would read as a rendering bug rather than
as the deliberate change it was.

Skin shapes come from WezTerm's `customglyph.rs`: eighth blocks are full-width
rectangles from `(8-n)·h/8`, and `░▒▓` are the whole cell at flat alpha.

This file stays as documentation of the other half of the story, and as the
reason issue #63 looks different on Linux and macOS: the terminal, not the font,
decides.
