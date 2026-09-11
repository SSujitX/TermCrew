<script lang="ts">
  import type { AgentMeta, Session } from '../types';
  import TerminalPane from './TerminalPane.svelte';
  import FileEditor from './FileEditor.svelte';
  import AgentMark from './AgentMark.svelte';
  import { Grid, Columns, Plus, Terminal, X, FileCode } from 'lucide-svelte';

  interface Props {
    agents?: AgentMeta[];
    sessions: Session[];
    focusedSessionId: string | null;
    clearEpoch?: number;
    clearSessionId?: string | null;
    openFiles?: string[];
    activeFile?: string | null;
    dirtyFiles?: Record<string, boolean>;
    onFocusSession: (id: string) => void;
    onKillSession: (id: string) => void;
    onRestartSession: (id: string) => void;
    onHandoffSession: (id: string) => void;
    onOpenLauncher: () => void;
    onAddEngine?: (engine: string) => void;
    onFocusFile?: (path: string) => void;
    onCloseFile?: (path: string) => void;
    onHideFile?: () => void;
    onDirtyFile?: (path: string, dirty: boolean) => void;
  }

  let {
    agents = [],
    sessions = [],
    focusedSessionId = null,
    clearEpoch = 0,
    clearSessionId = null,
    openFiles = [],
    activeFile = null,
    dirtyFiles = {},
    onFocusSession,
    onKillSession,
    onRestartSession,
    onHandoffSession,
    onOpenLauncher,
    onAddEngine,
    onFocusFile,
    onCloseFile,
    onHideFile,
    onDirtyFile,
  }: Props = $props();

  const fileName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

  let maximizedSessionId = $state<string | null>(null);
  let viewMode = $state<'grid' | 'tabs'>('grid');
  let lastGroupKey = $state('');
  let addOpen = $state(false);

  let readyAgents = $derived(agents.filter((a) => a.is_installed));
  let canAdd = $derived(sessions.length > 0 && sessions.length < 6 && !!onAddEngine);

  // Reset layout only when switching groups — adding/killing a pane must not remount PTYs.
  $effect(() => {
    const key = sessions[0]?.group_id ?? '';
    if (key !== lastGroupKey) {
      lastGroupKey = key;
      viewMode = sessions.length > 1 ? 'grid' : 'tabs';
      maximizedSessionId = null;
      addOpen = false;
    }
  });

  $effect(() => {
    if (maximizedSessionId && !sessions.some((s) => s.id === maximizedSessionId)) {
      maximizedSessionId = null;
    }
  });

  function toggleMaximize(sessionId: string) {
    maximizedSessionId = maximizedSessionId === sessionId ? null : sessionId;
  }

  let activeId = $derived(
    focusedSessionId && sessions.some((s) => s.id === focusedSessionId)
      ? focusedSessionId
      : sessions[0]?.id ?? null
  );

  let gridLayoutClass = $derived.by(() => {
    const count = sessions.length;
    if (count <= 1) return 'grid-cols-1 grid-rows-1';
    if (count === 2) return 'grid-cols-1 lg:grid-cols-2 grid-rows-1';
    if (count === 3) return 'grid-cols-1 md:grid-cols-2 lg:grid-cols-3';
    if (count === 4) return 'grid-cols-1 md:grid-cols-2 grid-rows-2';
    if (count <= 6) return 'grid-cols-1 md:grid-cols-2 lg:grid-cols-3 grid-rows-2';
    return 'grid-cols-1 md:grid-cols-3';
  });
</script>

