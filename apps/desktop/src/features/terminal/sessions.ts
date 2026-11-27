// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Session orchestration: opening tabs for hosts, quick connections, local shells and SFTP.
 * The actual connection is established by the TerminalView when it mounts (it owns the xterm
 * instance and the output channel).
 */
import { useMemo } from 'react';

import type { Host } from '@/lib/ipc';
import { newId } from '@/lib/ipc';
import { SFTP_TAB, useApp, type TerminalTab } from '@/stores/app';

import type { QuickConnect } from '../hosts/quickConnect';
import { useSftpStore } from '../sftp/store';

export interface SessionActions {
  connectHost: (h: Host) => void;
  connectQuick: (q: QuickConnect) => void;
  openLocal: () => void;
  openSftp: (h: Host) => void;
  openSerialPicker: () => void;
}

export function protocolFor(h: Host): TerminalTab['protocol'] {
  if (h.config.mosh.enabled) return 'mosh';
  if (h.config.telnet.enabled && h.config.ssh.enabled === false) return 'telnet';
  return 'ssh';
}

export function useSessions(): SessionActions {
  const openTab = useApp((s) => s.openTab);
  const activate = useApp((s) => s.activate);
  return useMemo(
    () => ({
      connectHost: (h) => {
        openTab({
          id: newId(),
          kind: 'terminal',
          title: h.label || h.address,
          hostId: h.id,
          protocol: protocolFor(h),
          sessionId: null,
          status: 'connecting',
          unread: false,
        });
      },
      connectQuick: (q) => {
        openTab({
          id: newId(),
          kind: 'terminal',
          title: `${q.username ? `${q.username}@` : ''}${q.host}`,
          hostId: null,
          protocol: 'ssh',
          sessionId: null,
          status: 'connecting',
          unread: false,
          quick: { host: q.host, port: q.port, username: q.username },
        });
      },
      openLocal: () => {
        openTab({
          id: newId(),
          kind: 'terminal',
          title: 'Local',
          hostId: null,
          protocol: 'local',
          sessionId: null,
          status: 'connecting',
          unread: false,
        });
      },
      openSftp: (h) => {
        useSftpStore.getState().setPane('right', { kind: 'remote', hostId: h.id, path: null });
        activate(SFTP_TAB);
      },
      openSerialPicker: () => {
        window.dispatchEvent(new CustomEvent('ssh-client:serial-picker'));
      },
    }),
    [openTab, activate],
  );
}
