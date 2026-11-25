//! `giftool`: reads a GIF, changes it and writes it again, or tells what it holds as `gifsicle -I`
//! does (choices 1874-1881). What each option does, and the algorithm the three versions share, is in
//! `trials/giftool/README.md`.
//!
//!     giftool --info [FILE...]
//!     giftool [--unoptimize] [-O1|-O2] [--crop X,Y+WxH] [--flip-horizontal|--flip-vertical]
//!             [--rotate-90|--rotate-180|--rotate-270] [-o OUT] [IN]
//!
//! The input is read whole, and its frames are read, changed and written one after another into the
//! output, which is written at the end, so that a GIF found broken halfway leaves nothing behind.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::exit;

const MAX_PIXELS: usize = 268435456;
const HASH_SIZE: usize = 5003;
const MAX_CODES: usize = 4096;
/// The color of a pixel nothing is drawn on; a drawn one has its top byte set.
const CLEAR: u32 = 0;
const OPAQUE: u32 = 0xFF000000;

/// A frame as it was read, its pixels not yet: where its table and its data lie in the input.
#[derive(Clone, Default)]
struct Frame {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    interlace: bool,
    local_bits: usize,
    local_at: usize,
    transparent: Option<u8>,
    disposal: u8,
    delay: u16,
    code_size: usize,
    data_at: usize,
    comments: Vec<Vec<u8>>,
    extensions: usize,
}

struct Gif {
    width: usize,
    height: usize,
    global_bits: usize,
    global_at: usize,
    background: u8,
    looping: Option<u16>,
    frames: Vec<Frame>,
    end_comments: Vec<Vec<u8>>,
    end_extensions: usize,
}

/// A frame to be written: its place, its table (none for the screen's) and its indices.
struct Out {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    table_bits: usize,
    table: Vec<u8>,
    transparent: Option<u8>,
    disposal: u8,
    delay: u16,
    comments: Vec<Vec<u8>>,
    pixels: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq)]
enum Flip {
    None,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy)]