<div class="flex flex-col h-full w-full bg-ink-950 overflow-hidden relative">
  {#if sessions.length === 0}
    <div class="flex-1 flex flex-col items-center justify-center p-8 text-center relative z-10">
      <div class="bezel w-16 h-16 rounded-sm flex items-center justify-center mb-6 scanlines">
        <Terminal class="w-7 h-7 text-phosphor" />
      </div>

      <p class="eyebrow mb-3">Idle</p>
      <h3 class="font-mono text-lg font-semibold text-bone tracking-wide mb-2">No sessions</h3>
      <p class="font-mono text-xs text-fog max-w-sm mb-8 leading-relaxed">
        Start a session to open a terminal. You can run several at once and switch in the list.
      </p>

      <button
        type="button"
        onclick={onOpenLauncher}
        class="inline-flex items-center gap-2 px-5 py-2.5 rounded-sm bg-phosphor text-ink-950 text-xs font-mono font-semibold tracking-wider uppercase hover:bg-phosphor-bright border border-phosphor active:brightness-90 transition-colors"
      >
        <Plus class="w-4 h-4 stroke-[2.5]" />
        <span>New session</span>
      </button>
    </div>
  {:else}
    <!-- Always-visible session switcher strip -->
    <div class="h-10 px-2 bg-ink-900 border-b border-line-strong flex items-stretch gap-2 flex-shrink-0 z-10">
      <div class="flex-1 flex items-stretch gap-0 overflow-x-auto no-scrollbar min-w-0">
        {#each sessions as s (s.id)}
          {@const sessionOn = activeId === s.id && !activeFile}
          <div
            class="relative flex items-center gap-0.5 h-full flex-shrink-0 {sessionOn
              ? 'bg-ink-800 after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-phosphor'
              : activeId === s.id
                ? 'bg-ink-800/70'
                : 'hover:bg-ink-800/50'}"
          >
            <button
              type="button"
              onclick={() => onFocusSession(s.id)}
              class="h-full px-2.5 text-[11px] font-mono tracking-wide flex items-center gap-2 {sessionOn
                ? 'text-phosphor'
                : 'text-fog hover:text-bone'}"
            >
              <span
                class="w-1.5 h-1.5 rounded-sm {s.status === 'running'
                  ? 'bg-phosphor pulse-glow'
                  : 'bg-ink-600'}"
              ></span>
              <span class="max-w-[140px] truncate">{s.role ?? s.name}</span>
              {#if s.group_label}
                <span class="text-[9px] text-dim truncate max-w-[100px] hidden md:inline">{s.group_label.split(' · ')[0]}</span>
              {/if}
            </button>
            <button
              type="button"
              onclick={() => onKillSession(s.id)}
              class="h-full w-6 flex items-center justify-center text-fog hover:text-alert"
              title="Close {s.name}"
              aria-label="Close {s.name}"
            >
              <X class="w-3 h-3" />
            </button>
          </div>
        {/each}
        {#if openFiles.length > 0}
          <span class="w-px self-center h-4 bg-line-strong mx-1.5 flex-shrink-0" aria-hidden="true"></span>
          {#each openFiles as f (f)}
            <div
              class="relative flex items-center gap-0.5 h-full flex-shrink-0 {activeFile === f
                ? 'bg-ink-800 after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-phosphor'
                : 'hover:bg-ink-800/50'}"
            >
              <button
                type="button"
                onclick={() => onFocusFile?.(f)}
                class="h-full px-2.5 text-[11px] font-mono tracking-wide flex items-center gap-1.5 {activeFile === f
                  ? 'text-phosphor'
                  : 'text-fog hover:text-bone'}"
                title={f}
              >
                <FileCode class="w-3 h-3 flex-shrink-0" />
                <span class="max-w-[140px] truncate">{fileName(f)}</span>
                {#if dirtyFiles[f]}
                  <span class="w-1.5 h-1.5 rounded-full bg-[#e2b93d] flex-shrink-0" title="Unsaved"></span>
                {/if}
              </button>
              <button
                type="button"
                onclick={() => onCloseFile?.(f)}
                class="h-full w-6 flex items-center justify-center text-fog hover:text-alert"
                title="Close {fileName(f)}"
                aria-label="Close {fileName(f)}"
              >
                <X class="w-3 h-3" />
              </button>
            </div>
          {/each}
        {/if}
      </div>

      <div class="flex items-center gap-2 flex-shrink-0">
        {#if canAdd}
          <div class="relative">
            <button
              type="button"
              onclick={() => (addOpen = !addOpen)}
              title="Add a pane to this session"
              class="h-7 w-7 flex items-center justify-center rounded-sm border border-line-strong text-fog hover:text-phosphor hover:border-phosphor/40"
            >
              <Plus class="w-3.5 h-3.5" />
            </button>
            {#if addOpen}
              <div
                class="absolute right-0 top-full mt-1 z-40 w-56 max-h-64 overflow-y-auto rounded-sm border border-line-strong bg-ink-900 py-1 shadow-lg"
              >
                {#if readyAgents.length === 0}
                  <p class="px-3 py-2 text-[10px] font-mono text-dim">No installed agents.</p>
                {:else}
                  {#each readyAgents as a (a.id)}
                    <button
                      type="button"
                      onclick={() => {
                        addOpen = false;
                        onAddEngine?.(a.id);
                      }}
                      class="w-full flex items-center gap-2 px-2.5 py-1.5 text-left hover:bg-ink-800"
                    >
                      <AgentMark id={a.id} name={a.name} installed={true} size="xs" />
                      <span class="text-[11px] font-mono text-bone truncate">{a.name}</span>
                    </button>
                  {/each}
                {/if}
              </div>
            {/if}
          </div>
        {/if}
        <div class="flex items-center border border-line-strong rounded-sm p-0.5 bg-ink-950">
          <button
            type="button"
            onclick={() => (viewMode = 'tabs')}
            class="px-2 py-1 rounded-sm text-[11px] font-mono tracking-wide transition-colors flex items-center gap-1.5 {viewMode === 'tabs'
              ? 'bg-ink-800 text-phosphor'
              : 'text-fog hover:text-bone'}"
            title="Focus one session"
          >
            <Columns class="w-3.5 h-3.5" />
            <span>One</span>
          </button>
          <button
            type="button"
            onclick={() => (viewMode = 'grid')}
            class="px-2 py-1 rounded-sm text-[11px] font-mono tracking-wide transition-colors flex items-center gap-1.5 {viewMode === 'grid'
              ? 'bg-ink-800 text-phosphor'
              : 'text-fog hover:text-bone'}"
            title="Show all sessions"
          >
            <Grid class="w-3.5 h-3.5" />
            <span>Split</span>
          </button>
        </div>

        {#if maximizedSessionId}
          <button
            type="button"
            onclick={() => (maximizedSessionId = null)}
            class="px-2.5 py-1 rounded-sm border border-line-strong text-fog text-[11px] font-mono tracking-wider hover:text-phosphor hover:border-phosphor/40 transition-colors"
          >
            Restore
          </button>
        {/if}
      </div>
    </div>

    {@const stacked = viewMode !== 'grid' || maximizedSessionId != null}
    <div class="flex-1 p-3 overflow-hidden relative">
      <div
        class={stacked ? 'contents' : `grid ${gridLayoutClass} gap-3 h-full w-full`}
      >
        {#each sessions as s (s.id)}
          {@const isActive =
            maximizedSessionId != null ? maximizedSessionId === s.id : activeId === s.id}
          <div
            class={stacked
              ? `absolute inset-3 ${isActive ? 'z-10 visible' : 'z-0 invisible pointer-events-none'}`
              : `h-full w-full min-h-[220px] overflow-hidden rounded-sm ${activeId === s.id ? 'ring-1 ring-phosphor/50' : ''}`}
            aria-hidden={stacked && !isActive}
          >
            <TerminalPane
              {agents}
              session={s}
              isMaximized={maximizedSessionId === s.id}
              isVisible={!stacked || isActive}
              {clearEpoch}
              {clearSessionId}
              onToggleMaximize={() => toggleMaximize(s.id)}
              onKill={onKillSession}
              onRestart={onRestartSession}
              onHandoff={onHandoffSession}
            />
          </div>
        {/each}
      </div>

      {#if openFiles.length > 0}
        <div
          class="absolute inset-0 z-20 {activeFile ? '' : 'invisible pointer-events-none'}"
          aria-hidden={!activeFile}
        >
          <FileEditor
            path={activeFile ?? openFiles[0]}
            {openFiles}
            active={!!activeFile}
            onClose={() => activeFile && onCloseFile?.(activeFile)}
            onHide={onHideFile}
            onDirty={onDirtyFile}
          />
        </div>
      {/if}
    </div>
  {/if}
</div>
