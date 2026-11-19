"""Pasha's badge, the mascot of sg-lang, redrawn from `badge-source.png`.

The badge was drawn as a raster (1254 px). This redraws it as clean vector art:

1. Each pixel is given one of the seven colours of the badge (k-means seeded with them).
2. Known faults of the raster are fixed: the fringe on the top of the tail, the place where the outline
   of the right cheek ran into the tail, and specks left by anti-aliasing.
3. What is round in the drawing is made exactly round: the disc, the nazar bead, and the eyes (iris
   crescent, pupil and highlight), which are also made mirror images of each other.
4. Every colour is traced (potrace) at three times the size and the layers are stacked, each one
   reaching under the layers above it so no seam shows between them.

`mark_body()` is the badge in a 256 box; `small_body()` is a cut drawn by hand for 16 to 48 px.
"""
import os

import numpy as np
import potrace
from PIL import Image
from scipy import ndimage as nd

HERE = os.path.dirname(os.path.abspath(__file__))

COLOURS = {
    "white": "#FFFFFF", "cobalt": "#0028A6", "navy": "#011E65", "pink": "#FD6E7A",
    "iris": "#01C5FE", "amber": "#FDA402", "sky": "#43D6FD",
}
NAMES = ["white", "cobalt", "navy", "pink", "iris", "amber", "sky"]
WHITE, COB, NAVY, PINK, IRIS, AMB, SKY = range(7)
SEEDS = [(242, 242, 242), (5, 40, 170), (5, 25, 95), (245, 105, 120), (0, 200, 250), (245, 165, 0), (70, 210, 250)]

# Geometry measured on source.png (pixels): circles fitted to the edges, then made symmetric about AX.
DISC = (624.55, 627.24, 577.56)
AX = 627.2
EYE_Y, IRIS_R, IRIS_DX = 558.7, 75.7, 139.3
PUP_Y, PUP_R, PUP_DX = 552.4, 46.1, 125.8
HL_Y, HL_R, HL_DX = 545.0, 17.0, 112.4
LID_Y, LID_R, LID_DX = 590.7, 74.6, 162.75
NAZAR = (582.3, 1025.6)
NAZAR_WHITE, NAZAR_SKY, NAZAR_NAVY = 116.5, 81.4, 44.5
K = 3  # tracing scale
SCALE = 128 / DISC[2] * 0.992  # source pixels -> the 256 viewBox, with a margin of one unit


def classify(img):
    centres = np.array(SEEDS, float)
    for _ in range(5):
        dist = ((img[:, :, None, :] - centres[None, None]) ** 2).sum(-1)
        lab = dist.argmin(-1)
        centres = np.array([np.median(img[lab == i], axis=0) for i in range(7)])
    return lab


def fix_labels(lab):
    h, w = lab.shape
    yy, xx = np.mgrid[:h, :w].astype(float)
    lab = lab.copy()
    # The top of the tail fades from cobalt to navy: it is cobalt.
    near = nd.binary_dilation(lab == COB, iterations=6)
    lab[(lab == NAVY) & near & (yy > 745)] = COB
    # The outline of the right cheek ran into the tail; it now ends in a point on the line between the
    # two white tips around it.
    (x1, y1), (x2, y2) = (1040, 697), (1003, 748)
    side = (x2 - x1) * (yy - y1) - (y2 - y1) * (xx - x1)
    lab[(lab == NAVY) & (yy > 690) & (xx > 990) & (xx < 1060) & (side < 0)] = COB
    lab[(lab == COB) & (yy > 690) & (yy < 749) & (xx > 990) & (xx < 1045) & (side >= 0)] = NAVY
    # Specks: pixels between two colours read as a third go to their neighbours.
    for _ in range(2):
        for c in range(7):
            lb, n = nd.label(lab == c)
            if n == 0:
                continue
            sizes = np.bincount(lb.ravel())
            for k in np.nonzero(sizes < 150)[0]:
                if k == 0:
                    continue
                m = lb == k
                ring = nd.binary_dilation(m, iterations=2) & ~m
                votes = np.bincount(lab[ring], minlength=7)
                votes[c] = 0
                lab[m] = votes.argmax()
    return lab


def upsample(lab):
    soft = [nd.zoom(nd.gaussian_filter((lab == c).astype(np.float32), 2.2), K, order=1) for c in range(7)]
    return np.argmax(np.stack(soft), 0).astype(np.int8)


def trace(mask):
    """Traces a mask (True = paint) into path data in the 256 viewBox."""
    paths = potrace.Bitmap(~mask).trace(turdsize=20, alphamax=1.2, opticurve=True, opttolerance=1.0)
    ox, oy = DISC[0], DISC[1]
    f = lambda pt: "%.2f %.2f" % ((pt.x / K - ox) * SCALE + 128, (pt.y / K - oy) * SCALE + 128)
    out = []
    for curve in paths:
        seg = ["M" + f(curve.start_point)]
        for s in curve:
            if s.is_corner:
                seg.append("L" + f(s.c) + " L" + f(s.end_point))
            else:
                seg.append("C" + f(s.c1) + " " + f(s.c2) + " " + f(s.end_point))
        pts = [curve.start_point] + [s.end_point for s in curve]
        w = (max(p.x for p in pts) - min(p.x for p in pts)) / K
        h = (max(p.y for p in pts) - min(p.y for p in pts)) / K
        if min(w, h) < 20 and max(w, h) < 60:
            continue  # a thin thread left by anti-aliasing
        out.append("".join(seg) + "Z")
    return "".join(out)


