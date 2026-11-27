// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Build-time generation of machine-readable companions of the site:
 *
 * - a clean Markdown copy of every page next to its HTML (`/guide/sftp.md`),
 * - `/llms.txt` (https://llmstxt.org) and `/llms-full.txt` (all pages concatenated),
 * - `/robots.txt` pointing at the sitemap.
 *
 * Runs from the VitePress `buildEnd` hook; no third-party plugin involved.
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import type { SiteConfig } from 'vitepress';

import { sections } from './nav';
import { DESCRIPTION, NAME, RELEASES_URL, REPO_URL } from './site';

interface Frontmatter {
  title?: string;
  description?: string;
  llms?: boolean;
}

export interface MarkdownPage {
  /** Source path relative to the docs root, e.g. `guide/sftp.md`. */
  file: string;
  /** Site route without base, e.g. `/guide/sftp`. */
  route: string;
  title: string;
  description: string;
  include: boolean;
  markdown: string;
}

/** Parses the small subset of YAML used in this site's frontmatter (`key: value` pairs). */
export function splitFrontmatter(source: string): { data: Frontmatter; body: string } {
  const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(source);
  if (!match) return { data: {}, body: source };
  const data: Record<string, string | boolean> = {};
  for (const line of (match[1] ?? '').split(/\r?\n/)) {
    const kv = /^([A-Za-z][\w-]*):\s*(.*)$/.exec(line);
    if (!kv?.[1]) continue;
    let value = (kv[2] ?? '').trim();
    if (/^(['"]).*\1$/.test(value)) value = value.slice(1, -1).replace(/''/g, "'");
    data[kv[1]] = value === 'false' ? false : value === 'true' ? true : value;
  }
  return { data: data as Frontmatter, body: source.slice(match[0].length) };
}

/** Maps a source file (`guide/index.md`) to its route (`/guide/`). */
export function routeOf(file: string): string {
  return '/' + file.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, '');
}

/** Maps a route (`/guide/`, `/guide/sftp#x`) to the URL of its Markdown copy. */
export function markdownUrl(route: string, siteUrl: string): string {
  const [pathname = '/', hash] = route.split('#');
  let file = pathname.endsWith('/') ? `${pathname}index.md` : pathname;
  if (!/\.[a-z0-9]+$/i.test(file)) file += '.md';
  return siteUrl.replace(/\/$/, '') + file + (hash ? `#${hash}` : '');
}

const CONTAINER_LABEL: Record<string, string> = {
  tip: 'Tip',
  info: 'Note',
  warning: 'Warning',
  danger: 'Important',
  details: 'Details',
};

/** Turns VitePress-flavoured Markdown into portable CommonMark/GFM. */
export function toPortableMarkdown(body: string, siteUrl: string): string {
  const out: string[] = [];
  let inFence = false;
  let quote = false;
  for (const raw of body.split(/\r?\n/)) {
    const line = raw;
    if (/^\s*(```|~~~)/.test(line)) {
      inFence = !inFence;
      // Code-group tabs (```json [Title]) become a bold label above a plain fence.
      const tab = /^(\s*(?:```|~~~)[\w-]*)\s+\[([^\]]+)\]\s*$/.exec(line);
      const lines = tab ? [`**${tab[2] ?? ''}**`, '', tab[1] ?? ''] : [line];
      for (const l of lines) out.push(quote ? `> ${l}`.trimEnd() : l);
      continue;
    }
    if (inFence) {
      out.push(quote ? `> ${line}` : line);
      continue;
    }
    const open = /^:::\s*(tip|info|warning|danger|details|code-group|v-pre)\s*(.*)$/.exec(line);
    if (open) {
      const kind = open[1] ?? '';
      if (kind in CONTAINER_LABEL) {
        const label = (open[2] ?? '').trim() || CONTAINER_LABEL[kind];
        out.push(`> **${label}**`, '>');
        quote = true;
      }
      continue;
    }
    if (/^:::\s*$/.test(line)) {
      quote = false;
      continue;
    }
    // Vue components, SFC blocks and layout wrappers have no meaning outside the site.
    if (/^\s*<\/?[A-Z][\w-]*(\s[^>]*)?\/?>\s*$/.test(line)) continue;
    if (/^\s*<\/?div(\s[^>]*)?>\s*$/.test(line)) continue;
    let text = line
      .replace(/\s*\{#[\w-]+\}\s*$/, '')
      .replace(/\]\((\/[^)\s]*)\)/g, (_m, href: string) => `](${markdownUrl(href, siteUrl)})`);
    if (text.trim() === '[[toc]]') continue;
    if (quote) text = text ? `> ${text}` : '>';
    out.push(text);
  }
  return (
    out
      .join('\n')
      .replace(/<script[\s\S]*?<\/script>\s*/g, '')
      .replace(/<style[\s\S]*?<\/style>\s*/g, '')
      .replace(/\n{3,}/g, '\n\n')
      .trim() + '\n'
  );
}

async function loadPages(config: SiteConfig, siteUrl: string): Promise<MarkdownPage[]> {
  const pages = await Promise.all(
    config.pages.map(async (file): Promise<MarkdownPage> => {
      const source = await readFile(path.join(config.srcDir, file), 'utf8');
      const { data, body } = splitFrontmatter(source);
      let markdown = toPortableMarkdown(body, siteUrl);
      // The H1, if any, is the first line (headings inside code blocks do not count).
      const h1 = /^#\s+(.+)\n/.exec(markdown)?.[1]?.trim();
      const title = data.title ?? h1 ?? NAME;
      if (!h1) {
        const intro = data.description ? `${data.description}\n\n` : '';
        markdown = `# ${title}\n\n${intro}${markdown}`;
      }
      return {
        file,
        route: routeOf(file),
        title,
        description: data.description ?? '',
        include: data.llms !== false,
        markdown,
      };
    }),
  );
  return pages.sort((a, b) => a.file.localeCompare(b.file));
}

function orderPages(pages: MarkdownPage[]): MarkdownPage[] {
  const byRoute = new Map(pages.map((p) => [p.route, p]));
  const ordered: MarkdownPage[] = [];
  for (const section of sections) {
    for (const item of section.items) {
      const page = byRoute.get(item.link);
      if (page) ordered.push(page);
    }
  }
  for (const page of pages) if (!ordered.includes(page)) ordered.push(page);
  return ordered.filter((p) => p.include);
}

export function buildLlmsTxt(pages: MarkdownPage[], siteUrl: string): string {
  const byRoute = new Map(pages.map((p) => [p.route, p]));
  const entry = (route: string, text: string) => {
    const page = byRoute.get(route);
    const desc = page?.description ? `: ${page.description}` : '';
    return `- [${text}](${markdownUrl(route, siteUrl)})${desc}`;
  };
  const lines = [
    `# ${NAME}`,
    '',
    `> ${DESCRIPTION}`,
    '',
    `${NAME} is a desktop and mobile application built with Tauri 2, a Rust core and a React UI, licensed GPL-3.0-or-later. There is no project-operated server: hosts, keys and passwords live in a local vault encrypted with XChaCha20-Poly1305 (keys wrapped with Argon2id and/or the OS key store), and multi-device sync goes through a folder, WebDAV or S3 storage the user owns. Every page below is also available as HTML by dropping the \`.md\` suffix; [llms-full.txt](${siteUrl}llms-full.txt) contains all of them in one file.`,
    '',
  ];
  for (const section of sections) {
    lines.push(`## ${section.text}`, '');
    for (const item of section.items) lines.push(entry(item.link, item.text));
    lines.push('');
  }
  lines.push(
    '## Optional',
    '',
    entry('/download', 'Download'),
    `- [Source code](${REPO_URL}): Rust crates under \`crates/\`, the Tauri app under \`apps/desktop/\``,
    `- [Releases](${RELEASES_URL}): installers, checksums, signatures and SBOMs`,
    '',
  );
  return lines.join('\n');
}

export function buildLlmsFullTxt(pages: MarkdownPage[], siteUrl: string): string {
  const header = [`# ${NAME} documentation`, '', `> ${DESCRIPTION}`, ''];
  const body = pages.map((p) =>
    [`<!-- Source: ${siteUrl.replace(/\/$/, '')}${p.route} -->`, '', p.markdown.trim()].join('\n'),
  );
  return [...header, body.join('\n\n---\n\n'), ''].join('\n');
}

export function buildRobotsTxt(siteUrl: string): string {
  const aiCrawlers = [
    'GPTBot',
    'OAI-SearchBot',
    'ChatGPT-User',
    'ClaudeBot',
    'Claude-User',
    'Claude-SearchBot',
    'anthropic-ai',
    'PerplexityBot',
    'Google-Extended',
    'Applebot-Extended',
    'CCBot',
    'Meta-ExternalAgent',
    'cohere-ai',
  ];
  return [
    '# Everyone, including AI crawlers, is welcome to read and index this site.',
    'User-agent: *',
    'Allow: /',
    '',
    ...aiCrawlers.flatMap((ua) => [`User-agent: ${ua}`, 'Allow: /', '']),
    `Sitemap: ${siteUrl}sitemap.xml`,
    '',
  ].join('\n');
}

/** Writes Markdown copies, llms.txt, llms-full.txt and robots.txt into the build output. */
export async function generateAiFiles(config: SiteConfig, siteUrl: string): Promise<void> {
  const pages = await loadPages(config, siteUrl);
  const llmsTxt = buildLlmsTxt(pages, siteUrl);
  await Promise.all(
    pages.map(async (page) => {
      const target = path.join(config.outDir, page.file);
      await mkdir(path.dirname(target), { recursive: true });
      // The landing page is made of components; its Markdown twin is the site index instead.
      await writeFile(target, page.route === '/' ? llmsTxt : page.markdown);
    }),
  );
  const ordered = orderPages(pages);
  await writeFile(path.join(config.outDir, 'llms.txt'), llmsTxt);
  await writeFile(path.join(config.outDir, 'llms-full.txt'), buildLlmsFullTxt(ordered, siteUrl));
  await writeFile(path.join(config.outDir, 'robots.txt'), buildRobotsTxt(siteUrl));
}
