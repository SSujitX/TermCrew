<script lang="ts">
  import { listDir, createFsEntry, deleteFsEntry, type DirListing, type FsEntry } from '../api';
  import { cn } from '../ui';
  import { fileIconFor, folderIconFor } from '../fileIcons';
  import Icon from '@iconify/svelte';
  import { ChevronRight, ChevronDown, RefreshCw, Plus, Trash2 } from '@lucide/svelte';

  interface Props {
    /** Absolute folder path to explore (focused session's working dir). */
    root: string | null;
    /** Called when a file row is clicked — opens the editor panel. */
    onOpenFile?: (path: string) => void;
    /** Called after a file/folder is deleted so an open editor can close. */
    onDeleted?: (path: string) => void;
  }

  let { root, onOpenFile, onDeleted }: Props = $props();

  type DirState = { loading: boolean; error: string; entries: FsEntry[] };

  let rootPath = $state<string | null>(null);
  let rootEntries = $state<DirState | null>(null);
  let expanded = $state<Record<string, boolean>>({});
  let children = $state<Record<string, DirState>>({});

  // New root (different focused session) → reset the whole tree.
  $effect(() => {
    if (root === rootPath) return;
    rootPath = root;
    rootEntries = null;
    expanded = {};
    children = {};
    if (root) void loadRoot(root);
    createParent = null;
    createName = '';
    createError = '';
    pendingDelete = null;
  });

  let createParent = $state<string | null>(null);
  let createName = $state('');
  let createBusy = $state(false);
  let createError = $state('');
  let createInput = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (createParent && createInput) createInput.focus();
  });
  let pendingDelete = $state<string | null>(null);
  let pendingDeleteAt = $state(0);
  let deleteBusy = $state(false);
  let deleteError = $state('');

  async function loadRoot(path: string) {
    rootEntries = { loading: true, error: '', entries: [] };
    try {
      const listing: DirListing = await listDir(path);
      if (rootPath !== path) return; // root switched mid-flight
      rootEntries = { loading: false, error: '', entries: listing.entries };
    } catch (err) {
      if (rootPath !== path) return;
      rootEntries = { loading: false, error: err instanceof Error ? err.message : String(err), entries: [] };
    }
  }

  function refresh() {
    expanded = {};
    children = {};
    if (rootPath) void loadRoot(rootPath);
  }

  async function ensureLoaded(path: string) {
    if (children[path] && !children[path].error) return;
    children = { ...children, [path]: { loading: true, error: '', entries: [] } };
    try {
      const listing = await listDir(path);
      children = { ...children, [path]: { loading: false, error: '', entries: listing.entries } };
    } catch (err) {
      children = {
        ...children,
        [path]: {
          loading: false,
          error: err instanceof Error ? err.message : String(err),
          entries: [],
        },
      };
    }
  }

  function joinUnder(base: string, seg: string): string {
    const slash = base.includes('\\') ? '\\' : '/';
    return base.endsWith('/') || base.endsWith('\\') ? `${base}${seg}` : `${base}${slash}${seg}`;
  }

  async function reveal(absPath: string) {
    if (!rootPath) return;
    const root = rootPath;
    const norm = (p: string) => p.replace(/[\\/]+/g, '/').replace(/\/$/, '');
    const r = norm(root);
    const a = norm(absPath);
    if (!a.toLowerCase().startsWith(r.toLowerCase())) return;
    const rest = a.slice(r.length).split('/').filter(Boolean);
    rest.pop();
    let cur = root;
    for (const seg of rest) {
      cur = joinUnder(cur, seg);
      expanded = { ...expanded, [cur]: true };
      await ensureLoaded(cur);
    }
  }

  function inferKind(rel: string): 'file' | 'dir' {
    const t = rel.trim().replace(/\\/g, '/');
    return t.endsWith('/') ? 'dir' : 'file';
  }

  function relUnderRoot(abs: string): string {
    if (!rootPath) return '';
    const norm = (p: string) => p.replace(/[\\/]+/g, '/').replace(/\/$/, '');
    const r = norm(rootPath);
    const a = norm(abs);
    if (a.toLowerCase() === r.toLowerCase()) return '';
    if (!a.toLowerCase().startsWith(r.toLowerCase())) return '';
    return a.slice(r.length).replace(/^\//, '');
  }

  function startCreate(parent: string, e?: MouseEvent) {
    e?.preventDefault();
    e?.stopPropagation();
    createParent = parent;
    createName = '';
    createError = '';
  }

  async function submitCreate() {
    if (!rootPath || !createParent || createBusy) return;
    const name = createName.trim();
    if (!name) return;
    const parentRel = relUnderRoot(createParent);
    const rel = parentRel
      ? `${parentRel}/${name.replace(/\\/g, '/').replace(/^\/+/, '')}`
      : name;
    createBusy = true;
    createError = '';
    try {
      const made = await createFsEntry(rootPath, rel, inferKind(rel));
      createParent = null;
      createName = '';
      refresh();
      await reveal(made.path);
      if (made.kind === 'file') onOpenFile?.(made.path);
    } catch (err) {
      createError = err instanceof Error ? err.message : String(err);
    } finally {
      createBusy = false;
    }
  }

  function onCreateKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void submitCreate();
    } else if (e.key === 'Escape') {
      createParent = null;
      createError = '';
    }
  }

  async function requestDelete(entry: FsEntry, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (deleteBusy || !rootPath) return;
    const path = entry.path;
    const now = Date.now();
    if (pendingDelete !== path) {
      pendingDelete = path;
      pendingDeleteAt = now;
      deleteError = '';
      return;
    }
    if (now - pendingDeleteAt < 400) return;
    if (
      entry.kind === 'dir' &&
      !confirm(`Delete ${entry.name} and everything inside? This cannot be undone.`)
    ) {
      pendingDelete = null;
      return;
    }
    deleteBusy = true;
    try {
      await deleteFsEntry(rootPath, path);
      pendingDelete = null;
      refresh();
      onDeleted?.(path);
    } catch (err) {
      deleteError = err instanceof Error ? err.message : String(err);
    } finally {
      deleteBusy = false;
    }
  }

  function toggleDir(entry: FsEntry) {
    const next = !expanded[entry.path];
    expanded = { ...expanded, [entry.path]: next };
    if (next) void ensureLoaded(entry.path);
  }

  /**
   * Flatten the lazily loaded tree into visible rows: walk from the root
   * through every expanded dir. Avoids recursive Svelte components while
   * keeping expansion state in one flat record.
   */
  let rows = $derived.by(() => {
    const out: { entry: FsEntry; depth: number }[] = [];
    const walk = (entries: FsEntry[], depth: number) => {
      for (const entry of entries) {
        out.push({ entry, depth });
        if (entry.kind === 'dir' && expanded[entry.path]) {
          const state = children[entry.path];
          if (state) walk(state.entries, depth + 1);
        }
      }
    };
    if (rootEntries) walk(rootEntries.entries, 0);
    return out;
  });

  const fileName = (p: string): string => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
