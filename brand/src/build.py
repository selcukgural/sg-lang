"""Builds every file of the sg-lang logo and of Pasha's badge.

- The logo is the S-cat (`logo.py`, from `logo-source.png`): a cat whose head and body make an S, with
  one blue and one amber eye. The main mark is one navy S; a second version keeps a sky band under the S
  for large, colourful places.
- The badge is Pasha's face in a cobalt circle with a nazar bead (`badge.py`, from `badge-source.png`),
  the mascot.
- The name is set in Outfit 600 (SIL OFL 1.1, `OFL-Outfit.txt`) as outlines.

Run from the repository root:

    uv venv /tmp/sg-logo && uv pip install --python /tmp/sg-logo/bin/python \\
        pillow==13.0.0 numpy==2.5.5 scipy==1.18.1 scikit-image==0.26.0 potracer==0.0.4 fonttools==4.68.0
    /tmp/sg-logo/bin/python brand/src/build.py

`rsvg-convert` (librsvg) renders the PNG files.
"""
import json
import os
import subprocess
import sys

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import badge  # noqa: E402
import logo   # noqa: E402

BRAND = os.path.dirname(HERE)
LOGO, MASCOT, WEB = (os.path.join(BRAND, d) for d in ("logo", "mascot", "web"))
TAGLINE = "Safe by design. Fast by default."
NAVY, SKY, BLUE, AMBER, WHITE = "#021A50", "#00A4FE", "#00CEFE", "#FEA602", "#FFFFFF"
r2 = lambda v: "%.2f" % v


def write(path, body, vb, title):
    with open(path, "w") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}"><title>{title}</title>\n{body}\n</svg>\n')


def png(svg, out, w, h=None):
    subprocess.run(["rsvg-convert", "-w", str(w), "-h", str(h or w), svg, "-o", out], check=True)


def text_paths(text, wght, tracking):
    font = instantiateVariableFont(TTFont(os.path.join(HERE, "Outfit.ttf")), {"wght": wght})
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    x, out, y0, y1 = 0, [], 0, 0
    for ch in text:
        g = glyphs[cmap[ord(ch)]]
        pen = SVGPathPen(glyphs)
        g.draw(TransformPen(pen, (1, 0, 0, -1, x, 0)))
        bounds = BoundsPen(glyphs)
        g.draw(bounds)
        if bounds.bounds:
            y0, y1 = min(y0, -bounds.bounds[3]), max(y1, -bounds.bounds[1])
        if pen.getCommands():
            out.append('<path d="%s"/>' % pen.getCommands())
        x += g.width + tracking
    return "".join(out), x - tracking, y0, y1, font["OS/2"].sxHeight


