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
  page; `build.mjs` fails on a link to a page, or to a place on a page, that is not there. Every part has its
  place in the menu at the top, About too; the foot of each page, centred, has the license and four links.
- **Words.** `../words/` holds one Markdown file for each word that has a page of its own (`read.md`,
  `atomic.md`, `list.md`): its forms, what it means, examples from the simplest to the hardest, and what the
  compiler refuses. `../words/index.txt` gives the line of every other name of the index. The types and modules
  of the library come from the headings and pages of `../docs/library/`.
- **Programs on a word's page.** A ```` ```sg ```` block is a whole program, and the ```` ```text ```` after it
  what it prints; ```` ```sg error ```` must be refused and ```` ```sg panic ```` must stop, the text after
  either being how the message starts; ```` ```sg test ```` runs its tests. A program of several files, or one
  that is to fail, is written as its files and a terminal: ```` ```sg file title="bank/main.sg" ```` (or
  ```` ```text title="shop/sg.project" ````) is written into a folder of the page under its title, and a
  ```` ```text ```` block whose lines start with `$ ` is a terminal: each command runs in that folder, starting
  at its root, and must write the lines under it (out and err together). `npm run check` runs them all, so
  no example on a word's page is left unchecked.
- **Names in programs.** In every program on every page, each word of the language and each type of the library
  is a link to its page (or to its line of the index), with what it is in one line; so is a name written alone
  in `code` in the text, and a command written alone (`sg build`) is a link to its page.
- **Manual pages.** Each word's file also becomes its manual page, `build/man/man7/sg-read.7`, and the text `man`
  shows for it, on the site under Manual. A command's file (`sg-build.md`, `kind: command`) becomes a page of
  section 1, `build/man/man1/sg-build.1` (choice 2087). `npm run check` reads them with mandoc's lint, and compares the text the site
  shows (and `sg help` prints) with the text mandoc lays out, line for line.
- **`sg help`** (choice 2082). The same text, without man's top and foot lines, and with the pages under SEE
  ALSO named `sg help ref` (`sg help sg-build` for a command) and wrapped again to 78 columns, is what
  `sg help read` prints, and `sg build --help` for a command (choice 2088):
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
  stylesheet shows the one that fits the theme. `make.py` there makes them in `assets/`, and the card shown when a
  link is shared (`social-card.png`, 1200 x 630), which every page names in its `og:image`. The repository's
  README shows the same two logos, and GitHub's preview of the repository is the same card (set by hand in the
  repository's settings, under Social preview).

Every page reads and works without its script; the script adds the search, the line shown over a name in a
program, a button to copy each program and command, the filter of the index, and the button for light or dark.

## Publishing

Cloudflare Pages builds `main` (project `sg-lang`): root directory `website/plain`, build command
`npm ci --ignore-scripts && npm run build`, output `build`, `NODE_VERSION=22`; a pull request gets a preview. No
secret is kept in the repository. `sg-lang.org` and `www.sg-lang.org` are the project's own domains.
`man.sg-lang.org` is a redirect rule of the zone, not a page: every address on it goes to the same path under
`sg-lang.org/manual/` (`man.sg-lang.org/sg-read.7.html` → `sg-lang.org/manual/sg-read.7.html`), with status 302,
so that it can still become a site of its own.
