// The web site of SG, sg-lang.org (choices 1383-1390): a static site built with Docusaurus and
// published by Cloudflare Pages. Its pages are written for the reader; the decisions behind them
// stay in the repository's `design/` and `design/decisions/`.
// @ts-check

const { themes: prismThemes } = require('prism-react-renderer');

const repo = 'https://github.com/selcukgural/sg-lang';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'SG',
  tagline: 'Plain to read. Fast to run. Memory safe by design.',
  favicon: 'img/favicon.svg',
  // The icons of the logo (choice 1405), made by brand/src/build.py: the S-cat in white on a navy tile.
  headTags: [
    { tagName: 'link', attributes: { rel: 'icon', href: '/favicon.ico', sizes: '48x48' } },
    { tagName: 'link', attributes: { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' } },
    { tagName: 'link', attributes: { rel: 'manifest', href: '/site.webmanifest' } },
    { tagName: 'meta', attributes: { name: 'theme-color', content: '#021A50' } },
  ],
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
      image: 'img/social-card.png',
      navbar: {
        title: 'SG',
        logo: { alt: 'SG', src: 'img/logo.svg', srcDark: 'img/logo-dark.svg' },
        items: [
          { type: 'docSidebar', sidebarId: 'learn', position: 'left', label: 'Learn' },
          { type: 'docSidebar', sidebarId: 'reference', position: 'left', label: 'Reference' },
          { type: 'docSidebar', sidebarId: 'library', position: 'left', label: 'Library' },
          { type: 'docSidebar', sidebarId: 'benchmarks', position: 'left', label: 'Benchmarks' },
          { to: '/docs/status', label: 'Status', position: 'left' },
          { type: 'docSidebar', sidebarId: 'about', position: 'left', label: 'About' },
          { to: '/docs/about/community', label: 'Community', position: 'right' },
          { href: repo, label: 'GitHub', position: 'right' },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          { title: 'Learn', items: [{ label: 'Install', to: '/docs/learn/install' }, { label: 'A tour of SG', to: '/docs/learn/tour' }, { label: 'SG for Rust programmers', to: '/docs/learn/for-rust' }] },
          { title: 'About', items: [{ label: 'Why SG', to: '/docs/about/why' }, { label: 'How it got here', to: '/docs/about/history' }, { label: 'The repository', to: '/docs/about/repository' }, { label: 'Community', to: '/docs/about/community' }] },
          { title: 'Project', items: [{ label: 'Status', to: '/docs/status' }, { label: 'Benchmarks', to: '/docs/benchmarks' }, { label: 'Source', href: repo }, { label: 'Mailing list', href: 'https://groups.google.com/g/sg-lang' }, { label: 'Discussions', href: `${repo}/discussions` }, { label: 'Security', href: `${repo}/blob/main/SECURITY.md` }] },
        ],
        copyright: 'SG is free software under the MIT or Apache 2.0 license. Pasha, the cat of SG.',
      },
      prism: { theme: prismThemes.github, darkTheme: prismThemes.dracula, additionalLanguages: ['rust', 'c', 'bash'] },
    }),
};

module.exports = config;