struct Crop {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

/// What the options ask for.
struct Options {
    unoptimize: bool,
    optimize: u8,
    crop: Option<Crop>,
    flip: Flip,
    rotate: u32,
}

/// The tables of the decoder and of the encoder, made once.
struct Lzw {
    prefix: Vec<u16>,
    suffix: Vec<u8>,
    first: Vec<u8>,
    length: Vec<u16>,
    joined: Vec<u8>,
    keys: Vec<i32>,
    codes: Vec<u16>,
}

type Failure = &'static str;

// ---- reading ----

const TOO_SOON: Failure = "the GIF ends too soon";

fn next(data: &[u8], at: &mut usize) -> Result<u8, Failure> {
    if *at >= data.len() {
        return Err(TOO_SOON);
    }
    let b = data[*at];
    *at += 1;
    Ok(b)
}

fn next16(data: &[u8], at: &mut usize) -> Result<u16, Failure> {
    if *at + 2 > data.len() {
        return Err(TOO_SOON);
    }
    let v = u16::from(data[*at]) | (u16::from(data[*at + 1]) << 8);
    *at += 2;
    Ok(v)
}

fn skip(data: &[u8], at: &mut usize, n: usize) -> Result<(), Failure> {
    if *at + n > data.len() {
        return Err(TOO_SOON);
    }
    *at += n;
    Ok(())
}

/// Past the sub-blocks, up to the empty one.
fn skip_blocks(data: &[u8], at: &mut usize) -> Result<(), Failure> {
    loop {
        let n = usize::from(next(data, at)?);
        if n == 0 {
            return Ok(());
        }
        skip(data, at, n)?;
    }
}

fn join_blocks(data: &[u8], at: &mut usize) -> Result<Vec<u8>, Failure> {
    let mut out = Vec::new();
    loop {
        let n = usize::from(next(data, at)?);
        if n == 0 {
            return Ok(out);
        }
        let from = *at;
        skip(data, at, n)?;
        out.extend_from_slice(&data[from..*at]);
    }
}

/// The whole GIF but its pixels; whatever is not as GIF has it is an error.
fn read_gif(data: &[u8]) -> Result<Gif, Failure> {
    if data.len() < 6 || !(&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
        return Err("not a GIF");
    }
    let mut at = 6;
    let mut gif = Gif {
        width: 0,
        height: 0,
        global_bits: 0,
        global_at: 0,
        background: 0,
        looping: None,
        frames: Vec::new(),
        end_comments: Vec::new(),
        end_extensions: 0,
    };
    gif.width = usize::from(next16(data, &mut at)?);
    gif.height = usize::from(next16(data, &mut at)?);
    let packed = next(data, &mut at)?;
    gif.background = next(data, &mut at)?;
    next(data, &mut at)?;
    if gif.width == 0 || gif.height == 0 {
        return Err("the screen has no size");
    }
    if gif.width * gif.height > MAX_PIXELS {
        return Err("the screen has more pixels than giftool takes");
    }
    if packed & 0x80 != 0 {
        gif.global_bits = usize::from(packed & 7) + 1;
        gif.global_at = at;
        skip(data, &mut at, 3 << gif.global_bits)?;
    }
    let mut frame = Frame::default();
    loop {
        let block = next(data, &mut at)?;
        if block == 0x3B {
            break;
        }
        if block == 0x21 {
            let kind = next(data, &mut at)?;
            if kind == 0xF9 {
                if next(data, &mut at)? != 4 {
                    return Err("a graphic control extension is not four bytes");
                }
                let p = next(data, &mut at)?;
                frame.disposal = (p >> 2) & 7;
                frame.delay = next16(data, &mut at)?;
                let t = next(data, &mut at)?;
                frame.transparent = if p & 1 != 0 { Some(t) } else { None };
                if next(data, &mut at)? != 0 {
                    return Err("a graphic control extension is not four bytes");
                }
            } else if kind == 0xFE {
                frame.comments.push(join_blocks(data, &mut at)?);
            } else if kind == 0xFF {
                if next(data, &mut at)? != 11 {
                    return Err("an application extension's name is not eleven bytes");
                }
                let name = at;
                skip(data, &mut at, 11)?;
                if &data[name..at] == b"NETSCAPE2.0" || &data[name..at] == b"ANIMEXTS1.0" {
                    if next(data, &mut at)? != 3 {
                        return Err("a loop extension is not three bytes");
                    }
                    next(data, &mut at)?;
                    gif.looping = Some(next16(data, &mut at)?);
                    if next(data, &mut at)? != 0 {
                        return Err("a loop extension is not three bytes");
                    }
                } else {
                    skip_blocks(data, &mut at)?;
                    frame.extensions += 1;
                }
            } else {
                skip_blocks(data, &mut at)?;
                frame.extensions += 1;
            }
        } else if block == 0x2C {
            frame.left = usize::from(next16(data, &mut at)?);
            frame.top = usize::from(next16(data, &mut at)?);
            frame.width = usize::from(next16(data, &mut at)?);
            frame.height = usize::from(next16(data, &mut at)?);
            let p = next(data, &mut at)?;
            if frame.width == 0 || frame.height == 0 {
                return Err("a frame has no size");
            }
            if frame.left + frame.width > 65535 || frame.top + frame.height > 65535 {
                return Err("a frame goes past 65535");
            }
            if frame.width * frame.height > MAX_PIXELS {
                return Err("a frame has more pixels than giftool takes");
            }
            if p & 0x80 != 0 {
                frame.local_bits = usize::from(p & 7) + 1;
                frame.local_at = at;
                skip(data, &mut at, 3 << frame.local_bits)?;
            }
            frame.interlace = p & 0x40 != 0;
            frame.code_size = usize::from(next(data, &mut at)?);
            if frame.code_size < 2 || frame.code_size > 11 {
                return Err("a frame's code size is not one GIF allows");
            }
            frame.data_at = at;
            skip_blocks(data, &mut at)?;
            gif.frames.push(std::mem::take(&mut frame));
        } else {
            return Err("a block of a kind GIF does not have");
        }
    }
    gif.end_comments = frame.comments;
    gif.end_extensions = frame.extensions;
    Ok(gif)
}

/// The indices of a frame from its LZW data, into `pixels[0..count]`.
fn decode(data: &[u8], frame: &Frame, lzw: &mut Lzw, pixels: &mut [u8]) -> Result<(), Failure> {
    let count = frame.width * frame.height;
    let mut at = frame.data_at;
    lzw.joined.clear();
    loop {
        let n = usize::from(data[at]);
        at += 1;
        if n == 0 {
            break;
        }
        lzw.joined.extend_from_slice(&data[at..at + n]);
        at += n;
    }
    let code_size = frame.code_size;
    let clear = 1usize << code_size;
    let end = clear + 1;
    for c in 0..clear.min(256) {
        lzw.suffix[c] = c as u8;
        lzw.first[c] = c as u8;
        lzw.length[c] = 1;
    }
    let mut width = code_size + 1;
    let mut mask = (1u32 << width) - 1;
    let mut free = clear + 2;
    let mut previous: Option<usize> = None;
    let mut pos = 0;
    let mut bits: u32 = 0;
    let mut have = 0;
    let mut i = 0;
    let n = lzw.joined.len();
    while pos < count {
        while have < width && i < n {
            bits |= u32::from(lzw.joined[i]) << have;
            have += 8;
            i += 1;
        }
        if have < width {
            break;
        }
        let code = (bits & mask) as usize;
        bits >>= width;
        have -= width;
        if code == clear {
            width = code_size + 1;
            mask = (1 << width) - 1;
            free = clear + 2;
            previous = None;
            continue;
        }
        if code == end {
            break;
        }
        if code > 255 && code < clear {
            return Err("a frame's data has a pixel past 255");
        }
        let Some(prev) = previous else {
            if code >= clear {
                return Err("a frame's data has a code that is not in its table");
            }
            pixels[pos] = code as u8;
            pos += 1;
            previous = Some(code);
            continue;
        };
        if code > free || (code == free && free >= MAX_CODES) {
            return Err("a frame's data has a code that is not in its table");
        }
        if free < MAX_CODES {
            lzw.prefix[free] = prev as u16;
            lzw.suffix[free] = if code < free { lzw.first[code] } else { lzw.first[prev] };
            lzw.first[free] = lzw.first[prev];
            lzw.length[free] = lzw.length[prev] + 1;
            free += 1;
            if free == (1 << width) && width < 12 {
                width += 1;
                mask = (1 << width) - 1;
            }
        }
        // The string of the code, written from its end back.
        let len = usize::from(lzw.length[code]);
        let mut stop = pos + len;
        let mut c = code;
        let mut k = stop - 1;
        if stop > count {
            while k >= count {
                c = usize::from(lzw.prefix[c]);
                k -= 1;
            }
            stop = count;
        }
        while k > pos {
            pixels[k] = lzw.suffix[c];
            c = usize::from(lzw.prefix[c]);
            k -= 1;
        }
        pixels[pos] = lzw.suffix[c];
        pos = stop;
        previous = Some(code);
    }
    if pos < count {
        return Err("a frame's data ends before its pixels do");
    }
    Ok(())
}

/// The rows of an interlaced frame put in their order.
fn deinterlace(from: &[u8], to: &mut [u8], width: usize, height: usize) {
    let mut row = 0;
    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
        let mut y = start;
        while y < height {
            to[y * width..(y + 1) * width].copy_from_slice(&from[row * width..(row + 1) * width]);
            row += 1;
            y += step;
        }
    }
}

// ---- writing ----

fn put16(out: &mut Vec<u8>, v: usize) {
    out.push((v & 0xFF) as u8);
    out.push((v >> 8) as u8);
}

