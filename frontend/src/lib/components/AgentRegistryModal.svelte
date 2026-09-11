<script lang="ts">
  import type { AgentMeta } from '../types';
  import { btn, badge, cn, displayHomePath } from '../ui';
  import { startAgentSetup, killSession } from '../api';
  import SetupTerminal from './SetupTerminal.svelte';
  import AgentMark from './AgentMark.svelte';
  import {
    X,
    Search,
    RefreshCw,
    Play,
    Bot,
    LayoutGrid,
    List,
    Download,
    ArrowUpCircle,
    Trash2,
    BookOpen,
  } from 'lucide-svelte';

  interface Props {
    isOpen: boolean;
    agents: AgentMeta[];
    onClose: () => void;
    onLaunchAgent: (agentId: string) => void;
    onRefresh: (fresh?: boolean) => void;
  }

  let { isOpen = false, agents = [], onClose, onLaunchAgent, onRefresh }: Props = $props();

  let searchQuery = $state('');
  let selectedFilter = $state<'all' | 'installed' | 'not-installed'>('all');
  let viewMode = $state<'grid' | 'list'>('grid');
  let setup = $state<{
    sessionId: string;
    title: string;
    command?: string;
  } | null>(null);
  let setupError = $state<string | null>(null);
  let pendingRemoveId = $state<string | null>(null);
  let busyAction = $state<string | null>(null);
  /** One setup console at a time — lock all Install/Update/Remove until Done. */
  let setupLocked = $derived(busyAction !== null || setup !== null);
  let copiedPath = $state<string | null>(null);

  let filteredAgents = $derived.by(() => {
    return agents.filter((agent) => {
      const matchesSearch =
        agent.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        agent.provider.toLowerCase().includes(searchQuery.toLowerCase()) ||
        agent.binary.toLowerCase().includes(searchQuery.toLowerCase()) ||
        (agent.description && agent.description.toLowerCase().includes(searchQuery.toLowerCase())) ||
        (agent.docs_url && agent.docs_url.toLowerCase().includes(searchQuery.toLowerCase()));

      if (!matchesSearch) return false;
      if (selectedFilter === 'installed') return agent.is_installed;
      if (selectedFilter === 'not-installed') return !agent.is_installed;
      return true;
    });
  });

  let readyCount = $derived(agents.filter((a) => a.is_installed).length);
  let missingCount = $derived(agents.length - readyCount);

  function launch(id: string) {
    const agent = agents.find((a) => a.id === id);
    if (!agent?.is_installed) return;
    void closeSetup();
    onLaunchAgent(id);
    onClose();
  }

  async function closeSetup() {
    const id = setup?.sessionId;
    setup = null;
    setupError = null;
    if (id) {
      try {
        await killSession(id);
      } catch {
        /* setup console may already be gone */
      }
    }
    onRefresh(true);
  }

  async function runSetup(agent: AgentMeta, action: 'install' | 'update' | 'uninstall') {
    if (setupLocked) return;
    const command =
      action === 'install'
        ? agent.install_cmd
        : action === 'update'
          ? agent.update_cmd
          : agent.uninstall_cmd;
    if (!command) return;

    if (action === 'uninstall') {
      if (pendingRemoveId !== agent.id) {
        pendingRemoveId = agent.id;
        return;
      }
      pendingRemoveId = null;
    }

    setupError = null;
    busyAction = `${agent.id}:${action}`;
    try {
      const session = await startAgentSetup(agent.id, action);
      setup = {
        sessionId: session.id,
        title: `${action} · ${agent.name}`,
        command,
      };
    } catch (err) {
      setupError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  function handleClose() {
    void closeSetup();
    onClose();
  }

  function displayPath(path: string): string {
    return displayHomePath(path);
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copiedPath = path;
      setTimeout(() => {
        if (copiedPath === path) copiedPath = null;
      }, 1400);
    } catch {
      /* clipboard denied */
    }
  }

  $effect(() => {
    if (!setup) return;
    let inflight = false;
    const timer = setInterval(() => {
      if (inflight) return;
      inflight = true;
      Promise.resolve(onRefresh(true)).finally(() => {
        inflight = false;
      });
    }, 8000);
    return () => clearInterval(timer);
  });
</script>

{#snippet docsLink(agent: AgentMeta)}
  {#if agent.docs_url}
    <a
      href={agent.docs_url}
      target="_blank"
      rel="noopener noreferrer"
      class="inline-flex items-center gap-1 text-[10px] font-mono text-phosphor hover:underline"
      title="Open official docs"
    >
      <BookOpen class="w-3 h-3" />
      <span>Docs</span>
    </a>
  {/if}
{/snippet}

{#snippet agentPath(agent: AgentMeta)}
  {#if agent.is_installed && agent.path}
    <button
      type="button"
      onclick={() => copyPath(agent.path ?? '')}
      class="text-left text-[10px] font-mono text-phosphor/80 hover:text-phosphor break-all leading-relaxed"
      title="Click to copy full path"
    >
      {displayPath(agent.path)}{#if copiedPath === agent.path}<span class="text-phosphor"> · copied</span>{/if}
    </button>
  {:else if !agent.is_installed && agent.manage_hint && !agent.install_cmd}
    <p class="text-[10px] font-mono text-dim break-words">{agent.manage_hint}</p>
  {/if}
{/snippet}

{#snippet agentActions(agent: AgentMeta)}
  <div class="flex flex-wrap items-center gap-1.5 mt-auto">
    {#if agent.is_installed}
      {#if agent.update_cmd}
        <button
          type="button"
          onclick={() => runSetup(agent, 'update')}
          disabled={setupLocked}
          class={btn({ variant: 'outline', size: 'xs' })}
          title={setupLocked ? 'Finish the current setup first' : agent.update_cmd}
        >
          <ArrowUpCircle class="w-3 h-3" />
          <span>Update</span>
        </button>
      {/if}
      {#if agent.uninstall_cmd}
        <button
          type="button"
          onclick={() => runSetup(agent, 'uninstall')}
          disabled={setupLocked}
          class={btn({ variant: pendingRemoveId === agent.id ? 'danger' : 'outline', size: 'xs' })}
          title={setupLocked ? 'Finish the current setup first' : agent.uninstall_cmd}
        >
          <Trash2 class="w-3 h-3" />
          <span>{pendingRemoveId === agent.id ? 'Sure?' : 'Remove'}</span>
        </button>
      {/if}
    {:else}
      <button
        type="button"
        onclick={() => runSetup(agent, 'install')}
        disabled={!agent.install_cmd || setupLocked}
        class={btn({ variant: 'outline', size: 'xs' })}
        title={setupLocked
          ? 'Finish the current setup first'
          : (agent.install_cmd ?? agent.manage_hint ?? 'Cannot install automatically')}
      >
        <Download class="w-3 h-3" />
        <span>Install</span>
      </button>
    {/if}
    <button
      type="button"
      onclick={() => launch(agent.id)}
      disabled={!agent.is_installed}
      class={btn({
        variant: agent.is_installed ? 'primary' : 'default',
        size: 'xs',
      })}
      title={agent.is_installed ? 'Open a new session' : 'Install this agent first'}
    >
      <Play class="w-3 h-3 fill-current" />
      <span>Launch</span>
    </button>
  </div>
{/snippet}

{#if isOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4 overflow-y-auto">
    <div
      class="fixed inset-0 bg-ink-950/85"
      onclick={handleClose}
      role="presentation"
    ></div>

    <div
      class="bezel relative z-10 bg-ink-900 border border-line-strong rounded-sm w-full max-w-5xl overflow-hidden flex flex-col max-h-[88vh] toast-enter"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      <div class="px-5 py-3 border-b border-line flex items-center justify-between bg-ink-850">
        <div class="flex items-center gap-3">
          <div class="w-7 h-7 rounded-sm bg-ink-800 border border-line-strong flex items-center justify-center text-phosphor">
            <Bot class="w-4 h-4" />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-xs font-mono font-bold text-bone tracking-[0.14em] uppercase">Agents</h2>
              <span class={badge({ variant: 'success' })}>
                {agents.filter((a) => a.is_installed).length}/{agents.length} ready
              </span>
            </div>
            <p class="font-mono text-[10px] text-fog uppercase tracking-wider">
              Installed CLIs on this machine
            </p>
          </div>
        </div>
        <div class="flex items-center gap-1">
          <button
            type="button"
            onclick={() => onRefresh()}
            class={btn({ variant: 'ghost', size: 'icon' })}
            title="Rescan PATH"
          >
            <RefreshCw class="w-4 h-4" />
          </button>
          <button
            type="button"
            onclick={handleClose}
            class={btn({ variant: 'ghost', size: 'icon' })}
            aria-label="Close"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div class="px-5 py-3 border-b border-line bg-ink-850/60 flex flex-wrap items-center justify-between gap-3">
        <div class="relative flex-1 min-w-[200px] max-w-md">
          <Search class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-fog" />
          <input
            type="text"
            bind:value={searchQuery}
            placeholder="Search agents…"
            class="w-full pl-9 pr-3 py-2 bg-ink-950 border border-line rounded-sm text-xs text-bone font-mono placeholder:text-dim focus:outline-none focus:border-phosphor"
          />
        </div>

        <div class="flex items-center gap-2">
          <div class="flex items-center gap-1 bg-ink-950 border border-line rounded-sm p-0.5">
            <button
              type="button"
              onclick={() => (selectedFilter = 'all')}
              class={cn(
                'px-2 py-1 rounded-sm font-mono text-[11px] uppercase tracking-wider',
                selectedFilter === 'all' ? 'bg-ink-800 text-bone' : 'text-fog hover:text-bone'
              )}
            >
              All {agents.length}
            </button>
            <button
              type="button"
              onclick={() => (selectedFilter = 'installed')}
              class={cn(
                'px-2 py-1 rounded-sm font-mono text-[11px] uppercase tracking-wider',
                selectedFilter === 'installed' ? 'bg-phosphor/10 text-phosphor' : 'text-fog hover:text-bone'
              )}
            >
              Ready {readyCount}
            </button>
            <button
              type="button"
              onclick={() => (selectedFilter = 'not-installed')}
              class={cn(
                'px-2 py-1 rounded-sm font-mono text-[11px] uppercase tracking-wider',
                selectedFilter === 'not-installed' ? 'bg-warning/10 text-warning' : 'text-fog hover:text-bone'
              )}
            >
              Missing {missingCount}
            </button>
          </div>

          <div class="flex items-center border border-line rounded-sm p-0.5 bg-ink-950">
            <button
              type="button"
              onclick={() => (viewMode = 'grid')}
              class={cn(
                'h-7 w-7 flex items-center justify-center rounded-sm',
                viewMode === 'grid' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone'
              )}
              title="Grid"
            >
              <LayoutGrid class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onclick={() => (viewMode = 'list')}
              class={cn(
                'h-7 w-7 flex items-center justify-center rounded-sm',
                viewMode === 'list' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone'
              )}
              title="List"
            >
              <List class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      <div class="p-4 overflow-y-auto flex-1">
        {#if filteredAgents.length === 0}
          <div class="text-center py-16 text-fog text-sm font-mono">
            No agents match “{searchQuery}”
          </div>
        {:else if viewMode === 'grid'}
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {#each filteredAgents as agent (agent.id)}
              <div
                class="rounded-sm border border-line bg-ink-850 hover:border-phosphor/30 transition-colors p-3 flex flex-col gap-3 min-h-[168px]"
              >
                <div class="flex items-start gap-2.5">
                  <AgentMark id={agent.id} name={agent.name} installed={agent.is_installed} />
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-mono font-bold text-bone truncate">{agent.name}</div>
                    <div class="text-[10px] font-mono text-fog truncate">{agent.binary}</div>
                  </div>
                  <span class={badge({ variant: agent.is_installed ? 'success' : 'warning' })}>
                    {agent.is_installed ? 'Ready' : 'Missing'}
                  </span>
                </div>

                <p class="text-[11px] text-fog font-mono leading-relaxed line-clamp-2">
                  {agent.description || 'CLI coding agent'}
                </p>
                {@render docsLink(agent)}
                {@render agentPath(agent)}

                {@render agentActions(agent)}
              </div>
            {/each}
          </div>
        {:else}
          <div class="space-y-1.5">
            {#each filteredAgents as agent (agent.id)}
              <div
                class="rounded-sm border border-line bg-ink-850 hover:border-line-strong transition-colors px-3 py-2 grid grid-cols-[auto_minmax(0,1fr)_4.75rem_minmax(12rem,auto)] items-center gap-3"
              >
                <AgentMark id={agent.id} name={agent.name} installed={agent.is_installed} size="sm" />
                <div class="min-w-0">
                  <div class="text-xs font-mono font-bold text-bone truncate">
                    {agent.name}
                    <span class="text-fog font-normal ml-1">{agent.binary}</span>
                  </div>
                  <div class="text-[10px] font-mono text-dim truncate">
                    {agent.description || agent.provider}
                  </div>
                  {@render docsLink(agent)}
                  {@render agentPath(agent)}
                </div>
                <div class="flex justify-end">
                  <span class={badge({ variant: agent.is_installed ? 'success' : 'warning' })}>
                    {agent.is_installed ? 'Ready' : 'Missing'}
                  </span>
                </div>
                <div class="flex justify-end [&_.mt-auto]:mt-0">
                  {@render agentActions(agent)}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      {#if setupError}
        <div class="px-5 py-2 border-t border-alert/40 bg-alert/10 text-[11px] font-mono text-alert">
          {setupError}
        </div>
      {/if}

      {#if setup}
        {#key setup.sessionId}
          <SetupTerminal
            sessionId={setup.sessionId}
            title={setup.title}
            command={setup.command}
            onClose={closeSetup}
            onRescan={() => onRefresh(true)}
          />
        {/key}
      {/if}

      <div class="px-5 py-3 border-t border-line bg-ink-850 flex items-center justify-between text-[10px] text-fog font-mono uppercase tracking-wider">
        <span>Install, update, and remove run in this shell · Launch needs a ready CLI</span>
        <button type="button" onclick={handleClose} class={btn({ variant: 'default', size: 'sm' })}>
          Done
        </button>
      </div>
    </div>
  </div>
{/if}
