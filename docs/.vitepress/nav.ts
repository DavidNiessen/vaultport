// SPDX-License-Identifier: GPL-3.0-or-later
/** Navigation and sidebar. The sidebar order is also the order of llms.txt and llms-full.txt. */
import type { DefaultTheme } from 'vitepress';

export interface DocSection {
  text: string;
  items: { text: string; link: string }[];
}

export const sections: DocSection[] = [
  {
    text: 'Getting started',
    items: [
      { text: 'Introduction', link: '/guide/' },
      { text: 'Installation', link: '/guide/installation' },
      { text: 'Quick start', link: '/guide/quick-start' },
    ],
  },
  {
    text: 'Concepts',
    items: [
      { text: 'The vault', link: '/concepts/vault' },
      { text: 'Groups & inheritance', link: '/concepts/groups-and-inheritance' },
      { text: 'Identities & keychain', link: '/concepts/identities-and-keychain' },
    ],
  },
  {
    text: 'Guides',
    items: [
      { text: 'The terminal', link: '/guide/terminal' },
      { text: 'Hosts & groups', link: '/guide/hosts-and-groups' },
      { text: 'SSH keys & hardware keys', link: '/guide/keys' },
      { text: 'Known hosts', link: '/guide/known-hosts' },
      { text: 'Jump hosts & proxies', link: '/guide/jump-hosts-and-proxies' },
      { text: 'Port forwarding', link: '/guide/port-forwarding' },
      { text: 'SFTP', link: '/guide/sftp' },
      { text: 'Workspaces & broadcast', link: '/guide/workspaces' },
      { text: 'Snippets & autocomplete', link: '/guide/snippets' },
      { text: 'Mosh', link: '/guide/mosh' },
      { text: 'Telnet', link: '/guide/telnet' },
      { text: 'Serial consoles', link: '/guide/serial' },
      { text: 'Importing', link: '/guide/importing' },
      { text: 'Cloud inventory', link: '/guide/cloud-inventory' },
      { text: 'Sync, backup & pairing', link: '/guide/sync' },
      { text: 'Session logs', link: '/guide/session-logs' },
      { text: 'Local AI', link: '/guide/local-ai' },
      { text: 'AI agents (MCP)', link: '/guide/ai-agents' },
      { text: 'Keyboard shortcuts', link: '/guide/keyboard-shortcuts' },
      { text: 'Phones & tablets', link: '/guide/mobile' },
    ],
  },
  {
    text: 'Security',
    items: [
      { text: 'Overview & threat model', link: '/security/' },
      { text: 'Vault cryptography', link: '/security/vault-crypto' },
      { text: 'Sync protocol', link: '/security/sync' },
      { text: 'Host-key policy', link: '/security/host-keys' },
      { text: 'App isolation', link: '/security/app-isolation' },
      { text: 'Supply chain', link: '/security/supply-chain' },
      { text: 'Reporting a vulnerability', link: '/security/disclosure' },
    ],
  },
  {
    text: 'Development',
    items: [
      { text: 'Build from source', link: '/development/building' },
      { text: 'Architecture', link: '/architecture' },
      { text: 'Contributing', link: '/development/contributing' },
      { text: 'Release process', link: '/development/releasing' },
    ],
  },
  {
    text: 'Help',
    items: [{ text: 'FAQ', link: '/faq' }],
  },
];

export const sidebar: DefaultTheme.Sidebar = {
  '/': sections.map((s) => ({ text: s.text, collapsed: false, items: s.items })),
};

export const nav: DefaultTheme.NavItem[] = [
  { text: 'Docs', link: '/guide/', activeMatch: '^/(guide|concepts|faq)' },
  { text: 'Security', link: '/security/', activeMatch: '^/security/' },
  { text: 'Develop', link: '/development/building', activeMatch: '^/(development|architecture)' },
  { text: 'Download', link: '/download' },
];
