// SPDX-License-Identifier: GPL-3.0-or-later
/** VitePress' local search index: one lazy loader per locale (a serialised MiniSearch index). */
declare module '@localSearchIndex' {
  const data: Record<string, (() => Promise<{ default: string }>) | undefined>;
  export default data;
}
