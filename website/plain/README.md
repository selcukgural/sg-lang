# The plain site

The next web site of SG: plain pages made from Markdown by one script (`build.mjs`), with one stylesheet, two small
scripts and no framework. It reads the same pages as the Docusaurus site beside it (`../docs/`), so the two do not
drift apart while both are kept; the Docusaurus site still publishes sg-lang.org until this one replaces it.

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
  shows for it, on the site under Manual and in `build/help/`. `npm run check` reads them with mandoc; the text
  the site shows is the text mandoc prints.
- **Search.** `build/search-index.js` lists every name, page and heading; the search runs in the page, with
  nothing sent anywhere.
- **Headers.** `assets/_headers` allows the site only its own files: no script or style is written into a page,
  and `build.mjs` fails if one is.
- **Logo.** The wildcat (`../../brand/wildcat/`), with its ground taken out: a light and a dark version, and the
  stylesheet shows the one that fits the theme. `make.py` there makes them in `assets/`.

Every page reads and works without its script; the script adds the search, the line shown over a name in a
program, a button to copy each program and command, the filter of the index, and the button for light or dark.
