// Runs every program of SG on the pages of the site, so a page never shows code that the language no
// longer takes (the rule of keeping everything in step). A block ```sg is a whole program: it must
// run and end with code 0, and a ```text block after it, before the next code, is what it prints.
// ```sg error must be refused by the checker, and ```sg panic must stop when it runs (a panic, or a
// `main` that ends with an error); for both the ```text after it is how the output starts. ```sg test runs
// its tests (`sg test`). ```sg nocheck is not run: a part of a program, or one that needs files beside it
// (modules, a C file); keep those few. The program is written as `example.sg` in a folder of its own,
// so the messages name that file. `SG` names the sg program (../interpreter/target/release/sg by default).
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const sg = process.env.SG || join(site, '..', 'interpreter', 'target', 'release', 'sg');
const work = mkdtempSync(join(tmpdir(), 'sg-site-'));

function pages(folder) {
  return readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    return statSync(path).isDirectory() ? pages(path) : name.endsWith('.md') ? [path] : [];
  });
}

function run(args) {
  try {
    return { code: 0, text: execFileSync(sg, args, { cwd: work, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }) };
  } catch (error) {
    return { code: error.status ?? 1, text: `${error.stdout ?? ''}${error.stderr ?? ''}` };
  }
}

let failures = 0;
let programs = 0;
for (const page of pages(join(site, 'docs'))) {
  const text = readFileSync(page, 'utf8');
  const blocks = [...text.matchAll(/^```(\w+)([^\n]*)\n([\s\S]*?)^```$/gm)].map((match) => ({ lang: match[1], meta: match[2].trim(), body: match[3] }));
  blocks.forEach((block, at) => {
    if (block.lang !== 'sg') return;
    programs += 1;
    writeFileSync(join(work, 'example.sg'), block.body);
    const kind = block.meta.split(/\s+/)[0];
    if (kind === 'nocheck') return;
    const result = run(kind === 'test' ? ['test', 'example.sg'] : ['example.sg']);
    const next = blocks[at + 1];
    const shown = next && next.lang === 'text' ? next.body.trimEnd() : null;
    const name = `${page.slice(site.length + 1)}, program ${programs}`;
    // An error is refused before the program runs; a panic is a program the checker takes.
    if (kind === 'error' || kind === 'panic') {
      const checked = run(['emit-c', 'example.sg']).code === 0;
      if (checked !== (kind === 'panic')) {
        failures += 1;
        console.error(`${name}: marked \`${kind}\`, but the checker ${checked ? 'takes' : 'refuses'} it`);
        return;
      }
      if (result.code === 0) {
        failures += 1;
        console.error(`${name}: should be refused, and ran:\n${result.text}`);
      } else if (shown !== null && !result.text.startsWith(shown)) {
        failures += 1;
        console.error(`${name}: the message differs.\nshown:\n${shown}\ngiven:\n${result.text}`);
      }
    } else if (result.code !== 0) {
      failures += 1;
      console.error(`${name}: ended with code ${result.code}:\n${result.text}`);
    } else if (shown !== null && result.text.trimEnd() !== shown) {
      failures += 1;
      console.error(`${name}: prints otherwise.\nshown:\n${shown}\ngiven:\n${result.text}`);
    }
  });
}
// The programs on the front page, in `src/pages/index.js`: the sample prints what is shown, and the two mistakes
// are refused with the messages shown, written for a file named `main.sg`.
const index = readFileSync(join(site, 'src', 'pages', 'index.js'), 'utf8');
const literal = (name) => {
  const found = index.match(new RegExp(`const ${name} = [\`']([\\s\\S]*?)[\`'];`));
  return found ? found[1].replaceAll('\\`', '`').replaceAll('\\$', '$') : null;
};
for (const [code, shown, refused] of [['sample', 'sampleOutput', false], ['mistake', 'mistakeMessage', true], ['dangle', 'dangleMessage', true]]) {
  const program = literal(code);
  if (program === null) continue;
  programs += 1;
  writeFileSync(join(work, 'main.sg'), program);
  const result = run(['main.sg']);
  const expected = literal(shown);
  if (refused ? result.code === 0 : result.code !== 0) {
    failures += 1;
    console.error(`src/pages/index.js, \`${code}\`: the program ended with code ${result.code}:\n${result.text}`);
  } else if (expected !== null && (refused ? !result.text.startsWith(expected) : result.text.trimEnd() !== expected)) {
    failures += 1;
    console.error(`src/pages/index.js, \`${code}\`: prints otherwise.\nshown:\n${expected}\ngiven:\n${result.text}`);
  }
}
console.log(`${programs} programs of SG on the pages, ${failures} wrong`);
process.exit(failures === 0 ? 0 : 1);
