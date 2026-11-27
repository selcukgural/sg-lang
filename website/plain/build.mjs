// Builds the web site of SG into `build/`: plain pages made from Markdown, one stylesheet, two small scripts,
// and the manual pages. It reads the same pages as the Docusaurus site (`../docs/`), the pages of the words of
// the language (`../words/`) and the front page (`home.html`); nothing is copied by hand.
//
// What it makes:
// - a page for every Markdown page of `../docs/` (Learn, Library, Benchmarks, About, Status), with the pages of
//   its section beside it;
// - the Reference: the index of every name (`../words/index.txt`, the pages of `../words/`, and the types and
//   modules of the library, taken from the headings of `../docs/library/`), and a page for each word;
// - from each word's page, its manual page as `man` reads it (`man/man7/sg-read.7`) and as the site shows it;
// - in every program on every page, each word of the language and each type of the library as a link to its
//   page, with what it is in one line;
// - the list the search looks in (`search-index.js`), and the headers Cloudflare gives each page.
// It fails on a link to a page or a place on a page that is not there, and on a script or a style written into
// a page, which the headers do not allow.
import { Marked } from 'marked';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync, copyFileSync } from 'node:fs';
import { dirname, join, posix, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const plain = dirname(fileURLToPath(import.meta.url));
const site = resolve(plain, '..');
const repo = resolve(site, '..');
const docs = join(site, 'docs');
const wordsDir = join(site, 'words');
const out = join(plain, 'build');
const github = 'https://github.com/selcukgural/sg-lang';
const today = new Date().toISOString().slice(0, 10);

// ---------------------------------------------------------------------------------------------------------
// Small helpers

const escape = (text) =>
  String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');

const posixPath = (path) => path.split('\\').join('/');

function frontMatter(text) {
  const found = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!found) return { meta: {}, body: text };
  const meta = {};
  for (const line of found[1].split('\n')) {
    const at = line.indexOf(':');
    if (at < 0) continue;
    let value = line.slice(at + 1).trim();
    if (/^(['"]).*\1$/.test(value)) value = value.slice(1, -1);
    meta[line.slice(0, at).trim()] = value;
  }
  return { meta, body: text.slice(found[0].length) };
}

// The id of a heading, as GitHub and Docusaurus make it, so links written for those keep working.
function slugger() {
  const seen = new Map();
  return (text) => {
    let base = text
      .toLowerCase()
      .replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '')
      .replace(/ /g, '-');
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    return count === 0 ? base : `${base}-${count}`;
  };
}

// The text of inline Markdown, with no marks: `read` and [the book](x.md) become read and the book.
const plainText = (md) =>
  md
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/(\*\*|__)(.*?)\1/g, '$2')
    .replace(/(\*|_)(.*?)\1/g, '$2');

const files = (folder) =>
  readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });

// ---------------------------------------------------------------------------------------------------------
// The pages of `../docs/`, and where each goes

/** Every page of the site: { src, url, title, description, section, body, kind } */
const pages = [];
const bySource = new Map();