fn put_blocks(out: &mut Vec<u8>, data: &[u8]) {
    for chunk in data.chunks(255) {
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
    out.push(0);
}

fn bits_for(n: usize) -> usize {
    let mut bits = 1;
    while (1 << bits) < n {
        bits += 1;
    }
    bits
}

/// The LZW of `pixels` in sub-blocks, after its code size (README, "Writing").
fn encode(out: &mut Vec<u8>, pixels: &[u8], code_size: usize, lzw: &mut Lzw) {
    out.push(code_size as u8);
    let clear = 1usize << code_size;
    let mut width = code_size + 1;
    let mut free = clear + 2;
    lzw.keys.fill(-1);
    // The bits not yet written, and the sub-block being filled: its length byte is at `length_at`.
    let mut bits = clear as u64;
    let mut have = width;
    let mut length_at = out.len();
    out.push(0);
    let mut in_block = 0;
    let mut ent = i32::from(pixels[0]);
    for &p in &pixels[1..] {
        let c = i32::from(p);
        let key = (c << 12) + ent;
        let mut h = ((c << 4) ^ ent) as usize;
        if lzw.keys[h] == key {
            ent = i32::from(lzw.codes[h]);
            continue;
        }
        if lzw.keys[h] >= 0 {
            let step = if h == 0 { 1 } else { HASH_SIZE - h };
            let mut found = false;
            loop {
                h = if h >= step { h - step } else { h + HASH_SIZE - step };
                if lzw.keys[h] == key {
                    found = true;
                    break;
                }
                if lzw.keys[h] < 0 {
                    break;
                }
            }
            if found {
                ent = i32::from(lzw.codes[h]);
                continue;
            }
        }
        bits |= (ent as u64) << have;
        have += width;
        if free >= (1 << width) && width < 12 {
            width += 1;
        }
        if free < MAX_CODES {
            lzw.keys[h] = key;
            lzw.codes[h] = free as u16;
            free += 1;
        } else {
            bits |= (clear as u64) << have;
            have += width;
            lzw.keys.fill(-1);
            width = code_size + 1;
            free = clear + 2;
        }
        while have >= 8 {
            out.push((bits & 0xFF) as u8);
            bits >>= 8;
            have -= 8;
            in_block += 1;
            if in_block == 255 {
                out[length_at] = 255;
                length_at = out.len();
                out.push(0);
                in_block = 0;
            }
        }
        ent = c;
    }
    bits |= (ent as u64) << have;
    have += width;
    if free >= (1 << width) && width < 12 {
        width += 1;
    }
    bits |= ((clear + 1) as u64) << have;
    have += width;
    while have > 0 {
        out.push((bits & 0xFF) as u8);
        bits >>= 8;
        have = have.saturating_sub(8);
        in_block += 1;
        if in_block == 255 {
            out[length_at] = 255;
            length_at = out.len();
            out.push(0);
            in_block = 0;
        }
    }
    if in_block > 0 {
        out[length_at] = in_block as u8;
        out.push(0);
    }
}

fn write_frame(out: &mut Vec<u8>, f: &Out, pixels: &[u8], global_bits: usize, lzw: &mut Lzw) {
    for comment in &f.comments {
        out.push(0x21);
        out.push(0xFE);
        put_blocks(out, comment);
    }
    if f.disposal != 0 || f.delay != 0 || f.transparent.is_some() {
        out.push(0x21);
        out.push(0xF9);
        out.push(4);
        match f.transparent {
            Some(_) => out.push((f.disposal << 2) | 1),
            None => out.push(f.disposal << 2),
        }
        put16(out, usize::from(f.delay));
        out.push(f.transparent.unwrap_or(0));
        out.push(0);
    }
    out.push(0x2C);
    put16(out, f.left);
    put16(out, f.top);
    put16(out, f.width);
    put16(out, f.height);
    let mut bits = global_bits;
    if f.table_bits > 0 {
        out.push(0x80 | (f.table_bits - 1) as u8);
        out.extend_from_slice(&f.table);
        bits = f.table_bits;
    } else {
        out.push(0);
    }
    let most = pixels.iter().copied().max().unwrap_or(0);
    let code_size = bits_for(usize::from(most) + 1).max(bits).max(2);
    encode(out, pixels, code_size, lzw);
}

// ---- frames kept as indices: cut to the screen and the crop, flipped, turned ----

/// The crop on the screen, and the size of what is written.
struct View {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    out_width: usize,
    out_height: usize,
}

/// A frame's place and size once flipped and turned.
struct Placed {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

/// A frame of `w` by `h` at `left`, `top` of the cropped screen, flipped and turned.
fn place(view: &View, o: &Options, left: usize, top: usize, w: usize, h: usize) -> Placed {
    let mut l = left;
    let mut t = top;
    if o.flip == Flip::Horizontal {
        l = view.width - left - w;
    } else if o.flip == Flip::Vertical {
        t = view.height - top - h;
    }
    match o.rotate {
        90 => Placed { left: view.height - t - h, top: l, width: h, height: w },
        180 => Placed { left: view.width - l - w, top: view.height - t - h, width: w, height: h },
        270 => Placed { left: t, top: view.width - l - w, width: h, height: w },
        _ => Placed { left: l, top: t, width: w, height: h },
    }
}

/// Where the pixels of a part of a picture come from once flipped and turned: the pixel at (x, y) of
/// the result is at `base + x * step_x + y * step_y` of the picture.
#[derive(Clone, Copy)]
struct Walk {
    base: isize,
    step_x: isize,
    step_y: isize,
    width: usize,
    height: usize,
}

/// The walk over the part `w` by `h` at `origin` of a picture `stride` wide: the flip undone after the
/// turn, each a map of (x, y) that is a sum of x, y and a number.
fn walk_of(o: &Options, origin: usize, stride: usize, w: usize, h: usize) -> Walk {
    let (wi, hi, stride) = (w as isize, h as isize, stride as isize);
    let (mut ax, mut bx, mut cx) = (1isize, 0isize, 0isize);
    let (mut ay, mut by, mut cy) = (0isize, 1isize, 0isize);
    let (mut ow, mut oh) = (w, h);
    if o.rotate == 90 {
        ax = 0;
        bx = 1;
        ay = -1;
        by = 0;
        cy = hi - 1;
        ow = h;
        oh = w;
    } else if o.rotate == 180 {
        ax = -1;
        cx = wi - 1;
        by = -1;
        cy = hi - 1;
    } else if o.rotate == 270 {
        ax = 0;
        bx = -1;
        cx = wi - 1;
        ay = 1;
        by = 0;
        ow = h;
        oh = w;
    }
    if o.flip == Flip::Horizontal {
        ax = -ax;
        bx = -bx;
        cx = wi - 1 - cx;
    } else if o.flip == Flip::Vertical {
        ay = -ay;
        by = -by;
        cy = hi - 1 - cy;
    }
    Walk {
        base: origin as isize + cx + cy * stride,
        step_x: ax + ay * stride,
        step_y: bx + by * stride,
        width: ow,
        height: oh,
    }
}

fn walk<T: Copy>(from: &[T], walk: Walk, to: &mut Vec<T>) {
    to.clear();
    for y in 0..walk.height {
        let mut at = walk.base + y as isize * walk.step_y;
        if walk.step_x == 1 {
            let start = at as usize;
            to.extend_from_slice(&from[start..start + walk.width]);
            continue;
        }
        for _ in 0..walk.width {
            to.push(from[at as usize]);
            at += walk.step_x;
        }
    }
}

fn table_copy(data: &[u8], at: usize, bits: usize) -> Vec<u8> {
    if bits > 0 { data[at..at + (3 << bits)].to_vec() } else { Vec::new() }
}

/// A frame with its indices: the part of it on the cropped screen, flipped and turned, into `turned`;
/// and whether nothing of it changes, so that its pixels are written as they were read.
fn index_frame(data: &[u8], f: &Frame, pixels: &[u8], view: &View, o: &Options, turned: &mut Vec<u8>) -> (Out, bool) {
    let x0 = f.left.max(view.x);
    let y0 = f.top.max(view.y);
    let x1 = (f.left + f.width).min(view.x + view.width);
    let y1 = (f.top + f.height).min(view.y + view.height);
    let mut out = Out {
        left: 0,
        top: 0,
        width: 1,
        height: 1,
        table_bits: f.local_bits,
        table: table_copy(data, f.local_at, f.local_bits),
        transparent: Some(0),
        disposal: 1,
        delay: f.delay,
        comments: f.comments.clone(),
        pixels: Vec::new(),
    };
    if x0 >= x1 || y0 >= y1 {
        // Nothing of it is on the screen: a pixel that is not drawn.
        turned.clear();
        turned.push(0);
        return (out, false);
    }
    let w = x1 - x0;
    let h = y1 - y0;
    let at = place(view, o, x0 - view.x, y0 - view.y, w, h);
    out.left = at.left;
    out.top = at.top;
    out.width = at.width;
    out.height = at.height;
    out.transparent = f.transparent;
    out.disposal = f.disposal;
    let whole = w == f.width && h == f.height && o.flip == Flip::None && o.rotate == 0;
    if !whole {
        walk(pixels, walk_of(o, (y0 - f.top) * f.width + x0 - f.left, f.width, w, h), turned);
    }
    (out, whole)
}

// ---- frames put together on the screen ----

/// The colors of a table, `OPAQUE` set; an index past it is black.
fn colors_of(data: &[u8], at: usize, bits: usize, colors: &mut [u32]) {
    colors.fill(OPAQUE);
    if bits == 0 {
        return;
    }
    let (table, _) = data[at..at + (3 << bits)].as_chunks::<3>();
    for (color, rgb) in colors.iter_mut().zip(table) {
        *color = OPAQUE | (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
    }
}

/// Indices and a table for the colors of a frame: the screen's table when it holds every one of them
/// (and an index none of them takes, when one is clear), or a table of their own in the order they come.
fn palette_of(colors: &[u32], global: &HashMap<u32, usize>, global_bits: usize, out: &mut Out) -> Result<(), Failure> {
    out.pixels.clear();
    let mut clear = false;
    let mut fits = global_bits > 0;
    if fits {
        let mut used = [false; 256];
        let mut last = CLEAR;
        let mut last_index = 0;
        for &c in colors {
            if c == CLEAR {
                clear = true;
                out.pixels.push(0);
                continue;
            }
            if c != last {
                let Some(&found) = global.get(&c) else {
                    fits = false;
                    break;
                };
                last = c;
                last_index = found;
                used[last_index] = true;
            }
            out.pixels.push(last_index as u8);
        }
        if fits {
            out.table_bits = 0;
            out.table.clear();
            out.transparent = None;
            if !clear {
                return Ok(());
            }
            if let Some(free) = (0..1 << global_bits).find(|&i| !used[i]) {
                out.transparent = Some(free as u8);
                // the clear pixels were written as 0 for now
                for (k, &c) in colors.iter().enumerate() {
                    if c == CLEAR {
                        out.pixels[k] = free as u8;
                    }
                }
                return Ok(());
            }
        }
    }
    out.pixels.clear();
    clear = false;
    let mut local: HashMap<u32, usize> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    let mut last = CLEAR;
    let mut last_index = 0;
    for &c in colors {
        if c == CLEAR {
            clear = true;
            out.pixels.push(0);
            continue;
        }
        if c != last {
            let index = *local.entry(c).or_insert(order.len());
            if index == order.len() {
                order.push(c);
            }
            last = c;
            last_index = index;
        }
        out.pixels.push((last_index & 0xFF) as u8);
    }
    let count = order.len() + usize::from(clear);
    if count > 256 {
        return Err("a frame has more than 256 colors");
    }
    if clear {
        let t = order.len() as u8;
        for (k, &c) in colors.iter().enumerate() {
            if c == CLEAR {
                out.pixels[k] = t;
            }
        }
        out.transparent = Some(t);
    } else {
        out.transparent = None;
    }
    let bits = bits_for(count.max(2));
    out.table_bits = bits;
    out.table.clear();
    for &c in &order {
        out.table.push((c >> 16) as u8);
        out.table.push((c >> 8) as u8);
        out.table.push(c as u8);
    }
    out.table.resize(3 << bits, 0);
    Ok(())
}

#[derive(Clone, Copy)]
struct Rect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

const NO_RECT: Rect = Rect { x0: 0, y0: 0, x1: 0, y1: 0 };

impl Rect {
    fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    fn joined(self, b: Rect) -> Rect {
        if self.is_empty() {
            return b;
        }
        if b.is_empty() {
            return self;
        }
        Rect { x0: self.x0.min(b.x0), y0: self.y0.min(b.y0), x1: self.x1.max(b.x1), y1: self.y1.max(b.y1) }
    }

    fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

/// The smallest rectangle holding the pixels where `before` and `after` differ; with `cleared` only
/// those drawn before and clear after.
fn differing(before: &[u32], after: &[u32], w: usize, h: usize, cleared: bool) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let a = before[row + x];
            let b = after[row + x];
            let differs = if cleared { a != CLEAR && b == CLEAR } else { a != b };
            if differs {
                if x < x0 {
                    x0 = x;
                }
                if x >= x1 {
                    x1 = x + 1;
                }
                if y < y0 {
                    y0 = y;
                }
                y1 = y + 1;
            }
        }
    }
    if x0 >= x1 {
        return NO_RECT;
    }
    Rect { x0, y0, x1, y1 }
}

/// A frame of `-O` not yet written: the next frame may still make it larger and have it cleared.
struct Pending {
    there: bool,
    rect: Rect,
    colors: Vec<u32>,
    delay: u16,
    disposal: u8,
    comments: Vec<Vec<u8>>,
}

fn write_pending(out: &mut Vec<u8>, p: &mut Pending, global: &HashMap<u32, usize>, global_bits: usize, lzw: &mut Lzw) -> Result<(), Failure> {
    let mut f = Out {
        left: p.rect.x0,
        top: p.rect.y0,
        width: p.rect.x1 - p.rect.x0,
        height: p.rect.y1 - p.rect.y0,
        table_bits: 0,
        table: Vec::new(),
        transparent: None,
        disposal: p.disposal,
        delay: p.delay,
        comments: std::mem::take(&mut p.comments),
        pixels: Vec::new(),
    };
    palette_of(&p.colors, global, global_bits, &mut f)?;
    write_frame(out, &f, &f.pixels, global_bits, lzw);
    p.there = false;
    Ok(())
}

/// The colors of `rect` of a screen `w` wide: of `now`, or clear where `before` shows the same, for -O2.
fn cut_colors(now: &[u32], before: &[u32], w: usize, rect: Rect, same_clear: bool, to: &mut Vec<u32>) {
    to.clear();
    for y in rect.y0..rect.y1 {
        let row = y * w;
        for x in rect.x0..rect.x1 {
            let c = now[row + x];
            if same_clear && before[row + x] == c {
                to.push(CLEAR);
            } else {
                to.push(c);
            }
        }
    }
}

// ---- the program ----

fn rewrite(data: &[u8], gif: &Gif, o: &Options) -> Result<Vec<u8>, Failure> {
    if gif.frames.is_empty() {
        return Err("the GIF has no frames");
    }
    let mut view = View { x: 0, y: 0, width: gif.width, height: gif.height, out_width: gif.width, out_height: gif.height };
    if let Some(crop) = o.crop {
        if crop.x >= gif.width || crop.y >= gif.height {
            return Err("the crop is outside the screen");
        }
        view.x = crop.x;
        view.y = crop.y;
        view.width = crop.width.min(gif.width - crop.x);
        view.height = crop.height.min(gif.height - crop.y);
    }
    if o.rotate == 90 || o.rotate == 270 {
        view.out_width = view.height;
        view.out_height = view.width;
    } else {
        view.out_width = view.width;
        view.out_height = view.height;
    }

    let mut out = Vec::with_capacity(data.len() + 1024);
    out.extend_from_slice(b"GIF89a");
    put16(&mut out, view.out_width);
    put16(&mut out, view.out_height);
    if gif.global_bits > 0 {
        out.push(0x80 | 0x70 | (gif.global_bits - 1) as u8);
    } else {
        out.push(0);
    }
    out.push(gif.background);
    out.push(0);
    if gif.global_bits > 0 {
        out.extend_from_slice(&data[gif.global_at..gif.global_at + (3 << gif.global_bits)]);
    }
    if let Some(count) = gif.looping {
        out.extend_from_slice(b"\x21\xFF\x0BNETSCAPE2.0\x03\x01");
        put16(&mut out, usize::from(count));
        out.push(0);
    }

    let mut lzw = Lzw {
        prefix: vec![0; MAX_CODES],
        suffix: vec![0; MAX_CODES],
        first: vec![0; MAX_CODES],
        length: vec![0; MAX_CODES],
        joined: Vec::new(),
        keys: vec![-1; HASH_SIZE],
        codes: vec![0; HASH_SIZE],
    };
    let mut indices: Vec<u8> = Vec::new();
    let mut rows: Vec<u8> = Vec::new();
    let together = o.unoptimize || o.optimize > 0;
    // The screen as drawn so far, what a frame of `previous` puts back, and the views of the screen.
    let (screen, seen) = if together { (gif.width * gif.height, view.out_width * view.out_height) } else { (0, 0) };
    let mut canvas = vec![CLEAR; screen];
    let mut saved: Vec<u32> = Vec::new();
    let mut colors = vec![OPAQUE; 256];
    let mut global: HashMap<u32, usize> = HashMap::new();
    if gif.global_bits > 0 {
        colors_of(data, gif.global_at, gif.global_bits, &mut colors);
        for (i, &c) in colors[..1 << gif.global_bits].iter().enumerate() {
            global.entry(c).or_insert(i);
        }
    }
    let mut turned: Vec<u8> = Vec::new();
    let screen_walk = walk_of(o, view.y * gif.width + view.x, gif.width, view.width, view.height);
    let mut now = vec![CLEAR; seen];
    let mut before = vec![CLEAR; seen];
    let mut base = vec![CLEAR; seen];
    let mut pending = Pending { there: false, rect: NO_RECT, colors: Vec::new(), delay: 0, disposal: 1, comments: Vec::new() };
    for frame in &gif.frames {
        let count = frame.width * frame.height;
        if indices.len() < count {
            indices = vec![0; count];
        }
        decode(data, frame, &mut lzw, &mut indices)?;
        if frame.interlace {
            if rows.len() < count {
                rows = vec![0; count];
            }
            deinterlace(&indices[..count], &mut rows, frame.width, frame.height);
            indices[..count].copy_from_slice(&rows[..count]);
        }
        if !together {
            let (f, whole) = index_frame(data, frame, &indices[..count], &view, o, &mut turned);
            if whole {
                write_frame(&mut out, &f, &indices[..count], gif.global_bits, &mut lzw);
            } else {
                write_frame(&mut out, &f, &turned, gif.global_bits, &mut lzw);
            }
            continue;
        }
        // Drawn on the screen.
        if frame.local_bits > 0 {
            colors_of(data, frame.local_at, frame.local_bits, &mut colors);
        } else {
            colors_of(data, gif.global_at, gif.global_bits, &mut colors);
        }
        let x0 = frame.left;
        let y0 = frame.top;
        let x1 = (frame.left + frame.width).min(gif.width);
        let y1 = (frame.top + frame.height).min(gif.height);
        if frame.disposal == 3 {
            saved.clear();
            for y in y0..y1 {
                if x0 < x1 {
                    saved.extend_from_slice(&canvas[y * gif.width + x0..y * gif.width + x1]);
                }
            }
        }
        for y in y0..y1 {
            let row = (y - frame.top) * frame.width;
            for x in x0..x1 {
                let p = indices[row + x - frame.left];
                if Some(p) == frame.transparent {
                    continue;
                }
                canvas[y * gif.width + x] = colors[usize::from(p)];
            }
        }
        walk(&canvas, screen_walk, &mut now);
        if frame.disposal == 2 {
            for y in y0..y1 {
                for x in x0..x1 {
                    canvas[y * gif.width + x] = CLEAR;
                }
            }
        } else if frame.disposal == 3 {
            let mut k = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    canvas[y * gif.width + x] = saved[k];
                    k += 1;
                }
            }
        }
        let vw = view.out_width;
        let vh = view.out_height;
        if o.optimize == 0 {
            let mut f = Out {
                left: 0,
                top: 0,
                width: vw,
                height: vh,
                table_bits: 0,
                table: Vec::new(),
                transparent: None,
                disposal: 2,
                delay: frame.delay,
                comments: frame.comments.clone(),
                pixels: Vec::new(),
            };
            palette_of(&now, &global, gif.global_bits, &mut f)?;
            write_frame(&mut out, &f, &f.pixels, gif.global_bits, &mut lzw);
            continue;
        }
        // -O: what was drawn and is clear now is cleared by the frame before, made larger to hold it.
        base.copy_from_slice(&before);
        if pending.there {
            let gone = differing(&before, &now, vw, vh, true);
            if !gone.is_empty() {
                let grown = pending.rect.joined(gone);
                let w = pending.rect.x1 - pending.rect.x0;
                let mut colors_before = Vec::new();
                for y in grown.y0..grown.y1 {
                    for x in grown.x0..grown.x1 {
                        if pending.rect.contains(x, y) {
                            colors_before.push(pending.colors[(y - pending.rect.y0) * w + x - pending.rect.x0]);
                        } else if o.optimize == 2 {
                            colors_before.push(CLEAR);
                        } else {
                            colors_before.push(before[y * vw + x]);
                        }
                    }
                }
                pending.rect = grown;
                pending.colors = colors_before;
                pending.disposal = 2;
                for y in grown.y0..grown.y1 {
                    base[y * vw + grown.x0..y * vw + grown.x1].fill(CLEAR);
                }
            }
            write_pending(&mut out, &mut pending, &global, gif.global_bits, &mut lzw)?;
        }
        let mut changed = differing(&base, &now, vw, vh, false);
        if changed.is_empty() {
            changed = Rect { x0: 0, y0: 0, x1: 1, y1: 1 };
        }
        pending.there = true;
        pending.rect = changed;
        cut_colors(&now, &base, vw, changed, o.optimize == 2, &mut pending.colors);
        pending.delay = frame.delay;
        pending.disposal = 1;
        pending.comments = frame.comments.clone();
        before.copy_from_slice(&now);
    }
    if pending.there {
        write_pending(&mut out, &mut pending, &global, gif.global_bits, &mut lzw)?;
    }
    for comment in &gif.end_comments {
        out.push(0x21);
        out.push(0xFE);
        put_blocks(&mut out, comment);
    }
    out.push(0x3B);
    Ok(out)
}