</script>

<div class="flex flex-col h-full min-h-0">
  {#if !root}
    <div class="px-2 py-3 text-[11px] font-mono text-dim leading-relaxed">
      Launch or focus a session to explore its folder.
    </div>
  {:else}
    <!-- Tree root header -->
    <div class="flex items-center gap-1 px-1.5 h-8 flex-shrink-0 border-b border-line">
      <Icon icon={folderIconFor(fileName(root), false)} class="w-4 h-4 flex-shrink-0" />
      <span class="flex-1 min-w-0 text-[10px] font-mono text-bone truncate" title={root}>
        {fileName(root)}
      </span>
      <button
        type="button"
        onclick={() => startCreate(root)}
        title="New file or folder — notes.txt or src/lib/util.ts"
        class="h-5 w-5 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors flex-shrink-0"
      >
        <Plus class="w-3 h-3" />
      </button>
      <button
        type="button"
        onclick={refresh}
        title="Refresh tree"
        class="h-5 w-5 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors flex-shrink-0"
      >
        <RefreshCw class="w-3 h-3" />
      </button>
    </div>
    {#if createParent}
      <div class="px-1.5 py-1 border-b border-line space-y-1">
        <input
          type="text"
          bind:this={createInput}
          bind:value={createName}
          onkeydown={onCreateKey}
          placeholder={createParent === root
            ? 'notes.txt or docs/'
            : `in ${fileName(createParent)} — file.txt or nested/`}
          class="w-full h-7 px-2 rounded-sm bg-ink-900 border border-line text-[10px] font-mono text-bone focus:outline-none focus:border-phosphor placeholder:text-dim"
        />
        {#if createError}
          <p class="text-[10px] font-mono text-alert leading-snug">{createError}</p>
        {/if}
      </div>
    {/if}
    {#if deleteError}
      <p class="px-1.5 py-1 text-[10px] font-mono text-alert leading-snug">{deleteError}</p>
    {/if}

    <div class="flex-1 overflow-y-auto overflow-x-hidden py-1 px-1">
      {#if rootEntries?.loading}
        <div class="px-1.5 py-2 text-[10px] font-mono text-dim">Loading…</div>
      {:else if rootEntries?.error}
        <div class="px-1.5 py-2 text-[10px] font-mono text-alert leading-relaxed">{rootEntries.error}</div>
      {:else if rows.length === 0}
        <div class="px-1.5 py-2 text-[10px] font-mono text-dim">Empty folder</div>
      {:else}
        {#each rows as row (row.entry.path)}
          {@const isDir = row.entry.kind === 'dir'}
          {@const isOpen = isDir && expanded[row.entry.path]}
          {@const dirState = isDir ? children[row.entry.path] : null}
          <div
            class={cn(
              'group flex items-center gap-1 h-6 pr-1 rounded-sm cursor-pointer',
              isDir ? 'hover:bg-ink-800' : 'hover:bg-ink-800/70'
            )}
            style="padding-left: {4 + row.depth * 12}px"
            role={isDir ? 'treeitem' : 'button'}
            onclick={() => (isDir ? toggleDir(row.entry) : onOpenFile?.(row.entry.path))}
            title={isDir ? row.entry.name : `Open in editor\n${row.entry.path}`}
          >
          {#if isDir}
            {#if isOpen}
              <ChevronDown class="w-3 h-3 text-fog flex-shrink-0" />
              <Icon icon={folderIconFor(row.entry.name, true)} class="w-4 h-4 flex-shrink-0" />
            {:else}
              <ChevronRight class="w-3 h-3 text-fog flex-shrink-0" />
              <Icon icon={folderIconFor(row.entry.name, false)} class="w-4 h-4 flex-shrink-0" />
            {/if}
            <span class="flex-1 min-w-0 text-[11px] font-mono text-bone truncate">{row.entry.name}</span>
            <button
              type="button"
              onclick={(e) => startCreate(row.entry.path, e)}
              title="New file or folder in {row.entry.name}"
              class="h-4 w-4 hidden group-hover:flex items-center justify-center rounded-sm text-fog hover:text-phosphor flex-shrink-0"
            >
              <Plus class="w-3 h-3" />
            </button>
            <button
              type="button"
              onclick={(e) => requestDelete(row.entry, e)}
              title={pendingDelete === row.entry.path ? 'Click again to delete' : `Delete ${row.entry.name}`}
              class={cn(
                'h-4 flex items-center justify-center rounded-sm flex-shrink-0',
                pendingDelete === row.entry.path
                  ? 'px-1 text-alert'
                  : 'w-4 hidden group-hover:flex text-fog hover:text-alert'
              )}
            >
              {#if pendingDelete === row.entry.path}
                <span class="text-[9px] font-mono font-bold">Sure?</span>
              {:else}
                <Trash2 class="w-3 h-3" />
              {/if}
            </button>
          {:else}
            {@const iconName = fileIconFor(row.entry.name)}
            <span class="w-3 flex-shrink-0"></span>
            <Icon icon={iconName} class="w-4 h-4 flex-shrink-0" />
            <span class="flex-1 min-w-0 text-[11px] font-mono text-fog group-hover:text-bone truncate">{row.entry.name}</span>
            <button
              type="button"
              onclick={(e) => requestDelete(row.entry, e)}
              title={pendingDelete === row.entry.path ? 'Click again to delete' : `Delete ${row.entry.name}`}
              class={cn(
                'h-4 flex items-center justify-center rounded-sm flex-shrink-0',
                pendingDelete === row.entry.path
                  ? 'px-1 text-alert'
                  : 'w-4 hidden group-hover:flex text-fog hover:text-alert'
              )}
            >
              {#if pendingDelete === row.entry.path}
                <span class="text-[9px] font-mono font-bold">Sure?</span>
              {:else}
                <Trash2 class="w-3 h-3" />
              {/if}
            </button>
          {/if}
          </div>
          {#if isDir && isOpen && dirState?.loading}
            <div class="text-[10px] font-mono text-dim py-0.5" style="padding-left: {16 + row.depth * 12}px">
              Loading…
            </div>
          {:else if isDir && isOpen && dirState?.error}
            <div class="text-[10px] font-mono text-alert py-0.5" style="padding-left: {16 + row.depth * 12}px">
              {dirState.error}
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>
