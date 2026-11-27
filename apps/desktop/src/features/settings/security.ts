// SPDX-License-Identifier: GPL-3.0-or-later
/** Security preferences stored in the vault settings document `security`. */
export interface SecuritySettings {
  autoLockMinutes: number;
  lockOnSleep: boolean;
  lockOnBackground: boolean;
}

export const DEFAULT_SECURITY_SETTINGS: SecuritySettings = {
  autoLockMinutes: 15,
  lockOnSleep: true,
  lockOnBackground: true,
};
