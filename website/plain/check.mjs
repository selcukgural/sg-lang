// Runs every program of SG on the pages this site adds to those of `../docs/` (which
// `../scripts/check-examples.mjs` runs): the pages of the words (`../words/*.md`) and the front page
// (`home.html`). The rules are the same: a block ```sg must run and end with code 0, and the ```text after it
// is what it prints; ```sg error must be refused by the checker and ```sg panic must stop when it runs, the
// ```text after either being how the message starts. On the front page the first program must print what is
// shown, and a program in <pre class="sg error"> must be refused with it.
// Then every manual page in build/man is read by mandoc's lint, where mandoc is installed.
// `SG` names the sg program (../../interpreter/target/release/sg by default).
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const plain = dirname(fileURLToPath(import.meta.url));
const site = join(plain, '..');
const sg = process.env.SG || join(site, '..', 'interpreter', 'target', 'release', 'sg');
const work = mkdtempSync(join(tmpdir(), 'sg-plain-'));

function run(args) {
  try {
    return { code: 0, text: execFileSync(sg, args, { cwd: work, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 30000 }) };
  } catch (error) {
    return { code: error.status ?? 1, text: `${error.stdout ?? ''}${error.stderr ?? ''}` };
  }
}

let failures = 0;
let programs = 0;
function check(name, file, body, kind, shown) {
  programs += 1;
  writeFileSync(join(work, file), body);
  const result = run(kind === 'test' ? ['test', file] : [file]);
  const fail = (why) => {
    failures += 1;
    console.error(`${name}: ${why}`);
  };
  if (kind === 'error' || kind === 'panic') {
    const checked = run(['emit-c', file]).code === 0;
    if (checked !== (kind === 'panic')) return fail(`marked \`${kind}\`, but the checker ${checked ? 'takes' : 'refuses'} it`);
    if (result.code === 0) return fail(`should be refused, and ran:\n${result.text}`);
    if (shown !== null && !result.text.startsWith(shown)) return fail(`the message differs.\nshown:\n${shown}\ngiven:\n${result.text}`);
  } else if (result.code !== 0) fail(`ended with code ${result.code}:\n${result.text}`);
  else if (shown !== null && result.text.trimEnd() !== shown) fail(`prints otherwise.\nshown:\n${shown}\ngiven:\n${result.text}`);
}

// The pages of the words.
const words = join(site, 'words');
for (const file of readdirSync(words).filter((f) => f.endsWith('.md'))) {
  const text = readFileSync(join(words, file), 'utf8');
  const blocks = [...text.matchAll(/^```(\w+)([^\n]*)\n([\s\S]*?)^```$/gm)].map((m) => ({ lang: m[1], meta: m[2].trim(), body: m[3] }));
  let at = 0;
  blocks.forEach((block, i) => {
    if (block.lang !== 'sg') return;
    at += 1;
    const kind = block.meta.split(/\s+/)[0];
    if (kind === 'nocheck') return;
    const next = blocks[i + 1];
    check(`words/${file}, program ${at}`, 'example.sg', block.body, kind, next && next.lang === 'text' ? next.body.trimEnd() : null);
  });
}

// The front page: programs in <pre class="sg">, each with the <pre class="out"> after it.
const home = readFileSync(join(plain, 'home.html'), 'utf8');
const decode = (s) => s.replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&');
const parts = [...home.matchAll(/<pre class="(sg(?: error)?|out)"[^>]*>([\s\S]*?)<\/pre>/g)].map((m) => ({ cls: m[1], body: decode(m[2].replace(/^<code>|<\/code>$/g, '')).replace(/^\n/, '') }));
let homeAt = 0;
parts.forEach((part, i) => {
  if (!part.cls.startsWith('sg')) return;
  homeAt += 1;
  const next = parts[i + 1];
  check(`home.html, program ${homeAt}`, 'main.sg', part.body, part.cls === 'sg error' ? 'error' : '', next && next.cls === 'out' ? next.body.trimEnd() : null);
});

// The manual pages, read by mandoc.
let manuals = 0;
const man = join(plain, 'build', 'man', 'man7');
let mandoc = null;
for (const candidate of ['/usr/bin/mandoc', '/usr/local/bin/mandoc', '/opt/homebrew/bin/mandoc']) if (existsSync(candidate)) mandoc = candidate;
if (!existsSync(man)) {
  console.error('build/man is not there: run `npm run build` first');
  failures += 1;
} else if (mandoc) {
  for (const file of readdirSync(man)) {
    manuals += 1;
    try {
      execFileSync(mandoc, ['-T', 'lint', '-W', 'warning', join(man, file)], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (error) {
      failures += 1;
      console.error(`${relative(plain, join(man, file))}: mandoc says\n${error.stdout ?? ''}${error.stderr ?? ''}`);
    }
  }
}

console.log(`${programs} programs of SG on the pages of the words and the front page, ${manuals} manual pages read by mandoc${mandoc ? '' : ' (no mandoc here)'}, ${failures} wrong`);
process.exit(failures === 0 ? 0 : 1);
