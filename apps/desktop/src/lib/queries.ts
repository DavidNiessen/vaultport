// SPDX-License-Identifier: GPL-3.0-or-later
/** React Query hooks over the core's entity commands, invalidated by vault change events. */
import {
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from '@tanstack/react-query';
import { useEffect } from 'react';
import { toast } from 'sonner';

import {
  commands,
  errorMessage,
  onVaultChanged,
  type Group,
  type Host,
  type Identity,
  type KeyMeta,
  type KnownHost,
  type PortForwardRule,
  type Snippet,
  type SnippetPackage,
  type WorkspaceTemplate,
} from './ipc';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false },
    mutations: {
      onError: (e) => {
        toast.error(errorMessage(e));
      },
    },
  },
});

const kindToKey: Record<string, string[]> = {
  host: ['hosts'],
  group: ['groups'],
  identity: ['identities'],
  key: ['keys'],
  snippet: ['snippets'],
  package: ['packages'],
  forward: ['forwards'],
  knownHost: ['knownHosts'],
  workspace: ['workspaces'],
  settings: ['settings'],
  history: ['history'],
};

/** Keeps the query cache in sync with changes made anywhere (other windows, sync, imports). */
export function useVaultEvents(enabled: boolean): void {
  const qc = useQueryClient();
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onVaultChanged((e) => {
      const key = kindToKey[e.kind];
      void qc.invalidateQueries(key ? { queryKey: key } : undefined);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [qc, enabled]);
}

function useList<T>(key: string, fn: () => Promise<T[]>): UseQueryResult<T[]> {
  return useQuery({ queryKey: [key], queryFn: fn });
}

export const useHosts = () => useList<Host>('hosts', () => commands.listHosts());
export const useGroups = () => useList<Group>('groups', () => commands.listGroups());
export const useIdentities = () => useList<Identity>('identities', () => commands.listIdentities());
export const useKeys = () => useList<KeyMeta>('keys', () => commands.listKeys());
export const useSnippets = () => useList<Snippet>('snippets', () => commands.listSnippets());
export const usePackages = () => useList<SnippetPackage>('packages', () => commands.listPackages());
export const useForwards = () =>
  useList<PortForwardRule>('forwards', () => commands.listForwards());
export const useKnownHosts = () =>
  useList<KnownHost>('knownHosts', () => commands.listKnownHosts());
export const useWorkspaces = () =>
  useList<WorkspaceTemplate>('workspaces', () => commands.listWorkspaces());

function useSave<T>(key: string, fn: (e: T) => Promise<T>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => qc.invalidateQueries({ queryKey: [key] }),
  });
}

function useDelete(key: string, fn: (id: string) => Promise<boolean>, also: string[] = []) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: [key] });
      for (const k of also) await qc.invalidateQueries({ queryKey: [k] });
    },
  });
}

export const useSaveHost = () => useSave<Host>('hosts', (e) => commands.saveHost(e));
export const useDeleteHost = () =>
  useDelete('hosts', (id) => commands.deleteHost(id), ['forwards']);
export const useSaveGroup = () => useSave<Group>('groups', (e) => commands.saveGroup(e));
export const useDeleteGroup = () =>
  useDelete('groups', (id) => commands.deleteGroup(id), ['hosts']);
export const useSaveIdentity = () =>
  useSave<Identity>('identities', (e) => commands.saveIdentity(e));
export const useDeleteIdentity = () => useDelete('identities', (id) => commands.deleteIdentity(id));
export const useSaveKey = () => useSave<KeyMeta>('keys', (e) => commands.saveKeyMeta(e));
export const useDeleteKey = () => useDelete('keys', (id) => commands.deleteKeyMeta(id));
export const useSaveSnippet = () => useSave<Snippet>('snippets', (e) => commands.saveSnippet(e));
export const useDeleteSnippet = () => useDelete('snippets', (id) => commands.deleteSnippet(id));
export const useSavePackage = () =>
  useSave<SnippetPackage>('packages', (e) => commands.savePackage(e));
export const useDeletePackage = () => useDelete('packages', (id) => commands.deletePackage(id));
export const useSaveForward = () =>
  useSave<PortForwardRule>('forwards', (e) => commands.saveForward(e));
export const useDeleteForward = () => useDelete('forwards', (id) => commands.deleteForward(id));
export const useSaveKnownHost = () =>
  useSave<KnownHost>('knownHosts', (e) => commands.saveKnownHost(e));
export const useDeleteKnownHost = () =>
  useDelete('knownHosts', (id) => commands.deleteKnownHost(id));
export const useSaveWorkspace = () =>
  useSave<WorkspaceTemplate>('workspaces', (e) => commands.saveWorkspace(e));
export const useDeleteWorkspace = () =>
  useDelete('workspaces', (id) => commands.deleteWorkspace(id));

/** Typed settings document stored (encrypted) in the vault. */
export function useSettings<T extends object>(name: string, defaults: T) {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: ['settings', name],
    queryFn: async () => {
      const v = await commands.getSettings(name);
      return { ...defaults, ...((v ?? {}) as Partial<T>) };
    },
  });
  const mutation = useMutation({
    mutationFn: async (value: T) => {
      await commands.saveSettings(name, value, false);
      return value;
    },
    onMutate: (value) => {
      qc.setQueryData(['settings', name], value);
    },
  });
  return {
    settings: query.data ?? defaults,
    loading: query.isLoading,
    update: (patch: Partial<T>) => {
      mutation.mutate({ ...(query.data ?? defaults), ...patch });
    },
  };
}
