// Runs every program of SG on the pages this site adds to those of `../docs/` (which
// `../scripts/check-examples.mjs` runs): the pages of the words (`../words/*.md`) and the front page
// (`home.html`). The rules are the same: a block ```sg must run and end with code 0, and the ```text after it
// is what it prints; ```sg error must be refused by the checker and ```sg panic must stop when it runs, the
// ```text after either being how the message starts. On the front page the first program must print what is
// shown, and a program in <pre class="sg error"> must be refused with it.
// A program of several files (modules, a package) or one that fails on purpose is written as files with a
// title, ```sg file title="bank/main.sg", and a terminal, ```text that starts with `$ `: see terminal() below.
// Then every manual page in build/man is read by mandoc's lint, where mandoc is installed.
// `SG` names the sg program (../../interpreter/target/release/sg by default).
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative } from 'node:path';
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

// A program of several files, as a page shows it: each ```sg file title="bank/main.sg" (or ```text
// title="shop/sg.project") is written into a folder of the page under its title, a later block of the same
// title taking its place; a ```text block whose first line starts with `$ ` is a terminal. Each `$` line runs
// in that folder with `sh`, `sg` being the sg program, and what it writes (out and err together) must be the
// lines under it. `$ cd x` moves within the folder; each terminal starts at the folder's root.
const bin = mkdtempSync(join(tmpdir(), 'sg-bin-'));
symlinkSync(sg, join(bin, 'sg'));
function terminal(name, folder, body) {
  programs += 1;
  let at = folder;
  const steps = [];
  for (const line of body.trimEnd().split('\n')) {
    if (line.startsWith('$ ')) steps.push({ command: line.slice(2), shown: [] });
    else if (steps.length > 0) steps[steps.length - 1].shown.push(line);
  }
  for (const { command, shown } of steps) {
    const cd = command.match(/^cd (\S+)$/);
    if (cd) {
      at = join(at, cd[1]);
      continue;
    }
    let given;
    try {
      given = execFileSync('sh', ['-c', `{ ${command}\n} 2>&1`], { cwd: at, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 60000, env: { ...process.env, PATH: `${bin}:${process.env.PATH}` } });
    } catch (error) {
      if (error.code === 'ETIMEDOUT' || error.signal) {
        failures += 1;
        console.error(`${name}: \`${command}\` did not end within 60 s`);
        return;
      }
      given = `${error.stdout ?? ''}${error.stderr ?? ''}`;
    }
    if (given.trimEnd() !== shown.join('\n').trimEnd()) {
      failures += 1;
      console.error(`${name}: \`${command}\` prints otherwise.\nshown:\n${shown.join('\n')}\ngiven:\n${given}`);
      return;
    }
  }
}

// The pages of the words.
const words = join(site, 'words');
for (const file of readdirSync(words).filter((f) => f.endsWith('.md'))) {
  const text = readFileSync(join(words, file), 'utf8');
  const blocks = [...text.matchAll(/^```(\w+)([^\n]*)\n([\s\S]*?)^```$/gm)].map((m) => ({ lang: m[1], meta: m[2].trim(), body: m[3] }));
  const folder = mkdtempSync(join(tmpdir(), 'sg-page-'));
  let at = 0;
  let terminals = 0;
  blocks.forEach((block, i) => {
    const title = block.meta.match(/title="([^"]*)"/);
    if (title) {
      const path = join(folder, title[1]);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, block.body);
    }
    if (block.lang === 'text' && block.body.startsWith('$ ')) {
      terminals += 1;
      terminal(`words/${file}, terminal ${terminals}`, folder, block.body);
      return;
    }
    if (block.lang !== 'sg') return;
    at += 1;
    const kind = block.meta.split(/\s+/)[0];
    if (kind === 'nocheck' || kind === 'file') return;
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
const manRoot = join(plain, 'build', 'man');
let mandoc = null;
for (const candidate of ['/usr/bin/mandoc', '/usr/local/bin/mandoc', '/opt/homebrew/bin/mandoc']) if (existsSync(candidate)) mandoc = candidate;
const manFiles = ['man1', 'man7'].flatMap((dir) => (existsSync(join(manRoot, dir)) ? readdirSync(join(manRoot, dir)).map((file) => join(manRoot, dir, file)) : []));
if (!existsSync(join(manRoot, 'man7'))) {
  console.error('build/man is not there: run `npm run build` first');
  failures += 1;
} else if (mandoc) {
  for (const path of manFiles) {
    const file = basename(path);
    manuals += 1;
    try {
      execFileSync(mandoc, ['-T', 'lint', '-W', 'warning', path], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (error) {
      failures += 1;
      console.error(`${relative(plain, path)}: mandoc says\n${error.stdout ?? ''}${error.stderr ?? ''}`);
      continue;
    }
    // The text the site shows for the page (and `sg help` prints) is the text mandoc lays out, line for line.
    const page = join(plain, 'build', 'manual', `${file}.html`);
    if (!existsSync(page)) continue;
    const plainLines = (text) => text.split('\n').map((line) => line.trimEnd()).join('\n').trim();
    const laid = execFileSync(mandoc, ['-T', 'utf8', '-O', 'width=78', path], { encoding: 'utf8' }).replace(/.\u0008/gu, '');
    const shownHtml = readFileSync(page, 'utf8').match(/<pre[^>]*>([\s\S]*?)<\/pre>/)[1];
    const shown = decode(shownHtml.replace(/<[^>]+>/g, '').replaceAll('&quot;', '"').replaceAll('&#39;', "'"));
    if (plainLines(laid) !== plainLines(shown)) {
      failures += 1;
      const a = plainLines(laid).split('\n');
      const b = plainLines(shown).split('\n');
      const at = a.findIndex((line, i) => line !== b[i]);
      console.error(`${relative(plain, page)}: differs from mandoc's text at line ${at + 1}:\nmandoc: ${a[at]}\nsite:   ${b[at]}`);
    }
  }
}

console.log(`${programs} programs of SG on the pages of the words and the front page, ${manuals} manual pages read by mandoc${mandoc ? '' : ' (no mandoc here)'}, ${failures} wrong`);
process.exit(failures === 0 ? 0 : 1);