// ---- --info, as gifsicle -I writes it ----

fn safe_text(s: &mut Vec<u8>, data: &[u8]) {
    let n = data.len();
    for (i, &b) in data.iter().enumerate() {
        if !(0x20..0x7F).contains(&b) || b == b'\\' {
            match b {
                7 => s.extend_from_slice(b"\\a"),
                8 => s.extend_from_slice(b"\\b"),
                12 => s.extend_from_slice(b"\\f"),
                10 => s.extend_from_slice(b"\\n"),
                13 => s.extend_from_slice(b"\\r"),
                9 => s.extend_from_slice(b"\\t"),
                11 => s.extend_from_slice(b"\\v"),
                b'\\' => s.extend_from_slice(b"\\\\"),
                0 => {
                    if n - i > 1 {
                        s.extend_from_slice(b"\\000");
                    }
                }
                _ => s.extend_from_slice(&[b'\\', b'0' + ((b >> 6) & 7), b'0' + ((b >> 3) & 7), b'0' + (b & 7)]),
            }
        } else {
            s.push(b);
        }
    }
}

fn info_of(gif: &Gif, name: &[u8]) -> Vec<u8> {
    let mut s = Vec::with_capacity(256);
    let n = gif.frames.len();
    if n == 0 {
        return s;
    }
    s.extend_from_slice(b"* ");
    s.extend_from_slice(name);
    if n == 1 {
        s.extend_from_slice(b" 1 image\n");
    } else {
        s.extend_from_slice(format!(" {n} images\n").as_bytes());
    }
    // Writing into a Vec<u8> cannot fail.
    let _ = writeln!(s, "  logical screen {}x{}", gif.width, gif.height);
    if gif.global_bits > 0 {
        let _ = writeln!(s, "  global color table [{}]", 1 << gif.global_bits);
        let _ = writeln!(s, "  background {}", gif.background);
    }
    match gif.looping {
        Some(0) => s.extend_from_slice(b"  loop forever\n"),
        Some(count) => {
            let _ = writeln!(s, "  loop count {count}");
        }
        None => {}
    }
    for c in &gif.end_comments {
        s.extend_from_slice(b"  end comment ");
        safe_text(&mut s, c);
        s.push(b'\n');
    }
    if gif.end_extensions > 0 {
        let _ = writeln!(s, "  end extensions {}", gif.end_extensions);
    }
    for (i, f) in gif.frames.iter().enumerate() {
        let _ = write!(s, "  + image #{i} {}x{}", f.width, f.height);
        if f.left != 0 || f.top != 0 {
            let _ = write!(s, " at {},{}", f.left, f.top);
        }
        if f.interlace {
            s.extend_from_slice(b" interlaced");
        }
        if let Some(t) = f.transparent {
            let _ = write!(s, " transparent {t}");
        }
        s.push(b'\n');
        for c in &f.comments {
            s.extend_from_slice(b"    comment ");
            safe_text(&mut s, c);
            s.push(b'\n');
        }
        if f.extensions > 0 {
            let _ = writeln!(s, "    extensions {}", f.extensions);
        }
        if f.local_bits > 0 {
            let _ = writeln!(s, "    local color table [{}]", 1 << f.local_bits);
        }
        if f.disposal != 0 || f.delay != 0 {
            s.extend_from_slice(b"   ");
            match f.disposal {
                0 => {}
                1 => s.extend_from_slice(b" disposal asis"),
                2 => s.extend_from_slice(b" disposal background"),
                3 => s.extend_from_slice(b" disposal previous"),
                d => {
                    let _ = write!(s, " disposal {d}");
                }
            }
            if f.delay != 0 {
                let _ = write!(s, " delay {}.{:02}s", f.delay / 100, f.delay % 100);
            }
            s.push(b'\n');
        }
    }
    s
}

