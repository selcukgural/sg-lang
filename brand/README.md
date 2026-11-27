# The sg-lang logo

The logo is the **S-cat**: a cat whose head and body make an S, with one blue and one amber eye, as Pasha,
the cat of SG, has. The name is written **sg-lang** in the logo, the repository and the domain, and the
language is called **SG** in text.

Tagline: **Small enough to know by heart. Safe and fast by construction.** (choice 2080). The social card
still shows an earlier one, **Safe by design. Fast by default.**, drawn as outlines by `src/build.py`; it is
drawn again with the next logo.

## The next logo: the wildcat

**The wildcat** (`wildcat/source.png`, 1536 x 1024) is the logo of the plain site (`website/plain/`, choice
2081): the letters SG in silver, a wildcat's head over the G, and a cyan ring of light, on a ground of `#00040c`.
It is a picture with a glow, not a drawing, so it is shown on a tile of its own ground in both themes;
`wildcat/make.py` cuts and sizes the files the site uses (`logo.webp`, `logo-small.webp`, `favicon.png`,
`apple-touch-icon.png` in `website/plain/assets/`). It does not yet read at 16 pixels, and it has no one-colour
or vector form; the S-cat below stays the logo of the README, the current site and the social card until those
are made.

## Files

| File | Use |
|---|---|
| `logo/sg-lang-mark.svg`, `-dark.svg` | The mark: a navy S, or a white one for dark backgrounds |
| `logo/sg-lang-mark-band.svg`, `-band-dark.svg` | The mark with a sky band under the S, for large, colourful places (the front page, the social card) |
| `logo/sg-lang-mark-mono.svg`, `-mono-white.svg` | One colour, for print, engraving and embroidery |
| `logo/sg-lang-horizontal.svg`, `-dark.svg`, `-band.svg` | The mark with the name beside it |
| `logo/sg-lang-stacked.svg`, `-dark.svg` | The name under the mark |
| `logo/sg-lang-wordmark.svg`, `-dark.svg` | The name alone |
| `web/favicon.svg`, `favicon.ico`, `favicon-16/32/48.png` | Browser tabs: the white mark on a navy tile, which reads on light and dark tabs |
| `web/icon.svg`, `icon-192.png`, `icon-512.png` | App icons, on the tile |
| `web/apple-touch-icon.png` | iOS home screen, 180 px |
| `web/maskable-512.png` | Android, with room for the round crop |
| `web/site.webmanifest` | The web app manifest |
| `web/hero.svg` | The animated mark of the front page: the S draws itself, the band slides in, the eyes open and blink (CSS only; still when the reader asks for reduced motion) |
| `web/social-card.svg`, `.png` | The preview shown when a link is shared, 1200 x 630 |
| `mascot/pasha-badge.svg`, `pasha-badge-small.svg` | Pasha's badge, the mascot: Pasha's face in a cobalt circle with a nazar bead; the small cut is for 16 to 48 px |
| `pasha.png` | The picture of Pasha |

The site uses copies of the web files and of the mark (`website/static/`, where `web/hero.svg` is `img/logo-hero.svg`);
copy them again after a rebuild.

## Colours

| Name | HEX | Where |
|---|---|---|
| Navy | `#021A50` | The S, the name on light backgrounds, the tile; the theme colour |
| Sky | `#00A4FE` | The band under the S |
| Blue eye | `#00CEFE` | The eye on the left (as you look at it) |
| Amber eye | `#FEA602` | The eye on the right |
| White | `#FFFFFF` | The S and the name on dark backgrounds |

Pasha's badge has its own colours (cobalt `#0028A6`, navy `#011E65`, pink `#FD6E7A`, sky `#43D6FD`); they are
in `src/badge.py`.

## Use

- Keep clear space around the mark of at least the height of an ear on every side.
- Use the navy mark on light backgrounds and the `-dark` files on dark ones.
- Below 32 px, use the mark without the name, on the tile where the background is not known.
- Do not recolour, stretch, rotate or outline the logo, swap the colours of the eyes, or set the name in
  another font.

## How it is made

Both drawings began as raster images (`src/logo-source.png`, `src/badge-source.png`). `src/build.py` redraws
them as vector art and exports every file above:

- **The S-cat** (`src/logo.py`): the stroke is thinned to its centre line with its width at every point; the
  line and the widths are smoothed, the head is made a mirror image about its axis, and the stroke is drawn
  again and traced. The band is taken from the source and smoothed; in the main mark it joins the stroke
  into one S.
- **The badge** (`src/badge.py`): each pixel is given one of the badge's colours, a few faults of the raster
  are fixed, what is round is made exactly round, and each colour is traced.

The steps and the versions of the tools it needs are at the top of `src/build.py`.

## License

The logo, the badge and the picture of Pasha are under the license of the project (MIT OR Apache-2.0). The
letters of the name are outlines of [Outfit](https://github.com/Outfitio/Outfit-Fonts). The font in `src/`
is under the SIL Open Font License 1.1 (`src/OFL-Outfit.txt`).
