"""Makes the images the web site uses from the wildcat logo (`source.png`, 1536 x 1024, drawn on #00040c).

- `logo.webp`: the whole mark with its glow, for the front page, 880 pixels wide.
- `logo-small.webp`: the mark without most of its glow, 88 pixels high, for the top of every page.
- `favicon.png` (64) and `apple-touch-icon.png` (180): a square around the letters.

They are written to `website/plain/assets/`. Run from the repository root:

    uv venv /tmp/sg-wildcat && uv pip install --python /tmp/sg-wildcat/bin/python pillow==13.0.0
    /tmp/sg-wildcat/bin/python brand/wildcat/make.py
"""
import os

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "..", "website", "plain", "assets")

source = Image.open(os.path.join(HERE, "source.png")).convert("RGB")

# The boxes, in pixels of the source: the mark with its glow, the mark alone, and a square on the letters.
WITH_GLOW = (215, 105, 1270, 915)
MARK = (280, 150, 1232, 810)
SQUARE = (330, 110, 1190, 970)


def save(box, size, name, **options):
    image = source.crop(box)
    image = image.resize(size, Image.LANCZOS)
    image.save(os.path.join(OUT, name), **options)
    print(name, image.size)


def width(box, w):
    return (w, round((box[3] - box[1]) * w / (box[2] - box[0])))


def height(box, h):
    return (round((box[2] - box[0]) * h / (box[3] - box[1])), h)


save(WITH_GLOW, width(WITH_GLOW, 880), "logo.webp", quality=88, method=6)
save(MARK, height(MARK, 88), "logo-small.webp", quality=90, method=6)
save(SQUARE, (64, 64), "favicon.png", optimize=True)
save(SQUARE, (180, 180), "apple-touch-icon.png", optimize=True)
