// SPDX-License-Identifier: GPL-3.0-or-later
import { defineConfig, type HeadConfig } from 'vitepress';

import { generateAiFiles } from './llms';
import { nav, sidebar } from './nav';
import { DESCRIPTION, NAME, REPO_URL, TAGLINE } from './site';

/** `/ssh-client/` on GitHub Pages; override with DOCS_BASE (e.g. `/` for a custom domain). */
function normalizeBase(value: string): string {
  const trimmed = value.trim().replace(/^\/+|\/+$/g, '');
  return trimmed ? `/${trimmed}/` : '/';
}

const base = normalizeBase(process.env.DOCS_BASE ?? '/ssh-client/');
const origin = (process.env.DOCS_ORIGIN ?? 'https://davidniessen.github.io').replace(/\/+$/, '');
const siteUrl = `${origin}${base}`;
const ogImage = `${siteUrl}og.png`;

const jsonLd = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: NAME,
  description: DESCRIPTION,
  url: siteUrl,
  downloadUrl: `${siteUrl}download`,
  image: ogImage,
  applicationCategory: 'DeveloperApplication',
  applicationSubCategory: 'SSH client',
  operatingSystem: 'Windows, macOS, Linux, iOS, iPadOS, Android',
  license: 'https://spdx.org/licenses/GPL-3.0-or-later.html',
  isAccessibleForFree: true,
  offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
  codeRepository: REPO_URL,
  featureList: [
    'SSH, SFTP, Mosh, Telnet and serial sessions',
    'Jump hosts, HTTP CONNECT and SOCKS5 proxies',
    'Local, remote and dynamic port forwarding',
    'End-to-end encrypted vault (XChaCha20-Poly1305, Argon2id)',
    'Serverless sync through a folder, WebDAV or S3 storage you own',
    'FIDO2 security keys, Secure Enclave and Windows Hello keys',
    'Post-quantum key exchange (mlkem768x25519-sha256)',
    'Optional on-device AI command generation, off by default',
  ],
};

export default defineConfig({
  lang: 'en-US',
  title: NAME,
  titleTemplate: `:title · ${NAME}`,
  description: DESCRIPTION,
  base,
  cleanUrls: true,
  lastUpdated: true,
  appearance: 'dark',
  metaChunk: true,
  sitemap: { hostname: siteUrl },

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}logo.svg` }],
    ['link', { rel: 'icon', type: 'image/png', sizes: '32x32', href: `${base}favicon-32.png` }],
    ['link', { rel: 'apple-touch-icon', sizes: '180x180', href: `${base}apple-touch-icon.png` }],
    ['meta', { name: 'theme-color', content: '#0b0d14' }],
    ['meta', { name: 'color-scheme', content: 'dark light' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: NAME }],
    ['meta', { property: 'og:image', content: ogImage }],
    ['meta', { property: 'og:image:width', content: '1200' }],
    ['meta', { property: 'og:image:height', content: '630' }],
    ['meta', { property: 'og:image:alt', content: `${NAME}: ${TAGLINE}` }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:image', content: ogImage }],
    ['link', { rel: 'alternate', type: 'text/plain', title: 'llms.txt', href: `${base}llms.txt` }],
  ],

  transformHead({ pageData, title, description }) {
    if (pageData.isNotFound || pageData.relativePath === '404.md') return [];
    const route = pageData.relativePath.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, '');
    const url = `${siteUrl}${route}`;
    const head: HeadConfig[] = [
      ['link', { rel: 'canonical', href: url }],
      [
        'link',
        { rel: 'alternate', type: 'text/markdown', href: `${base}${pageData.relativePath}` },
      ],
      ['meta', { property: 'og:title', content: title }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: url }],
      ['meta', { name: 'twitter:title', content: title }],
      ['meta', { name: 'twitter:description', content: description }],
    ];
    if (pageData.relativePath === 'index.md') {
      head.push(['script', { type: 'application/ld+json' }, JSON.stringify(jsonLd)]);
    }
    return head;
  },

  async buildEnd(config) {
    await generateAiFiles(config, siteUrl);
  },

  markdown: {
    theme: { light: 'github-light', dark: 'github-dark' },
    config(md) {
      // Snippet placeholders such as `{{name:default}}` must not be read as Vue interpolation.
      const renderInlineCode = md.renderer.rules.code_inline;
      md.renderer.rules.code_inline = (tokens, idx, options, env, self) => {
        tokens[idx]?.attrSet('v-pre', '');
        return renderInlineCode
          ? renderInlineCode(tokens, idx, options, env, self)
          : self.renderToken(tokens, idx, options);
      };
    },
  },

  themeConfig: {
    logo: { src: '/logo.svg', alt: '' },
    siteTitle: NAME,
    nav,
    sidebar,
    outline: { level: [2, 3], label: 'On this page' },
    socialLinks: [{ icon: 'github', link: REPO_URL, ariaLabel: 'ssh-client on GitHub' }],
    editLink: {
      pattern: `${REPO_URL}/edit/main/docs/:path`,
      text: 'Edit this page on GitHub',
    },
    lastUpdated: {
      text: 'Last updated',
      formatOptions: { dateStyle: 'medium' },
    },
    search: { provider: 'local' },
    docFooter: { prev: 'Previous', next: 'Next' },
    externalLinkIcon: true,
  },
});
