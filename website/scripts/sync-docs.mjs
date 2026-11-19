// Copies the pages the site takes from the repository's own documents, so they stay as each slice
// leaves them (the rule of keeping everything in step): `design/06-rust-to-sg.md` becomes
// `docs/learn/for-rust.md`. Links that point into the repository are made links to it on GitHub.
// Run before `start` and `build` (package.json).
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, normalize, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(site, '..');
const github = 'https://github.com/selcukgural/sg-lang/blob/main';

const pages = [
  {
    from: 'design/06-rust-to-sg.md',
    to: 'docs/learn/for-rust.md',
    front: '---\ntitle: SG for Rust programmers\ndescription: What a Rust programmer writes, and what SG writes for it.\n---\n\n',
  },
];

for (const page of pages) {
  const folder = dirname(page.from);
  let text = readFileSync(join(repo, page.from), 'utf8');
  // `[text](04-decisions.md)` and `[text](../interpreter/README.md)`: to the file on GitHub.
  // Only outside code: `byte[16](255)` is not a link.
  text = text
    .split(/(`[^`\n]*`)/)
    .map((part) =>
      part.startsWith('`')
        ? part
        : part.replace(/\]\((?!https?:|#|mailto:)([^)\s]+)\)/g, (_, target) => {
            const [path, anchor] = target.split('#');
            const inRepo = relative(repo, normalize(join(repo, folder, path))).split('\\').join('/');
            return `](${github}/${inRepo}${anchor ? `#${anchor}` : ''})`;
          }),
    )
    .join('');
  // A table cell may hold `<` and `{`, which MDX would read as JSX: the page is read as plain Markdown.
  writeFileSync(join(site, page.to), `${page.front}<!-- Made from ${page.from} by scripts/sync-docs.mjs: change that file, not this one. -->\n\n${text}`);
}
