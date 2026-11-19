"""The S-cat, the logo of sg-lang, redrawn from `logo-source.png`.

The source is a raster sheet; its large drawing (top left) is measured, not copied:

1. The navy stroke is thinned to its centre line (a skeleton), with its width at every point.
2. The centre line and the widths are smoothed, the head is made a mirror image about its axis, and both
   ends are drawn out into points.
3. The stroke is drawn again from that line as a row of discs, smoothed, and traced (potrace).
4. The sky band under the S is taken from the source, smoothed, with its thin end beside the navy taken off.
   For the main logo (no band) the band joins the navy into one S, and the corner where they met inside
   the lower bowl is rounded.

`build()` returns the path data in a 512 x 512 box, the mark centred and 448 high, and the centre line of
the stroke, which the animated mark on the front page draws along.
"""
import os
from collections import deque

import numpy as np
import potrace
from PIL import Image, ImageDraw
from scipy import ndimage as nd
from skimage.morphology import skeletonize

HERE = os.path.dirname(os.path.abspath(__file__))
K = 4                                # drawing scale
X0, Y0, WW, HH = 40, 80, 720, 840    # the part of the source that holds the large drawing
EYE_Y, EYE_DX, EYE_R = 405.8, 90.25, 42.0   # measured on the source: centres 331.9 and 512.4, 84 px wide


def source():
    img = np.asarray(Image.open(os.path.join(HERE, "logo-source.png")).convert("RGB")).astype(int)[:900, :760]
    r, g, b = img[..., 0], img[..., 1], img[..., 2]
    navy = nd.binary_opening((r < 60) & (g < 60) & (b > 50) & (b < 140), iterations=2)
    sky = (r < 60) & (g > 120) & (b > 200)
    biggest = lambda m: (lambda lb: lb == np.bincount(lb.ravel())[1:].argmax() + 1)(nd.label(m)[0])
    return biggest(navy), biggest(sky)


def centre_line(navy):
    """The skeleton of the stroke as one ordered line, with the stroke's width at each point."""
    dt = nd.distance_transform_edt(navy)
    pts = set(zip(*np.nonzero(skeletonize(navy))))

    def bfs(start):
        prev, queue, last = {start: None}, deque([start]), start
        while queue:
            p = last = queue.popleft()
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    q = (p[0] + dy, p[1] + dx)
                    if q in pts and q not in prev:
                        prev[q] = p
                        queue.append(q)
        return last, prev

    a, _ = bfs(min(pts))
    b, prev = bfs(a)
    line, p = [], b
    while p is not None:
        line.append(p)
        p = prev[p]
    P = np.array([(x, y) for y, x in line], float)
    W = np.array([2 * dt[y, x] for y, x in line])
    if P[0, 1] > P[-1, 1]:          # start at the cheek, the top end of the S
        P, W = P[::-1], W[::-1]
    return P, W


def resample(P, W, step=1.0):
    s = np.r_[0, np.cumsum(np.hypot(*np.diff(P, axis=0).T))]
    t = np.arange(0, s[-1], step)
    return np.c_[np.interp(t, s, P[:, 0]), np.interp(t, s, P[:, 1])], np.interp(t, s, W)


def smooth(P, sigma):
    return np.c_[nd.gaussian_filter1d(P[:, 0], sigma, mode="nearest"), nd.gaussian_filter1d(P[:, 1], sigma, mode="nearest")]


def symmetric_head(P, W):
    head = np.nonzero(P[:, 1] < 300)[0]
    mid = head[np.argmin(np.abs(P[head, 0] - np.median(P[head, 0])))]
    right = head[head < mid]
    left = head[head > mid]
    rt = right[np.argmin(P[right, 1])]          # ear tips: the highest points either side
    lt = left[np.argmin(P[left, 1])]
    axis = (P[rt, 0] + P[lt, 0]) / 2
    span = np.arange(rt, lt)
    c = span[np.argmin(np.abs(P[span, 0] - axis))]   # the middle of the crown
    yc = 345
    i_r = np.nonzero((np.arange(len(P)) < rt) & (P[:, 1] > yc))[0].max()
    i_l = lt + np.nonzero(P[lt:, 1] > yc)[0][0]
    half = P[c:i_l + 1]
    mirrored = np.c_[2 * axis - half[::-1, 0], half[::-1, 1]]
    P2 = np.vstack([P[:i_r], mirrored, P[c + 1:]])
    W2 = np.r_[W[:i_r], W[c:i_l + 1][::-1], W[c + 1:]]
    return smooth(P2, 3), nd.gaussian_filter1d(W2, 6, mode="nearest"), axis


