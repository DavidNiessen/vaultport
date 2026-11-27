// SPDX-License-Identifier: GPL-3.0-or-later
import { Code2, FolderPlus, Play, Plus, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { Badge, Select, TagInput } from '@/components/ui/controls';
import { Field, Input, Textarea } from '@/components/ui/input';
import { EmptyState } from '@/components/ui/misc';
import { ContextMenu, Dialog } from '@/components/ui/overlay';
import { newId, type Snippet } from '@/lib/ipc';
import { EMPTY } from '@/lib/empty';
import {
  useDeleteSnippet,
  useHosts,
  usePackages,
  useSavePackage,
  useSaveSnippet,
  useSettings,
  useSnippets,
} from '@/lib/queries';
import { useApp } from '@/stores/app';

import { AiGenerateButton } from '../ai/AiGenerateButton';
import { DEFAULT_AI_SETTINGS, type AiSettings } from '../ai/settings';
import { protocolFor } from '../terminal/sessions';

/** Extracts `{{name:default}}` placeholders (mirrors `Snippet::variables` in the core). */
export function snippetVariables(script: string): { name: string; default: string | null }[] {
  const out: { name: string; default: string | null }[] = [];
  const re = /\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?::([^}]*))?\}\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(script)) !== null) {
    const name = m[1] ?? '';
    if (!out.some((v) => v.name === name)) out.push({ name, default: m[2]?.trim() ?? null });
  }
  return out;
}

export function renderSnippet(script: string, values: Record<string, string>): string {
  return script.replace(
    /\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?::([^}]*))?\}\}/g,
    (all, name: string, def?: string) => values[name] ?? def?.trim() ?? all,
  );
}

function SnippetEditor({ snippet, onClose }: { snippet: Snippet; onClose: () => void }) {
  const { t } = useTranslation();
  const packages = usePackages().data ?? EMPTY;
  const save = useSaveSnippet();
  const { settings: ai } = useSettings<AiSettings>('ai', DEFAULT_AI_SETTINGS);
  const [draft, setDraft] = useState(snippet);
  const vars = snippetVariables(draft.script);
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={snippet.createdAt ? t('snippets.edit') : t('snippets.new')}
      className="sm:w-[min(92vw,640px)]"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            disabled={!draft.label.trim() || !draft.script}
            onClick={() => save.mutate(draft, { onSuccess: onClose })}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Field label={t('snippets.description')}>
          {(id) => (
            <div className="flex gap-2">
              <Input
                id={id}
                value={draft.label}
                placeholder={t('snippets.descriptionPlaceholder')}
                onChange={(e) => setDraft({ ...draft, label: e.target.value })}
              />
              {ai.enabled ? (
                <AiGenerateButton
                  mode="snippet"
                  description={draft.label}
                  onResult={(script) => setDraft((d) => ({ ...d, script }))}
                />
              ) : null}
            </div>
          )}
        </Field>
        <Field
          label={t('snippets.script')}
          hint={t('snippets.variablesHint', {
            a: '{{name}}',
            b: '{{name:default}}',
            interpolation: { escapeValue: false },
          })}
        >
          {(id) => (
            <Textarea
              id={id}
              rows={8}
              className="font-mono text-[12px]"
              value={draft.script}
              onChange={(e) => setDraft({ ...draft, script: e.target.value })}
              placeholder="sudo systemctl restart {{service:nginx}}"
            />
          )}
        </Field>
        {vars.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {vars.map((v) => (
              <Badge key={v.name} tone="accent">
                {v.name}
                {v.default ? ` = ${v.default}` : ''}
              </Badge>
            ))}
          </div>
        ) : null}
        <div className="grid grid-cols-2 gap-3">
          <Field label={t('snippets.package')}>
            {(id) => (
              <Select
                id={id}
                value={draft.package ?? ''}
                onChange={(e) => setDraft({ ...draft, package: e.target.value || null })}
              >
                <option value="">{t('common.none')}</option>
                {packages.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t('hosts.tags')}>
            {() => (
              <TagInput
                ariaLabel={t('hosts.tags')}
                value={draft.tags}
                onChange={(tags) => setDraft({ ...draft, tags })}
              />
            )}
          </Field>
        </div>
      </div>
    </Dialog>
  );
}

/** Asks for snippet variables and target hosts, then opens a session per host and runs it. */
export function RunSnippetDialog({
  snippet,
  onClose,
  presetHosts,
}: {
  snippet: Snippet;
  onClose: () => void;
  presetHosts?: string[];
}) {
  const { t } = useTranslation();
  const hosts = useHosts().data ?? EMPTY;
  const save = useSaveSnippet();
  const openTab = useApp((s) => s.openTab);
  const vars = snippetVariables(snippet.script);
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(vars.map((v) => [v.name, v.default ?? ''])),
  );
  const [targets, setTargets] = useState<string[]>(presetHosts ?? snippet.targets);
  const run = () => {
    const script = renderSnippet(snippet.script, values);
    for (const id of targets) {
      const h = hosts.find((x) => x.id === id);
      if (!h) continue;
      openTab({
        id: newId(),
        kind: 'terminal',
        title: h.label,
        hostId: h.id,
        protocol: protocolFor(h),
        sessionId: null,
        status: 'connecting',
        unread: false,
        initialInput: `${script}\r`,
      });
    }
    save.mutate({ ...snippet, targets, runCount: snippet.runCount + 1 });
    onClose();
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={t('snippets.runTitle', { label: snippet.label })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" disabled={targets.length === 0} onClick={run}>
            <Play className="size-4" />
            {t('snippets.runOn', { count: targets.length })}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        {vars.map((v) => (
          <Field key={v.name} label={v.name}>
            {(id) => (
              <Input
                id={id}
                value={values[v.name] ?? ''}
                onChange={(e) => setValues({ ...values, [v.name]: e.target.value })}
              />
            )}
          </Field>
        ))}
        <pre className="bg-surface-2 selectable max-h-40 overflow-auto rounded-xl p-3 font-mono text-[12px] whitespace-pre-wrap">
          {renderSnippet(snippet.script, values)}
        </pre>
        <div>
          <p className="text-muted mb-2 text-[12px] font-medium">{t('snippets.targets')}</p>
          <div className="grid max-h-60 gap-1 overflow-y-auto">
            {hosts.map((h) => (
              <label
                key={h.id}
                className="hover:bg-surface-2 flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-[13px]"
              >
                <input
                  type="checkbox"
                  className="accent-[var(--accent)]"
                  checked={targets.includes(h.id)}
                  onChange={(e) =>
                    setTargets(
                      e.target.checked ? [...targets, h.id] : targets.filter((x) => x !== h.id),
                    )
                  }
                />
                <OsIcon os={h.os} size="sm" />
                {h.label}
              </label>
            ))}
          </div>
        </div>
      </div>
    </Dialog>
  );
}

