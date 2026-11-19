# The web site of SG

The source of [sg-lang.org](https://sg-lang.org) (choices 1383-1390 in `interpreter/README.md`): a static site
built with Docusaurus, published by Cloudflare Pages from this folder.

    npm ci --ignore-scripts      # Node 22 (.nvmrc); the packages are pinned in package-lock.json
    npm start                    # a server that rebuilds as the pages change
    npm run build                # the site, in build/; then checks its scripts against the CSP
    npm run check                # runs every program of SG on the pages (needs a built `sg`)

- `docs/` holds the pages: `learn/` (install, the tour), `reference/` and `library/` (pointers into the
  repository until the preview) and `status.md`. `learn/for-rust.md` is made from
  `design/06-rust-to-sg.md` by `scripts/sync-docs.mjs` before every start and build: change that file.
- Every ```` ```sg ```` block on a page is a whole program, and `npm run check` runs it with `sg`: the
  ```` ```text ```` block after it is what it prints. ```` ```sg error ```` must be refused, and ```` ```sg test
  ```` runs its tests. A slice that changes what a page shows changes the page in the same commit.
- The two programs of the front page are checked by `npm run check` too, the second being a mistake that must
  be refused with the message the page shows.
- `src/theme/prism-include-languages.js` colours SG with the same words as the VS Code grammar.
- `static/_headers` holds the headers Cloudflare gives each page: a CSP that allows no inline script but the
  one Docusaurus writes for the colour mode, by its hash (`scripts/check-csp.mjs` fails the build if another
  appears), HSTS and the others. There is no analytics and no search service: search is built into the site.
- `overrides` in `package.json` lift packages that Docusaurus pulls in to the versions their security fixes came
  in (`serialize-javascript`, `tinypool`, `uuid`); `editors/vscode` does the same for `minimatch`. Take one out when
  the package that needs it moves past it.
- Publishing: Cloudflare Pages builds `main` from this folder (`npm ci --ignore-scripts && npm run build`,
  output `build`); a pull request gets a preview. No secret is kept in the repository.