function addDoc(src, section, extra = {}) {
  const text = readFileSync(src, 'utf8');
  const { meta, body } = frontMatter(text);
  const rel = posixPath(relative(docs, src)).replace(/\.md$/, '');
  // `book/07-ownership` is `book/ownership.html`, as on the Docusaurus site.
  const url = rel.split('/').map((part) => part.replace(/^\d+-/, '')).join('/') + '.html';
  const h1 = body.match(/^# (.+)$/m);
  const page = {
    src,
    url,
    title: meta.title || (h1 ? plainText(h1[1]) : rel),
    description: meta.description || '',
    position: Number(meta.sidebar_position ?? 999),
    section,
    body,
    kind: 'doc',
    ...extra,
  };
  pages.push(page);
  bySource.set(src, page);
  return page;
}

const ordered = (folder) =>
  readdirSync(folder)
    .filter((name) => name.endsWith('.md'))
    .map((name) => join(folder, name))
    .map((src) => ({ src, position: Number(frontMatter(readFileSync(src, 'utf8')).meta.sidebar_position ?? 999) }))
    .sort((a, b) => (a.src.endsWith('index.md') ? -1 : b.src.endsWith('index.md') ? 1 : a.position - b.position || a.src.localeCompare(b.src)))
    .map((page) => page.src);

const SECTIONS = [
  { id: 'learn', title: 'Learn', description: 'Install SG, take the tour, and read the book.' },
  { id: 'reference', title: 'Reference', description: 'Every word, type and sign of the language, each with its own page.' },
  { id: 'library', title: 'Library', description: 'The types that come with SG, each with its methods.' },
  { id: 'manual', title: 'Manual', description: 'The manual pages, as man shows them in a terminal.' },
  { id: 'benchmarks', title: 'Benchmarks', description: 'How fast SG is, and how it is measured.' },
  { id: 'about', title: 'About', description: 'Why SG, how it got here, and where to talk about it.' },
];
const sectionOf = Object.fromEntries(SECTIONS.map((s) => [s.id, s]));
/** The pages of each section, in the order of its list; a group is { group, pages }. */
const sectionPages = Object.fromEntries(SECTIONS.map((s) => [s.id, []]));

// Learn: install, the tour, the book, and SG for Rust programmers.
sectionPages.learn.push(addDoc(join(docs, 'learn/install.md'), 'learn'));
sectionPages.learn.push(addDoc(join(docs, 'learn/tour.md'), 'learn'));
sectionPages.learn.push({ group: 'The SG book', pages: ordered(join(docs, 'learn/book')).map((src) => addDoc(src, 'learn')) });
if (existsSync(join(docs, 'learn/for-rust.md'))) sectionPages.learn.push(addDoc(join(docs, 'learn/for-rust.md'), 'learn'));
for (const id of ['library', 'benchmarks', 'about']) {
  const folder = join(docs, id);
  if (existsSync(folder)) for (const src of ordered(folder)) sectionPages[id].push(addDoc(src, id));
}
const statusPage = addDoc(join(docs, 'status.md'), null);

// ---------------------------------------------------------------------------------------------------------
// The names of the language: the index, the pages of the words, and the types of the library

const KIND = {
  keyword: { title: 'Keywords', label: 'keyword' },
  type: { title: 'Primitive types', label: 'type' },
  value: { title: 'Values with a name', label: 'value' },
  sign: { title: 'Signs', label: 'sign' },
  'library-type': { title: 'Types of the library', label: 'library type' },
  module: { title: 'Modules of the library', label: 'module' },
  command: { title: 'Commands', label: 'command' },
};

/** Every name of the index: { name, kind, summary, url, page } */
const names = [];
const nameOf = new Map(); // a name as it is written in code -> its entry

// Pages of words, one Markdown file each.
const wordPages = [];
for (const file of readdirSync(wordsDir).filter((f) => f.endsWith('.md')).sort()) {
  const src = join(wordsDir, file);
  const { meta, body } = frontMatter(readFileSync(src, 'utf8'));
  const slug = file.replace(/\.md$/, '');
  const page = {
    src,
    url: `reference/${slug}.html`,
    title: meta.title || meta.name,
    name: meta.name,
    description: meta.summary,
    section: 'reference',
    body,
    kind: 'word',
    meta,
    manName: `sg-${slug}`,
  };
  pages.push(page);
  bySource.set(src, page);
  wordPages.push(page);
}

// The lines of the index that have no page yet.
for (const line of readFileSync(join(wordsDir, 'index.txt'), 'utf8').split('\n')) {
  if (!line.trim() || line.startsWith('#')) continue;
  const [kind, name, summary] = line.split(' | ').map((part) => part.trim());
  if (!KIND[kind]) throw new Error(`words/index.txt: no kind \`${kind}\` (${line})`);
  if (wordPages.some((p) => p.meta.name === name)) continue;
  names.push({ name, kind, summary, url: `reference/index.html#${slugOfName(name)}` });
}
function slugOfName(name) {
  return 'n-' + name.replace(/[^A-Za-z0-9]+/g, (s) => '-' + [...s].map((c) => c.charCodeAt(0).toString(16)).join('') + '-').replace(/-+$/, '');
}
for (const page of wordPages) {
  names.push({ name: page.meta.name, kind: page.meta.kind, summary: page.meta.summary, url: page.url, page });
}

// The types of the library: every heading of a library page that names a type, as ## `List<T>`.
for (const page of sectionPages.library) {
  if (page.url === 'library/index.html') {
    continue;
  }
  names.push({ name: page.title, kind: 'module', summary: page.description, url: page.url });
  const slug = slugger();
  for (const found of page.body.matchAll(/^(#{2,3}) (.+)$/gm)) {
    const id = slug(plainText(found[2]));
    const type = found[2].match(/^`((?:shared locked |atomic )?[A-Z][A-Za-z0-9]*)(<[^`]*>)?[^`]*`/);
    if (!type || found[1] !== '##') continue;
    const name = type[1];
    if (name.length < 2) continue;
    if (/^(shared locked|atomic)/.test(name)) continue;
    if (names.some((n) => n.name === name)) continue;
    // Until the type has a page of its own (as List has), its line says where it is.
    const summary = `A type of the library, in ${page.title}.`;
    names.push({ name, kind: 'library-type', summary, url: `${page.url}#${id}` });
  }
}
for (const entry of names) {
  if (['keyword', 'type', 'value', 'library-type'].includes(entry.kind)) nameOf.set(entry.name, entry);
}

// ---------------------------------------------------------------------------------------------------------
// Colouring a program of SG, with each name a link

const TOKEN = /(\/\/[^\n]*)|(\$?"""[\s\S]*?""")|(\$?"(?:\\.|\{[^{}"]*\}|[^"\\\n])*")|('(?:\\[^'\n]+|[^'\\\n])')|(\b(?:0x[\dA-Fa-f_]+|0b[01_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?)\b)|([A-Za-z_]\w*)|(\s+|[\s\S])/g;

function colour(code, from) {
  let html = '';
  let last = '';
  for (const m of code.matchAll(TOKEN)) {
    const [text, comment, raw, string, char, number, word] = m;
    if (comment) html += `<span class="cm">${escape(comment)}</span>`;
    else if (raw || string || char) html += `<span class="st">${escape(text)}</span>`;
    else if (number) html += `<span class="nu">${escape(number)}</span>`;
    else if (word && last !== '.' && nameOf.has(word)) {
      const entry = nameOf.get(word);
      const cls = entry.kind === 'keyword' ? 'kw' : entry.kind === 'library-type' ? 'lt' : 'ty';
      html += `<a class="${cls}" href="${escape(link(from, entry.url))}" title="${escape(entry.summary)}">${escape(word)}</a>`;
    } else html += escape(text);
    if (!/^\s+$/.test(text)) last = text;
  }
  return html;
}

// ---------------------------------------------------------------------------------------------------------
// Links between pages

/** The address of `target` (a path from the root of the site, with #place) as seen from page `from`. */
function link(from, target) {
  if (/^[a-z]+:/.test(target)) return target;
  const [path, hash] = target.split('#');
  const rel = path ? posix.relative(posix.dirname(from), path) || posix.basename(path) : '';
  return rel + (hash !== undefined ? `#${hash}` : '');
}

/** A link written in the Markdown of `page`, made an address on the site. */
function resolveLink(page, href) {
  if (/^(https?:|mailto:)/.test(href) || href.startsWith('#')) return href;
  const [path, hash] = href.split('#');
  let target;
  if (path.startsWith('/docs/')) {
    const base = join(docs, path.slice(6));
    target = [base + '.md', join(base, 'index.md')].find((p) => bySource.has(p));
  } else {
    target = resolve(dirname(page.src), decodeURI(path));
  }
  const to = target && bySource.get(target);
  if (to) return link(page.url, to.url + (hash ? `#${hash}` : ''));
  const inRepo = target ? posixPath(relative(repo, target)) : null;
  if (inRepo && !inRepo.startsWith('..') && existsSync(target)) return `${github}/blob/main/${inRepo}${hash ? `#${hash}` : ''}`;
  problems.push(`${posixPath(relative(repo, page.src))}: a link to \`${href}\`, which is no page`);
  return href;
}

const problems = [];

// ---------------------------------------------------------------------------------------------------------
// Markdown into HTML

let current = null; // the page being written
let inLink = 0; // inside a link or a heading: a name in code is not made a link there
let headings = []; // the headings of the page being written, for the list beside it
let slug = slugger();

const marked = new Marked({ gfm: true });
marked.use({
  renderer: {
    heading({ tokens, depth, text }) {
      inLink += 1;
      const inner = this.parser.parseInline(tokens);
      inLink -= 1;
      const id = slug(plainText(text));
      if (depth === 2) headings.push({ id, text: plainText(text) });
      if (depth === 1) return `<h1>${inner}</h1>\n`;
      return `<h${depth} id="${escape(id)}"><a class="here" href="#${escape(id)}" aria-hidden="true" tabindex="-1">#</a>${inner}</h${depth}>\n`;
    },
    link({ href, title, tokens }) {
      inLink += 1;
      const inner = this.parser.parseInline(tokens);
      inLink -= 1;
      const outside = /^https?:/.test(href) && !href.startsWith('https://sg-lang.org');
      return `<a href="${escape(href)}"${title ? ` title="${escape(title)}"` : ''}${outside ? ' rel="noopener"' : ''}>${inner}</a>`;
    },
    codespan({ text }) {
      // marked gives the text of `code` with & < > " ' already written as entities.
      const raw = text.replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&quot;', '"').replaceAll('&#39;', "'").replaceAll('&amp;', '&');
      const key = raw.replace(/<.*>$/, '');
      const entry = nameOf.get(key);
      const self = current.kind === 'word' && entry && entry.page === current;
      if (inLink === 0 && entry && !self && (raw === key || /^[A-Z]/.test(key))) {
        return `<a class="code-link" href="${escape(link(current.url, entry.url))}" title="${escape(entry.summary)}"><code>${escape(raw)}</code></a>`;
      }
      return `<code>${escape(raw)}</code>`;
    },
    code(token) {
      const [lang = '', ...rest] = (token.lang || '').split(/\s+/);
      const meta = rest.join(' ');
      if (lang === 'sg') {
        const kind = rest[0] || '';
        const title = meta.match(/title="([^"]*)"/);
        const label = kind === 'error' ? 'Refused' : kind === 'panic' ? 'Stops' : '';
        return (
          (title ? `<p class="file">${escape(title[1])}</p>` : '') +
          `<pre class="sg${label ? ` ${kind}` : ''}"${label ? ` data-label="${label}"` : ''}><code>${colour(token.text, current.url)}</code></pre>\n`
        );
      }
      const title = meta.match(/title="([^"]*)"/);
      const file = title ? `<p class="file">${escape(title[1])}</p>` : '';
      // A terminal: commands after `$ ` and what they write, which the check of the site runs (check.mjs).
      if (lang === 'text' && token.text.startsWith('$ ')) {
        return `<pre class="out" data-label="In a terminal"><code>${escape(token.text)}</code></pre>\n`;
      }
      if (lang === 'text' && token.after && !title) {
        const label = token.after === 'error' ? 'The compiler says' : token.after === 'panic' ? 'It stops with' : 'Output';
        return `<pre class="out" data-label="${label}"><code>${escape(token.text)}</code></pre>\n`;
      }
      return `${file}<pre class="plain"><code>${escape(token.text)}</code></pre>\n`;
    },
  },
});

