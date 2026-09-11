<script lang="ts">
  import type { AgentMeta, PresetType, LaunchPresetRequest } from '../types';
  import { btn, badge, cn } from '../ui';
  import { X, Play, Terminal, Cpu, Folder, FolderOpen, ArrowUp, Check } from '@lucide/svelte';
  import AgentMark from './AgentMark.svelte';
  import { browseDirs, getRecentWorkdirs, pickFolderNative, type DirListing, type RecentWorkdir } from '../api';

  interface Props {
    isOpen: boolean;
    agents: AgentMeta[];
    defaultPreset?: PresetType;
    preselectAgentId?: string | null;
    onClose: () => void;
    onLaunch: (request: LaunchPresetRequest) => void;
  }

  let {
    isOpen = false,
    agents = [],
    defaultPreset = 'pair',
    preselectAgentId = null,
    onClose,
    onLaunch,
  }: Props = $props();

  let selectedPreset = $state<PresetType>('pair');
  let selectedAgentIds = $state<string[]>([]);
  let sessionCount = $state<number>(2);
  let taskPrompt = $state<string>('');
  let workingDir = $state<string>('');
  let appliedOpenKey = $state<string | null>(null);

  let recentDirs = $state<RecentWorkdir[]>([]);
  let browseOpen = $state(false);
  let browseListing = $state<DirListing | null>(null);
  let browseLoading = $state(false);
  let browseError = $state('');

  $effect(() => {
    selectedPreset = defaultPreset;
    const defaults = { solo: 1, pair: 2, workbench: 2, swarm: 4 };
    sessionCount = defaults[defaultPreset] ?? 2;
  });

  // Recents refresh per open; keyed on isOpen only so typing doesn't refetch.
  $effect(() => {
    if (isOpen) void loadRecents();
  });

  $effect(() => {
    if (!isOpen) {
      appliedOpenKey = null;
      browseOpen = false;
      browseListing = null;
      browseError = '';
      workingDir = '';
      return;
    }
    if (preselectAgentId) {
      const ready = agents.some((a) => a.id === preselectAgentId && a.is_installed);
      if (!ready) return;
      if (appliedOpenKey === preselectAgentId) return;
      appliedOpenKey = preselectAgentId;
      selectedAgentIds = [preselectAgentId];
      return;
    }
    if (agents.length === 0) return;
    if (appliedOpenKey === '__default__') return;
    appliedOpenKey = '__default__';
    if (selectedAgentIds.length === 0) {
      const first = agents.find((a) => a.is_installed) ?? agents[0];
      if (first) selectedAgentIds = [first.id];
    }
  });

  const PRESETS: { id: PresetType; label: string; count: string; hint: string }[] = [
    { id: 'solo',      label: 'Solo',      count: '1+',  hint: 'Same folder. One terminal per agent' },
    { id: 'pair',      label: 'Pair',      count: '2+',  hint: 'Same folder. First lead, next review' },
    { id: 'workbench', label: 'Workbench', count: '2+',  hint: 'Same folder. Agents plus a shell' },
    { id: 'swarm',     label: 'Swarm',     count: '2+',  hint: 'Each worker gets its own copy' },
  ];

  function selectPreset(p: typeof PRESETS[number]) {
    selectedPreset = p.id;
    if (selectedAgentIds.length <= 1) {
      const defaults = { solo: 1, pair: 2, workbench: 2, swarm: 4 };
      sessionCount = defaults[p.id] ?? 2;
    }
  }

  let selectedAgents = $derived(
    selectedAgentIds
      .map((id) => agents.find((a) => a.id === id))
      .filter((a): a is AgentMeta => !!a)
  );

  let multiSelect = $derived(selectedAgentIds.length > 1);
  let showCountStepper = $derived(!multiSelect && (selectedPreset === 'solo' || selectedPreset === 'swarm'));

  function toggleAgent(agent: AgentMeta) {
    if (!agent.is_installed) return;
    const i = selectedAgentIds.indexOf(agent.id);
    if (i >= 0) {
      if (selectedAgentIds.length === 1) return;
      selectedAgentIds = selectedAgentIds.filter((id) => id !== agent.id);
      return;
    }
    selectedAgentIds = [...selectedAgentIds, agent.id];
  }

  let willLaunchList = $derived.by(() => {
    const items: { label: string; binary: string }[] = [];
    const picked = selectedAgents.length > 0 ? selectedAgents : [];
    if (picked.length === 0) return items;

    if (selectedPreset === 'solo') {
      const roster = multiSelect ? picked : Array.from({ length: sessionCount }, () => picked[0]);
      roster.forEach((agent, i) => {
        const role = roster.length === 1 ? 'Lead' : `Lead ${i + 1}`;
        items.push({ label: `${agent.name} (${role})`, binary: agent.binary });
      });
    } else if (selectedPreset === 'pair') {
      const lead = picked[0];
      const reviewers = multiSelect ? picked.slice(1) : [picked[0]];
      items.push({ label: `${lead.name} (Lead)`, binary: lead.binary });
      reviewers.forEach((agent, i) => {
        items.push({ label: `${agent.name} (Review ${i + 1})`, binary: agent.binary });
      });
    } else if (selectedPreset === 'workbench') {
      picked.forEach((agent, i) => {
        const role = picked.length === 1 ? 'Agent' : `Agent ${i + 1}`;
        items.push({ label: `${agent.name} (${role})`, binary: agent.binary });
      });
      const defaultShell = agents.find((a) => a.id === 'shell');
      items.push({
        label: `${defaultShell?.name ?? 'Shell'} (Shell)`,
        binary: 'shell',
      });
    } else {
      const roster = multiSelect ? picked : Array.from({ length: sessionCount }, () => picked[0]);
      roster.forEach((agent, i) => {
        items.push({ label: `${agent.name} (Worker ${i + 1})`, binary: agent.binary });
      });
    }
    return items;
  });

  let launchCount = $derived(willLaunchList.length);
  let agentStep = $derived(showCountStepper ? 4 : 3);
  let taskStep = $derived(showCountStepper ? 5 : 4);

  function handleLaunch() {
    if (selectedAgentIds.length === 0 || !workingDir.trim()) return;
    const missing = selectedAgents.filter((a) => !a.is_installed);
    if (missing.length > 0) return;
    onLaunch({
      preset: selectedPreset,
      agentId: selectedAgentIds[0],
      agentIds: selectedAgentIds,
      count: sessionCount,
      task: taskPrompt.trim() || undefined,
      workingDir: workingDir.trim(),
    });
    onClose();
  }

  async function loadRecents() {
    recentDirs = await getRecentWorkdirs();
  }

  async function loadListing(path?: string) {
    browseLoading = true;
    browseError = '';
    try {
      browseListing = await browseDirs(path);
    } catch (err) {
      browseError = err instanceof Error ? err.message : String(err);
      browseListing = null;
    } finally {
      browseLoading = false;
    }
  }

  async function openBrowser() {
    // Native Explorer / Finder window first — the browser page itself cannot
    // get real folder paths from a file dialog, but the local backend can.
    try {
      const picked = await pickFolderNative('Select working folder for agents');
      if (picked) {
        workingDir = picked;
        return;
      }
      return; // user closed / cancelled the native dialog
    } catch {
      // Native picker unavailable (dialog already open, headless host) —
      // fall back to the inline folder browser.
    }
    browseOpen = true;
    void loadListing(workingDir.trim() || undefined);
  }

  function usePickedFolder() {
    if (!browseListing) return;
    workingDir = browseListing.path;
    browseOpen = false;
  }

  function shortLabel(p: string): string {
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? p;
  }

  function isRecentSelected(p: string): boolean {
    return (
      workingDir.trim().replace(/[\\/]+$/, '').toLowerCase() === p.replace(/[\\/]+$/, '').toLowerCase()
    );
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') handleLaunch();
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
    onkeydown={handleKeydown}
  >
    <div
      class="absolute inset-0 bg-ink-950/85"
      role="presentation"
      onclick={onClose}
    ></div>

    <div
      class="bezel relative z-10 bg-ink-900 border border-line-strong rounded-sm w-full max-w-[720px] overflow-hidden flex flex-col toast-enter"
      role="dialog"
      aria-modal="true"
    >
      <div class="flex items-center justify-between px-5 py-3 border-b border-line bg-ink-850">
        <div class="flex items-center gap-3">
          <div class="w-7 h-7 rounded-sm bg-ink-800 border border-line-strong flex items-center justify-center text-phosphor">
            <Cpu class="w-3.5 h-3.5" />
          </div>
          <div>
            <h2 class="text-xs font-mono font-bold text-bone tracking-[0.14em] uppercase">New session</h2>
            <p class="text-[10px] font-mono text-fog tracking-wider uppercase">Folder, layout, then how many and agents</p>
          </div>
        </div>
        <button type="button" onclick={onClose} class={btn({ variant: 'ghost', size: 'icon' })} aria-label="Close">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="p-5 space-y-5 overflow-y-auto max-h-[75vh]">

        <div>
          <div class="flex items-center justify-between mb-1.5">
            <label
              for="launch-folder"
              class="block text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-phosphor"
            >
              1. Folder
            </label>
            <span class="text-[10px] text-fog font-mono uppercase tracking-wider">Where agents work</span>
          </div>

          {#if recentDirs.length > 0}
            <div class="flex flex-wrap gap-1.5 mb-2">
              {#each recentDirs.slice(0, 5) as rd (rd.path)}
                <button
                  type="button"
                  onclick={() => (workingDir = rd.path)}
                  title={`${rd.path} · used ${rd.use_count}×`}
                  class={cn(
                    "inline-flex items-center gap-1.5 h-7 px-2.5 rounded-sm border text-[11px] font-mono transition-colors max-w-full",
                    isRecentSelected(rd.path)
                      ? "border-phosphor bg-phosphor/10 text-phosphor-bright"
                      : "border-line bg-ink-850 text-fog hover:border-line-strong hover:text-bone"
                  )}
                >
                  <FolderOpen class="w-3 h-3 flex-shrink-0" />
                  <span class="truncate max-w-[180px]">{shortLabel(rd.path)}</span>
                </button>
              {/each}
            </div>
          {/if}

          <div class="flex gap-2">
            <input
              id="launch-folder"
              type="text"
              bind:value={workingDir}
              placeholder="Choose a folder — required to start"
              class="flex-1 min-w-0 h-9 px-3 rounded-sm bg-ink-850 border border-line text-xs text-bone font-mono focus:outline-none focus:border-phosphor placeholder:text-dim"
            />
            <button
              type="button"
              onclick={openBrowser}
              title="Browse folders on the machine"
              class={btn({ variant: 'ghost', size: 'sm' })}
            >
              <Folder class="w-3.5 h-3.5" />
              <span>Browse</span>
            </button>
          </div>

          {#if browseOpen}
            <div class="mt-2 border border-line rounded-sm bg-ink-850 overflow-hidden">
              <div class="flex items-center justify-between gap-2 px-2.5 py-1.5 border-b border-line bg-ink-900">
                <span
                  class="font-mono text-[10px] text-bone truncate flex-1 min-w-0"
                  title={browseListing?.path ?? ''}
                >
                  {browseListing?.path ?? 'Listing…'}
                </span>
                <div class="flex items-center gap-1 flex-shrink-0">
                  {#if browseListing?.parent}
                    <button
                      type="button"
                      onclick={() => loadListing(browseListing?.parent ?? undefined)}
                      title="Up one level"
                      class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors"
                    >
                      <ArrowUp class="w-3.5 h-3.5" />
                    </button>
                  {/if}
                  <button
                    type="button"
                    onclick={usePickedFolder}
                    disabled={!browseListing}
                    class="inline-flex items-center gap-1 h-6 px-2 rounded-sm bg-phosphor text-ink-950 text-[10px] font-bold disabled:opacity-40"
                  >
                    <Check class="w-3 h-3" />
                    <span>Use this folder</span>
                  </button>
                </div>
              </div>
              <div class="max-h-44 overflow-y-auto">
                {#if browseLoading}
                  <div class="px-3 py-3 text-[11px] font-mono text-fog">Listing folders…</div>
                {:else if browseError}
                  <div class="px-3 py-3 text-[11px] font-mono text-alert">{browseError}</div>
                {:else if !browseListing || browseListing.entries.length === 0}
                  <div class="px-3 py-3 text-[11px] font-mono text-fog">No subfolders here</div>
                {:else}
                  {#each browseListing.entries as e (e.path)}
                    <button
                      type="button"
                      onclick={() => loadListing(e.path)}
                      class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs font-mono text-fog hover:bg-ink-800 hover:text-bone transition-colors"
                    >
                      <Folder class="w-3.5 h-3.5 text-phosphor flex-shrink-0" />
                      <span class="truncate">{e.name}</span>
                    </button>
                  {/each}
                {/if}
              </div>
            </div>
          {/if}
        </div>

        <div>
          <p class="block text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-phosphor mb-2">
            2. Layout
          </p>
          <div class="grid grid-cols-4 gap-2">
            {#each PRESETS as p}
              <button
                type="button"
                onclick={() => selectPreset(p)}
                title={p.hint}
                class={cn(
                  "flex flex-col items-start p-2.5 rounded-sm border text-left transition-colors min-w-0",
                  selectedPreset === p.id
                    ? "border-phosphor bg-phosphor/10 text-bone"
                    : "border-line bg-ink-850 text-fog hover:border-line-strong hover:text-bone"
                )}
              >
                <div class="flex items-center justify-between w-full mb-1">
                  <span class={cn("text-xs font-mono font-bold", selectedPreset === p.id ? "text-phosphor-bright" : "text-bone")}>{p.label}</span>
                  <span class={badge({ variant: selectedPreset === p.id ? 'success' : 'default' })}>
                    {p.count}
                  </span>
                </div>
                <span class="text-[11px] text-fog leading-snug line-clamp-1">{p.hint}</span>
              </button>
            {/each}
          </div>
        </div>

        {#if showCountStepper}
          <div>
            <p class="block text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-phosphor mb-2">
              3. How many
            </p>
            <div class="flex items-center gap-3">
              <div class="inline-flex border border-line rounded-sm p-0.5 bg-ink-850">
                {#each [1,2,3,4,5,6] as n}
                  <button
                    type="button"
                    onclick={() => (sessionCount = n)}
                    class={cn(
                      "h-7 w-8 font-mono text-xs font-bold rounded-sm transition-colors",
                      sessionCount === n
                        ? "bg-phosphor text-ink-950"
                        : "text-fog hover:text-bone hover:bg-ink-700"
                    )}
                  >{n}</button>
                {/each}
              </div>
              <span class="font-mono text-xs text-fog">
                {sessionCount === 1 ? 'Single terminal node' : `${sessionCount} copies of ${selectedAgents[0]?.name ?? 'this agent'}`}
              </span>
            </div>
          </div>
        {/if}

        <div>
          <div class="flex items-center justify-between mb-2">
            <p class="text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-phosphor">
              {agentStep}. Agents
            </p>
            <span class="font-mono text-[10px] text-phosphor uppercase tracking-wider flex items-center gap-1.5">
              <span class="w-1.5 h-1.5 rounded-sm bg-phosphor"></span>
              {agents.filter(a => a.is_installed).length} ready
            </span>
          </div>

          <div class="flex flex-wrap gap-2">
            {#each agents as agent}
              {@const order = selectedAgentIds.indexOf(agent.id)}
              <button
                type="button"
                onclick={() => toggleAgent(agent)}
                disabled={!agent.is_installed}
                title={agent.is_installed ? (order >= 0 ? `Selected ${order + 1} — click to remove` : 'Click to add') : 'Install this agent first'}
                class={cn(
                  "inline-flex items-center gap-2 h-8 px-3 rounded-sm border text-xs font-mono font-semibold transition-colors",
                  order >= 0
                    ? "border-phosphor bg-phosphor/10 text-phosphor-bright"
                    : "border-line bg-ink-850 text-fog hover:border-line-strong hover:text-bone disabled:opacity-40 disabled:cursor-not-allowed"
                )}
              >
                {#if order >= 0}
                  <span class="w-4 h-4 rounded-sm bg-phosphor text-ink-950 text-[10px] font-bold flex items-center justify-center">{order + 1}</span>
                {:else}
                  <AgentMark id={agent.id} name={agent.name} installed={agent.is_installed} size="xs" />
                {/if}
                <span>{agent.name}</span>
                <span class={cn("w-1.5 h-1.5 rounded-sm", agent.is_installed ? "bg-phosphor" : "bg-ink-600")}></span>
              </button>
            {/each}
          </div>
          <p class="mt-2 text-[10px] font-mono text-fog uppercase tracking-wider">
            {#if preselectAgentId}
              First is the agent you launched · click others to add
            {:else}
              Click ready agents to add them · Order is launch order
            {/if}
          </p>
        </div>

        <div>
          <div class="flex items-center justify-between mb-1.5">
            <label
              for="launch-task"
              class="text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-phosphor"
            >
              {taskStep}. Task
            </label>
            <span class="text-[10px] text-fog font-mono uppercase tracking-wider">Sent on start</span>
          </div>
          <textarea
            id="launch-task"
            bind:value={taskPrompt}
            rows={2}
            placeholder="Describe what the agent should implement or inspect on launch..."
            class="w-full p-3 rounded-sm bg-ink-850 border border-line text-xs text-bone focus:outline-none focus:border-phosphor placeholder:text-dim resize-none leading-relaxed font-mono"
          ></textarea>
        </div>

        <div>
          <p class="block text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-fog mb-1.5">
            Will start
          </p>
          <div class="bg-ink-850 border border-line rounded-sm overflow-hidden divide-y divide-line">
            {#each willLaunchList as item}
              <div class="flex items-center justify-between h-9 px-3 text-xs font-mono">
                <div class="flex items-center gap-2.5">
                  <Terminal class="w-3.5 h-3.5 text-phosphor flex-shrink-0" />
                  <span class="text-bone">{item.label}</span>
                </div>
                <span class={badge({ variant: 'default' })}>{item.binary}</span>
              </div>
            {/each}
          </div>
        </div>
      </div>

      <div class="flex items-center justify-between px-5 py-3 border-t border-line bg-ink-850">
        <span class="font-mono text-[10px] text-fog uppercase tracking-wider">
          <span class="text-phosphor font-bold">{selectedPreset}</span> · {launchCount} terminal{launchCount > 1 ? 's' : ''}
        </span>
        <div class="flex items-center gap-2">
          <button type="button" onclick={onClose} class={btn({ variant: 'ghost', size: 'sm' })}>
            Cancel
          </button>
          <button
            type="button"
            onclick={handleLaunch}
            disabled={selectedAgentIds.length === 0 || !workingDir.trim()}
            title={!workingDir.trim() ? 'Choose a folder first' : undefined}
            class={btn({ variant: 'primary', size: 'sm' })}
          >
            <Play class="w-3.5 h-3.5 fill-current" />
            <span>
              {#if selectedAgents.length === 1}
                Launch {selectedAgents[0].name}
              {:else}
                Launch {selectedAgents.length} agents
              {/if}
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