fn read_input(name: &OsString) -> io::Result<Vec<u8>> {
    if name != "-" {
        return std::fs::read(name);
    }
    let mut all = Vec::new();
    io::stdin().lock().read_to_end(&mut all)?;
    Ok(all)
}

fn shown(name: &OsString) -> String {
    if name == "-" { "<stdin>".to_string() } else { Path::new(name).display().to_string() }
}

fn fail(name: &str, message: &dyn std::fmt::Display) -> ! {
    eprintln!("giftool: {name}: {message}");
    exit(1)
}

fn usage(message: &str) -> ! {
    eprintln!("giftool: {message}");
    eprintln!("Try 'giftool --help'.");
    exit(2)
}

const HELP: &str = "\
Reads a GIF, changes it and writes it again, or tells what it holds.

Usage: giftool --info [FILE...]
       giftool [--unoptimize] [-O1|-O2] [--crop X,Y+WxH] [--flip-horizontal|--flip-vertical]
               [--rotate-90|--rotate-180|--rotate-270] [-o OUT] [IN]

Options:
  -I, --info             Tell what each GIF holds, as gifsicle -I does
      --unoptimize       Write each frame whole, as the screen shows it
  -O, --optimize LEVEL   1: each frame only where the screen changed; 2: also clear what did not
      --crop X,Y+WxH     Keep only this part of the screen
      --flip-horizontal  Mirror left to right
      --flip-vertical    Mirror top to bottom
      --rotate-90        Turn a quarter clockwise
      --rotate-180       Turn half round
      --rotate-270       Turn a quarter the other way
  -o, --output OUT       Where to write the GIF; the output when not given
  -h, --help             Print this help

