"""The GIFs of the trial, made from a seed (choice 1878), the same bytes on every machine. They are
written by an encoder of their own, not by `giftool`'s, and in the ways encoders in the world write:
tables of 2 to 256 colors, a code size larger than the table needs, sub-blocks of every length, a
table that fills and is cleared, one that fills and is never cleared (a "deferred clear"), clears
sent early, interlaced frames, local tables, a transparent index, the four disposals, comments and
extensions `giftool` does not know.

    python3 trials/giftool/tests/inputs.py OUT_DIR     writes the GIFs of the tests and of the measurements
"""

import os
import random
import struct
import sys


def lzw(pixels, min_code_size, mode="clear", rng=None):
    """The LZW data of `pixels`, as bytes (not cut in sub-blocks). `mode`: `clear` clears the table
    when it is full, `deferred` never clears it, `early` clears it at a size drawn from `rng`."""
    clear = 1 << min_code_size
    end = clear + 1
    out = bytearray()
    acc = 0
    nacc = 0
    width = min_code_size + 1
    nxt = clear + 2
    table = {}
    limit = 4096 if mode != "early" else rng.randrange(clear + 3, 4096)

    def put(code):
        nonlocal acc, nacc
        acc |= code << nacc
        nacc += width
        while nacc >= 8:
            out.append(acc & 0xFF)
            acc >>= 8
            nacc -= 8

    put(clear)
    if not pixels:
        put(end)
        if nacc:
            out.append(acc & 0xFF)
        return bytes(out)
    ent = pixels[0]
    for c in pixels[1:]:
        key = (ent << 8) | c
        code = table.get(key)
        if code is not None:
            ent = code
            continue
        put(ent)
        if nxt >= (1 << width) and width < 12:
            width += 1
        if nxt < limit:
            table[key] = nxt
            nxt += 1
        elif mode != "deferred":
            put(clear)
            table.clear()
            width = min_code_size + 1
            nxt = clear + 2
            if mode == "early":
                limit = rng.randrange(clear + 3, 4096)
        ent = c
    put(ent)
    if nxt >= (1 << width) and width < 12:
        width += 1
    put(end)
    if nacc:
        out.append(acc & 0xFF)
    return bytes(out)


def sub_blocks(data, rng=None):
    """`data` in sub-blocks of 255 bytes, or of lengths drawn from `rng`, and the empty one."""
    out = bytearray()
    at = 0
    while at < len(data):
        n = 255 if rng is None else rng.choice((1, 2, 3, 17, 100, 254, 255, 255, 255))
        piece = data[at:at + n]
        out.append(len(piece))
        out += piece
        at += n
    out.append(0)
    return bytes(out)


def interlaced(pixels, width, height):
    rows = []
    for start, step in ((0, 8), (4, 8), (2, 4), (1, 2)):
        rows.extend(range(start, height, step))
    return b"".join(pixels[y * width:(y + 1) * width] for y in rows)


def table_bytes(colors, bits):
    raw = bytearray()
    for i in range(1 << bits):
        r, g, b = colors[i] if i < len(colors) else (0, 0, 0)
        raw += bytes((r, g, b))
    return bytes(raw)


def palette(rng, n):
    return [(rng.randrange(256), rng.randrange(256), rng.randrange(256)) for _ in range(n)]


def bits_for(n):
    bits = 1
    while (1 << bits) < n:
        bits += 1
    return bits


class Writer:
    def __init__(self, width, height, global_colors=None, background=0, loop=None, version=b"GIF89a"):
        self.out = bytearray(version)
        flags = 0
        if global_colors is not None:
            bits = bits_for(len(global_colors))
            flags = 0x80 | 0x70 | (bits - 1)
        self.out += struct.pack("<HHBBB", width, height, flags, background, 0)
        if global_colors is not None:
            self.out += table_bytes(global_colors, bits)
        if loop is not None:
            self.out += b"\x21\xFF\x0BNETSCAPE2.0\x03\x01" + struct.pack("<H", loop) + b"\x00"

    def comment(self, text, rng=None):
        self.out += b"\x21\xFE" + sub_blocks(text, rng)

    def extension(self, kind, data):
        self.out += bytes((0x21, kind)) + sub_blocks(data)

    def application(self, name, data):
        self.out += b"\x21\xFF\x0B" + name + sub_blocks(data)

    def frame(self, left, top, width, height, pixels, local=None, transparent=-1, disposal=0,
              delay=0, interlace=False, min_code_size=None, mode="clear", rng=None, control=True):
        if control:
            packed = (disposal << 2) | (1 if transparent >= 0 else 0)
            self.out += b"\x21\xF9\x04" + struct.pack("<BHB", packed, delay, max(transparent, 0)) + b"\x00"
        flags = 0
        bits = 0
        if local is not None:
            bits = bits_for(len(local))
            flags |= 0x80 | (bits - 1)
        if interlace:
            flags |= 0x40
        self.out += b"\x2C" + struct.pack("<HHHHB", left, top, width, height, flags)
        if local is not None:
            self.out += table_bytes(local, bits)
        if min_code_size is None:
            min_code_size = max(2, bits_for(max(pixels) + 1)) if pixels else 2
        data = interlaced(pixels, width, height) if interlace else pixels
        self.out += bytes((min_code_size,)) + sub_blocks(lzw(data, min_code_size, mode, rng), rng)

    def end(self):
        return bytes(self.out) + b"\x3B"


