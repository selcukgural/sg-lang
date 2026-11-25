"""A model of `giftool` in Python, written from the GIF89a specification and from `trials/giftool/README.md`,
not from the three versions (choice 1880). It reads a GIF as the three must read it, puts its frames
together on a screen as a viewer shows them, and says what each option must make of them; the tests
hold what the three wrote to it, picture by picture. It also writes `gifsicle -I`'s text (choice
1877). No Pillow: the model's decoder is a second one, written apart from the three.
"""

MAX_PIXELS = 1 << 28  # choice 1881: a frame or a screen of more pixels is refused


class GifError(Exception):
    pass


class Frame:
    def __init__(self):
        self.left = self.top = self.width = self.height = 0
        self.interlace = False
        self.local = None          # a list of (r, g, b), or None
        self.transparent = -1
        self.disposal = 0
        self.delay = 0
        self.min_code_size = 0
        self.pixels = None         # bytes, row by row, not interlaced
        self.comments = []         # each comment's bytes, in order
        self.extensions = 0        # extensions that are neither a comment nor known


class Gif:
    def __init__(self):
        self.width = self.height = 0
        self.global_table = None
        self.background = 0
        self.loop = None
        self.frames = []
        self.end_comments = []
        self.end_extensions = 0


class Reader:
    def __init__(self, data):
        self.data = data
        self.at = 0

    def byte(self):
        if self.at >= len(self.data):
            raise GifError("the GIF ends too soon")
        b = self.data[self.at]
        self.at += 1
        return b

    def u16(self):
        lo = self.byte()
        return lo | (self.byte() << 8)

    def take(self, n):
        if self.at + n > len(self.data):
            raise GifError("the GIF ends too soon")
        b = self.data[self.at:self.at + n]
        self.at += n
        return b

    def blocks(self):
        """The bytes of the sub-blocks up to the empty one, joined."""
        out = bytearray()
        while True:
            n = self.byte()
            if n == 0:
                return bytes(out)
            out += self.take(n)

    def table(self, bits):
        raw = self.take(3 * (1 << bits))
        return [(raw[i], raw[i + 1], raw[i + 2]) for i in range(0, len(raw), 3)]


def decode_lzw(min_code_size, data, count):
    """The `count` indices of a frame from its LZW data (README, "Reading")."""
    if min_code_size < 2 or min_code_size > 11:
        raise GifError("a frame's code size is not one GIF allows")
    clear = 1 << min_code_size
    end = clear + 1
    prefix = [0] * 4096
    suffix = [0] * 4096
    length = [0] * 4096
    for c in range(min(clear, 256)):
        suffix[c] = c
        length[c] = 1
    width = min_code_size + 1
    nxt = clear + 2
    prev = -1
    out = bytearray()
    bits = 0
    nbits = 0
    at = 0
    n = len(data)
    while len(out) < count:
        while nbits < width and at < n:
            bits |= data[at] << nbits
            nbits += 8
            at += 1
        if nbits < width:
            break
        code = bits & ((1 << width) - 1)
        bits >>= width
        nbits -= width
        if code == clear:
            width = min_code_size + 1
            nxt = clear + 2
            prev = -1
            continue
        if code == end:
            break
        if 255 < code < clear:
            raise GifError("a frame's data has a pixel past 255")
        if prev < 0:
            if code >= clear:
                raise GifError("a frame's data has a code that is not in its table")
            out.append(code)
            prev = code
            continue
        if code > nxt or (code == nxt and nxt >= 4096):
            raise GifError("a frame's data has a code that is not in its table")
        if code < nxt:
            first = code
            while length[first] > 1:
                first = prefix[first]
            first = suffix[first]
        else:
            first = prev
            while length[first] > 1:
                first = prefix[first]
            first = suffix[first]
        if nxt < 4096:
            prefix[nxt] = prev
            suffix[nxt] = first
            length[nxt] = length[prev] + 1
            nxt += 1
            if nxt == (1 << width) and width < 12:
                width += 1
        # write the string of `code`
        c = code
        s = bytearray(length[c])
        i = length[c] - 1
        while i >= 0:
            s[i] = suffix[c]
            c = prefix[c]
            i -= 1
        out += s
        prev = code
    if len(out) < count:
        raise GifError("a frame's data ends before its pixels do")
    return bytes(out[:count])