`-` or no file is the standard input.
";

/// The command line as it was given: GNU's way, `--name value`, `--name=value`, `-I`, `-O2`, `-IO 2`;
/// options may come after the files, `--` makes every word after it a file, a lone `-` is one. Given
/// more than once, an option keeps its last value.
#[derive(Default)]
struct Arguments {
    info: bool,
    unoptimize: bool,
    optimize: Option<String>,
    crop: Option<String>,
    flip_horizontal: bool,
    flip_vertical: bool,
    rotate_90: bool,
    rotate_180: bool,
    rotate_270: bool,
    output: Option<OsString>,
    files: Vec<OsString>,
}

fn help() -> ! {
    let mut stdout = io::stdout().lock();
    if stdout.write_all(HELP.as_bytes()).and_then(|()| stdout.flush()).is_err() {
        exit(1);
    }
    exit(0)
}

fn parse_arguments() -> Arguments {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut a = Arguments::default();
    let mut words_only = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if words_only || arg == "-" || !arg.as_encoded_bytes().starts_with(b"-") {
            a.files.push(arg.clone());
            continue;
        }
        if arg == "--" {
            words_only = true;
            continue;
        }
        let word = arg.to_string_lossy();
        if let Some(body) = word.strip_prefix("--") {
            let (name, value) = match body.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (body, None),
            };
            let flag = match name {
                "help" if value.is_none() => help(),
                "info" => Some(&mut a.info),
                "unoptimize" => Some(&mut a.unoptimize),
                "flip-horizontal" => Some(&mut a.flip_horizontal),
                "flip-vertical" => Some(&mut a.flip_vertical),
                "rotate-90" => Some(&mut a.rotate_90),
                "rotate-180" => Some(&mut a.rotate_180),
                "rotate-270" => Some(&mut a.rotate_270),
                "optimize" | "crop" | "output" => None,
                "help" => usage("option '--help' takes no value"),
                _ => usage(&format!("unknown option '--{name}'")),
            };
            if let Some(flag) = flag {
                if value.is_some() {
                    usage(&format!("option '--{name}' takes no value"));
                }
                *flag = true;
                continue;
            }
            let value = match value {
                Some(v) => OsString::from(v),
                None if i < args.len() => {
                    i += 1;
                    args[i - 1].clone()
                }
                None => usage(&format!("option '--{name}' needs a value")),
            };
            match name {
                "optimize" => a.optimize = Some(value.to_string_lossy().into_owned()),
                "crop" => a.crop = Some(value.to_string_lossy().into_owned()),
                _ => a.output = Some(value),
            }
            continue;
        }
        // Short letters, one or more in a word: `-I`, `-O2`, `-O 2`, `-IO2`.
        let bytes = word.as_bytes();
        let mut at = 1;
        while at < bytes.len() {
            let letter = bytes[at];
            if !letter.is_ascii_alphanumeric() {
                usage(&format!("unknown option '{word}'"));
            }
            at += 1;
            match letter {
                b'h' => help(),
                b'I' => a.info = true,
                b'O' | b'o' => {
                    let value = if at < bytes.len() {
                        OsString::from(&word[at..])
                    } else if i < args.len() {
                        i += 1;
                        args[i - 1].clone()
                    } else {
                        usage(&format!("option '-{}' needs a value", letter as char))
                    };
                    if letter == b'O' {
                        a.optimize = Some(value.to_string_lossy().into_owned());
                    } else {
                        a.output = Some(value);
                    }
                    break;
                }
                _ => usage(&format!("unknown option '-{}'", letter as char)),
            }
        }
    }
    a
}