function render(page, body) {
  current = page;
  headings = [];
  slug = slugger();
  const tokens = marked.lexer(body);
  // A ```text block right after a program is what the program printed, or how it was refused.
  let previous = null;
  marked.walkTokens(tokens, (token) => {
    if (token.type === 'link') token.href = resolveLink(current, token.href);
    if (token.type !== 'code') return;
    const [lang, kind] = (token.lang || '').split(/\s+/);
    if (lang === 'text' && previous) token.after = previous;
    previous = lang === 'sg' ? kind || 'run' : null;
  });
  // A wide table scrolls in its own box, not the page.
  return marked.parser(tokens).replaceAll('<table>', '<div class="scroll"><table>').replaceAll('</table>', '</table></div>');
}

// ---------------------------------------------------------------------------------------------------------
// The frame of every page

// The wildcat logo (brand/wildcat/), with its ground taken out: one for light pages, one for dark, and the
// stylesheet shows the one that fits the page. `to` makes its address from the page.
const LOGO = (cls, to) => {
  const [file, w, h] = cls === 'big' ? ['logo', 880, 676] : ['logo-small', 127, 88];
  return `<span class="logo ${cls}"><img class="for-light" src="${to(`${file}-light.webp`)}" width="${w}" height="${h}" alt="SG"><img class="for-dark" src="${to(`${file}-dark.webp`)}" width="${w}" height="${h}" alt=""></span>`;
};

const NAV = [
  ['Learn', 'learn/index.html', 'learn'],
  ['Reference', 'reference/index.html', 'reference'],
  ['Library', 'library/index.html', 'library'],
  ['Manual', 'manual/index.html', 'manual'],
  ['Benchmarks', 'benchmarks/index.html', 'benchmarks'],
  ['Status', 'status.html', 'status'],
];

