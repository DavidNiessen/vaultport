// SPDX-License-Identifier: GPL-3.0-or-later
/** Event names emitted by the agent module in the core (`src-tauri/src/agent/mod.rs`). */
export const APPROVAL_EVENT = 'agent://approval';
export const APPROVAL_DONE_EVENT = 'agent://approval-done';
export const AUDIT_EVENT = 'agent://audit';
export const OPEN_TERMINAL_EVENT = 'agent://open-terminal';
/** An agent typed into a terminal: show its tab (payload: the session id). */
export const SHOW_SESSION_EVENT = 'agent://show-session';