/// A number of `X,Y+WxH`: one to nine decimal digits.
fn number(s: &str) -> Option<usize> {
    if s.is_empty() || s.len() > 9 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_crop(text: &str) -> Option<Crop> {
    let c = text.find(',')?;
    let p = text.find('+')?;
    let t = text.find('x')?;
    if !(c < p && p < t) {
        return None;
    }
    let crop = Crop {
        x: number(&text[..c])?,
        y: number(&text[c + 1..p])?,
        width: number(&text[p + 1..t])?,
        height: number(&text[t + 1..])?,
    };
    if crop.width == 0 || crop.height == 0 {
        return None;
    }
    Some(crop)
}

fn info(files: &[OsString]) -> ! {
    let mut names = files.to_vec();
    if names.is_empty() {
        names.push(OsString::from("-"));
    }
    let mut failed = false;
    let mut stdout = io::stdout().lock();
    for name in &names {
        let shown = shown(name);
        let data = match read_input(name) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("giftool: {shown}: {e}");
                failed = true;
                continue;
            }
        };
        match read_gif(&data) {
            Ok(gif) => {
                let label = if name == "-" { b"<stdin>".as_slice() } else { name.as_encoded_bytes() };
                if let Err(e) = stdout.write_all(&info_of(&gif, label)) {
                    fail("<stdout>", &e);
                }
            }
            Err(message) => {
                eprintln!("giftool: {shown}: {message}");
                failed = true;
            }
        }
    }
    if let Err(e) = stdout.flush() {
        fail("<stdout>", &e);
    }
    exit(if failed { 1 } else { 0 })
}

