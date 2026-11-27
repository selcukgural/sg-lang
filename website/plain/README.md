# The plain site

The web site of SG, [sg-lang.org](https://sg-lang.org): plain pages made from Markdown by one script (`build.mjs`),
with one stylesheet, two small scripts and no framework. It reads the same pages as the Docusaurus site beside it
(`../docs/`), which published sg-lang.org until 27 November 2026 and is kept, not published.

    npm ci --ignore-scripts      # Node 22; one package, marked, pinned in package-lock.json
    npm run build                # the site, in build/ (runs ../scripts/sync-docs.mjs first)
    npm run check                # runs every program of the word pages and the front page (needs a built `sg`),
                                 # and reads the manual pages with mandoc, where it is installed
    npm run serve                # build/ at http://127.0.0.1:8044

What it makes, and from what:

- **Pages.** Every Markdown page of `../docs/` becomes a page, with the pages of its part (Learn, Library,
  Benchmarks, About) beside it and its headings on the right. A link to another `.md` page becomes a link to its
  page; `build.mjs` fails on a link to a page, or to a place on a page, that is not there.
- **Words.** `../words/` holds one Markdown file for each word that has a page of its own (`read.md`,
  `atomic.md`, `list.md`): its forms, what it means, examples from the simplest to the hardest, and what the
  compiler refuses. `../words/index.txt` gives the line of every other name of the index. The types and modules
  of the library come from the headings and pages of `../docs/library/`.
- **Names in programs.** In every program on every page, each word of the language and each type of the library
  is a link to its page (or to its line of the index), with what it is in one line; so is a name written alone
  in `code` in the text.
- **Manual pages.** Each word's file also becomes its manual page, `build/man/man7/sg-read.7`, and the text `man`
  shows for it, on the site under Manual. `npm run check` reads them with mandoc; the text the site shows is the
  text mandoc prints.
- **`sg help`** (choice 2082). The same text, without man's top and foot lines, is what `sg help read` prints:
  the build writes it into `../../interpreter/src/help/` (`read.txt`, and `names.tsv`, a line for every name of
  the index), which `sg` carries in itself. Commit those files with the change of a word; `cargo test` fails
  when a word has no page there, or its first line differs from its Markdown.
- **Search.** `build/search-index.js` lists every name, page and heading; the search runs in the page, with
  nothing sent anywhere.
- **Headers.** `assets/_headers` allows the site only its own files: no script or style is written into a page,
  and `build.mjs` fails if one is.
- **Old addresses.** `assets/_redirects` sends the addresses of the Docusaurus site (`/docs/learn/tour`) to the
  same pages here (`/learn/tour`). An address with no page gets `404.html`, with status 404; its links start at
  the root, since it is served at any depth.
- **Logo.** The wildcat (`../../brand/wildcat/`), with its ground taken out: a light and a dark version, and the
  stylesheet shows the one that fits the theme. `make.py` there makes them in `assets/`.

Every page reads and works without its script; the script adds the search, the line shown over a name in a
program, a button to copy each program and command, the filter of the index, and the button for light or dark.

## Publishing

Cloudflare Pages builds `main` (project `sg-lang`): root directory `website/plain`, build command
`npm ci --ignore-scripts && npm run build`, output `build`, `NODE_VERSION=22`; a pull request gets a preview. No
secret is kept in the repository. `sg-lang.org` and `www.sg-lang.org` are the project's own domains.
`man.sg-lang.org` is a redirect rule of the zone, not a page: every address on it goes to the same path under
`sg-lang.org/manual/` (`man.sg-lang.org/sg-read.7.html` → `sg-lang.org/manual/sg-read.7.html`), with status 302,
so that it can still become a site of its own.