function frame({ url, title, description, section, main, wide = false, side = '', onPage = '' }) {
  const root = posix.relative(posix.dirname(url), '.') || '.';
  const to = (target) => link(url, target);
  const nav = NAV.map(
    ([label, target, id]) =>
      `<a href="${to(target)}"${id === section || (id === 'status' && url === 'status.html') ? ' aria-current="page"' : ''}>${label}</a>`,
  ).join('\n      ');
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escape(title === 'SG' ? 'SG' : `${title} · SG`)}</title>
<meta name="description" content="${escape(description || '')}">
<meta name="sg-root" content="${root}/">
<meta property="og:type" content="website">
<meta property="og:site_name" content="SG">
<meta property="og:title" content="${escape(title)}">
<meta property="og:description" content="${escape(description || '')}">
<meta property="og:url" content="${escape(`https://sg-lang.org/${url.replace(/(^|\/)index\.html$/, '$1').replace(/\.html$/, '')}`)}">
<meta property="og:image" content="https://sg-lang.org/social-card.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="SG, with a wildcat's head over the G: Small enough to know by heart. Safe and fast by construction.">
<meta name="twitter:card" content="summary_large_image">
<link rel="stylesheet" href="${to('style.css')}">
<link rel="icon" href="${to('favicon.png')}" type="image/png">
<link rel="apple-touch-icon" href="${to('apple-touch-icon.png')}">
<script src="${to('theme.js')}"></script>
<script src="${to('search-index.js')}" defer></script>
<script src="${to('site.js')}" defer></script>
</head>
<body>
<header class="top">
  <div class="bar">
    <a class="name" href="${to('index.html')}" aria-label="SG, the front page">${LOGO('small', to)}</a>
    <nav class="menu" aria-label="Site">
      ${nav}
    </nav>
    <form class="find" role="search" action="${to('reference/index.html')}">
      <input id="q" name="q" type="search" placeholder="Search" aria-label="Search the site" autocomplete="off">
      <kbd class="key">/</kbd>
      <ul id="found" hidden></ul>
    </form>
    <button id="theme" type="button" class="theme" hidden>Dark</button>
  </div>
</header>
<div class="${wide ? 'wide' : side ? 'layout' : 'single'}">
${side}<main id="main" class="doc">
${main}
</main>
${onPage}</div>
<footer class="foot">
  <p>SG is free software under the MIT or Apache 2.0 license.</p>
  <p><a href="${to('about/why.html')}">Why SG</a> <a href="${to('about/community.html')}">Community</a> <a href="${github}">Source</a> <a href="${github}/blob/main/SECURITY.md">Security</a></p>
</footer>
</body>
</html>
`;
}

function sideNav(page) {
  if (!page.section) return '';
  const section = sectionOf[page.section];
  const item = (p) => `<li><a href="${link(page.url, p.url)}"${p === page ? ' aria-current="page"' : ''}>${escape(p.navTitle || p.title)}</a></li>`;
  const list = sectionPages[page.section]
    .map((entry) => (entry.group ? `<li class="group"><span>${escape(entry.group)}</span><ul>${entry.pages.map(item).join('')}</ul></li>` : item(entry)))
    .join('\n');
  return `<nav class="side" aria-label="${escape(section.title)}">
<p class="side-title"><a href="${link(page.url, `${page.section}/index.html`)}">${escape(section.title)}</a></p>
<ul>
${list}
</ul>
</nav>
`;
}

function onPageNav(list) {
  if (list.length < 2) return '';
  return `<nav class="onpage" aria-label="On this page">
<p>On this page</p>
<ul>
${list.map((h) => `<li><a href="#${escape(h.id)}">${escape(h.text)}</a></li>`).join('\n')}
</ul>
</nav>
`;
}

const flat = (id) => sectionPages[id].flatMap((entry) => (entry.group ? entry.pages : [entry]));

function prevNext(page) {
  if (!page.section) return '';
  const list = flat(page.section);
  const at = list.indexOf(page);
  if (at < 0) return '';
  const before = list[at - 1];
  const after = list[at + 1];
  if (!before && !after) return '';
  return `<nav class="pager" aria-label="Pages">