def deinterlace(pixels, width, height):
    out = bytearray(len(pixels))
    rows = []
    for start, step in ((0, 8), (4, 8), (2, 4), (1, 2)):
        rows.extend(range(start, height, step))
    for i, y in enumerate(rows):
        out[y * width:(y + 1) * width] = pixels[i * width:(i + 1) * width]
    return bytes(out)


def read(data, decode=True):
    r = Reader(data)
    head = r.take(6) if len(data) >= 6 else None
    if head not in (b"GIF87a", b"GIF89a"):
        raise GifError("not a GIF")
    g = Gif()
    g.width = r.u16()
    g.height = r.u16()
    packed = r.byte()
    g.background = r.byte()
    r.byte()
    if g.width == 0 or g.height == 0:
        raise GifError("the screen has no size")
    if g.width * g.height > MAX_PIXELS:
        raise GifError("the screen has more pixels than giftool takes")
    if packed & 0x80:
        g.global_table = r.table((packed & 7) + 1)
    frame = Frame()
    while True:
        block = r.byte()
        if block == 0x3B:
            break
        if block == 0x21:
            kind = r.byte()
            if kind == 0xF9:
                if r.byte() != 4:
                    raise GifError("a graphic control extension is not four bytes")
                p = r.byte()
                frame.disposal = (p >> 2) & 7
                frame.delay = r.u16()
                t = r.byte()
                frame.transparent = t if p & 1 else -1
                if r.byte() != 0:
                    raise GifError("a graphic control extension is not four bytes")
            elif kind == 0xFE:
                frame.comments.append(r.blocks())
            elif kind == 0xFF:
                if r.byte() != 11:
                    raise GifError("an application extension's name is not eleven bytes")
                name = r.take(11)
                if name in (b"NETSCAPE2.0", b"ANIMEXTS1.0"):
                    if r.byte() != 3:
                        raise GifError("a loop extension is not three bytes")
                    r.byte()
                    g.loop = r.u16()
                    if r.byte() != 0:
                        raise GifError("a loop extension is not three bytes")
                else:
                    r.blocks()
                    frame.extensions += 1
            else:
                r.blocks()
                frame.extensions += 1
        elif block == 0x2C:
            frame.left = r.u16()
            frame.top = r.u16()
            frame.width = r.u16()
            frame.height = r.u16()
            p = r.byte()
            if frame.width == 0 or frame.height == 0:
                raise GifError("a frame has no size")
            if frame.left + frame.width > 65535 or frame.top + frame.height > 65535:
                raise GifError("a frame goes past 65535")
            if frame.width * frame.height > MAX_PIXELS:
                raise GifError("a frame has more pixels than giftool takes")
            if p & 0x80:
                frame.local = r.table((p & 7) + 1)
            frame.interlace = bool(p & 0x40)
            frame.min_code_size = r.byte()
            if frame.min_code_size < 2 or frame.min_code_size > 11:
                raise GifError("a frame's code size is not one GIF allows")
            lzw = r.blocks()
            if decode:
                px = decode_lzw(frame.min_code_size, lzw, frame.width * frame.height)
                if frame.interlace:
                    px = deinterlace(px, frame.width, frame.height)
                frame.pixels = px
            g.frames.append(frame)
            frame = Frame()
        else:
            raise GifError("a block of a kind GIF does not have")
    g.end_comments = frame.comments
    g.end_extensions = frame.extensions
    return g


# ---- gifsicle -I (choice 1877) ----

DISPOSALS = ["none", "asis", "background", "previous", "4", "5", "6", "7"]


def safe(data):
    out = []
    for i, b in enumerate(data):
        left = len(data) - i
        if b < 0x20 or b >= 0x7F or b == 0x5C:
            esc = {7: "\\a", 8: "\\b", 12: "\\f", 10: "\\n", 13: "\\r", 9: "\\t", 11: "\\v", 0x5C: "\\\\"}
            if b in esc:
                out.append(esc[b])
            elif b == 0:
                if left > 1:
                    out.append("\\000")
            else:
                out.append("\\%03o" % b)
        else:
            out.append(chr(b))
    return "".join(out)