def mark_body():
    img = np.asarray(Image.open(os.path.join(HERE, "badge-source.png")).convert("RGB")).astype(float)
    u = upsample(fix_labels(classify(img)))
    hk, wk = u.shape
    Y, X = np.mgrid[:hk, :wk].astype(np.float32)
    X = (X + 0.5) / K
    Y = (Y + 0.5) / K
    d = lambda cx, cy: np.hypot(X - cx, Y - cy)
    under = {}
    # Eyes: the crescent is the iris circle less the pupil circle, cut by the eyelid circle.
    for sgn, col in [(-1, IRIS), (1, AMB)]:
        ic = (AX + sgn * IRIS_DX, EYE_Y)
        inside_iris = d(*ic) < IRIS_R
        inside_pupil = d(AX + sgn * PUP_DX, PUP_Y) < PUP_R
        inside_lid = d(AX + sgn * LID_DX, LID_Y) < LID_R
        highlight = d(AX + sgn * HL_DX, HL_Y) < HL_R + 2
        u[(d(*ic) < IRIS_R + 3) & ((u != WHITE) | highlight)] = NAVY
        crescent = inside_iris & ~inside_pupil & inside_lid
        u[(u == col) & ~crescent] = NAVY
        u[crescent] = col
        under[col] = d(*ic) < IRIS_R + 6
    # The nazar bead is drawn as circles on the cobalt.
    nz = d(*NAZAR)
    u[nz < NAZAR_WHITE + 1.5] = COB
    inside = d(DISC[0], DISC[1]) < DISC[2] - 1.5
    bgl, _ = nd.label(u == WHITE)
    background = np.isin(bgl, [bgl[0, 0], bgl[0, -1], bgl[-1, 0], bgl[-1, -1]])
    inside &= ~nd.binary_dilation(background, iterations=2 * K)
    near_cobalt = nd.binary_dilation(u == COB, iterations=3 * K)
    layers = [
        ("white", inside & (u != COB) & (nz > NAZAR_WHITE) & ~((u == NAVY) & near_cobalt)),
        ("pink", u == PINK),
        ("iris", (u == IRIS) | ((u == NAVY) & under[IRIS])),
        ("amber", (u == AMB) | ((u == NAVY) & under[AMB])),
        ("navy", (u == NAVY) & inside),
    ]
    c = COLOURS
    body = ['<circle cx="128" cy="128" r="%.2f" fill="%s"/>' % (128 * 0.992, c["cobalt"])]
    for name, mask in layers:
        body.append('<path fill="%s" fill-rule="evenodd" d="%s"/>' % (c[name], trace(mask)))
    for g in (-1, 1):
        body.append('<circle cx="%.2f" cy="%.2f" r="%.2f" fill="%s"/>' % (
            (AX + g * HL_DX - DISC[0]) * SCALE + 128, (HL_Y - DISC[1]) * SCALE + 128, HL_R * SCALE, c["white"]))
    nx, ny = (NAZAR[0] - DISC[0]) * SCALE + 128, (NAZAR[1] - DISC[1]) * SCALE + 128
    for r, col in [(NAZAR_WHITE, "white"), (NAZAR_SKY, "sky"), (NAZAR_NAVY, "navy")]:
        body.append('<circle cx="%.2f" cy="%.2f" r="%.2f" fill="%s"/>' % (nx, ny, r * SCALE, c[col]))
    return "\n".join(body)


def small_body():
    """The small-size cut, drawn by hand: the head, the two eyes and the bead; no fur, whiskers or tail."""
    c = COLOURS
    head = ("M58 34 L102 72 Q128 64 154 72 L198 34 Q216 70 216 112 "
            "C216 158 178 178 128 178 C78 178 40 158 40 112 Q40 70 58 34Z")
    ears = "M68 58 L94 80 Q80 88 72 98 Q64 78 68 58Z M188 58 L162 80 Q176 88 184 98 Q192 78 188 58Z"
    body = ['<circle cx="128" cy="128" r="128" fill="%s"/>' % c["cobalt"],
            '<path fill="%s" d="%s"/>' % (c["white"], head),
            '<path fill="%s" d="%s"/>' % (c["pink"], ears)]
    for cx, col, look in [(90, "iris", 8), (166, "amber", -8)]:
        body += ['<circle cx="%d" cy="124" r="23" fill="%s"/>' % (cx, c["navy"]),
                 '<circle cx="%d" cy="124" r="18" fill="%s"/>' % (cx, c[col]),
                 '<circle cx="%d" cy="122" r="11" fill="%s"/>' % (cx + look, c["navy"]),
                 '<circle cx="%d" cy="117" r="4.5" fill="%s"/>' % (cx + look + (3 if look > 0 else -3), c["white"])]
    body += ['<path fill="%s" d="M116 150 Q128 146 140 150 L128 162Z"/>' % c["pink"],
             '<circle cx="128" cy="214" r="30" fill="%s"/>' % c["white"],
             '<circle cx="128" cy="214" r="20" fill="%s"/>' % c["sky"],
             '<circle cx="128" cy="214" r="10" fill="%s"/>' % c["navy"]]
    return "\n".join(body)
