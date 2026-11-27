// SPDX-License-Identifier: GPL-3.0-or-later
/** Session log preferences stored in the vault settings document `logging` (read by the core). */
export interface LoggingSettings {
  enabled: boolean;
}

export const DEFAULT_LOGGING_SETTINGS: LoggingSettings = {
  enabled: true,
};