def picture(rng, width, height, colors):
    """The indices of a picture: flat, noise, stripes or a sprite on a flat ground."""
    kind = rng.choice(("flat", "noise", "stripes", "sprite", "sprite", "stripes"))
    if kind == "flat":
        return bytes((rng.randrange(colors),)) * (width * height)
    mapping = bytes(i % colors for i in range(256))
    if kind == "noise":
        return rng.randbytes(width * height).translate(mapping)
    if kind == "stripes":
        step = rng.randrange(1, 9)
        row = bytes(((x // step) % colors for x in range(width)))
        rows = []
        for y in range(height):
            shift = (y // step) % width
            rows.append(row[shift:] + row[:shift])
        return b"".join(rows)
    ground = rng.randrange(colors)
    px = bytearray(bytes((ground,)) * (width * height))
    sw, sh = max(1, width // 3), max(1, height // 3)
    sx, sy = rng.randrange(width - sw + 1), rng.randrange(height - sh + 1)
    blob = rng.randbytes(sw * sh).translate(mapping)
    for y in range(sh):
        px[(sy + y) * width + sx:(sy + y) * width + sx + sw] = blob[y * sw:(y + 1) * sw]
    return bytes(px)


def animation(seed, width, height, frames, colors=255, small=True, everything=True, shared=True):
    """An animation of `frames` frames: the first covers the screen, the others a place of it (a
    fifth of each side to the whole of it when `small` is false). With `everything`, frames take
    local tables, transparency, every disposal, interlacing, comments, unknown extensions and every
    way of the encoder. A local table holds colors of the screen's, as an encoder that made one
    palette for the whole writes them, so that the screen never shows more colors than the screen's
    table, which has one index to spare for the clear pixels of `-O` when `colors` is 255."""
    rng = random.Random(f"gif-{seed}")
    gcolors = palette(rng, colors)
    w = Writer(width, height, gcolors, background=rng.randrange(colors), loop=0)
    if everything:
        w.application(b"XMP DataXMP", b"<x:xmpmeta/>" * 3)
        w.comment(b"made from a seed\n\x01\x7f\xff\\end")
    for i in range(frames):
        if i == 0:
            fx, fy, fw, fh = 0, 0, width, height
        else:
            lo = 0.05 if small else 0.2
            hi = 0.3 if small else 1.0
            fw = max(1, int(width * rng.uniform(lo, hi)))
            fh = max(1, int(height * rng.uniform(lo, hi)))
            fx, fy = rng.randrange(width - fw + 1), rng.randrange(height - fh + 1)
        local = None
        n = colors
        if everything and rng.random() < 0.2:
            n = min(rng.choice((2, 3, 4, 7, 16, 100, 256)), colors)
            local = rng.sample(gcolors, n) if shared else palette(rng, n)
        px = picture(rng, fw, fh, n)
        transparent = -1
        if everything and rng.random() < 0.4:
            transparent = rng.randrange(n)
        disposal = rng.choice((0, 1, 1, 2, 3)) if everything else 1
        if everything and rng.random() < 0.05:
            w.comment(f"frame {i}".encode(), rng)
        if everything and rng.random() < 0.02:
            w.extension(0x01, b"plain text, not drawn")
        mode = rng.choice(("clear", "clear", "deferred", "early")) if everything else "clear"
        mcs = None
        if everything and rng.random() < 0.1:
            mcs = rng.randrange(bits_for(n), 12) if bits_for(n) < 12 else None
            mcs = max(2, mcs) if mcs is not None else None
        w.frame(fx, fy, fw, fh, px, local, transparent, disposal, delay=rng.choice((0, 2, 5, 10, 100, 65535)),
                interlace=everything and rng.random() < 0.15, min_code_size=mcs, mode=mode,
                rng=rng if everything else None)
    if everything:
        w.comment(b"the end")
    return w.end()


def large(seed, side):
    """One frame of `side` by `side`, 256 colors, stripes with noise in them."""
    rng = random.Random(f"gif-large-{seed}")
    w = Writer(side, side, palette(rng, 256))
    rows = []
    row = bytes((x // 3) % 256 for x in range(side * 2))
    for y in range(side):
        shift = (y * 7) % side
        r = bytearray(row[shift:shift + side])
        if y % 16 < 4:
            at = (y * 131) % (side - 64)
            r[at:at + 64] = rng.randbytes(64)
        rows.append(bytes(r))
    w.frame(0, 0, side, side, b"".join(rows), control=False)
    return w.end()


def edges():
    """Small GIFs at the edges of what GIF allows, each named for what it holds; and broken ones,
    whose names start with `bad-`."""
    rng = random.Random("gif-edges")
    out = {}
    w = Writer(1, 1, [(1, 2, 3), (4, 5, 6)])
    w.frame(0, 0, 1, 1, b"\x01", control=False)
    out["one-pixel"] = w.end()
    w = Writer(3, 2, [(9, 9, 9)] * 2, version=b"GIF87a")
    w.frame(0, 0, 3, 2, b"\x00\x01\x00\x01\x00\x01", control=False)
    out["gif87a"] = w.end()
    w = Writer(10, 10)
    w.frame(0, 0, 10, 10, bytes(range(4)) * 25, local=[(255, 0, 0), (0, 255, 0), (0, 0, 255), (1, 1, 1)])
    w.frame(5, 5, 10, 10, bytes(100), local=[(7, 7, 7), (8, 8, 8)], disposal=2)
    w.frame(20, 20, 3, 3, bytes(9), local=[(7, 7, 7), (8, 8, 8)])
    out["no-global-past-the-screen"] = w.end()
    w = Writer(8, 8, [(0, 0, 0), (255, 255, 255)])
    w.frame(0, 0, 8, 8, bytes([5] * 64), control=False)
    out["index-past-the-table"] = w.end()
    w = Writer(70, 50, palette(rng, 4), loop=3)
    for i, d in enumerate((0, 1, 2, 3, 3, 2, 1, 0)):
        w.frame(i * 5, i * 3, 30, 20, picture(rng, 30, 20, 4), transparent=i % 4, disposal=d, delay=i * 7)
    w.comment(b"\x00a\x00")
    out["every-disposal"] = w.end()
    w = Writer(64, 64, palette(rng, 256), loop=65535)
    w.frame(0, 0, 64, 64, rng.randbytes(64 * 64), mode="deferred")
    w.frame(0, 0, 64, 64, rng.randbytes(64 * 64), mode="early", rng=rng)
    w.frame(0, 0, 64, 64, rng.randbytes(64 * 64), interlace=True, min_code_size=11, rng=rng)
    out["deferred-early-wide"] = w.end()
    w = Writer(4, 4, palette(rng, 2))
    w.frame(0, 0, 4, 4, bytes(16), control=False)
    w.frame(0, 0, 4, 4, bytes([1] * 16))
    w.application(b"ANIMEXTS1.0", b"\x01\x05\x00")
    out["two-frames-loop-after"] = w.end()
    w = Writer(9, 13, palette(rng, 16))
    w.frame(0, 0, 9, 13, picture(rng, 9, 13, 16), interlace=True)
    w.frame(2, 1, 5, 11, picture(rng, 5, 11, 16), interlace=True, transparent=3, disposal=3)
    out["interlaced-odd"] = w.end()
    big = Writer(300, 200, palette(rng, 200), loop=0)
    for i in range(30):
        x, y = rng.randrange(250), rng.randrange(150)
        big.frame(x, y, 50, 50, picture(rng, 50, 50, 200), transparent=rng.choice((-1, 0, 5)),
                  disposal=rng.choice((0, 1, 2, 3)), delay=4)
    out["sprites"] = big.end()
    w = Writer(4, 4, palette(rng, 4))
    out["no-frames"] = w.end()
    out["many-colors"] = many_colors()
    # Broken ones.
    good = out["every-disposal"]
    out["bad-truncated"] = good[:len(good) // 2]
    out["bad-not-a-gif"] = b"PNG\x89 not a gif at all"
    out["bad-empty"] = b""
    w = Writer(4, 4, palette(rng, 4))
    w.frame(0, 0, 4, 4, bytes(16), min_code_size=12, control=False)
    out["bad-code-size"] = w.end()
    w = Writer(4, 4, palette(rng, 4))
    w.out += b"\x2C" + struct.pack("<HHHHB", 0, 0, 4, 4, 0) + b"\x02\x02" + bytes((0xC4, 0x01)) + b"\x00\x3B"
    out["bad-code-past-table"] = bytes(w.out)
    w = Writer(4, 4, palette(rng, 4))
    w.out += b"\x2C" + struct.pack("<HHHHB", 0, 0, 0, 4, 0)
    w.out += bytes((2,)) + sub_blocks(lzw(b"", 2)) + b"\x3B"
    out["bad-zero-width"] = bytes(w.out)
    w = Writer(4, 4, palette(rng, 4))
    w.frame(0, 0, 4, 4, bytes(8), control=False)
    out["bad-too-few-pixels"] = w.end()
    w = Writer(4, 4, palette(rng, 4))
    w.out += b"\x2C" + struct.pack("<HHHHB", 0, 0, 4, 4, 0) + b"\x02" + (b"\xFF" + bytes(255)) * 4
    out["bad-no-end-of-blocks"] = bytes(w.out)
    w = Writer(16384, 16385, palette(rng, 2))
    out["bad-screen-too-large"] = w.end()
    w = Writer(4, 4, palette(rng, 2))
    w.out += b"\x2C" + struct.pack("<HHHHB", 0, 0, 65535, 65535, 0) + b"\x02\x00\x3B"
    out["bad-frame-too-large"] = bytes(w.out)
    w = Writer(4, 4, palette(rng, 2))
    w.out += b"\x2C" + struct.pack("<HHHHB", 65000, 0, 600, 4, 0) + b"\x02\x00\x3B"
    out["bad-past-65535"] = bytes(w.out)
    w = Writer(4, 4, palette(rng, 2))
    w.out += b"\x21\xF9\x05\x00\x00\x00\x00\x00\x00"
    out["bad-control-length"] = w.end()
    w = Writer(4, 4, palette(rng, 2))
    w.out += b"\x99"
    out["bad-block-kind"] = w.end()
    w = Writer(4, 4, palette(rng, 2))
    w.application(b"NETSCAPE2.0", b"\x01\x00")
    out["bad-loop-length"] = w.end()
    return out


def many_colors():
    """Two frames whose screen, once the second is drawn, shows 512 colors: more than a frame of
    `--unoptimize` can hold (choice 1876)."""
    w = Writer(32, 16)
    left = [(i, 0, 0) for i in range(256)]
    right = [(0, i, 0) for i in range(256)]
    w.frame(0, 0, 16, 16, bytes(range(256)), local=left, disposal=1)
    w.frame(16, 0, 16, 16, bytes(range(256)), local=right, disposal=1)
    return w.end()


MEASURED = {
    # name -> how it is made (choice 1879)
    "animation": lambda: animation(1, 480, 270, 2000),
    "animation-full": lambda: animation(2, 480, 270, 300, small=False),
    "large": lambda: large(1, 8192),
}


def small_gifs(folder, count=10_000):
    rng = random.Random("gif-small")
    os.makedirs(folder, exist_ok=True)
    for i in range(count):
        data = animation(f"s{i}", rng.randrange(8, 64), rng.randrange(8, 64), rng.randrange(1, 6), colors=16)
        with open(os.path.join(folder, f"{i:05d}.gif"), "wb") as f:
            f.write(data)


def write_all(out_dir, measured=True):
    os.makedirs(out_dir, exist_ok=True)
    for name, data in edges().items():
        with open(os.path.join(out_dir, name + ".gif"), "wb") as f:
            f.write(data)
    with open(os.path.join(out_dir, "test-animation.gif"), "wb") as f:
        f.write(animation("test", 120, 90, 40))
    with open(os.path.join(out_dir, "test-animation-own-colors.gif"), "wb") as f:
        f.write(animation("own", 100, 70, 30, colors=256, shared=False))
    if measured:
        for name, make in MEASURED.items():
            path = os.path.join(out_dir, name + ".gif")
            if not os.path.exists(path):
                with open(path + ".part", "wb") as f:
                    f.write(make())
                os.replace(path + ".part", path)
        if not os.path.exists(os.path.join(out_dir, "small")):
            small_gifs(os.path.join(out_dir, "small.part"))
            os.replace(os.path.join(out_dir, "small.part"), os.path.join(out_dir, "small"))


if __name__ == "__main__":
    write_all(sys.argv[1], measured="--tests-only" not in sys.argv)
