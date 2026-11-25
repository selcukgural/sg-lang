# giftool

`giftool` reads a GIF, changes it and writes it again, or tells what it holds as `gifsicle -I` tells it:
written in SG, in Rust and in Zig, the same algorithm each time, from the GIF89a specification (choices
1874-1881). Nothing of gifsicle's code is in it: gifsicle 1.99 is a witness in the tests and is measured
beside the three, being the tool people use for this work.

```
giftool --info [FILE...]
giftool [--unoptimize] [-O1|-O2] [--crop X,Y+WxH] [--flip-horizontal|--flip-vertical]
        [--rotate-90|--rotate-180|--rotate-270] [-o OUT] [IN]
```

`-` or no file is the standard input, and the output is the standard output unless `-o` names a file.
A GIF that is not as GIF has it is told on the standard error as `giftool: NAME: why` and the program
ends with 1, having written nothing; a wrong use ends it with 2. The three word their errors each their
own way, and the tests compare only the output and the code.

## Reading

The input is read whole into memory. It must start with `GIF87a` or `GIF89a`; then the screen
(width, height, flags, background, aspect), the screen's table when the flags have one, and blocks up to
the trailer `;`. Each of these is an error:

- the file ends before the trailer;
- a screen or a frame of no width or no height (a frame of width 0 is not taken as the screen's width);
- a screen, or a frame, of more than 2^28 pixels (choice 1881): 65535 by 65535 would ask 16 GB for a
  screen of colors, and the limit is the same in the three. gifsicle has none;
- a frame whose left plus width, or top plus height, passes 65535;
- a code size below 2 or above 11;
- a graphic control extension that is not four bytes and the end of its blocks;
- an application extension whose name is not eleven bytes, or a loop extension (`NETSCAPE2.0` or
  `ANIMEXTS1.0`) that is not three bytes and the end of its blocks;
- a block that is not `,`, `!` or `;`.

A graphic control extension gives the next frame its disposal, delay and transparent index; a later one
before the same frame takes the place of the earlier. A comment belongs to the next frame, and those after
the last frame to the end. Other extensions (plain text, an application other than the loop) are counted
for `--info` and not written again. The loop count is that of the last loop extension.

`--info` does not decode the frames, as gifsicle does not: a GIF whose frames' data is broken is told as it
is. Everything else decodes each frame, one after another.

### Decoding a frame

The frame's sub-blocks are joined, then read as codes from the lowest bit up. With a code size `m`, `clear`
is `2^m` and `end` is `clear + 1`; the width starts at `m + 1` and the first free code at `clear + 2`.

- `clear` sets the width and the free code back; `end` ends the frame, and what follows it is not read.
- A code between 256 and `clear` (with `m` of 9 or more) is a pixel past 255: an error.
- The first code after a `clear`, or at the start, must be below `clear`: it is one pixel.
- Then a code above the free one, or equal to it when the table is full, is an error. Otherwise, while
  the free code is below 4096, it becomes the previous code's string and the first byte of this code's
  string (of the previous one's, when the code is the free one); and when the free code reaches `2^width`
  and the width is below 12, the width grows by one. A full table stays full until a `clear` (the
  "deferred clear" of some encoders).
- A code's string is written from its end back; what passes the frame's pixels is dropped.
- The frame's data ending, or `end`, before every pixel has a value is an error; pixels past the last
  are dropped.

An interlaced frame's rows are then put in order: rows 0, 8, 16..., then 4, 12..., then 2, 6..., then 1,
3....

## Writing

The output is built in memory and written when the last frame is: `GIF89a`, the screen (its size after
the crop and the turn), the screen's table and background as they were read, the aspect 0, the loop
extension `NETSCAPE2.0` when the input had one, the frames, the comments of the end, and `;`. For each
frame, in order: its comments (sub-blocks of 255 bytes), a graphic control extension when its disposal, its
delay or its transparent index is set, the image descriptor (never interlaced), its own table when it has
one, and its data.

The code size of a frame is the largest of 2, the bits of its table (its own, or the screen's), and the
bits that hold its largest index.

### Encoding a frame

Written as `compress` and giflib write it, so that the three write the same bytes:

- the codes go out from the lowest bit up, in sub-blocks of 255 bytes, the last one shorter;
- first `clear`, then, for each pixel after the first: the key `(pixel << 12) + ent` (`ent` the code of the
  string so far, which starts as the first pixel) is looked for in a table of 5003 slots, at
  `(pixel << 4) ^ ent`, then, when that slot holds another key, at that place minus `5003 - place` (or
  minus 1 at place 0), and again, wrapping below 0, until the key or an empty slot is found;
- found, `ent` takes its code; not found, `ent` is written, the width grows by one when the free code is
  at least `2^width` and the width below 12, and then the key takes the free code in the empty slot found,
  while the free code is below 4096; when it is not, `clear` is written, the table emptied, and the width
  and the free code set back. `ent` becomes the pixel;
- at the end `ent` is written, the width grown by the same rule, and `end`.

## The options

### Kept as indices

With neither `--unoptimize` nor `-O`, each frame keeps its indices and its table. Its part on the screen
and inside the crop is cut out (the rest of a frame past the screen is never shown), then flipped, then
turned clockwise, and its place moves with it. A frame with nothing left is written as one pixel at 0,0
that is not drawn: index 0, transparent 0, disposal `asis`, its table and its delay kept, so that the
frames keep their number and their time.

The crop is `X,Y+WxH`, four numbers in decimal; a crop past the screen is cut to it, and one that starts
outside it is an error. One flip and one turn at most.

### Put together: `--unoptimize` and `-O`

The frames are drawn on a screen of colors as a viewer draws them: a screen starts clear; a frame draws its
pixels that are not its transparent index where it lies on the screen, an index past its table black;
after it is shown, `background` clears its place (as browsers and gifsicle do, not to the background
color) and `previous` puts back what was there; the disposals 4 to 7 do nothing. The screen after each
frame is then cropped, flipped and turned.

`--unoptimize` writes each such screen whole, with the disposal `background`, so that what goes clear on
the screen is clear in the GIF too.

`-O1` and `-O2` write each frame as only the part that changed (choice 1876). The frame waits until the
next one is seen: when a pixel the waiting frame shows is clear in the next, the waiting frame is made
larger, to the smallest rectangle holding its own and every such pixel, and given the disposal
`background`; the screen it leaves is then that screen with this rectangle cleared. The next frame is the
smallest rectangle holding every pixel that differs between the screen it is drawn on and the screen it
must give (one pixel at 0,0 when none differs), with the disposal `asis`. `-O1` writes in it the colors
the screen must show; `-O2` writes clear each pixel that is already the same, which makes runs that LZW
packs better. Where a frame is made larger, its new pixels are clear in `-O2` and the screen's colors in
`-O1`. `-O` with `--unoptimize` is `-O`.

The colors of a frame written this way take indices so:

- the screen's table, when it holds every color of the frame, each the first index of its color, and, when
  the frame has clear pixels, the lowest index none of its colors takes is its transparent index;
- otherwise a table of its own: the colors in the order they first come, row by row, the transparent
  index after them when there are clear pixels, filled with black to a power of two, at least 2. More than
  256 is an error (choice 1876): a frame of more colors than a GIF's table holds needs its colors reduced,
  which gifsicle does and `giftool` does not.

## `--info`

What `gifsicle -I` writes, byte for byte, as gifsicle 1.99 writes it (choice 1877; 1.96, the one of Debian
and Ubuntu, writes the comments of the end before the loop): for each file `* NAME N images` (`image` for one,
`<stdin>` for the input), `logical screen WxH`, the screen's table and the background when there is a
table, `loop forever` or `loop count N`, the comments of the end and the number of other extensions there,
then for each frame `+ image #I WxH`, ` at X,Y` when not at 0,0, ` interlaced`, ` transparent T`, its
comments, the number of its other extensions, its own table, and `disposal` and `delay S.CCs` when set. A
comment's bytes below 32, from 127 up and `\` are written as gifsicle writes them (`\n`, `\\`, `\177`,
`\000` but for a last NUL, which is left out). A GIF with no frames writes nothing, as gifsicle. gifsicle
reads what it can of a broken GIF and tells it; `giftool` tells only that it is broken. A frame named by
gifsicle's own extension (`0xCE`) is counted as an extension.

## What each language uses

| | Reading the input | The color of a pixel | Colors to indices |
|---|---|---|---|
| SG | `File.readBytes`, `Stdin.readSome` | `uint`, `0` clear and the top byte set when drawn | `HashMap<uint, nint>` |
| Rust | `std::fs::read`, `read_to_end` | `u32`, the same | `HashMap<u32, usize>` |
| Zig | `std.fs.cwd().readFileAlloc`, the input read to its end | `u32`, the same | `std.AutoHashMap(u32, usize)` |

Each parses its own arguments: SG with `CommandLine`, Rust and Zig by hand, as their other trials.

## Tests

- `trials/giftool/tests/model.py`, a reader and a compositor written in Python from the specification and this
  page, not from the three, and `gifsicle -I`'s text (choice 1880). `trials/giftool/tests/inputs.py` makes the
  GIFs from a seed with an encoder of its own: tables of 2 to 256 colors, code sizes larger than the table
  needs, sub-blocks of every length, tables cleared when full, never cleared and cleared early, interlaced
  frames of odd heights, frames past the screen, indices past the table, every disposal, comments with
  bytes to escape, extensions not known; and broken ones.
- `python3 -I trials/giftool/tests/check.py [BINARY...]` runs each build on each GIF with every option: what each
  writes, read back by the model and drawn, must show screen for screen what the model draws of the input,
  cropped, flipped and turned; the delays, the loop and the comments must be kept; gifsicle must read it
  with no warning; `--info` must be gifsicle's text; a broken GIF must end with 1 and write nothing. The
  model is held to gifsicle first: its `--info` is `gifsicle -I`'s on every GIF that is not broken, and
  what gifsicle's `--unoptimize` writes draws as the model draws the input.
- `python3 trials/measure.py --check gif` holds the builds to each other, byte for byte.
- `interpreter/tests/cli.rs`, `the_trial_of_gif`, runs the SG version interpreted and compiled on every test run.
