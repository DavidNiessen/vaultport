// SPDX-License-Identifier: GPL-3.0-or-later
import { create } from 'zustand';

export type PaneSource =
  { kind: 'local'; path: string | null } | { kind: 'remote'; hostId: string; path: string | null };

interface SftpState {
  left: PaneSource;
  right: PaneSource | null;
  setPane: (side: 'left' | 'right', source: PaneSource | null) => void;
}

export const useSftpStore = create<SftpState>()((set) => ({
  left: { kind: 'local', path: null },
  right: null,
  setPane: (side, source) => {
    if (side === 'left' && source) set({ left: source });
    else if (side === 'right') set({ right: source });
  },
}));
