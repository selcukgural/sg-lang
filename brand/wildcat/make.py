"""Makes the images the web site uses from the wildcat logo (`source.png`, 1536 x 1024, drawn on #00040c).

The picture is light drawn on a dark ground: its glow is light added to the ground. So the ground is taken out
by undoing that addition, which leaves each pixel's colour and how much of it there is (its alpha):

- `logo-dark.webp`: for dark pages, the colours as drawn, the ground made transparent.
- `logo-light.webp`: for light pages. Silver on white would vanish, so what is grey or white (the letters, the
  line of the cat) is made dark navy, the cyan ring is kept a little darker, and the faintest glow is dropped.
- `logo-small-dark.webp`, `logo-small-light.webp`: the same, without most of the glow, 88 pixels high, for the
  top of every page.
- `favicon.png` (64) and `apple-touch-icon.png` (180): a square around the letters, on the ground as drawn.

They are written to `website/plain/assets/`. Run from the repository root:

    uv venv /tmp/sg-wildcat && uv pip install --python /tmp/sg-wildcat/bin/python pillow==13.0.0 numpy==2.5.5
    /tmp/sg-wildcat/bin/python brand/wildcat/make.py
"""
import os

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "..", "website", "plain", "assets")

source = Image.open(os.path.join(HERE, "source.png")).convert("RGB")
GROUND = np.array([0, 4, 12], dtype=float)
# The ground is not quite flat: away from the drawing it rises by up to 9 (of 255). What rises less than this is
# the ground, not light, or it would show as a faint box around the logo.
FLOOR = 12 / 243
NAVY = np.array([11, 18, 32], dtype=float)  # the text colour of the light pages
SLATE = np.array([70, 84, 104], dtype=float)  # the shadows of the letters, on light pages

# The boxes, in pixels of the source: the mark with its glow, the mark alone, and a square on the letters.
WITH_GLOW = (215, 105, 1270, 915)
MARK = (280, 150, 1232, 810)
SQUARE = (330, 110, 1190, 970)


def unground(rgb):
    """Light added to the ground, made a colour with an alpha: alpha is how far the brightest channel rose."""
    lifted = np.clip(rgb - GROUND, 0, None)
    alpha = np.clip(lifted.max(axis=2) / (255 - GROUND.min()), 0, 1)
    colour = np.where(alpha[..., None] > 0, lifted / np.maximum(alpha[..., None], 1e-6) + GROUND, 0)
    alpha = np.clip((alpha - FLOOR) / (1 - FLOOR), 0, 1)
    return np.clip(colour, 0, 255), alpha


def for_dark(rgb):
    colour, alpha = unground(rgb)
    return colour, alpha


def for_light(rgb):
    colour, alpha = unground(rgb)
    # How grey a pixel is: 0 for the cyan of the ring, 1 for the silver of the letters.
    spread = (colour.max(axis=2) - colour.min(axis=2)) / np.maximum(colour.max(axis=2), 1)
    grey = np.clip(1 - spread / 0.35, 0, 1)[..., None]
    light = colour.mean(axis=2, keepdims=True) / 255
    dark = NAVY + (SLATE - NAVY) * (1 - light)
    cyan = colour * 0.78
    colour = grey * dark + (1 - grey) * cyan
    # The faint glow reads as haze on white: below a fifth it is dropped, and it reaches full at three fifths.
    alpha = np.clip((alpha - 0.2) / 0.4, 0, 1)
    # Silver reads lighter than it is: the letters are made solid where they were nearly so.
    alpha = np.where(grey[..., 0] > 0.5, np.clip(alpha * 1.6, 0, 1), alpha)
    return colour, alpha


def save(box, size, name, mode=None, **options):
    image = source.crop(box).resize(size, Image.LANCZOS)
    if mode is not None:
        colour, alpha = mode(np.asarray(image).astype(float))
        rgba = np.dstack([colour, alpha * 255]).round().astype(np.uint8)
        image = Image.fromarray(rgba, "RGBA")
    image.save(os.path.join(OUT, name), **options)
    print(name, image.size)


def width(box, w):
    return (w, round((box[3] - box[1]) * w / (box[2] - box[0])))


def height(box, h):
    return (round((box[2] - box[0]) * h / (box[3] - box[1])), h)


save(WITH_GLOW, width(WITH_GLOW, 880), "logo-dark.webp", for_dark, quality=90, method=6)
save(WITH_GLOW, width(WITH_GLOW, 880), "logo-light.webp", for_light, quality=90, method=6)
save(MARK, height(MARK, 88), "logo-small-dark.webp", for_dark, quality=92, method=6)
save(MARK, height(MARK, 88), "logo-small-light.webp", for_light, quality=92, method=6)
save(SQUARE, (64, 64), "favicon.png", optimize=True)
save(SQUARE, (180, 180), "apple-touch-icon.png", optimize=True)