fn main() {
    let a = parse_arguments();
    if a.info {
        info(&a.files);
    }

    let mut o = Options { unoptimize: a.unoptimize, optimize: 0, crop: None, flip: Flip::None, rotate: 0 };
    if let Some(level) = &a.optimize {
        o.optimize = match level.as_str() {
            "1" => 1,
            "2" => 2,
            _ => usage(&format!("-O takes 1 or 2, not '{level}'")),
        };
    }
    if let Some(text) = &a.crop {
        match parse_crop(text) {
            Some(crop) => o.crop = Some(crop),
            None => usage(&format!("--crop takes X,Y+WxH, not '{text}'")),
        }
    }
    if a.flip_horizontal && a.flip_vertical {
        usage("one flip at most");
    }
    if a.flip_horizontal {
        o.flip = Flip::Horizontal;
    } else if a.flip_vertical {
        o.flip = Flip::Vertical;
    }
    let turns = [(a.rotate_90, 90), (a.rotate_180, 180), (a.rotate_270, 270)];
    if turns.iter().filter(|(given, _)| *given).count() > 1 {
        usage("one rotation at most");
    }
    if let Some(&(_, degrees)) = turns.iter().find(|(given, _)| *given) {
        o.rotate = degrees;
    }
    if a.files.len() > 1 {
        usage("one GIF at a time");
    }
    let name = a.files.first().cloned().unwrap_or_else(|| OsString::from("-"));
    let shown = shown(&name);
    let data = read_input(&name).unwrap_or_else(|e| fail(&shown, &e));
    let gif = read_gif(&data).unwrap_or_else(|message| fail(&shown, &message));
    let written = rewrite(&data, &gif, &o).unwrap_or_else(|message| fail(&shown, &message));
    match a.output {
        Some(target) if target != "-" => {
            if let Err(e) = std::fs::write(&target, &written) {
                fail(&Path::new(&target).display().to_string(), &e);
            }
        }
        _ => {
            let mut stdout = io::stdout().lock();
            if let Err(e) = stdout.write_all(&written).and_then(|()| stdout.flush()) {
                fail("<stdout>", &e);
            }
        }
    }
}
