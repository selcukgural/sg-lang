// The web site of SG, sg-lang.org (choices 1383-1390): a static site built with Docusaurus and
// published by Cloudflare Pages. Its pages are written for the reader; the decisions behind them
// stay in the repository's `design/` and `interpreter/README.md`.
// @ts-check

const { themes: prismThemes } = require('prism-react-renderer');

const repo = 'https://github.com/selcukgural/sg-lang';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'SG',
  tagline: 'As fast and small as C, memory safe like Rust, written like C#.',
  favicon: 'img/favicon.svg',
  url: 'https://sg-lang.org',
  baseUrl: '/',
  trailingSlash: false,
  // Its banner is a script written into the page; the base URL is known, and the CSP allows no such script.
  baseUrlIssueBanner: false,
  onBrokenLinks: 'throw',
  markdown: { hooks: { onBrokenMarkdownLinks: 'throw' } },
  i18n: { defaultLocale: 'en', locales: ['en'] },
  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: `${repo}/edit/main/website/`,
          showLastUpdateTime: false,
        },
        blog: false,
        theme: { customCss: require.resolve('./src/css/custom.css') },
      }),
    ],
  ],
  themes: [
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      /** Search built into the site, with no service outside it (choice 1386). */
      ({ hashed: true, language: ['en'], indexBlog: false, docsRouteBasePath: '/docs', highlightSearchTermsOnTargetPage: true }),
    ],
  ],
  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: { respectPrefersColorScheme: true },
      image: 'img/pasha.png',
      navbar: {
        title: 'SG',
        logo: { alt: 'SG', src: 'img/logo.svg' },
        items: [
          { type: 'docSidebar', sidebarId: 'learn', position: 'left', label: 'Learn' },
          { type: 'docSidebar', sidebarId: 'reference', position: 'left', label: 'Reference' },
          { type: 'docSidebar', sidebarId: 'library', position: 'left', label: 'Library' },
          { to: '/docs/status', label: 'Status', position: 'left' },
          { href: repo, label: 'GitHub', position: 'right' },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          { title: 'Learn', items: [{ label: 'Install', to: '/docs/learn/install' }, { label: 'A tour of SG', to: '/docs/learn/tour' }, { label: 'SG for Rust programmers', to: '/docs/learn/for-rust' }] },
          { title: 'Project', items: [{ label: 'Status', to: '/docs/status' }, { label: 'Source', href: repo }, { label: 'Security', href: `${repo}/blob/main/SECURITY.md` }] },
        ],
        copyright: 'SG is free software under the MIT or Apache 2.0 license. Pasha, the cat of SG.',
      },
      prism: { theme: prismThemes.github, darkTheme: prismThemes.dracula, additionalLanguages: ['rust', 'c', 'bash'] },
    }),
};

module.exports = config;
