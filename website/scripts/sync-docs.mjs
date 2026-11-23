// Copies the pages the site takes from the repository's own documents, so they stay as each slice
// leaves them (the rule of keeping everything in step): `design/06-rust-to-sg.md` becomes
// `docs/learn/for-rust.md`, and the pages of `benchmarks/` become the section Benchmarks. A link to
// another page copied here stays a link on the site; any other link into the repository is made a link
// to it on GitHub. Run before `start` and `build` (package.json).
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, normalize, posix, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(site, '..');
const github = 'https://github.com/selcukgural/sg-lang/blob/main';

const benchmark = (from, to, title, description, position) => ({
  from: `benchmarks/${from}`,
  to: `docs/benchmarks/${to}`,
  title,
  description,
  position,
});

const pages = [
  {
    from: 'design/06-rust-to-sg.md',
    to: 'docs/learn/for-rust.md',
    title: 'SG for Rust programmers',
    description: 'What a Rust programmer writes, and what SG writes for it.',
  },
  benchmark('README.md', 'index.md', 'Benchmarks', 'How fast SG is against C, Rust and Zig, and how it is measured.', 0),
  benchmark('against-c.md', 'against-c.md', 'SG against C', 'Sixty programs written in SG and in C by hand, built by the same C compiler.', 1),
  benchmark('against-c-windows.md', 'against-c-windows.md', 'SG against C on Windows', 'The same programs on Windows 11 on Arm.', 2),
  benchmark('programs.md', 'programs.md', 'Programs against Rust and Zig', 'wc, grep and jq written in SG, Rust and Zig.', 3),
  benchmark('library.md', 'library.md', 'The library against Rust and Zig', 'The modules of std against Rust crates and the Zig library.', 4),
  benchmark('server.md', 'server.md', 'Servers against Rust, Zig and C', 'A store that speaks Redis\'s protocol and an HTTP server, on Linux, under load and idle, and where they break.', 5),
  benchmark('robustness.md', 'robustness.md', 'What the servers stand', 'The servers against clients that reset, close half, trickle, do not read, send garbage, and SIGTERM.', 6),
  benchmark('slower-than-c.md', 'slower-than-c.md', 'Slower than C', 'Every place where SG was found slower, and what would fix it.', 7),
  benchmark('method.md', 'method.md', 'How it is measured', 'The rules of fairness, the machines, and how to run it all again.', 8),
];

// The page of the site each copied document becomes, by its path in the repository.
const copied = new Map(pages.map((page) => [page.from, page.to]));

for (const page of pages) {
  const folder = dirname(page.from);
  let text = readFileSync(join(repo, page.from), 'utf8');
  // `[text](04-decisions.md)` and `[text](../interpreter/README.md)`: to the page on the site when it is
  // copied too, else to the file on GitHub. Only outside code: `byte[16](255)` is not a link.
  text = text
    .split(/(`[^`\n]*`)/)
    .map((part) =>
      part.startsWith('`')
        ? part
        : part.replace(/\]\((?!https?:|#|mailto:)([^)\s]+)\)/g, (_, target) => {
            const [path, anchor] = target.split('#');
            const inRepo = relative(repo, normalize(join(repo, folder, path))).split('\\').join('/');
            const hash = anchor ? `#${anchor}` : '';
            if (copied.has(inRepo)) {
              return `](${posix.relative(posix.dirname(page.to), copied.get(inRepo))}${hash})`;
            }
            return `](${github}/${inRepo}${hash})`;
          }),
    )
    .join('');
  const front = [
    '---',
    `title: ${page.title}`,
    `description: ${page.description}`,
    ...(page.position === undefined ? [] : [`sidebar_position: ${page.position}`]),
    `custom_edit_url: ${github.replace('/blob/', '/edit/')}/${page.from}`,
    '---',
    '',
  ].join('\n');
  // A table cell may hold `<` and `{`, which MDX would read as JSX: the page is read as plain Markdown.
  mkdirSync(dirname(join(site, page.to)), { recursive: true });
  writeFileSync(join(site, page.to), `${front}\n<!-- Made from ${page.from} by scripts/sync-docs.mjs: change that file, not this one. -->\n\n${text}`);
}