export function SnippetsView() {
  const { t } = useTranslation();
  const snippets = useSnippets().data ?? EMPTY;
  const packages = usePackages().data ?? EMPTY;
  const savePackage = useSavePackage();
  const del = useDeleteSnippet();
  const [pkg, setPkg] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<Snippet | null>(null);
  const [running, setRunning] = useState<Snippet | null>(null);
  const filtered = useMemo(
    () =>
      snippets
        .filter((s) => (pkg ? s.package === pkg : true))
        .filter(
          (s) =>
            !query ||
            s.label.toLowerCase().includes(query.toLowerCase()) ||
            s.script.toLowerCase().includes(query.toLowerCase()),
        )
        .sort((a, b) => b.runCount - a.runCount || a.label.localeCompare(b.label)),
    [snippets, pkg, query],
  );
  const blank = (): Snippet => ({
    id: newId(),
    label: '',
    script: '',
    package: pkg,
    tags: [],
    targets: [],
    runCount: 0,
    createdAt: 0,
    updatedAt: 0,
  });

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 px-4 pt-5 sm:px-6">
        <Input
          className="max-w-xs"
          aria-label={t('common.search')}
          placeholder={t('snippets.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="ml-auto flex gap-2">
          <Button
            size="sm"
            onClick={() => {
              const label = window.prompt(t('snippets.packageName'));
              if (label) savePackage.mutate({ id: newId(), label, createdAt: 0 });
            }}
          >
            <FolderPlus className="size-4" />
            {t('snippets.newPackage')}
          </Button>
          <Button variant="primary" size="sm" onClick={() => setEditing(blank())}>
            <Plus className="size-4" />
            <span className="font-semibold tracking-wide uppercase">{t('snippets.new')}</span>
          </Button>
        </div>
      </div>
      {packages.length > 0 ? (
        <div className="flex gap-1.5 overflow-x-auto px-4 pt-3 sm:px-6">
          <button type="button" onClick={() => setPkg(null)}>
            <Badge tone={pkg === null ? 'accent' : 'neutral'}>{t('snippets.all')}</Badge>
          </button>
          {packages.map((p) => (
            <button key={p.id} type="button" onClick={() => setPkg(p.id)}>
              <Badge tone={pkg === p.id ? 'accent' : 'neutral'}>{p.label}</Badge>
            </button>
          ))}
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
        {filtered.length === 0 ? (
          <EmptyState
            icon={<Code2 />}
            title={t('snippets.empty')}
            description={t('snippets.emptyDescription', {
              example: '{{name:default}}',
              interpolation: { escapeValue: false },
            })}
            action={
              <Button variant="primary" onClick={() => setEditing(blank())}>
                {t('snippets.new')}
              </Button>
            }
          />
        ) : (
          <div className="grid grid-cols-1 gap-2.5 lg:grid-cols-2 2xl:grid-cols-3">
            {filtered.map((s) => (
              <ContextMenu
                key={s.id}
                items={[
                  { label: t('snippets.run'), icon: <Play />, onSelect: () => setRunning(s) },
                  { label: t('common.edit'), onSelect: () => setEditing(s) },
                  { separator: true },
                  {
                    label: t('common.delete'),
                    icon: <Trash2 />,
                    danger: true,
                    onSelect: () => del.mutate(s.id),
                  },
                ]}
              >
                <div className="bg-surface-2 group flex flex-col gap-2 rounded-[var(--radius-card)] p-3.5">
                  <div className="flex items-center gap-2">
                    <span className="text-accent min-w-0 flex-1 truncate font-medium">
                      {s.label}
                    </span>
                    {s.runCount > 0 ? (
                      <span className="text-subtle text-[11px]">
                        {t('snippets.runs', { count: s.runCount })}
                      </span>
                    ) : null}
                  </div>
                  <pre className="text-muted line-clamp-2 font-mono text-[11.5px] whitespace-pre-wrap">
                    {s.script}
                  </pre>
                  <div className="flex gap-1.5">
                    <Button size="sm" variant="primary" onClick={() => setRunning(s)}>
                      <Play className="size-3.5" />
                      {t('snippets.run')}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setEditing(s)}>
                      {t('common.edit')}
                    </Button>
                    {s.tags.map((tag) => (
                      <Badge key={tag} className="self-center">
                        {tag}
                      </Badge>
                    ))}
                  </div>
                </div>
              </ContextMenu>
            ))}
          </div>
        )}
      </div>
      {editing ? <SnippetEditor snippet={editing} onClose={() => setEditing(null)} /> : null}
      {running ? <RunSnippetDialog snippet={running} onClose={() => setRunning(null)} /> : null}
    </div>
  );
}