def info(g, name):
    """What `gifsicle -I` writes of a GIF; nothing for one with no frames, as gifsicle."""
    if not g.frames:
        return ""
    lines = []
    n = len(g.frames)
    lines.append(f"* {name} {n} image{'' if n == 1 else 's'}")
    lines.append(f"  logical screen {g.width}x{g.height}")
    if g.global_table is not None:
        lines.append(f"  global color table [{len(g.global_table)}]")
        lines.append(f"  background {g.background}")
    if g.loop is not None:
        lines.append("  loop forever" if g.loop == 0 else f"  loop count {g.loop}")
    for c in g.end_comments:
        lines.append("  end comment " + safe(c))
    if g.end_extensions:
        lines.append(f"  end extensions {g.end_extensions}")
    for i, f in enumerate(g.frames):
        s = f"  + image #{i} {f.width}x{f.height}"
        if f.left or f.top:
            s += f" at {f.left},{f.top}"
        if f.interlace:
            s += " interlaced"
        if f.transparent >= 0:
            s += f" transparent {f.transparent}"
        lines.append(s)
        for c in f.comments:
            lines.append("    comment " + safe(c))
        if f.extensions:
            lines.append(f"    extensions {f.extensions}")
        if f.local is not None:
            lines.append(f"    local color table [{len(f.local)}]")
        if f.disposal or f.delay:
            s = "   "
            if f.disposal:
                s += " disposal " + DISPOSALS[f.disposal]
            if f.delay:
                s += " delay %d.%02ds" % (f.delay // 100, f.delay % 100)
            lines.append(s)
    return "".join(l + "\n" for l in lines)


# ---- the pictures a viewer shows ----

CLEAR = -1  # a pixel nothing has been drawn on, or that was cleared


def composites(g):
    """The screen after each frame, as a list of the colors of its pixels: (r, g, b), or CLEAR.
    A frame is drawn only where it lies on the screen; an index past its table is black; the
    disposal `background` clears the frame's place, `previous` puts back what was there."""
    W, H = g.width, g.height
    canvas = [CLEAR] * (W * H)
    out = []
    for f in g.frames:
        table = f.local if f.local is not None else g.global_table
        x0, y0 = f.left, f.top
        x1, y1 = min(f.left + f.width, W), min(f.top + f.height, H)
        saved = None
        if f.disposal == 3:
            saved = [canvas[y * W + x0:y * W + x1] for y in range(y0, y1)] if x0 < x1 else []
        for y in range(y0, y1):
            row = (y - f.top) * f.width
            for x in range(x0, x1):
                p = f.pixels[row + x - f.left]
                if p == f.transparent:
                    continue
                canvas[y * W + x] = table[p] if table is not None and p < len(table) else (0, 0, 0)
        out.append(list(canvas))
        if f.disposal == 2:
            for y in range(y0, y1):
                for x in range(x0, x1):
                    canvas[y * W + x] = CLEAR
        elif f.disposal == 3 and saved is not None:
            for i, y in enumerate(range(y0, y1)):
                canvas[y * W + x0:y * W + x1] = saved[i]
    return out


def parse_crop(text):
    """`X,Y+WxH` -> (x, y, w, h), or None."""
    try:
        pos, size = text.split("+")
        x, y = pos.split(",")
        w, h = size.split("x")
        vals = [int(v) for v in (x, y, w, h)]
    except ValueError:
        return None
    if any(not s.isdigit() for s in (x, y, w, h)):
        return None
    return tuple(vals)


def transform(screens, W, H, crop, flip, rotate):
    """The screens cropped, then flipped, then turned clockwise; and the new screen's size."""
    if crop is not None:
        cx, cy, cw, ch = crop
        x1, y1 = min(cx + cw, W), min(cy + ch, H)
        nw, nh = x1 - cx, y1 - cy
        screens = [[s[y * W + x] for y in range(cy, y1) for x in range(cx, x1)] for s in screens]
        W, H = nw, nh
    if flip == "h":
        screens = [[s[y * W + (W - 1 - x)] for y in range(H) for x in range(W)] for s in screens]
    elif flip == "v":
        screens = [[s[(H - 1 - y) * W + x] for y in range(H) for x in range(W)] for s in screens]
    if rotate == 90:
        # new (x, y) shows old (y, H - 1 - x); the new screen is H wide and W high
        screens = [[s[(H - 1 - x) * W + y] for y in range(W) for x in range(H)] for s in screens]
        W, H = H, W
    elif rotate == 180:
        screens = [[s[(H - 1 - y) * W + (W - 1 - x)] for y in range(H) for x in range(W)] for s in screens]
    elif rotate == 270:
        screens = [[s[x * W + (W - 1 - y)] for y in range(W) for x in range(H)] for s in screens]
        W, H = H, W
    return screens, W, H