def point_ends(P, W):
    def extend(P, W):
        d = P[-1] - P[-8]
        d /= np.hypot(*d)
        length = W[-1] * 0.9
        t = np.arange(1, length)[:, None]
        return np.vstack([P, P[-1] + t * d]), np.r_[W, W[-1] * (1 - t[:, 0] / length) ** 0.7]
    P, W = extend(P[::-1], W[::-1])
    return extend(P[::-1], W[::-1])


def discs(P, W):
    img = Image.new("L", (WW * K, HH * K), 0)
    draw = ImageDraw.Draw(img)
    for (x, y), w in zip(P, W):
        r, cx, cy = w / 2 * K, (x - X0) * K, (y - Y0) * K
        if r > 0.5:
            draw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=255)
    return np.asarray(img) > 127


def blur(m, sigma):
    return nd.gaussian_filter(m.astype(np.float32), sigma) > 0.5


def opening(m, r):
    return nd.distance_transform_edt(~(nd.distance_transform_edt(m) > r)) <= r


def closing(m, r):
    return nd.distance_transform_edt(nd.distance_transform_edt(~m) <= r) > r


def trace(mask, scale, ox, oy):
    paths = potrace.Bitmap(~mask).trace(turdsize=10, alphamax=1.15, opticurve=True, opttolerance=0.6)
    f = lambda pt: "%.2f %.2f" % ((pt.x / K + X0 - ox) * scale + 256, (pt.y / K + Y0 - oy) * scale + 256)
    out = []
    for curve in paths:
        seg = ["M" + f(curve.start_point)]
        for s in curve:
            seg.append(("L" + f(s.c) + " L" + f(s.end_point)) if s.is_corner
                       else ("C" + f(s.c1) + " " + f(s.c2) + " " + f(s.end_point)))
        out.append("".join(seg) + "Z")
    return "".join(out)


def build():
    navy_src, sky_src = source()
    P, W = centre_line(navy_src)
    P, W = resample(P, W)
    P, W = smooth(P, 8), nd.gaussian_filter1d(W, 26, mode="nearest")
    P, W, axis = symmetric_head(P, W)
    P, W = point_ends(P, W)
    navy = blur(discs(P, W), 2 * K)

    sky_big = nd.zoom(nd.gaussian_filter(sky_src.astype(np.float32), 3.0), K, order=1) > 0.5
    sky = sky_big[Y0 * K:Y0 * K + HH * K, X0 * K:X0 * K + WW * K]
    sky = np.pad(sky, ((0, HH * K - sky.shape[0]), (0, WW * K - sky.shape[1])))
    Yg, Xg = np.mgrid[:HH * K, :WW * K]
    Xs, Ys = Xg / K + X0, Yg / K + Y0
    sky = np.where(Xs > 440, opening(sky, 9 * K), sky)   # the thin end beside the navy

    whole = blur(closing(navy | sky, 16 * K), 3 * K)
    whole = blur(whole, 2 * K)
    bowl = (Xs > 380) & (Xs < 640) & (Ys > 590) & (Ys < 780)
    whole = np.where(bowl, ~opening(~whole, 48 * K), whole)

    ys, xs = np.nonzero(navy | sky)
    x0, x1, y0, y1 = xs.min() / K + X0, xs.max() / K + X0, ys.min() / K + Y0, ys.max() / K + Y0
    scale = 448 / max(x1 - x0, y1 - y0)
    ox, oy = (x0 + x1) / 2, (y0 + y1) / 2
    eyes = [((axis + s * EYE_DX - ox) * scale + 256, (EYE_Y - oy) * scale + 256) for s in (-1, 1)]
    # The centre line, from the cheek to the bottom tip, for drawing the stroke in an animation.
    pts = P[::6].tolist() + [P[-1].tolist()]
    line = "M" + " L".join("%.1f %.1f" % ((x - ox) * scale + 256, (y - oy) * scale + 256) for x, y in pts)
    return {
        "line": line,
        "line_width": float(W.max() * scale),
        "stroke": trace(navy, scale, ox, oy),
        "band": trace(sky & ~navy, scale, ox, oy),
        "whole": trace(whole, scale, ox, oy),
        "eyes": eyes,
        "eye_r": EYE_R * scale,
        "box": (256 - (x1 - x0) * scale / 2, 256 - (y1 - y0) * scale / 2, (x1 - x0) * scale, (y1 - y0) * scale),
    }