def main():
    for d in (LOGO, MASCOT, WEB):
        os.makedirs(d, exist_ok=True)
    m = logo.build()
    eyes = lambda c1, c2: "".join('<circle cx="%.2f" cy="%.2f" r="%.2f" fill="%s"/>' % (x, y, m["eye_r"], c)
                                  for (x, y), c in zip(m["eyes"], (c1, c2)))
    marks = {
        "mark": f'<path fill="{NAVY}" d="{m["whole"]}"/>' + eyes(BLUE, AMBER),
        "mark-dark": f'<path fill="{WHITE}" d="{m["whole"]}"/>' + eyes(BLUE, AMBER),
        "mark-band": f'<path fill="{SKY}" d="{m["band"]}"/><path fill="{NAVY}" d="{m["stroke"]}"/>' + eyes(BLUE, AMBER),
        "mark-band-dark": f'<path fill="{SKY}" d="{m["band"]}"/><path fill="{WHITE}" d="{m["stroke"]}"/>' + eyes(BLUE, AMBER),
        "mark-mono": f'<path fill="{NAVY}" d="{m["whole"]}"/>' + eyes(NAVY, NAVY),
        "mark-mono-white": f'<path fill="{WHITE}" d="{m["whole"]}"/>' + eyes(WHITE, WHITE),
    }
    titles = {"mark": "sg-lang logo", "mark-dark": "sg-lang logo, for dark backgrounds",
              "mark-band": "sg-lang logo, with the band", "mark-band-dark": "sg-lang logo, with the band, for dark backgrounds",
              "mark-mono": "sg-lang logo, one colour", "mark-mono-white": "sg-lang logo, one colour, white"}
    for name, body in marks.items():
        write(os.path.join(LOGO, f"sg-lang-{name}.svg"), body, "0 0 512 512", titles[name])

    # Lockups: the mark cut to its own box, 256 high.
    bx, by, bw, bh = m["box"]
    k = 256 / bh
    placed = lambda body: f'<g transform="scale({k:.5f}) translate({r2(-bx)} {r2(-by)})">{body}</g>'
    wm, w, y0, y1, xh = text_paths("sg-lang", 600, -10)
    sc, gap = 0.40 * 256 / xh, 0.16 * 256
    mw = bw * k
    hw, base = mw + gap + w * sc, 128 + xh * sc / 2 + 0.06 * 256   # the name sits a little low, by the body
    for name, mark, fill, title in [("horizontal", "mark", NAVY, "sg-lang logo"),
                                    ("horizontal-dark", "mark-dark", WHITE, "sg-lang logo, for dark backgrounds"),
                                    ("horizontal-band", "mark-band", NAVY, "sg-lang logo, with the band")]:
        body = f'{placed(marks[mark])}\n<g fill="{fill}" transform="translate({r2(mw + gap)} {r2(base)}) scale({sc:.5f})">{wm}</g>'
        write(os.path.join(LOGO, f"sg-lang-{name}.svg"), body, f"0 0 {r2(hw)} 256", title)
    sc2 = 1.6 * mw / w
    sh, sw = 256 + 0.10 * 256 + (y1 - y0) * sc2, max(mw, w * sc2)
    for name, mark, fill, title in [("stacked", "mark", NAVY, "sg-lang logo, stacked"),
                                    ("stacked-dark", "mark-dark", WHITE, "sg-lang logo, stacked, for dark backgrounds")]:
        body = (f'<g transform="translate({r2((sw - mw) / 2)} 0)">{placed(marks[mark])}</g>\n'
                f'<g fill="{fill}" transform="translate({r2((sw - w * sc2) / 2)} {r2(256 + 0.10 * 256 - y0 * sc2)}) '
                f'scale({sc2:.5f})">{wm}</g>')
        write(os.path.join(LOGO, f"sg-lang-{name}.svg"), body, f"0 0 {r2(sw)} {r2(sh)}", title)
    for name, fill in [("wordmark", NAVY), ("wordmark-dark", WHITE)]:
        write(os.path.join(LOGO, f"sg-lang-{name}.svg"), f'<g fill="{fill}" transform="translate(0 {r2(-y0)})">{wm}</g>',
              f"0 0 {r2(w)} {r2(y1 - y0)}", "sg-lang")

    # Pasha's badge, the mascot.
    write(os.path.join(MASCOT, "pasha-badge.svg"), badge.mark_body(), "0 0 256 256", "Pasha, the mascot of sg-lang")
    write(os.path.join(MASCOT, "pasha-badge-small.svg"), badge.small_body(), "0 0 256 256", "Pasha, small sizes")

    # Web icons: the white mark on a navy tile with round corners; a tile reads on light and dark tabs.
    def tile(scale, radius=112):
        cx, cy = bx + bw / 2, by + bh / 2
        return (f'<rect width="512" height="512" rx="{radius}" fill="{NAVY}"/>'
                f'<g transform="translate(256 256) scale({scale}) translate({r2(-cx)} {r2(-cy)})">{marks["mark-dark"]}</g>')
    write(os.path.join(WEB, "favicon.svg"), tile(0.86), "0 0 512 512", "sg-lang")
    for z in (16, 32, 48):
        png(os.path.join(WEB, "favicon.svg"), os.path.join(WEB, f"favicon-{z}.png"), z)
    ims = [Image.open(os.path.join(WEB, f"favicon-{z}.png")).convert("RGBA") for z in (16, 32, 48)]
    ims[2].save(os.path.join(WEB, "favicon.ico"), sizes=[(16, 16), (32, 32), (48, 48)], append_images=ims[:2])
    write(os.path.join(WEB, "icon.svg"), tile(0.78), "0 0 512 512", "sg-lang")
    for z in (192, 512):
        png(os.path.join(WEB, "icon.svg"), os.path.join(WEB, f"icon-{z}.png"), z)
    tmp = os.path.join(WEB, "_tile.svg")
    for scale, radius, out, z in [(0.74, 0, "apple-touch-icon.png", 180), (0.62, 0, "maskable-512.png", 512)]:
        write(tmp, tile(scale, radius), "0 0 512 512", "sg-lang")
        png(tmp, os.path.join(WEB, out), z)
    os.remove(tmp)
    with open(os.path.join(WEB, "site.webmanifest"), "w") as f:
        json.dump({"name": "sg-lang", "short_name": "sg-lang", "icons": [
            {"src": "/icon-192.png", "sizes": "192x192", "type": "image/png"},
            {"src": "/icon-512.png", "sizes": "512x512", "type": "image/png"},
            {"src": "/maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable"}],
            "theme_color": NAVY, "background_color": NAVY, "display": "standalone"}, f, indent=2)
        f.write("\n")

    # The animated mark of the front page (on its dark hero): the stroke draws itself along its centre line,
    # the band slides in, the eyes open, and then blink now and then. CSS only; still for reduced motion.
    (e1x, e1y), (e2x, e2y) = m["eyes"]
    er = m["eye_r"]
    hero = f"""<style>
.draw {{ stroke-dasharray: 1; stroke-dashoffset: 1; animation: draw 1.6s cubic-bezier(.6,0,.3,1) .2s forwards; }}
.band {{ opacity: 0; transform: translate(-24px, 10px); animation: band .6s ease-out 1.5s forwards; }}
.eye {{ transform-box: fill-box; transform-origin: center; transform: scale(0);
        animation: open .35s ease-out 1.9s forwards, blink 5s ease-in-out 3.5s infinite; }}
@keyframes draw {{ to {{ stroke-dashoffset: 0; }} }}
@keyframes band {{ to {{ opacity: 1; transform: none; }} }}
@keyframes open {{ 0% {{ transform: scale(0); }} 70% {{ transform: scale(1.15); }} 100% {{ transform: scale(1); }} }}
@keyframes blink {{ 0%, 90%, 100% {{ transform: scale(1); }} 94% {{ transform: scale(1, .1); }} }}
@media (prefers-reduced-motion: reduce) {{
  .draw, .band, .eye {{ animation: none; stroke-dashoffset: 0; opacity: 1; transform: none; }}
}}
</style>
<defs><mask id="reveal" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512">
<path class="draw" d="{m["line"]}" pathLength="1" fill="none" stroke="#FFFFFF" stroke-width="{m["line_width"] + 8:.1f}" stroke-linecap="butt" stroke-linejoin="round"/>
</mask></defs>
<path class="band" fill="{SKY}" d="{m["band"]}"/>
<path fill="{WHITE}" mask="url(#reveal)" d="{m["stroke"]}"/>
<circle class="eye" cx="{e1x:.2f}" cy="{e1y:.2f}" r="{er:.2f}" fill="{BLUE}"/>
<circle class="eye" cx="{e2x:.2f}" cy="{e2y:.2f}" r="{er:.2f}" fill="{AMBER}"/>"""
    write(os.path.join(WEB, "hero.svg"), hero, "0 0 512 512", "sg-lang")

    # Social card, 1200 x 630: the lockup with the band, and the tagline.
    tg, tw, ty0, _, _ = text_paths(TAGLINE, 400, 0)
    lh = 270
    ls = lh / 256
    band_body = (f'{placed(marks["mark-band"])}\n'
                 f'<g fill="{NAVY}" transform="translate({r2(mw + gap)} {r2(base)}) scale({sc:.5f})">{wm}</g>')
    tsc = 860 / tw
    body = (f'<rect width="1200" height="630" fill="#F3F7FF"/>\n'
            f'<g transform="translate({(1200 - hw * ls) / 2:.2f} 110) scale({ls:.5f})">{band_body}</g>\n'
            f'<g fill="{NAVY}" transform="translate({(1200 - tw * tsc) / 2:.2f} {460 - ty0 * tsc:.2f}) scale({tsc:.5f})">{tg}</g>')
    write(os.path.join(WEB, "social-card.svg"), body, "0 0 1200 630", "sg-lang: " + TAGLINE)
    png(os.path.join(WEB, "social-card.svg"), os.path.join(WEB, "social-card.png"), 1200, 630)


if __name__ == "__main__":
    main()
