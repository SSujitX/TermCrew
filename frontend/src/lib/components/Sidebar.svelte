<script lang="ts">
  import type { AgentMeta, Session } from '../types';
  import { cn, displayHomePath } from '../ui';
  import { X, Plus, ChevronRight, ChevronDown, Pencil, FolderOpen } from 'lucide-svelte';
  import FileTree from './FileTree.svelte';
  import AgentMark from './AgentMark.svelte';
  import { getStorageInfo, openStorageFolder, openFolder } from '../api';
  import { onMount } from 'svelte';

  interface Props {
    agents?: AgentMeta[];
    sessions: Session[];
    selectedSessionId: string | null;
    selectedGroupId: string | null;
    /** Folder shown by the Files tab — focused session's working dir. */
    fileTreeRoot: string | null;
    /** Called when a file in the Files tab is clicked — opens the editor. */
    onOpenFile?: (path: string) => void;
    onDeleted?: (path: string) => void;
    onSelectSession: (id: string) => void;
    onSelectGroup: (groupId: string) => void;
    onKillSession: (id: string) => void;
    onKillGroup: (groupId: string) => void;
    onRenameGroup: (groupId: string, label: string) => void;
    onOpenLauncher: () => void;
  }

  interface SessionGroup {
    id: string;
    label: string;
    preset: string;
    created_at: string;
    nodes: Session[];
  }

  let {
    agents = [],
    sessions = [],
    selectedSessionId = null,
    selectedGroupId = null,
    fileTreeRoot = null,
    onOpenFile,
    onDeleted,
    onSelectSession,
    onSelectGroup,
    onKillSession,
    onKillGroup,
    onRenameGroup,
    onOpenLauncher,
  }: Props = $props();

  let tab = $state<'sessions' | 'files'>('sessions');

  let collapsed = $state<Record<string, boolean>>({});
  let lastOpenedGroupId = $state<string | null>(null);
  let renamingId = $state<string | null>(null);
  let renameDraft = $state('');
  let storageRoot = $state('');
  let storageBusy = $state(false);
  let pathCopied = $state(false);

  onMount(() => {
    getStorageInfo()
      .then((info) => {
        storageRoot = info.root;
      })
      .catch(() => {
        storageRoot = '';
      });
  });

  async function handleOpenStorage() {
    storageBusy = true;
    try {
      await openStorageFolder();
    } catch {
      // Explorer may still open even when spawn reports oddly
    } finally {
      storageBusy = false;
    }
  }

  let groups = $derived.by(() => {
    const map = new Map<string, SessionGroup>();
    for (const s of sessions) {
      const gid = s.group_id || s.id;
      let g = map.get(gid);
      if (!g) {
        g = {
          id: gid,
          label: s.group_label || `Session · ${s.preset}`,
          preset: s.preset,
          created_at: s.created_at,
          nodes: [],
        };
        map.set(gid, g);
      }
      g.nodes.push(s);
      if (s.created_at < g.created_at) g.created_at = s.created_at;
    }
    const list = [...map.values()];
    list.sort((a, b) => a.created_at.localeCompare(b.created_at));
    for (const g of list) {
      g.nodes.sort((a, b) => a.created_at.localeCompare(b.created_at));
    }
    return list;
  });

  // Expand only when switching to a different session group — never undo a manual collapse.
  $effect(() => {
    if (!selectedGroupId || selectedGroupId === lastOpenedGroupId) return;
    lastOpenedGroupId = selectedGroupId;
    if (collapsed[selectedGroupId]) {
      const next = { ...collapsed };
      delete next[selectedGroupId];
      collapsed = next;
    }
  });

  let runningCount = $derived(sessions.filter((s) => s.status === 'running').length);

  function toggleGroup(id: string, e?: MouseEvent) {
    e?.preventDefault();
    e?.stopPropagation();
    collapsed = { ...collapsed, [id]: !collapsed[id] };
  }

  function isExpanded(id: string): boolean {
    return !collapsed[id];
  }

  function nodeLabel(s: Session): string {
    if (s.role) return s.role;
    return s.name;
  }

  function sessionDir(s: Session): string {
    return s.worktree_path || s.working_dir;
  }

  async function copyFileRoot() {
    if (!fileTreeRoot) return;
    try {
      await navigator.clipboard.writeText(fileTreeRoot);
      pathCopied = true;
      setTimeout(() => (pathCopied = false), 1200);
    } catch {
      /* clipboard may be denied */
    }
  }

  async function openSessionDir(path: string, e?: MouseEvent) {
    e?.preventDefault();
    e?.stopPropagation();
    if (!path) return;
    try {
      await openFolder(path);
    } catch {
      /* Explorer/Finder may still open */
    }
  }

  function startRename(group: SessionGroup, e?: MouseEvent) {
    e?.preventDefault();
    e?.stopPropagation();
    renamingId = group.id;
    renameDraft = group.label;
  }

  function commitRename(groupId: string) {
    const next = renameDraft.trim();
    renamingId = null;
    if (!next) return;
    const current = groups.find((g) => g.id === groupId);
    if (current && current.label === next) return;
    onRenameGroup(groupId, next);
  }

  function cancelRename() {
    renamingId = null;
    renameDraft = '';
  }

  function focusOnMount(node: HTMLInputElement) {
    queueMicrotask(() => {
      node.focus();
      node.select();
    });
  }
