// SPDX-License-Identifier: GPL-3.0-or-later
/** Local AI preferences (vault settings document `ai`). AI is off by default. */
export interface AiSettings {
  enabled: boolean;
  /** Allow attaching selected terminal text as context. */
  useSelection: boolean;
}

export const DEFAULT_AI_SETTINGS: AiSettings = { enabled: false, useSelection: true };
