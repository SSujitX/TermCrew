<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { AgentMeta, LaunchPresetRequest, Session } from './lib/types';
  import {
    getAgents,
    getSessions,
    getWorkspacePath,
    launchPreset,
    addToGroup,
    killSession,
    restartSession,
    broadcastMessage,
    handoffReview,
    renameSessionGroup,
  } from './lib/api';
  import LauncherModal from './lib/components/LauncherModal.svelte';
  import AgentRegistryModal from './lib/components/AgentRegistryModal.svelte';
  import SkillsRegistryModal from './lib/components/SkillsRegistryModal.svelte';
  import TerminalGrid from './lib/components/TerminalGrid.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import SupervisorCockpit from './lib/components/SupervisorCockpit.svelte';
  import BroadcastBar from './lib/components/BroadcastBar.svelte';
  import {
    Bot,
    Sparkles,
    Activity,
    AlertCircle,
    CheckCircle,
    Terminal,
    Trash2,
  } from 'lucide-svelte';
  import { btn } from './lib/ui';
  import { APP_VERSION } from './lib/version';

  // Svelte 5 state runes
  let agents = $state<AgentMeta[]>([]);
  let sessions = $state<Session[]>([]);
  let workspacePath = $state('');

  let isLauncherOpen = $state(false);
  let launcherAgentId = $state<string | null>(null);
  let isRegistryOpen = $state(false);
  let isSkillsOpen = $state(false);
  let isSupervisorOpen = $state(false);
  let openFiles = $state<string[]>([]);
  let activeFile = $state<string | null>(null);
  let dirtyFiles = $state<Record<string, boolean>>({});
  let focusedSessionId = $state<string | null>(null);
  let focusedGroupId = $state<string | null>(null);
  let clearEpoch = $state(0);
  let clearSessionId = $state<string | null>(null);
  let notification = $state<{ text: string; type: 'info' | 'success' | 'error' } | null>(null);
  let pendingDelete = $state<null | {
    kind: 'session' | 'group';
    id: string;
    title: string;
    body: string;
  }>(null);
  let restartingIds = $state<Set<string>>(new Set());

  function showToast(text: string, type: 'info' | 'success' | 'error' = 'info') {
    notification = { text, type };
    setTimeout(() => {
      notification = null;
    }, 4000);
  }

  function errorMessage(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function loadData(fresh = false) {
    try {
      agents = await getAgents(fresh);
    } catch {
      // Keep whatever agents are already loaded
    }
    try {
      sessions = await getSessions();
    } catch {
      // Keep whatever sessions are already loaded
    }
    try {
      const path = await getWorkspacePath();
      if (path) workspacePath = path;
    } catch {
      // Keep prior path
    }
  }

  function groupOf(s: Session): string {
    return s.group_id || s.id;
  }

  function ensureFocus(list: Session[], preferredId?: string | null) {
    if (list.length === 0) {
      focusedSessionId = null;
      focusedGroupId = null;
      return;
    }
    const prefer = preferredId && list.some((s) => s.id === preferredId) ? preferredId : null;
    const keep = focusedSessionId && list.some((s) => s.id === focusedSessionId) ? focusedSessionId : null;
    const nextId = prefer ?? keep ?? list[0].id;
    focusedSessionId = nextId;
    const node = list.find((s) => s.id === nextId);
    focusedGroupId = node ? groupOf(node) : groupOf(list[0]);
  }

  function selectSession(id: string) {
    const node = sessions.find((s) => s.id === id);
    if (!node) return;
    focusedSessionId = id;
    focusedGroupId = groupOf(node);
    activeFile = null;
  }

  function selectGroup(groupId: string) {
    const members = sessions.filter((s) => groupOf(s) === groupId);
    if (members.length === 0) return;
    focusedGroupId = groupId;
    const keep =
      focusedSessionId && members.some((s) => s.id === focusedSessionId)
        ? focusedSessionId
        : members[0].id;
    focusedSessionId = keep;
    activeFile = null;
  }

  function samePath(a: string, b: string) {
    const n = (s: string) => s.replace(/[\\/]+/g, '/').toLowerCase();
    return n(a) === n(b);
  }

  function openFile(path: string) {
    if (!openFiles.some((p) => samePath(p, path))) openFiles = [...openFiles, path];
    activeFile = openFiles.find((p) => samePath(p, path)) ?? path;
  }

  function hideEditor() {
    activeFile = null;
  }

  function closeFile(path: string) {
    const dirty = Object.entries(dirtyFiles).some(([k, v]) => v && samePath(k, path));
    if (dirty) {
      const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
      if (!confirm(`Discard unsaved changes to ${name}?`)) return;
    }
    openFiles = openFiles.filter((p) => !samePath(p, path));
    const next = { ...dirtyFiles };
    for (const k of Object.keys(next)) {
      if (samePath(k, path)) delete next[k];
    }
    dirtyFiles = next;
    if (activeFile && samePath(activeFile, path)) activeFile = null;
  }

  function markDirty(path: string, dirty: boolean) {
    if (dirtyFiles[path] === dirty) return;
    dirtyFiles = { ...dirtyFiles, [path]: dirty };
  }

  let activeGroupSessions = $derived(
    (focusedGroupId
      ? sessions.filter((s) => groupOf(s) === focusedGroupId)
      : sessions
    )
      .slice()
      .sort((a, b) => a.created_at.localeCompare(b.created_at))
  );

  onMount(() => {
    loadData().then(() => ensureFocus(sessions));
    // Poll live session status so exited sessions and backend restarts are reflected.
    const timer = setInterval(async () => {
      if (restartingIds.size > 0) return;
      try {
        sessions = await getSessions();
        ensureFocus(sessions);
      } catch {
        // Backend hiccup or offline; keep showing current state
      }
    }, 5000);
    return () => clearInterval(timer);
  });

  async function handleAddEngine(engine: string) {
    if (!focusedGroupId) return;
    try {
      const added = await addToGroup(focusedGroupId, engine);
      if (!sessions.some((s) => s.id === added.id)) {
        sessions = [...sessions, added];
      }
      focusedSessionId = added.id;
      showToast(`Added ${added.role ?? added.name}.`, 'success');
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  }

  async function handleLaunch(request: LaunchPresetRequest) {
    const n = request.agentIds?.length ?? 1;
    showToast(`Spawning ${request.preset.toUpperCase()} (${n} agent${n === 1 ? '' : 's'})…`);
    try {
      const newSessions = await launchPreset(request);
      sessions = [...sessions, ...newSessions];
      ensureFocus(sessions, newSessions[0]?.id ?? null);
      showToast(`Session${newSessions.length === 1 ? '' : 's'} ready (${newSessions.length}).`, 'success');
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  }

  function requestKillSession(sessionId: string) {
    const node = sessions.find((s) => s.id === sessionId);
    const name = node?.role ?? node?.name ?? 'this terminal';
    const hasWorktree = Boolean(node?.worktree_path);
    pendingDelete = {
      kind: 'session',
      id: sessionId,
      title: `Delete ${name}?`,
      body: hasWorktree
        ? 'This closes the terminal and deletes its isolated git worktree (agent branch + uncommitted work in that worktree).'
        : 'This closes the terminal and removes it from the list.',
    };
  }

  async function handleKillSession(sessionId: string) {
    try {
      await killSession(sessionId);
      const next = sessions.filter((s) => s.id !== sessionId);
      sessions = next;
      ensureFocus(next);
      showToast('Removed.');
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  }

  async function handleRenameGroup(groupId: string, label: string) {
    try {
      const next = await renameSessionGroup(groupId, label);
      sessions = sessions.map((s) =>
        (s.group_id || s.id) === groupId ? { ...s, group_label: next } : s
      );
      showToast(`Renamed to ${next}.`, 'success');
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  }

  function requestKillGroup(groupId: string) {
    const members = sessions.filter((s) => (s.group_id || s.id) === groupId);
    if (members.length === 0) return;
    const label = members[0].group_label || 'this session';
    const worktreeCount = members.filter((s) => s.worktree_path).length;
    pendingDelete = {
      kind: 'group',
      id: groupId,
      title: `Delete ${label}?`,
      body:
        worktreeCount > 0
          ? `This closes ${members.length} terminal${members.length === 1 ? '' : 's'} and deletes ${worktreeCount} isolated git worktree${worktreeCount === 1 ? '' : 's'} (agent branches + uncommitted work in those worktrees).`
          : members.length > 1
            ? `This closes all ${members.length} terminals and removes the session.`
            : 'This closes the terminal and removes the session.',
    };
  }

  async function handleKillGroup(groupId: string) {
    const members = sessions.filter((s) => (s.group_id || s.id) === groupId);
    try {
      await Promise.all(members.map((s) => killSession(s.id)));
      const next = sessions.filter((s) => (s.group_id || s.id) !== groupId);
      sessions = next;
      ensureFocus(next);
      showToast('Session removed.');
    } catch (err) {
      showToast(errorMessage(err), 'error');
      try {
        sessions = await getSessions();
        ensureFocus(sessions);
      } catch {
        /* keep local list */
      }
    }
  }

  async function confirmDelete() {
    const pending = pendingDelete;
    pendingDelete = null;
    if (!pending) return;
    if (pending.kind === 'session') await handleKillSession(pending.id);
    else await handleKillGroup(pending.id);
  }

  function focusDialog(node: HTMLDivElement) {
    queueMicrotask(() => node.focus());
  }

  function handleClear(sessionIds: string[]) {
    if (sessionIds.length === 1) {
      clearSessionId = sessionIds[0];
    } else {
      // null = every mounted pane; grid only mounts the active session group
      clearSessionId = null;
    }
    clearEpoch += 1;
    showToast(
      sessionIds.length === 1 ? 'Cleared the chat box.' : `Cleared chat boxes in ${sessionIds.length} panes.`
    );
  }

  async function handleRestartSession(sessionId: string) {
    restartingIds = new Set([...restartingIds, sessionId]);
    try {
      const fresh = await restartSession(sessionId);
      // Drop then re-add so the TTY remounts and reconnects (id is preserved).
      sessions = sessions.filter((s) => s.id !== sessionId);
      await tick();
      sessions = [...sessions, fresh];
      ensureFocus(sessions, fresh.id);
      showToast(
        fresh.status === 'running' ? `Relaunched ${fresh.name}.` : `Restarted ${fresh.name}.`,
        'success'
      );
    } catch (err) {
      showToast(errorMessage(err), 'error');
      try {
        sessions = await getSessions();
        ensureFocus(sessions, sessionId);
      } catch {
        /* keep current */
      }
    } finally {
      const next = new Set(restartingIds);
      next.delete(sessionId);
      restartingIds = next;
    }
  }

  function pickReviewTarget(sourceId: string): Session | null {
    const source = sessions.find((s) => s.id === sourceId);
    if (!source) return null;
    const group = sessions
      .filter((s) => s.group_id === source.group_id)
      .slice()
      .sort((a, b) => a.created_at.localeCompare(b.created_at));
    if (group.length < 2) return null;
    const i = group.findIndex((s) => s.id === sourceId);
    if (i < 0) return null;
    return group[(i + 1) % group.length] ?? null;
  }

  async function handleHandoffSession(sessionId: string) {
    const target = pickReviewTarget(sessionId);
    if (!target) {
      showToast('No companion in this session to send a review to.', 'error');
      return;
    }
    try {
      await handoffReview(sessionId, target.id);
      showToast(`Review sent to ${target.name}.`, 'success');
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  }

  async function handleBroadcast(message: string, sessionIds?: string[]) {
    try {
      const delivered = await broadcastMessage(message, sessionIds);
      const scope = sessionIds?.length === 1 ? 'active TTY' : `${delivered} terminal${delivered === 1 ? '' : 's'}`;
      showToast(`Ran on ${scope}.`, 'success');
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  }

  let focusedSession = $derived(
    sessions.find((s) => s.id === focusedSessionId) ?? null
  );

  // Files tab explores the focused session's folder (its worktree when isolated).
  let fileTreeRoot = $derived(
    focusedSession
      ? focusedSession.worktree_path ?? focusedSession.working_dir
      : workspacePath || null
  );

  function openLauncher(agentId?: string) {
    launcherAgentId = agentId ?? null;
    isRegistryOpen = false;
    isLauncherOpen = true;
  }

  function handleDirectLaunchAgent(agentId: string) {
    openLauncher(agentId);
  }

  const runningCount = $derived(sessions.filter((s) => s.status === 'running').length);
</script>

<div class="flex flex-col h-screen w-screen bg-ink-950 text-bone select-none font-mono overflow-hidden">
  <!-- Hardware bezel header -->
  <header class="h-14 px-4 bg-ink-900 border-b border-line-strong flex items-center justify-between flex-shrink-0 z-20">
    <div class="flex items-center gap-5">
      <div class="flex items-center gap-3">
        <div class="w-8 h-8 rounded-sm border border-phosphor/50 bg-ink-950 flex items-center justify-center overflow-hidden">
          <img src="/logo-dark.png" alt="" class="w-7 h-7 object-contain" />
        </div>
        <div>
          <div class="flex items-baseline gap-2">
            <span class="font-[family-name:var(--font-display)] text-sm tracking-[0.2em] text-phosphor-bright uppercase">
              TermCrew
            </span>
            <span class="text-[10px] font-mono text-fog tracking-wider">v{APP_VERSION}</span>
          </div>
          <span
            class="block max-w-xl text-[10px] font-mono text-dim leading-snug truncate"
            title="A local web console for running many AI coding CLIs at once — real terminals, real PTYs, your repo, your machine."
          >
            A local web console for running many AI coding CLIs at once — real terminals, real PTYs, your repo, your machine.
          </span>
        </div>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <div class="flex items-center gap-2 px-2.5 py-1 bg-ink-850 border border-line rounded-sm font-mono text-xs">
        <span
          class="w-1.5 h-1.5 rounded-sm {runningCount > 0
            ? 'bg-phosphor pulse-glow'
            : 'bg-dim'}"
        ></span>
        <span class="text-fog">
          <span class="text-phosphor font-semibold">{runningCount}</span>
          <span class="text-dim">/{sessions.length}</span>
          <span class="ml-1 tracking-wider uppercase text-[10px]">live</span>
        </span>
      </div>

      <button
        type="button"
        onclick={() => (isRegistryOpen = true)}
        class={btn({ variant: 'default', size: 'sm' })}
      >
        <Bot class="w-3.5 h-3.5 text-phosphor" />
        <span class="hidden sm:inline">Agents</span>
      </button>

      <button
        type="button"
        onclick={() => (isSkillsOpen = true)}
        class={btn({ variant: 'default', size: 'sm' })}
      >
        <Sparkles class="w-3.5 h-3.5 text-phosphor" />
        <span class="hidden sm:inline">Skills</span>
      </button>

      <button
        type="button"
        onclick={() => (isSupervisorOpen = !isSupervisorOpen)}
        class={btn({
          variant: isSupervisorOpen ? 'violet' : 'default',
          size: 'sm',
        })}
      >
        <Terminal class="w-3.5 h-3.5 {isSupervisorOpen ? 'text-phosphor' : 'text-fog'}" />
        <span class="hidden sm:inline">Goals</span>
      </button>
    </div>
  </header>

  <!-- Bezel toast -->
  {#if notification}
    <div
      class="fixed bottom-20 right-6 z-50 px-3 py-2.5 bezel text-xs text-bone flex items-center gap-3 toast-enter max-w-md"
    >
      {#if notification.type === 'success'}
        <div class="p-1 rounded-sm border border-phosphor/40 bg-phosphor/10 text-phosphor">
          <CheckCircle class="w-3.5 h-3.5" />
        </div>
      {:else if notification.type === 'error'}
        <div class="p-1 rounded-sm border border-alert/40 bg-alert/10 text-alert">
          <AlertCircle class="w-3.5 h-3.5" />
        </div>
      {:else}
        <div class="p-1 rounded-sm border border-phosphor/30 bg-ink-850 text-phosphor">
          <Activity class="w-3.5 h-3.5" />
        </div>
      {/if}
      <span class="font-mono text-bone/90">{notification.text}</span>
    </div>
  {/if}

  <main class="flex-1 flex overflow-hidden relative">
    <Sidebar
      {agents}
      {sessions}
      selectedSessionId={focusedSessionId}
      selectedGroupId={focusedGroupId}
      {fileTreeRoot}
      onSelectSession={selectSession}
      onSelectGroup={selectGroup}
      onKillSession={requestKillSession}
      onKillGroup={requestKillGroup}
      onRenameGroup={handleRenameGroup}
      onOpenLauncher={() => openLauncher()}
      onOpenFile={openFile}
      onDeleted={(p) => {
        const norm = (s: string) => s.replace(/[\\/]+/g, '/').toLowerCase();
        const gone = norm(p);
        for (const f of openFiles) {
          const open = norm(f);
          if (open === gone || open.startsWith(`${gone}/`)) closeFile(f);
        }
      }}
    />

    <div class="flex-1 min-w-0 h-full flex flex-col overflow-hidden">
      <div class="flex-1 min-h-0 overflow-hidden">
        <TerminalGrid
          {agents}
          sessions={activeGroupSessions}
          focusedSessionId={focusedSessionId}
          {clearEpoch}
          {clearSessionId}
          onFocusSession={selectSession}
          onKillSession={requestKillSession}
          onRestartSession={handleRestartSession}
          onHandoffSession={handleHandoffSession}
          onOpenLauncher={() => openLauncher()}
          onAddEngine={handleAddEngine}
          {openFiles}
          {activeFile}
          {dirtyFiles}
          onFocusFile={openFile}
          onCloseFile={closeFile}
          onHideFile={hideEditor}
          onDirtyFile={markDirty}
        />
      </div>
      <BroadcastBar
        activeSessionsCount={activeGroupSessions.filter((s) => s.status === 'running').length}
        sessionCount={sessions.length}
        activeSession={focusedSession}
        groupSessions={activeGroupSessions}
        onBroadcast={handleBroadcast}
        onCloseAll={() => focusedGroupId && requestKillGroup(focusedGroupId)}
        onClear={handleClear}
      />
    </div>

    <SupervisorCockpit
      sessions={activeGroupSessions}
      folder={focusedSession ? focusedSession.worktree_path ?? focusedSession.working_dir : null}
      isOpen={isSupervisorOpen}
      onClose={() => (isSupervisorOpen = false)}
      onToast={showToast}
    />
  </main>

  <LauncherModal
    isOpen={isLauncherOpen}
    {agents}
    preselectAgentId={launcherAgentId}
    onClose={() => {
      isLauncherOpen = false;
      launcherAgentId = null;
    }}
    onLaunch={handleLaunch}
  />

  <AgentRegistryModal
    isOpen={isRegistryOpen}
    {agents}
    onClose={() => (isRegistryOpen = false)}
    onLaunchAgent={handleDirectLaunchAgent}
    onRefresh={loadData}
  />

  <SkillsRegistryModal
    isOpen={isSkillsOpen}
    workdir={workspacePath}
    onClose={() => (isSkillsOpen = false)}
  />

  {#if pendingDelete}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed inset-0 z-[60] flex items-center justify-center p-4"
      onkeydown={(e) => {
        if (e.key === 'Escape') pendingDelete = null;
        if (e.key === 'Enter') void confirmDelete();
      }}
    >
      <div
        class="absolute inset-0 bg-ink-950/85"
        role="presentation"
        onclick={() => (pendingDelete = null)}
      ></div>
      <div
        class="bezel relative z-10 bg-ink-900 border border-line-strong rounded-sm w-full max-w-sm overflow-hidden toast-enter outline-none"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="delete-title"
        tabindex="-1"
        use:focusDialog
      >
        <div class="flex items-start gap-3 px-4 py-4">
          <div class="w-8 h-8 rounded-sm border border-alert/40 bg-alert/10 text-alert flex items-center justify-center flex-shrink-0">
            <Trash2 class="w-4 h-4" />
          </div>
          <div class="min-w-0">
            <h2 id="delete-title" class="text-xs font-mono font-bold text-bone tracking-wide">
              {pendingDelete.title}
            </h2>
            <p class="mt-1.5 text-[11px] font-mono text-fog leading-relaxed">
              {pendingDelete.body}
            </p>
          </div>
        </div>
        <div class="px-4 py-3 border-t border-line bg-ink-850 flex justify-end gap-2">
          <button type="button" onclick={() => (pendingDelete = null)} class={btn({ variant: 'default', size: 'sm' })}>
            Cancel
          </button>
          <button type="button" onclick={() => void confirmDelete()} class={btn({ variant: 'danger', size: 'sm' })}>
            Delete
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