${before ? `<a class="prev" href="${link(page.url, before.url)}"><span>Previous</span>${escape(before.title)}</a>` : '<span></span>'}
${after ? `<a class="next" href="${link(page.url, after.url)}"><span>Next</span>${escape(after.title)}</a>` : ''}
</nav>`;
}

// ---------------------------------------------------------------------------------------------------------
// Manual pages: roff for `man`, and the text `man` shows, from the same Markdown

/** The blocks of a word's page, simplified: { type: 'h2'|'h3'|'p'|'code'|'rows', ... } */
function blocksOf(body) {
  const blocks = [];
  const walk = (tokens) => {
    for (const t of tokens) {
      if (t.type === 'heading') blocks.push({ type: t.depth === 2 ? 'h2' : 'h3', text: plainText(t.text) });
      else if (t.type === 'paragraph') blocks.push({ type: 'p', inline: t.text });
      else if (t.type === 'code') {
        // A block with a title is a file of a program of several files: its name goes above it.
        const title = (t.lang || '').match(/title="([^"]*)"/);
        if (title) blocks.push({ type: 'p', inline: `The file ${title[1]}:` });
        blocks.push({ type: 'code', lang: (t.lang || '').split(/\s+/)[0], kind: (t.lang || '').split(/\s+/)[1] || '', text: t.text });
      }
      else if (t.type === 'table') blocks.push({ type: 'rows', rows: t.rows.map((r) => r.map((c) => c.text)) });
      else if (t.type === 'list') blocks.push(...t.items.map((i) => ({ type: 'p', inline: `- ${i.text}` })));
    }
  };
  walk(marked.lexer(body));
  return blocks;
}

const roffText = (text) => text.replaceAll('\\', '\\e').replace(/^([.'])/gm, '\\&$1');
// An argument of a macro in quotes: a quote inside it is written twice.
const roffArg = (text) => `"${roffText(text).replaceAll('"', '""')}"`;
function roffInline(md) {
  return md
    .split(/(`[^`]*`)/)
    .map((part) => (part.startsWith('`') ? `\\fB${roffText(part.slice(1, -1)).replaceAll('-', '\\-')}\\fR` : roffText(plainText(part))))
    .join('')
    .replace(/\n/g, ' ');
}
const SECTION_NAME = { Forms: 'SYNOPSIS', 'What it means': 'DESCRIPTION', Examples: 'EXAMPLES' };

function roffOf(page) {
  const name = page.manName.toUpperCase();
  const lines = [
    `.\\" Made by website/plain/build.mjs from website/words/${posix.basename(page.src)}; change that file.`,
    `.TH ${name} 7 ${today} "SG" "SG reference"`,
    '.SH NAME',
    `${roffText(page.meta.name)} \\- ${roffText(page.meta.summary.replace(/\.$/, '').replace(/^./, (c) => c.toLowerCase()))}`,
  ];
  for (const block of blocksOf(page.body)) {
    if (block.type === 'h2') {
      const title = SECTION_NAME[block.text] || block.text.toUpperCase();
      lines.push(`.SH ${title.includes(' ') ? roffArg(title) : title}`);
    }
    else if (block.type === 'h3') lines.push(`.SS ${roffArg(block.text)}`);
    else if (block.type === 'p') lines.push('.PP', roffInline(block.inline));
    else if (block.type === 'rows') {
      for (const row of block.rows) lines.push('.TP', `\\fB${roffText(plainText(row[0])).replaceAll('-', '\\-')}\\fR`, roffText(plainText(row[1])));
      lines.push('.PP');
    } else if (block.type === 'code') {
      lines.push('.PP', '.RS 4', '.nf', roffText(block.text), '.fi', '.RE');
    }
  }
  const see = (page.meta.see || '').split(',').map((s) => s.trim()).filter(Boolean);
  lines.push('.SH "SEE ALSO"');
  const seePages = see.map((s) => wordPages.find((p) => p.meta.name === s)).filter(Boolean);
  lines.push(seePages.map((w, i) => `.BR ${w.manName} (7)${i === seePages.length - 1 ? '' : ','}`).join('\n') || '.BR sg-index (7)');
  lines.push('.PP', `https://sg\\-lang.org/${page.url}`);
  // A paragraph macro right after a heading is one too many: man starts a paragraph there anyway.
  // A paragraph macro next to a heading, or with nothing after it, is one too many.
  const kept = lines.filter((l) => l !== '');
  return kept.filter((l, i) => !(l === '.PP' && (/^\.(SH|SS|PP)( |$)/.test(kept[i - 1] ?? '') || /^\.(SH|SS)( |$)/.test(kept[i + 1] ?? '.SH')))).join('\n') + '\n';
}

// The text `man` shows for a page, laid out as mandoc lays it out, 78 columns wide. In HTML, `bold` marks
// the parts man shows in bold.
function wrap(text, indent, width = 78) {
  const words = text.split(/\s+/).filter(Boolean);
  const lines = [];
  let line = '';
  for (const word of words) {
    const bare = word.replace(/\u0001|\u0002/g, '');
    const now = line.replace(/\u0001|\u0002/g, '');
    if (now && indent + now.length + 1 + bare.length > width) {
      lines.push(' '.repeat(indent) + line);
      line = word;
    } else line = line ? `${line} ${word}` : word;
  }
  if (line) lines.push(' '.repeat(indent) + line);
  return lines.join('\n');
}
// \u0001 and \u0002 open and close bold; made <b> in HTML and dropped in plain text.
const textInline = (md) => md.split(/(`[^`]*`)/).map((part) => (part.startsWith('`') ? `\u0001${part.slice(1, -1)}\u0002` : plainText(part))).join('').replace(/\n/g, ' ');

function manText(page) {
  const head = `${page.manName.toUpperCase()}(7)`;
  const middle = 'SG reference';
  const gap = Math.max(1, Math.floor((78 - head.length * 2 - middle.length) / 2));
  const out = [`${head}${' '.repeat(gap)}${middle}${' '.repeat(78 - head.length * 2 - middle.length - gap)}${head}`, ''];
  out.push('\u0001NAME\u0002', wrap(`\u0001${page.meta.name}\u0002 - ${page.meta.summary.replace(/\.$/, '').replace(/^./, (c) => c.toLowerCase())}`, 5), '');
  for (const block of blocksOf(page.body)) {
    if (block.type === 'h2') out.push(`\u0001${SECTION_NAME[block.text] || block.text.toUpperCase()}\u0002`);
    else if (block.type === 'h3') out.push(`   \u0001${block.text}\u0002`);
    else if (block.type === 'p') out.push(wrap(textInline(block.inline), 5), '');
    else if (block.type === 'rows') {
      for (const row of block.rows) {
        const tag = plainText(row[0]);
        const text = wrap(plainText(row[1]), 12);
        out.push(tag.length < 7 ? `     \u0001${tag}\u0002${' '.repeat(7 - tag.length)}${text.slice(12)}` : `     \u0001${tag}\u0002\n${text}`, '');
      }
    } else if (block.type === 'code') out.push(block.text.split('\n').map((l) => (l ? `         ${l}` : '')).join('\n'), '');
  }
  const see = (page.meta.see || '').split(',').map((s) => s.trim()).filter(Boolean);
  out.push('\u0001SEE ALSO\u0002', wrap(see.map((s) => wordPages.find((p) => p.meta.name === s)).filter(Boolean).map((p) => `\u0001${p.manName}\u0002(7)`).join(', ') || 'sg-index(7)', 5), '', wrap(`https://sg-lang.org/${page.url}`, 5), '');
  // The date in the middle of the line, as mandoc puts it.
  const foot = `SG${' '.repeat(Math.floor((78 - today.length) / 2) - 2)}${today}`;
  out.push(`${foot}${' '.repeat(Math.max(1, 78 - foot.length - head.length))}${head}`);
  return out.join('\n').replace(/\n{3,}/g, '\n\n') + '\n';
}
const manHtml = (text) => escape(text).replace(/\u0001/g, '<b>').replace(/\u0002/g, '</b>');
const manPlain = (text) => text.replace(/\u0001|\u0002/g, '');
// What `sg help <word>` prints (choice 2082): the text man shows, without the line at its top and the line
// with the date at its foot, so that it changes only when the page does.
// The pages it names are pages of `sg help` too.
const helpText = (page) =>
  manPlain(manText(page)).split('\n').slice(2, -2).join('\n').replace(/\n+$/, '').replace(/\bsg-([a-z0-9-]+)\(7\)/g, 'sg help $1') + '\n';

// ---------------------------------------------------------------------------------------------------------
// Writing the site

rmSync(out, { recursive: true, force: true });
const written = new Map(); // url -> html
const write = (url, html) => {
  written.set(url, html);
  mkdirSync(dirname(join(out, url)), { recursive: true });
  writeFileSync(join(out, url), html);
};

// The pages of ../docs/.
for (const page of pages.filter((p) => p.kind === 'doc')) {
  let body = page.body;
  if (!/^# /m.test(body)) body = `# ${page.title}\n\n${body}`;
  const html = render(page, body);
  const trail = page.section ? `<p class="trail"><a href="${link(page.url, `${page.section}/index.html`)}">${escape(sectionOf[page.section].title)}</a></p>\n` : '';
  write(
    page.url,
    frame({
      url: page.url,
      title: page.title,
      description: page.description,
      section: page.section ?? (page === statusPage ? 'status' : null),
      main: trail + html + prevNext(page),
      side: sideNav(page),
      onPage: onPageNav(headings),
    }),
  );
  page.headings = headings;
}

// The front page of each section that has no index.md.
for (const section of SECTIONS) {
  const url = `${section.id}/index.html`;
  if (pages.some((p) => p.url === url) || section.id === 'reference' || section.id === 'manual') continue;
  const page = { url, title: section.title, section: section.id };
  const items = sectionPages[section.id]
    .map((entry) =>
      entry.group
        ? `<h2 id="${slugger()(entry.group)}">${escape(entry.group)}</h2>\n<dl class="pages">${entry.pages.map((p) => `<dt><a href="${link(url, p.url)}">${escape(p.title)}</a></dt><dd>${escape(p.description)}</dd>`).join('\n')}</dl>`
        : `<dl class="pages"><dt><a href="${link(url, entry.url)}">${escape(entry.title)}</a></dt><dd>${escape(entry.description)}</dd></dl>`,
    )
    .join('\n');
  write(url, frame({ url, title: section.title, description: section.description, section: section.id, main: `<h1>${escape(section.title)}</h1>\n<p class="lead">${escape(section.description)}</p>\n${items}`, side: sideNav(page) }));
}

// The pages of the words.
sectionPages.reference = [{ url: 'reference/index.html', title: 'The index', navTitle: 'The index' }, ...wordPages];
sectionPages.reference[0].section = 'reference';
for (const page of wordPages) {
  const body = render(page, page.body);
  const kind = KIND[page.meta.kind];
  const see = (page.meta.see || '').split(',').map((s) => s.trim()).filter(Boolean).map((s) => names.find((n) => n.name === s)).filter(Boolean);
  const more = [];
  if (page.meta.learn) more.push(`<li><a href="${escape(resolveLink(page, page.meta.learn))}">In the book</a></li>`);
  if (page.meta.library) more.push(`<li><a href="${escape(resolveLink(page, page.meta.library))}">Every method, in the library</a></li>`);
  headings.push({ id: 'see-also', text: 'See also' });
  const main = `<p class="trail"><a href="index.html">Reference</a> <span aria-hidden="true">/</span> ${escape(kind.title)}</p>
<h1 class="word"><code>${escape(page.title)}</code></h1>
<p class="lead">${escape(page.meta.summary)}</p>
<p class="note">In a terminal: <code>man ${escape(page.manName)}</code>. <a href="../manual/${escape(page.manName)}.7.html">The manual page</a></p>
${body}
<h2 id="see-also"><a class="here" href="#see-also" aria-hidden="true" tabindex="-1">#</a>See also</h2>
<ul class="see">
${see.map((n) => `<li><a href="${escape(link(page.url, n.url))}"><code>${escape(n.name)}</code></a> ${escape(n.summary)}</li>`).join('\n')}
${more.join('\n')}
</ul>`;
  write(page.url, frame({ url: page.url, title: page.title, description: page.meta.summary, section: 'reference', main, side: sideNav(page), onPage: onPageNav(headings) }));
  page.headings = headings;
}

// The index of every name.
{
  const url = 'reference/index.html';
  const groups = Object.entries(KIND).map(([kind, info]) => ({ kind, info, list: names.filter((n) => n.kind === kind).sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' })) }));
  const row = (n) => {
    const own = n.url.startsWith('reference/index.html#');
    const id = own ? n.url.split('#')[1] : slugOfName(n.name);
    const cell = own ? `<code>${escape(n.name)}</code>` : `<a href="${escape(link(url, n.url))}"><code>${escape(n.name)}</code></a>`;
    return `<tr id="${id}"${n.page ? ' class="has-page"' : ''}><td>${cell}</td><td>${escape(n.summary)}</td></tr>`;
  };
  const main = `<h1>The index</h1>
<p class="lead">Every name in SG on one page: the words of the language, its types and signs, the types and modules
of the library, and the commands.</p>
<p class="note">A name in bold has a page of its own, from the simplest use to the hardest; the others say what they
are in one line until theirs is written. Every name in a program on this site leads here or to its page.</p>
<div class="filter"><label for="f">Filter</label><input id="f" type="search" placeholder="Filter: lock, slice, json" autocomplete="off"><span id="fcount" class="note"></span></div>
<ul class="toc">${groups.map((g) => `<li><a href="#${g.kind}">${escape(g.info.title)}</a> <span>${g.list.length}</span></li>`).join('')}</ul>
<div id="names">
${groups.map((g) => `<h2 id="${g.kind}">${escape(g.info.title)}</h2>\n<div class="scroll"><table class="idx">\n${g.list.map(row).join('\n')}\n</table></div>`).join('\n')}
</div>`;
  write(url, frame({ url, title: 'The index', description: SECTIONS[1].description, section: 'reference', main, side: sideNav(sectionPages.reference[0]) }));
}

// The manual pages: roff, the text man shows, and the list of them.
sectionPages.manual = [{ url: 'manual/index.html', title: 'The manual pages', section: 'manual' }, ...wordPages.map((p) => ({ url: `manual/${p.manName}.7.html`, title: `${p.manName}(7)`, section: 'manual', word: p }))];
for (const entry of sectionPages.manual.slice(1)) {
  const page = entry.word;
  const roff = roffOf(page);
  mkdirSync(join(out, 'man/man7'), { recursive: true });
  writeFileSync(join(out, `man/man7/${page.manName}.7`), roff);
  const text = manText(page);
  mkdirSync(join(out, 'help'), { recursive: true });
  writeFileSync(join(out, `help/${page.meta.name.toLowerCase()}.txt`), helpText(page));
  const main = `<p class="trail"><a href="index.html">Manual</a></p>
<h1>${escape(page.manName)}(7)</h1>
<p class="note">What <code>man ${escape(page.manName)}</code> shows. The same page on the site: <a href="${link(entry.url, page.url)}"><code>${escape(page.title)}</code></a>. The file: <a href="../man/man7/${escape(page.manName)}.7">${escape(page.manName)}.7</a></p>
<pre class="term"><code>${manHtml(text)}</code></pre>`;
  write(entry.url, frame({ url: entry.url, title: `${page.manName}(7)`, description: page.meta.summary, section: 'manual', main, side: sideNav(entry) }));
}
{
  // sg-index(7): every name, as the index page lists them.
  const lines = [`.\\" Made by website/plain/build.mjs from website/words/; change those files.`, `.TH SG-INDEX 7 ${today} "SG" "SG reference"`, '.SH NAME', 'sg\\-index \\- every name in SG', '.SH DESCRIPTION'];
  for (const [kind, info] of Object.entries(KIND)) {
    lines.push(`.SS "${info.title}"`);
    for (const n of names.filter((x) => x.kind === kind).sort((a, b) => a.name.localeCompare(b.name))) lines.push('.TP', `\\fB${roffText(n.name).replaceAll('-', '\\-')}\\fR`, roffText(n.summary));
  }
  lines.push('.SH "SEE ALSO"', wordPages.map((p, i) => `.BR ${p.manName} (7)${i < wordPages.length - 1 ? ',' : ''}`).join('\n'));
  writeFileSync(join(out, 'man/man7/sg-index.7'), lines.join('\n') + '\n');
  const url = 'manual/index.html';
  const main = `<h1>The manual pages</h1>
<p class="lead">The page of each word of SG, as <code>man</code> shows it in a terminal. It is made from the same file as
the page on the site, so the two never differ.</p>
<h2 id="pages">The pages</h2>
<div class="scroll"><table class="idx">
${wordPages.map((p) => `<tr><td><a href="${p.manName}.7.html">${escape(p.manName)}(7)</a></td><td>${escape(p.meta.summary)}</td></tr>`).join('\n')}
<tr><td><a href="../man/man7/sg-index.7">sg-index(7)</a></td><td>every name in SG, as <a href="../reference/index.html">the index</a> lists them</td></tr>
</table></div>
<h2 id="in-a-terminal">In a terminal</h2>
<pre class="plain"><code>man sg-read              # the page of a word
man sg-index             # every name
man -k sg-               # the pages of SG</code></pre>
<p>The pages are installed with SG. From a copy of the repository, after <code>npm run build</code> in
<code>website/plain</code>: <code>man -M "$PWD/build/man" sg-read</code>.</p>
<h2 id="sg-help">From sg itself</h2>
<pre class="plain"><code>sg help read             # the page of a word, with no network
sg help index            # every name, with what it is in one line
sg help                  # the words with a page</code></pre>
<p><code>sg</code> carries the same pages in itself, so they work on Windows too, and for a build from source where
nothing installs the manual pages. What it prints for <code>read</code>: <a href="../help/read.txt">help/read.txt</a>.</p>`;
  write(url, frame({ url, title: 'Manual', description: SECTIONS[3].description, section: 'manual', main, side: sideNav(sectionPages.manual[0]) }));
}

// What `sg` carries for `sg help` (choice 2082), written into the interpreter's source, where Cargo finds it:
// the text of each word's page, and a line for every name of the index. A file of a word that has no page any
// more is taken out.
{
  const dir = join(plain, '..', '..', 'interpreter', 'src', 'help');
  mkdirSync(dir, { recursive: true });
  const keep = new Set(['names.tsv']);
  for (const page of wordPages) {
    const file = `${posix.basename(page.src, '.md')}.txt`;
    keep.add(file);
    writeFileSync(join(dir, file), helpText(page));
  }
  for (const file of readdirSync(dir)) if (file.endsWith('.txt') && !keep.has(file)) rmSync(join(dir, file));
  // name, kind, the part of the index it is in, what it is, the file of its page (or nothing), its address: one
  // name to a line, in the order of the index. No field holds a tab or a line break.
  const address = (url) => `https://sg-lang.org/${url.replace(/(^|\/)index\.html/, '$1').replace(/\.html/, '')}`;
  const rows = [];
  for (const kind of Object.keys(KIND)) {
    for (const n of names.filter((x) => x.kind === kind).sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }))) {
      // A line of the index begins small and has no full stop; the summary of a page is a sentence: as man's
      // NAME line, the sentence is made a line too.
      const summary = n.summary.replace(/\.$/, '').replace(/^[A-Z](?=[a-z ])/, (c) => c.toLowerCase());
      const fields = [n.name, KIND[kind].label, KIND[kind].title, summary, n.page ? posix.basename(n.page.src, '.md') : '', address(n.url)];
      if (fields.some((f) => /[\t\n]/.test(f))) problems.push(`the name \`${n.name}\`: a tab or a line break in its line for sg help`);
      rows.push(fields.join('\t'));
    }
  }
  writeFileSync(join(dir, 'names.tsv'), `# Made by website/plain/build.mjs for \`sg help\`; change website/words/ and build the site.\n${rows.join('\n')}\n`);
}

// The front page.
{
  const url = 'index.html';
  current = { url, kind: 'home' };
  const home = readFileSync(join(plain, 'home.html'), 'utf8')
    .replace(/<pre class="sg( error)?">([\s\S]*?)<\/pre>/g, (_, error, code) => {
      const text = code.replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&');
      return `<pre class="sg${error ? ' error" data-label="Refused' : ''}"><code>${colour(text.replace(/^\n/, ''), url)}</code></pre>`;
    })
    .replaceAll('{{logo}}', LOGO('big', (target) => link(url, target)));
  write(url, frame({ url, title: 'SG', description: 'SG, a small systems language that makes fast software safe by construction.', section: null, main: home, wide: true }));
}

// The page for an address with no page. Cloudflare Pages serves it, with status 404, at whatever depth the
// address asked for, so its links start at the root of the site instead of at the page.
{
  const url = '404.html';
  const main = `<h1>No page here</h1>
<p class="lead">There is no page at this address. The site was made anew in November 2026, and some addresses
changed with it.</p>
<p>Look for a word, a type or a page with the search at the top (or press <kbd>/</kbd>), or start from
<a href="index.html">the front page</a>, <a href="learn/index.html">Learn</a> or
<a href="reference/index.html">the index of every name</a>.</p>`;
  const html = frame({ url, title: 'No page here', description: 'There is no page at this address.', section: null, main })
    .replace(/(\s(?:href|src|action)=")(?![a-z]+:|\/|#)/g, '$1/')
    .replace('<meta name="sg-root" content="./">', '<meta name="sg-root" content="/">');
  write(url, html);
}

// The search: every name, page and heading.
{
  const entries = [];
  for (const n of names) entries.push([n.name, KIND[n.kind].label, n.summary, n.url]);
  for (const p of pages) {
    if (p.kind === 'word') continue;
    entries.push([p.title, p.section ? sectionOf[p.section].title.toLowerCase() : 'page', p.description, p.url]);
    for (const h of p.headings || []) entries.push([h.text, `in ${p.title}`, '', `${p.url}#${h.id}`]);
  }
  // One entry for one address: a module of the library is also a page.
  const seen = new Set();
  const unique = entries.filter((e) => (seen.has(e[3]) ? false : seen.add(e[3])));
  const json = JSON.stringify(unique).replaceAll('<', '\\u003c').replaceAll('>', '\\u003e').replaceAll('&', '\\u0026');
  writeFileSync(join(out, 'search-index.js'), `// Made by build.mjs: [name, kind, what it is, address].\nself.SG_SEARCH = ${json};\n`);
}

// The files beside the pages.
for (const name of ['style.css', 'site.js', 'theme.js', 'logo-light.webp', 'logo-dark.webp', 'logo-small-light.webp', 'logo-small-dark.webp', 'favicon.png', 'apple-touch-icon.png', 'social-card.png', '_headers', '_redirects']) copyFileSync(join(plain, 'assets', name), join(out, name));

// ---------------------------------------------------------------------------------------------------------
// Checks: every link leads somewhere, and no page carries a script or a style of its own.

const ids = new Map();
for (const [url, html] of written) ids.set(url, new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
for (const [url, html] of written) {
  if (/<script(?![^>]*\ssrc=)/.test(html)) problems.push(`${url}: a script written into the page`);
  if (/\sstyle="/.test(html) || /<style/.test(html)) problems.push(`${url}: a style written into the page`);
  if (/\son[a-z]+="/.test(html)) problems.push(`${url}: an event handler written into the page`);
  for (const [, href] of html.matchAll(/\shref="([^"]+)"/g)) {
    if (/^[a-z]+:/.test(href)) continue;
    const [path, hash] = href.replaceAll('&amp;', '&').split('#');
    const target = path ? posix.normalize(posix.join(posix.dirname(url), path)) : url;
    if (!existsSync(join(out, target))) {
      problems.push(`${url}: a link to \`${href}\`, which is no file`);
      continue;
    }
    if (hash && written.has(target) && !ids.get(target).has(decodeURIComponent(hash))) problems.push(`${url}: a link to \`${href}\`, whose place is not on the page`);
  }
}

const count = written.size;
if (problems.length) {
  console.error([...new Set(problems)].join('\n'));
  console.error(`${problems.length} problem(s); ${count} pages written to build/`);
  process.exit(1);
}
console.log(`${count} pages, ${wordPages.length} pages of words and their manual pages, ${names.length} names in the index, written to build/`);