</script>

<aside class="w-[240px] flex-shrink-0 bg-ink-900 border-r border-line-strong flex flex-col overflow-hidden">
  <div class="h-10 px-2 border-b border-line flex items-center flex-shrink-0 gap-1">
    <button
      type="button"
      onclick={() => (tab = 'sessions')}
      class={cn(
        'h-6 px-2 rounded-sm text-[10px] font-mono uppercase tracking-[0.18em] transition-colors',
        tab === 'sessions' ? 'bg-ink-800 text-phosphor-bright' : 'text-fog hover:text-bone'
      )}
    >
      Sessions
    </button>
    <button
      type="button"
      onclick={() => (tab = 'files')}
      class={cn(
        'h-6 px-2 rounded-sm text-[10px] font-mono uppercase tracking-[0.18em] transition-colors',
        tab === 'files' ? 'bg-ink-800 text-phosphor-bright' : 'text-fog hover:text-bone'
      )}
    >
      Files
    </button>
  </div>

  {#if tab === 'sessions'}
    <div class="h-6 px-2 border-b border-line bg-ink-850/60 flex items-center justify-end flex-shrink-0">
      <span class="text-[10px] font-mono text-fog tabular-nums">
        <span class="text-phosphor">{groups.length}</span>
        <span class="text-dim"> grp</span>
        <span class="text-dim mx-0.5">·</span>
        <span class="text-phosphor">{runningCount}</span>
        <span class="text-dim">/{sessions.length} live</span>
      </span>
    </div>
  {/if}

  {#if tab === 'sessions'}
    <div class="flex-1 overflow-y-auto overflow-x-hidden py-1.5 px-1.5 space-y-1">
    {#if groups.length === 0}
      <div class="px-1.5 py-3 text-[11px] font-mono text-dim leading-relaxed">
        No sessions yet.<br />Start one to open a terminal.
      </div>
    {:else}
      {#each groups as group (group.id)}
        {@const expanded = isExpanded(group.id)}
        {@const groupRunning = group.nodes.filter((n) => n.status === 'running').length}
        {@const isActiveGroup = selectedGroupId === group.id}
        <div
          class={cn(
            'rounded-sm border overflow-hidden',
            isActiveGroup ? 'border-phosphor/40 bg-ink-850' : 'border-line bg-ink-900'
          )}
        >
          <div
            class={cn(
              'group/hdr flex items-center gap-0.5 h-8 pl-0.5 pr-1',
              isActiveGroup ? 'bg-ink-800/80' : 'hover:bg-ink-850'
            )}
          >
            <button
              type="button"
              class="flex-shrink-0 h-6 w-6 flex items-center justify-center text-fog hover:text-phosphor"
              onclick={(e) => toggleGroup(group.id, e)}
              aria-label={expanded ? 'Collapse' : 'Expand'}
            >
              {#if expanded}
                <ChevronDown class="w-3 h-3" />
              {:else}
                <ChevronRight class="w-3 h-3" />
              {/if}
            </button>
            {#if renamingId === group.id}
              <input
                type="text"
                bind:value={renameDraft}
                class="flex-1 min-w-0 h-6 px-1.5 bg-ink-950 border border-phosphor/50 rounded-sm text-[11px] font-mono text-bone focus:outline-none"
                onkeydown={(e) => {
                  if (e.key === 'Enter') commitRename(group.id);
                  if (e.key === 'Escape') cancelRename();
                }}
                onblur={() => commitRename(group.id)}
                onclick={(e) => e.stopPropagation()}
                use:focusOnMount
              />
            {:else}
              <button
                type="button"
                class="flex items-center gap-1.5 flex-1 min-w-0 text-left py-0.5"
                onclick={() => onSelectGroup(group.id)}
                ondblclick={(e) => startRename(group, e)}
                title="Double-click to rename"
              >
                <span
                  class="w-1.5 h-1.5 rounded-sm flex-shrink-0 {groupRunning > 0
                    ? 'bg-phosphor pulse-glow'
                    : 'bg-ink-600'}"
                ></span>
                <div class="min-w-0 flex-1">
                  <div
                    class="text-[11px] font-mono truncate leading-tight {isActiveGroup
                      ? 'text-phosphor-bright'
                      : 'text-bone'}"
                  >
                    {group.label}
                  </div>
                  <div class="text-[9px] font-mono text-dim truncate leading-none mt-0.5 tracking-wider uppercase">
                    {group.nodes.length} node{group.nodes.length === 1 ? '' : 's'} · {group.preset}
                  </div>
                </div>
              </button>
              <button
                type="button"
                onclick={(e) => startRename(group, e)}
                class="opacity-0 group-hover/hdr:opacity-100 h-5 w-5 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-850 transition-opacity flex-shrink-0"
                title="Rename session"
                aria-label="Rename {group.label}"
              >
                <Pencil class="w-3 h-3" />
              </button>
            {/if}
            <button
              type="button"
              onclick={() => onKillGroup(group.id)}
              class="opacity-0 group-hover/hdr:opacity-100 h-5 w-5 flex items-center justify-center rounded-sm text-fog hover:text-alert hover:bg-alert/10 transition-opacity flex-shrink-0"
              title="Close entire session"
              aria-label="Close {group.label}"
            >
              <X class="w-3 h-3" />
            </button>
          </div>

          {#if expanded}
            <ul class="pb-1 pl-2 pr-1">
              {#each group.nodes as session, i (session.id)}
                {@const isSelected = selectedSessionId === session.id}
                {@const isLast = i === group.nodes.length - 1}
                <li class="group relative flex items-center">
                  <span class="absolute left-0 top-0 bottom-0 w-3 pointer-events-none" aria-hidden="true">
                    <span
                      class="absolute left-1 top-0 w-px bg-line-strong {isLast
                        ? 'h-[14px]'
                        : 'bottom-0'}"
                    ></span>
                    <span class="absolute left-1 top-[13px] w-2.5 h-px bg-line-strong"></span>
                  </span>
                  <div
                    class={cn(
                      'relative ml-3.5 flex-1 flex items-center gap-1.5 h-7 pl-1.5 pr-0.5 rounded-sm border',
                      isSelected
                        ? 'bg-ink-800 border-phosphor/40'
                        : 'border-transparent hover:bg-ink-800/70'
                    )}
                  >
                    <button
                      type="button"
                      class="absolute inset-0 z-0 rounded-sm"
                      onclick={() => onSelectSession(session.id)}
                      aria-label="Select {session.name}"
                    ></button>
                    <span
                      class={cn(
                        'relative z-10 w-1.5 h-1.5 rounded-sm flex-shrink-0',
                        session.status === 'running' ? 'bg-phosphor' : 'bg-ink-600'
                      )}
                    ></span>
                    <span class="relative z-10 flex-shrink-0">
                      <AgentMark
                        id={session.engine}
                        name={agents.find((a) => a.id === session.engine)?.name ?? session.engine}
                        installed={true}
                        size="xs"
                      />
                    </span>
                    <span class="relative z-10 flex-1 min-w-0 text-[10px] font-mono text-bone truncate leading-none pointer-events-none" title={sessionDir(session)}>
                      {nodeLabel(session)}
                    </span>
                    <button
                      type="button"
                      onclick={(e) => {
                        e.stopPropagation();
                        onKillSession(session.id);
                      }}
                      class="relative z-10 opacity-0 group-hover:opacity-100 flex-shrink-0 h-5 w-5 flex items-center justify-center rounded-sm text-fog hover:text-alert hover:bg-alert/10 transition-opacity"
                      title="Close node"
                      aria-label="Close {session.name}"
                    >
                      <X class="w-2.5 h-2.5" />
                    </button>
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
  {:else}
    <div class="flex-1 min-h-0 overflow-hidden flex flex-col">
      {#if fileTreeRoot}
        <div class="flex-shrink-0 px-2 py-1.5 border-b border-line bg-ink-850/60 flex items-start gap-1">
          <button
            type="button"
            onclick={copyFileRoot}
            class="flex-1 min-w-0 text-left text-[10px] font-mono text-fog hover:text-phosphor leading-snug break-all"
            title={fileTreeRoot}
          >
            {pathCopied ? 'Copied' : displayHomePath(fileTreeRoot)}
          </button>
          <button
            type="button"
            onclick={(e) => openSessionDir(fileTreeRoot, e)}
            class="flex-shrink-0 h-5 w-5 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800"
            title="Open folder"
            aria-label="Open folder"
          >
            <FolderOpen class="w-3 h-3" />
          </button>
        </div>
      {/if}
      <div class="flex-1 min-h-0 overflow-hidden py-1.5 px-1.5">
        <FileTree root={fileTreeRoot} onOpenFile={onOpenFile} {onDeleted} />
      </div>
    </div>
  {/if}

  <div class="flex-shrink-0 border-t border-line-strong px-1.5 pt-1.5 pb-1 space-y-1.5">
    {#if storageRoot}
      <div class="px-1 py-1 rounded-sm border border-line bg-ink-900/80">
        <div class="flex items-center justify-between gap-1 mb-0.5">
          <span class="text-[9px] font-mono uppercase tracking-wider text-dim">Data</span>
          <button
            type="button"
            onclick={handleOpenStorage}
            disabled={storageBusy}
            class="inline-flex items-center gap-1 h-5 px-1.5 rounded-sm text-[9px] font-mono uppercase tracking-wider text-phosphor border border-phosphor/30 hover:bg-phosphor/10 disabled:opacity-50"
            title="Open TermCrew data folder"
          >
            <FolderOpen class="w-3 h-3" />
            Open
          </button>
        </div>
        <p class="text-[9px] font-mono text-fog leading-snug break-all" title={storageRoot}>
          {displayHomePath(storageRoot)}
        </p>
      </div>
    {/if}
    <button
      type="button"
      onclick={onOpenLauncher}
      class="w-full h-8 inline-flex items-center justify-center gap-1.5 rounded-sm bg-phosphor text-ink-950 text-[11px] font-mono font-semibold tracking-wider uppercase hover:bg-phosphor-bright active:scale-[0.98] border border-phosphor transition-transform duration-100"
    >
      <Plus class="w-3.5 h-3.5" />
      <span>New session</span>
    </button>
  </div>
</aside>
