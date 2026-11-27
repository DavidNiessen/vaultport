// SPDX-License-Identifier: GPL-3.0-or-later
import { FolderOpen, Trash2, Zap } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Field, Input } from '@/components/ui/input';
import { Card } from '@/components/ui/misc';
import type { Group } from '@/lib/ipc';
import { EMPTY } from '@/lib/empty';
import { useBreakpoint } from '@/lib/platform';
import { useDeleteGroup, useGroups, useHosts, useSaveGroup } from '@/lib/queries';
import { useApp } from '@/stores/app';

import { useSessions } from '../terminal/sessions';
import { ConfigEditor, GroupSelect, InspectorShell } from './HostInspector';
import { hostsInView } from './model';
import { useAutoSave } from './useAutoSave';

export function GroupInspector({ group }: { group: Group }) {
  const { t } = useTranslation();
  const bp = useBreakpoint();
  const auto = bp !== 'phone';
  const save = useSaveGroup();
  const del = useDeleteGroup();
  const hosts = useHosts().data ?? EMPTY;
  const groups = useGroups().data ?? EMPTY;
  const { select, setCurrentGroup } = useApp();
  const sessions = useSessions();
  const { draft, update, dirty, flush } = useAutoSave(group, (g) => save.mutate(g), { auto });
  const members = hostsInView(hosts, groups, group.id, '', []);

  return (
    <InspectorShell
      title={t('hosts.groupDetails')}
      onClose={() => select(null)}
      footer={
        <>
          {!auto ? (
            <Button className="flex-1" disabled={!dirty} onClick={flush}>
              {t('common.save')}
            </Button>
          ) : null}
          <Button className="flex-1" onClick={() => setCurrentGroup(group.id)}>
            <FolderOpen className="size-4" />
            {t('hosts.openGroup')}
          </Button>
          <Button
            variant="primary"
            className="flex-1"
            disabled={members.length === 0}
            onClick={() => members.forEach((h) => sessions.connectHost(h))}
          >
            <Zap className="size-4" />
            {t('hosts.quickConnect')}
          </Button>
        </>
      }
    >
      <Card title={t('hosts.general')}>
        <div className="grid gap-3">
          <Field label={t('hosts.label')}>
            {(id) => (
              <Input
                id={id}
                value={draft.label}
                onChange={(e) => update((d) => ({ ...d, label: e.target.value }))}
              />
            )}
          </Field>
          <Field label={t('hosts.parentGroup')}>
            {(id) => (
              <GroupSelect
                id={id}
                exclude={draft.id}
                value={draft.parent}
                onChange={(p) => update((d) => ({ ...d, parent: p }))}
              />
            )}
          </Field>
          <p className="text-subtle text-xs">
            {t('hosts.groupInheritanceHint', { count: members.length })}
          </p>
        </div>
      </Card>
      <ConfigEditor
        selfId={draft.id}
        config={draft.config}
        inheritedHint={Boolean(draft.parent)}
        onChange={(fn) => update((d) => ({ ...d, config: fn(d.config) }))}
      />
      <Button
        variant="ghost"
        className="text-danger self-start"
        onClick={() => {
          del.mutate(draft.id);
          select(null);
        }}
      >
        <Trash2 className="size-4" />
        {t('hosts.deleteGroup')}
      </Button>
    </InspectorShell>
  );
}
