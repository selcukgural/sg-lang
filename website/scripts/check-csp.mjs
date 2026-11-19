// After a build: every script written into a page (not a data block such as JSON-LD) must be allowed by
// its hash in static/_headers, or the browser would refuse it once the site is published (choice 1389).
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const headers = readFileSync(join(site, 'static', '_headers'), 'utf8');
const allowed = new Set([...headers.matchAll(/'sha256-([A-Za-z0-9+/=]+)'/g)].map((match) => match[1]));

function pages(folder) {
  return readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    return statSync(path).isDirectory() ? pages(path) : name.endsWith('.html') ? [path] : [];
  });
}

const missing = new Map();
for (const page of pages(join(site, 'build'))) {
  for (const match of readFileSync(page, 'utf8').matchAll(/<script(?![^>]*\bsrc=)([^>]*)>([\s\S]*?)<\/script>/g)) {
    if (/type="application\/(?:ld\+)?json"/.test(match[1]) || match[2].trim() === '') continue;
    const hash = createHash('sha256').update(match[2]).digest('base64');
    if (!allowed.has(hash)) missing.set(hash, page.slice(site.length + 1));
  }
}
for (const [hash, page] of missing) console.error(`a script in ${page} is not allowed: add 'sha256-${hash}' to static/_headers`);
process.exit(missing.size === 0 ? 0 : 1);
