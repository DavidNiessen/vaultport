// SPDX-License-Identifier: GPL-3.0-or-later
// Tauri isolation hook: every IPC message from the UI passes through this sandboxed iframe
// before it is encrypted and handed to the core. It rejects malformed payloads so that a
// compromised frontend dependency cannot smuggle arbitrary data into the IPC layer.
'use strict';

const MAX_PAYLOAD_BYTES = 8 * 1024 * 1024;
const COMMAND_PATTERN = /^(plugin:[a-z0-9-]+\|)?[a-z0-9_]+$/;

window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (payload === null || typeof payload !== 'object') {
    throw new Error('invalid IPC payload');
  }
  if (typeof payload.cmd !== 'string' || !COMMAND_PATTERN.test(payload.cmd)) {
    throw new Error('invalid IPC command');
  }
  try {
    const size = JSON.stringify(payload.payload ?? null).length;
    if (size > MAX_PAYLOAD_BYTES) {
      throw new Error('IPC payload too large');
    }
  } catch (e) {
    if (e instanceof Error && e.message === 'IPC payload too large') throw e;
    // Binary payloads (ArrayBuffer / Uint8Array) cannot be stringified; allow them.
  }
  return payload;
};
