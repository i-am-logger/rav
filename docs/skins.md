# Drawing a skin

A skin is one SVG: the shape of a single **rung**, the unit a bar is built from.
rav stacks it up each bar and clips the stack to the level.

```
rav --skin ./segment.svg
```

That selects the `segment` bar style, which is the one a drawing replaces. `b`
cycles away to the ladders made of block characters and back.

## Only the shape is used

The drawing is a **mask**. Its alpha decides which pixels belong to a rung, and
the theme decides what colour they are — so a bar still reddens as it rises, and
a skin cannot override a theme. Colours inside the file are ignored; the shipped
`assets/skins/segment.svg` is plain white for that reason.

Draw in white, and think in coverage: solid where the rung is, transparent where
it is not, partial for a soft edge.

## Size and shape

Use a `viewBox` and set `preserveAspectRatio="none"`. The drawing is stretched to
fill one rung, whatever that is on the reader's terminal — a rung is one text
cell tall and one bar wide, so it might be 24×60 device pixels or 10×20. A skin
that insists on its own proportions would leave gaps the theme never asked for.

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" preserveAspectRatio="none">
  <path fill="#ffffff" d="M 14 6 H 86 L 96 24 V 76 L 86 94 H 14 L 4 76 V 24 Z"/>
</svg>
```

Leave a gap at the top and bottom if you want a visible seam between rungs; fill
edge to edge if you want a continuous bar. Both read as a ladder because the
level clips the stack, not because of the artwork.

## What is supported

`usvg` parses the file with no font stack and no raster image support, so:

| works | does not |
|---|---|
| paths, rectangles, circles, polygons | `<text>` — there are no fonts |
| gradients and opacity, as coverage | embedded PNG or JPEG |
| groups and transforms | external files of any kind |

A file that will not parse is a startup error naming the path. A file that parses
to nothing draws nothing, which looks like a bar that has gone missing rather
than like a mistake — check it renders somewhere else first.

## Where it does not apply

The panel says so rather than leaving it a mystery. Press `h` and read the bar
style row:

| | |
|---|---|
| `segment` | the drawing is on screen |
| `segment - needs pixels` | the terminal draws block characters, which have no way to carry a picture |
| `segment - needs a flat view` | `v` has the field at an angle; a mask cannot follow a leaning bar, so the plain ladder is drawn instead |

## Cost

The file is parsed and rasterised once, and the whole ladder is stamped once per
size — not per frame. Measured at 2400×1440 with eighty bands, a drawn skin costs
8.3 ms a frame against 4.8 ms for the block ladder, inside a 16.7 ms budget.
Complicated artwork costs no more per frame than simple artwork; it costs more
once, when the window changes size.
