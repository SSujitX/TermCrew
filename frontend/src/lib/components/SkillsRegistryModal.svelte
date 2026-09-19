<script lang="ts">
  import type { DetectedSkill, MarketplaceSkill, SkillRoot } from '../types';
  import { btn, badge, cn, displayHomePath } from '../ui';
  import {
    getSkills,
    getSkillContent,
    setSkillEnabled,
    copySkill,
    deleteSkill,
    installSkill,
    openSkillFolder,
    searchSkillsMarketplace,
    startSkillsMarketplaceInstall,
    killSession,
  } from '../api';
  import SetupTerminal from './SetupTerminal.svelte';
  import AgentMark from './AgentMark.svelte';
  import MarkdownPreview from './MarkdownPreview.svelte';
  import {
    X,
    Search,
    RefreshCw,
    Sparkles,
    BookMarked,
    LayoutGrid,
    List,
    Download,
    Trash2,
    FolderOpen,
    Copy,
    Eye,
    Power,
    GitBranch,
    HardDrive,
    Flame,
    TrendingUp,
    ChevronLeft,
    ChevronRight,
  } from 'lucide-svelte';

  interface Props {
    isOpen: boolean;
    workdir?: string;
    onClose: () => void;
  }

  let { isOpen = false, workdir = '', onClose }: Props = $props();

  let tab = $state<'installed' | 'marketplace'>('installed');
  let searchQuery = $state('');
  let selectedFilter = $state<string>('all');
  let viewMode = $state<'grid' | 'list'>('grid');

  let roots = $state<SkillRoot[]>([]);
  let skills = $state<DetectedSkill[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let busyAction = $state<string | null>(null);

  let preview = $state<{ path: string; content: string; truncated: boolean } | null>(null);
  let pendingDeleteId = $state<string | null>(null);
  let pendingDeleteConfirm = $state<string | null>(null);
  let copyTargetFor = $state<string | null>(null);
  let copyRootId = $state('');

  let installSource = $state<'local' | 'git'>('local');
  let installPath = $state('');
  let installRootId = $state('');
  let installName = $state('');

  let marketQuery = $state('');
  let marketCommitted = $state('');
  let marketView = $state<'hot' | 'trending' | 'all-time'>('hot');
  let marketPage = $state(0);
  let marketTotal = $state(0);
  let marketHasMore = $state(false);
  let marketResults = $state<MarketplaceSkill[]>([]);
  let marketSearching = $state(false);
  let marketFetchGen = $state(0);
  let marketHarness = $state('global');
  let marketInstallFor = $state<string | null>(null);

  const MARKET_VIEWS = [
    { id: 'hot', label: 'Hot', icon: Flame },
    { id: 'trending', label: 'Trending', icon: TrendingUp },
    { id: 'all-time', label: 'Most viewed', icon: Eye },
  ] as const;

  let setup = $state<{
    sessionId: string;
    title: string;
    command?: string;
  } | null>(null);
  let setupLocked = $derived(busyAction !== null || setup !== null);

  const HARNESS_AGENT: Record<string, string> = {
    claude: 'claude',
    codex: 'codex',
    cursor: 'cursor-agent',
    'cursor-agent': 'cursor-agent',
    opencode: 'opencode',
    openclaw: 'openclaw',
    hermes: 'hermes',
    gemini: 'gemini',
    kiro: 'kiro',
    goose: 'goose',
  };

  const MARKET_HARNESSES = [
    { id: 'global', label: 'Global' },
    { id: 'claude', label: 'Claude' },
    { id: 'codex', label: 'Codex' },
    { id: 'cursor', label: 'Cursor' },
    { id: 'opencode', label: 'OpenCode' },
    { id: 'openclaw', label: 'OpenClaw' },
    { id: 'gemini', label: 'Gemini' },
    { id: 'goose', label: 'Goose' },
    { id: 'kiro', label: 'Kiro' },
  ];

  let writableRoots = $derived(roots.filter((r) => r.writable));
  let harnesses = $derived(
    [...new Set(skills.map((s) => s.harness).filter(Boolean))].sort(),
  );
  let enabledCount = $derived(skills.filter((s) => s.enabled).length);
  let disabledCount = $derived(skills.length - enabledCount);
  let projectCount = $derived(skills.filter((s) => s.scope === 'project').length);
  let harnessCounts = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const skill of skills) {
      counts[skill.harness] = (counts[skill.harness] ?? 0) + 1;
    }
    return counts;
  });

  function filterChipClass(active: boolean, tone: 'default' | 'phosphor' | 'warning' = 'default') {
    return cn(
      'inline-flex items-center gap-1 px-2 py-1 rounded-sm font-mono text-[11px] uppercase tracking-wider',
      active
        ? tone === 'warning'
          ? 'bg-warning/10 text-warning'
          : tone === 'phosphor'
            ? 'bg-phosphor/10 text-phosphor'
            : 'bg-ink-800 text-bone'
        : 'text-fog hover:text-bone',
    );
  }

  let filteredSkills = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    return skills.filter((skill) => {
      const matchesSearch =
        !q ||
        skill.name.toLowerCase().includes(q) ||
        skill.description.toLowerCase().includes(q) ||
        skill.harness.toLowerCase().includes(q) ||
        skill.scope.toLowerCase().includes(q) ||
        skill.path.toLowerCase().includes(q);

      if (!matchesSearch) return false;
      if (selectedFilter === 'all') return true;
      if (selectedFilter === 'enabled') return skill.enabled;
      if (selectedFilter === 'disabled') return !skill.enabled;
      if (selectedFilter === 'project') return skill.scope === 'project';
      return skill.harness === selectedFilter;
    });
  });

  function agentMarkId(harness: string): string {
    return HARNESS_AGENT[harness.toLowerCase()] ?? harness;
  }

  function displayPath(path: string): string {
    return displayHomePath(path);
  }

  let loadGen = 0;

  async function loadCatalog(fresh = false) {
    const gen = ++loadGen;
    loading = true;
    loadError = null;
    try {
      const catalog = await getSkills(workdir || null, fresh);
      if (gen !== loadGen) return;
      roots = catalog.roots;
      skills = catalog.skills;
      const writable = catalog.roots.filter((r) => r.writable);
      if (!installRootId && writable.length > 0) {
        installRootId = writable[0].id;
      } else if (installRootId && !writable.some((r) => r.id === installRootId)) {
        installRootId = writable[0]?.id ?? '';
      }
    } catch (err) {
      if (gen !== loadGen) return;
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      if (gen === loadGen) loading = false;
    }
  }

  async function closeSetup() {
    const id = setup?.sessionId;
    setup = null;
    if (id) {
      try {
        await killSession(id);
      } catch {
        /* setup console may already be gone */
      }
    }
    await loadCatalog(true);
  }

  function handleClose() {
    void closeSetup();
    preview = null;
    pendingDeleteId = null;
    pendingDeleteConfirm = null;
    copyTargetFor = null;
    actionError = null;
    onClose();
  }

  async function toggleEnabled(skill: DetectedSkill) {
    if (busyAction) return;
    busyAction = `prefs:${skill.id}`;
    actionError = null;
    try {
      await setSkillEnabled(skill.path, !skill.enabled);
      await loadCatalog(true);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  async function showPreview(skill: DetectedSkill) {
    busyAction = `preview:${skill.id}`;
    actionError = null;
    try {
      preview = await getSkillContent(skill.skill_md || skill.path, workdir || null);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  async function openFolder(skill: DetectedSkill) {
    busyAction = `open:${skill.id}`;
    actionError = null;
    try {
      await openSkillFolder(skill.path, workdir || null);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  function startCopy(skill: DetectedSkill) {
    copyTargetFor = skill.id;
    copyRootId = writableRoots.find((r) => r.id !== skill.root_id)?.id ?? writableRoots[0]?.id ?? '';
  }

  async function confirmCopy(skill: DetectedSkill) {
    if (!copyRootId || busyAction) return;
    busyAction = `copy:${skill.id}`;
    actionError = null;
    try {
      await copySkill(workdir || null, {
        source_path: skill.path,
        target_root_id: copyRootId,
      });
      copyTargetFor = null;
      await loadCatalog(true);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  async function runDelete(skill: DetectedSkill) {
    if (pendingDeleteId !== skill.id) {
      pendingDeleteId = skill.id;
      pendingDeleteConfirm = null;
      return;
    }
    if (pendingDeleteConfirm !== skill.id) {
      pendingDeleteConfirm = skill.id;
      return;
    }
    if (busyAction) return;
    busyAction = `delete:${skill.id}`;
    actionError = null;
    try {
      await deleteSkill(workdir || null, skill.path);
      pendingDeleteId = null;
      pendingDeleteConfirm = null;
      await loadCatalog(true);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  function deleteLabel(skill: DetectedSkill): string {
    if (pendingDeleteConfirm === skill.id) return 'Confirm DELETE';
    if (pendingDeleteId === skill.id) return 'Sure?';
    return 'Delete';
  }

  async function runInstall() {
    if (!installPath.trim() || !installRootId || setupLocked) return;
    busyAction = 'install';
    actionError = null;
    try {
      const result = await installSkill(workdir || null, {
        source: installSource,
        path_or_url: installPath.trim(),
        target_root_id: installRootId,
        name: installName.trim() || undefined,
      });
      if (result.kind === 'setup') {
        setup = {
          sessionId: result.session.id,
          title: 'Skills · Install · git',
          command: result.command,
        };
        installPath = '';
        installName = '';
      } else {
        installPath = '';
        installName = '';
        await loadCatalog(true);
      }
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
    }
  }

  async function loadMarket(q: string, view: string, page: number) {
    const gen = ++marketFetchGen;
    marketSearching = true;
    actionError = null;
    try {
      const res = await searchSkillsMarketplace(q, { view, page, perPage: 9 });
      if (gen !== marketFetchGen) return;
      marketResults = res.skills;
      marketTotal = res.total;
      marketHasMore = res.has_more;
    } catch (err) {
      if (gen !== marketFetchGen) return;
      actionError = err instanceof Error ? err.message : String(err);
      marketResults = [];
      marketTotal = 0;
      marketHasMore = false;
    } finally {
      if (gen === marketFetchGen) marketSearching = false;
    }
  }

  async function runMarketSearch() {
    const q = marketQuery.trim();
    if (marketCommitted === q && marketPage === 0) {
      await loadMarket(q, marketView, 0);
      return;
    }
    marketCommitted = q;
    marketPage = 0;
  }

  function setMarketView(view: 'hot' | 'trending' | 'all-time') {
    if (marketView === view) return;
    marketView = view;
    marketPage = 0;
  }

  async function runMarketInstall(item: MarketplaceSkill) {
    if (setupLocked) return;
    marketInstallFor = item.id;
    busyAction = `market:${item.id}`;
    actionError = null;
    try {
      const result = await startSkillsMarketplaceInstall({
        source: item.source,
        skill: item.skill_id || undefined,
        harness: marketHarness,
        global: true,
      });
      setup = {
        sessionId: result.session.id,
        title: `Skills · Install · ${item.name}`,
        command: result.command,
      };
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busyAction = null;
      marketInstallFor = null;
    }
  }

  $effect(() => {
    if (!isOpen) return;
    void workdir;
    void loadCatalog(true);
    void searchSkillsMarketplace('', { view: 'hot', page: 0, perPage: 9 });
  });

  $effect(() => {
    if (!isOpen || tab !== 'marketplace') return;
    const q = marketQuery.trim();
    const t = setTimeout(() => {
      if (marketCommitted !== q) {
        marketCommitted = q;
        marketPage = 0;
      }
    }, 180);
    return () => clearTimeout(t);
  });

  $effect(() => {
    if (!isOpen || tab !== 'marketplace') return;
    const view = marketView;
    const page = marketPage;
    const q = marketCommitted;
    void loadMarket(q, view, page);
  });

  $effect(() => {
    if (!setup) return;
    let inflight = false;
    const timer = setInterval(() => {
      if (inflight) return;
      inflight = true;
      loadCatalog(true).finally(() => {
        inflight = false;
      });
    }, 8000);
    return () => clearInterval(timer);
  });
</script>

{#snippet skillActions(skill: DetectedSkill)}
  <div class="flex flex-wrap items-center gap-1.5 mt-auto">
    <button
      type="button"
      onclick={() => toggleEnabled(skill)}
      disabled={busyAction !== null}
      class={btn({ variant: skill.enabled ? 'outline' : 'primary', size: 'xs' })}
      title={skill.enabled ? 'Hide in TermCrew list (does not remove from harness)' : 'Show in TermCrew list'}
    >
      <Power class="w-3 h-3" />
      <span>{skill.enabled ? 'Hide' : 'Show'}</span>
    </button>
    <button
      type="button"
      onclick={() => showPreview(skill)}
      disabled={busyAction !== null}
      class={btn({ variant: 'outline', size: 'xs' })}
      title="Preview SKILL.md"
    >
      <Eye class="w-3 h-3" />
      <span>Preview</span>
    </button>
    <button
      type="button"
      onclick={() => (copyTargetFor === skill.id ? (copyTargetFor = null) : startCopy(skill))}
      disabled={busyAction !== null || writableRoots.length === 0}
      class={btn({ variant: copyTargetFor === skill.id ? 'primary' : 'outline', size: 'xs' })}
      title="Copy to another root"
    >
      <Copy class="w-3 h-3" />
      <span>Copy</span>
    </button>
    {#if !skill.readonly}
      <button
        type="button"
        onclick={() => runDelete(skill)}
        disabled={busyAction !== null}
        class={btn({
          variant: pendingDeleteId === skill.id ? 'danger' : 'outline',
          size: 'xs',
        })}
        title="Delete skill folder"
      >
        <Trash2 class="w-3 h-3" />
        <span>{deleteLabel(skill)}</span>
      </button>
    {/if}
    <button
      type="button"
      onclick={() => openFolder(skill)}
      disabled={busyAction !== null}
      class={btn({ variant: 'outline', size: 'xs' })}
      title="Open folder"
    >
      <FolderOpen class="w-3 h-3" />
      <span>Open</span>
    </button>
  </div>
  {#if copyTargetFor === skill.id}
    <div class="flex flex-wrap items-center gap-2 mt-2 pt-2 border-t border-line">
      <select
        bind:value={copyRootId}
        class="flex-1 min-w-[10rem] bg-ink-950 border border-line rounded-sm text-[11px] font-mono text-bone px-2 py-1.5 focus:outline-none focus:border-phosphor"
      >
        {#each writableRoots as root (root.id)}
          <option value={root.id}>{root.label}</option>
        {/each}
      </select>
      <button
        type="button"
        onclick={() => confirmCopy(skill)}
        disabled={!copyRootId || busyAction !== null}
        class={btn({ variant: 'primary', size: 'xs' })}
      >
        Copy here
      </button>
    </div>
  {/if}
{/snippet}

{#if isOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4 overflow-y-auto">
    <div
      class="fixed inset-0 bg-ink-950/85"
      onclick={handleClose}
      role="presentation"
    ></div>

    <div
      class="bezel relative z-10 bg-ink-900 border border-line-strong rounded-sm w-full max-w-5xl h-[88vh] overflow-hidden flex flex-col toast-enter"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      <div class="px-5 py-3 border-b border-line flex items-center justify-between bg-ink-850">
        <div class="flex items-center gap-3">
          <div class="w-7 h-7 rounded-sm bg-ink-800 border border-line-strong flex items-center justify-center text-phosphor">
            <Sparkles class="w-4 h-4" />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-xs font-mono font-bold text-bone tracking-[0.14em] uppercase">Skills</h2>
              <span class={badge({ variant: 'success' })}>
                {enabledCount}/{skills.length} shown
              </span>
            </div>
            <p class="font-mono text-[10px] text-fog uppercase tracking-wider">
              Harness + project skill folders
            </p>
          </div>
        </div>
        <div class="flex items-center gap-1">
          <button
            type="button"
            onclick={() => loadCatalog(true)}
            class={btn({ variant: 'ghost', size: 'icon' })}
            title="Rescan skills"
            disabled={loading}
          >
            <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
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

      <div class="px-5 py-2 border-b border-line bg-ink-850/80 flex items-center gap-1">
        <button
          type="button"
          onclick={() => (tab = 'installed')}
          class={cn(
            'px-3 py-1.5 rounded-sm font-mono text-[11px] uppercase tracking-wider',
            tab === 'installed' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone'
          )}
        >
          Installed
        </button>
        <button
          type="button"
          onclick={() => (tab = 'marketplace')}
          class={cn(
            'px-3 py-1.5 rounded-sm font-mono text-[11px] uppercase tracking-wider',
            tab === 'marketplace' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone'
          )}
        >
          Marketplace
        </button>
      </div>

      {#if tab === 'installed'}
      <div class="flex-1 min-h-0 flex flex-col min-w-0">
        <div class="px-5 py-3 border-b border-line bg-ink-850/60 flex flex-col gap-2.5">
          <div class="flex items-center gap-2">
            <div class="relative flex-1 min-w-0">
              <Search class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-fog" />
              <input
                type="text"
                bind:value={searchQuery}
                placeholder="Search skills…"
                class="w-full pl-9 pr-3 py-2 bg-ink-950 border border-line rounded-sm text-xs text-bone font-mono placeholder:text-dim focus:outline-none focus:border-phosphor"
              />
            </div>
            <div class="flex items-center border border-line rounded-sm p-0.5 bg-ink-950 shrink-0">
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

          <div class="flex items-center gap-2 flex-wrap">
            <div class="flex items-center gap-0.5 bg-ink-950 border border-line rounded-sm p-0.5 flex-wrap">
              <button
                type="button"
                onclick={() => (selectedFilter = 'all')}
                class={filterChipClass(selectedFilter === 'all')}
              >
                All <span class="tabular-nums text-dim">{skills.length}</span>
              </button>
              {#each harnesses as h (h)}
                <button
                  type="button"
                  onclick={() => (selectedFilter = h)}
                  class={filterChipClass(selectedFilter === h, 'phosphor')}
                >
                  {h} <span class="tabular-nums {selectedFilter === h ? 'text-phosphor/70' : 'text-dim'}">{harnessCounts[h] ?? 0}</span>
                </button>
              {/each}
            </div>
            <div class="flex items-center gap-0.5 bg-ink-950 border border-line rounded-sm p-0.5">
              <button
                type="button"
                onclick={() => (selectedFilter = 'enabled')}
                class={filterChipClass(selectedFilter === 'enabled', 'phosphor')}
              >
                Shown <span class="tabular-nums {selectedFilter === 'enabled' ? 'text-phosphor/70' : 'text-dim'}">{enabledCount}</span>
              </button>
              <button
                type="button"
                onclick={() => (selectedFilter = 'disabled')}
                class={filterChipClass(selectedFilter === 'disabled', 'warning')}
              >
                Hidden <span class="tabular-nums {selectedFilter === 'disabled' ? 'text-warning/70' : 'text-dim'}">{disabledCount}</span>
              </button>
              <button
                type="button"
                onclick={() => (selectedFilter = 'project')}
                class={filterChipClass(selectedFilter === 'project')}
              >
                Project <span class="tabular-nums text-dim">{projectCount}</span>
              </button>
            </div>
          </div>
        </div>

        <div class="p-4 overflow-y-auto flex-1 min-h-0">
          {#if loadError}
            <div class="text-center py-8 text-alert text-sm font-mono">{loadError}</div>
          {:else if loading && skills.length === 0}
            <div class="text-center py-16 text-fog text-sm font-mono">Scanning skills…</div>
          {:else if filteredSkills.length === 0}
            <div class="text-center py-16 text-fog text-sm font-mono">
              No skills match “{searchQuery || selectedFilter}”
            </div>
          {:else if viewMode === 'grid'}
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {#each filteredSkills as skill (skill.id)}
                <div
                  class="rounded-sm border border-line bg-ink-850 hover:border-phosphor/30 transition-colors p-3 flex flex-col gap-3 min-h-[168px]"
                >
                  <div class="flex items-start gap-2.5">
                    <AgentMark
                      id={agentMarkId(skill.harness)}
                      name={skill.harness}
                      installed={skill.enabled}
                    />
                    <div class="min-w-0 flex-1">
                      <div class="text-sm font-mono font-bold text-bone truncate">{skill.name}</div>
                      <div class="text-[10px] font-mono text-fog truncate">
                        {skill.harness} · {skill.scope}
                      </div>
                    </div>
                    <span class={badge({ variant: skill.enabled ? 'success' : 'warning' })}>
                      {skill.enabled ? 'Shown' : 'Hidden'}
                    </span>
                  </div>
                  <p class="text-[11px] text-fog font-mono leading-relaxed line-clamp-2">
                    {skill.description || 'Agent skill'}
                  </p>
                  <button
                    type="button"
                    class="text-left text-[10px] font-mono text-phosphor/80 hover:text-phosphor break-all leading-relaxed"
                    title={skill.path}
                  >
                    {displayPath(skill.path)}
                  </button>
                  {@render skillActions(skill)}
                </div>
              {/each}
            </div>
          {:else}
            <div class="space-y-1.5">
              {#each filteredSkills as skill (skill.id)}
                <div
                  class="rounded-sm border border-line bg-ink-850 hover:border-line-strong transition-colors px-3 py-2 flex flex-col gap-2"
                >
                  <div class="flex items-center gap-3">
                    <AgentMark
                      id={agentMarkId(skill.harness)}
                      name={skill.harness}
                      installed={skill.enabled}
                      size="sm"
                    />
                    <div class="min-w-0 flex-1">
                      <div class="text-xs font-mono font-bold text-bone truncate">
                        {skill.name}
                        <span class="text-fog font-normal ml-1">{skill.harness} · {skill.scope}</span>
                      </div>
                      <div class="text-[10px] font-mono text-dim truncate">
                        {skill.description || displayPath(skill.path)}
                      </div>
                    </div>
                    <span class={badge({ variant: skill.enabled ? 'success' : 'warning' })}>
                      {skill.enabled ? 'Shown' : 'Hidden'}
                    </span>
                  </div>
                  {@render skillActions(skill)}
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="px-4 py-3 border-t border-line bg-ink-850 flex-shrink-0">
          <div class="flex items-center gap-2 mb-2">
            <BookMarked class="w-3.5 h-3.5 text-phosphor" />
            <h3 class="text-[11px] font-mono font-bold text-bone uppercase tracking-wider">
              Install local / git
            </h3>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <div class="flex items-center gap-1 bg-ink-950 border border-line rounded-sm p-0.5">
              <button
                type="button"
                onclick={() => (installSource = 'local')}
                class={cn(
                  'px-2 py-1 rounded-sm font-mono text-[11px] uppercase tracking-wider inline-flex items-center gap-1',
                  installSource === 'local' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone'
                )}
              >
                <HardDrive class="w-3 h-3" />
                Local
              </button>
              <button
                type="button"
                onclick={() => (installSource = 'git')}
                class={cn(
                  'px-2 py-1 rounded-sm font-mono text-[11px] uppercase tracking-wider inline-flex items-center gap-1',
                  installSource === 'git' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone'
                )}
              >
                <GitBranch class="w-3 h-3" />
                Git
              </button>
            </div>
            <input
              type="text"
              bind:value={installPath}
              placeholder={installSource === 'local' ? 'Path to skill folder…' : 'Git URL…'}
              class="flex-1 min-w-[14rem] px-3 py-1.5 bg-ink-950 border border-line rounded-sm text-xs text-bone font-mono placeholder:text-dim focus:outline-none focus:border-phosphor"
            />
            <select
              bind:value={installRootId}
              class="min-w-[10rem] bg-ink-950 border border-line rounded-sm text-[11px] font-mono text-bone px-2 py-1.5 focus:outline-none focus:border-phosphor"
            >
              {#each writableRoots as root (root.id)}
                <option value={root.id}>{root.label}</option>
              {/each}
            </select>
            <input
              type="text"
              bind:value={installName}
              placeholder="Name (optional)"
              class="w-36 px-3 py-1.5 bg-ink-950 border border-line rounded-sm text-xs text-bone font-mono placeholder:text-dim focus:outline-none focus:border-phosphor"
            />
            <button
              type="button"
              onclick={runInstall}
              disabled={!installPath.trim() || !installRootId || busyAction !== null}
              class={btn({ variant: 'primary', size: 'xs' })}
            >
              <Download class="w-3 h-3" />
              <span>Install</span>
            </button>
          </div>
        </div>
      </div>
      {:else}
      <div class="flex-1 min-h-0 flex flex-col min-w-0">
        <div class="px-5 py-3 border-b border-line bg-ink-850/60 flex flex-col gap-2.5">
          <div class="flex flex-wrap items-center gap-3">
            <div class="relative flex-1 min-w-[200px] max-w-lg">
              <Search class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-fog" />
              <input
                type="text"
                bind:value={marketQuery}
                onkeydown={(e) => {
                  if (e.key === 'Enter') void runMarketSearch();
                }}
                placeholder="Search skills.sh…"
                class="w-full pl-9 pr-3 py-2 bg-ink-950 border border-line rounded-sm text-xs text-bone font-mono placeholder:text-dim focus:outline-none focus:border-phosphor"
              />
            </div>
            <button
              type="button"
              onclick={runMarketSearch}
              disabled={marketSearching}
              class={btn({ variant: 'primary', size: 'sm' })}
            >
              <Search class="w-3.5 h-3.5" />
              <span>{marketSearching ? 'Searching…' : 'Search'}</span>
            </button>
            <select
              bind:value={marketHarness}
              class="bg-ink-950 border border-line rounded-sm text-[11px] font-mono text-bone px-2 py-2 focus:outline-none focus:border-phosphor"
              title="Install target harness"
            >
              {#each MARKET_HARNESSES as h (h.id)}
                <option value={h.id}>{h.label}</option>
              {/each}
            </select>
          </div>
          <div class="flex items-center gap-0.5">
            {#each MARKET_VIEWS as v (v.id)}
              {@const Icon = v.icon}
              <button
                type="button"
                onclick={() => setMarketView(v.id)}
                class={filterChipClass(marketView === v.id, 'phosphor')}
              >
                <Icon class="w-3 h-3" />
                {v.label}
              </button>
            {/each}
          </div>
        </div>

        <div class="p-4 overflow-y-auto flex-1 min-h-0">
          {#if marketSearching && marketResults.length === 0}
            <div class="text-center py-16 text-fog text-sm font-mono">Loading marketplace…</div>
          {:else if marketResults.length === 0}
            <div class="text-center py-16 text-fog text-sm font-mono">
              {marketCommitted ? `No skills match “${marketCommitted}”` : 'Search skills.sh to browse packages'}
            </div>
          {:else}
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {#each marketResults as item (item.id)}
                <div
                  class="rounded-sm border border-line bg-ink-850 hover:border-phosphor/30 transition-colors p-3 flex flex-col gap-3 min-h-[140px]"
                >
                  <div class="flex items-start gap-2.5">
                    <div class="w-8 h-8 rounded-sm bg-ink-800 border border-line flex items-center justify-center text-phosphor flex-shrink-0">
                      <Sparkles class="w-4 h-4" />
                    </div>
                    <div class="min-w-0 flex-1">
                      <div class="text-sm font-mono font-bold text-bone truncate">{item.name}</div>
                      <div class="text-[10px] font-mono text-fog truncate">
                        {item.source}
                        {#if item.skill_id}
                          <span class="text-dim"> · {item.skill_id}</span>
                        {/if}
                      </div>
                    </div>
                  </div>
                  <div class="mt-auto flex items-center justify-between gap-2">
                    <span class={badge({ variant: 'default' })}>
                      {item.installs.toLocaleString()} installs
                    </span>
                    <button
                      type="button"
                      onclick={() => runMarketInstall(item)}
                      disabled={setupLocked}
                      class={btn({ variant: 'primary', size: 'xs' })}
                      title={setupLocked ? 'Finish the current setup first' : `Install to ${marketHarness}`}
                    >
                      <Download class="w-3 h-3" />
                      <span>{marketInstallFor === item.id ? 'Starting…' : 'Install'}</span>
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        {#if marketTotal > 0}
          <div class="px-5 py-2.5 border-t border-line bg-ink-850/80 flex items-center justify-between gap-3">
            <span class="text-[11px] font-mono text-fog uppercase tracking-wider">
              Page {marketPage + 1}
              <span class="text-dim"> · {marketResults.length} of {marketTotal}</span>
            </span>
            <div class="flex items-center gap-1">
              <button
                type="button"
                onclick={() => (marketPage = Math.max(0, marketPage - 1))}
                disabled={marketPage === 0 || marketSearching}
                class={btn({ variant: 'ghost', size: 'xs' })}
              >
                <ChevronLeft class="w-3.5 h-3.5" />
                Prev
              </button>
              <button
                type="button"
                onclick={() => (marketPage += 1)}
                disabled={!marketHasMore || marketSearching}
                class={btn({ variant: 'ghost', size: 'xs' })}
              >
                Next
                <ChevronRight class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        {/if}
      </div>
      {/if}

      {#if preview}
        <div class="absolute inset-0 z-20 flex flex-col p-4 bg-ink-950/80">
          <div class="bezel bg-ink-900 border border-line-strong rounded-sm flex-1 min-h-0 w-full flex flex-col overflow-hidden">
            <div class="px-4 py-2.5 border-b border-line flex items-center justify-between bg-ink-850">
              <div class="min-w-0">
                <div class="text-[11px] font-mono font-bold text-bone uppercase tracking-wider">
                  SKILL.md
                </div>
                <div class="text-[10px] font-mono text-fog truncate">{displayPath(preview.path)}</div>
              </div>
              <button
                type="button"
                onclick={() => (preview = null)}
                class={btn({ variant: 'ghost', size: 'icon' })}
                aria-label="Close preview"
              >
                <X class="w-4 h-4" />
              </button>
            </div>
            {#key preview.path}
              <div class="flex-1 min-h-0 flex flex-col">
                <MarkdownPreview content={preview.content} truncated={preview.truncated} />
              </div>
            {/key}
          </div>
        </div>
      {/if}

      {#if actionError}
        <div class="px-5 py-2 border-t border-alert/40 bg-alert/10 text-[11px] font-mono text-alert">
          {actionError}
        </div>
      {/if}

      {#if setup}
        {#key setup.sessionId}
          <SetupTerminal
            sessionId={setup.sessionId}
            title={setup.title}
            command={setup.command}
            onClose={closeSetup}
            onRescan={() => loadCatalog(true)}
          />
        {/key}
      {/if}

      <div class="px-5 py-3 border-t border-line bg-ink-850 flex items-center justify-between text-[10px] text-fog font-mono uppercase tracking-wider">
        <span>Prefs toggle · copy/delete write to allowlisted roots · marketplace uses npx skills</span>
        <button type="button" onclick={handleClose} class={btn({ variant: 'default', size: 'sm' })}>
          Done
        </button>
      </div>
    </div>
  </div>
{/if}
