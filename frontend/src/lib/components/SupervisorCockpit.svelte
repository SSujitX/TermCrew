<script lang="ts">
  import type { Session } from '../types';
  import { broadcastMessage, handoffReview } from '../api';
  import {
    detectTestCommand,
    laneOf,
    playbookLine,
    reviewSends,
    running,
    testTarget,
  } from '../playbook';
  import { btn, badge, cn } from '../ui';
  import { ArrowRight, GitPullRequest, ShieldCheck, Zap, X, Cpu } from '@lucide/svelte';

  interface Props {
    sessions: Session[];
    folder: string | null;
    isOpen: boolean;
    onClose: () => void;
    onToast: (text: string, type?: 'info' | 'success' | 'error') => void;
  }

  let { sessions = [], folder = null, isOpen = false, onClose, onToast }: Props = $props();

  let goalInput = $state('');
  let busy = $state(false);
  let lastGoal = $state('');
  let testCmd = $state<string | null>(null);
  let log = $state<{ t: string; msg: string }[]>([]);

  let live = $derived(running(sessions));
  let pairs = $derived(reviewSends(sessions));
  let verify = $derived(testTarget(sessions));

  $effect(() => {
    if (!isOpen || !folder) {
      testCmd = null;
      return;
    }
    const root = folder;
    void detectTestCommand(root).then((cmd) => {
      if (folder === root) testCmd = cmd;
    });
  });

  function stamp(msg: string) {
    log = [...log, { t: new Date().toLocaleTimeString(), msg }].slice(-40);
  }

  async function runPlaybook() {
    const goal = goalInput.trim();
    if (!goal || busy) return;
    if (live.length === 0) {
      onToast('Launch a running session first.', 'error');
      return;
    }
    busy = true;
    lastGoal = goal;
    let sent = 0;
    try {
      for (const s of live) {
        const line = playbookLine(s, goal, sessions);
        if (!line) continue;
        await broadcastMessage(line, [s.id]);
        sent += 1;
        stamp(`${s.role ?? s.name}: ${laneOf(s)}`);
      }
      if (sent === 0) {
        onToast('Nothing to send — only a shell is running. Launch an agent.', 'error');
      } else {
        onToast(`Playbook typed into ${sent} pane${sent === 1 ? '' : 's'}.`, 'success');
      }
    } catch (err) {
      onToast(err instanceof Error ? err.message : String(err), 'error');
    } finally {
      busy = false;
    }
  }

  async function sendReviews() {
    if (pairs.length === 0) {
      onToast('Need a running builder and a Review pane (Pair).', 'error');
      return;
    }
    busy = true;
    try {
      for (const { source, target } of pairs) {
        await handoffReview(source.id, target.id);
        stamp(`diff ${source.role ?? source.name} → ${target.role ?? target.name}`);
      }
      onToast(`Review packet sent (${pairs.length}).`, 'success');
    } catch (err) {
      onToast(err instanceof Error ? err.message : String(err), 'error');
    } finally {
      busy = false;
    }
  }

  async function runTests() {
    if (!testCmd) {
      onToast('No test command in this folder (package.json / Cargo.toml / go.mod / pytest).', 'error');
      return;
    }
    if (!verify) {
      onToast('No running shell or builder to run tests.', 'error');
      return;
    }
    busy = true;
    try {
      await broadcastMessage(testCmd, [verify.id]);
      stamp(`${testCmd} → ${verify.role ?? verify.name}`);
      onToast(`Ran ${testCmd} on ${verify.role ?? verify.name}.`, 'success');
    } catch (err) {
      onToast(err instanceof Error ? err.message : String(err), 'error');
    } finally {
      busy = false;
    }
  }
</script>

{#if isOpen}
  <div
    class="w-80 sm:w-96 bg-ink-900 border-l border-phosphor/40 h-full flex flex-col flex-shrink-0 z-30"
  >
    <div class="h-12 px-4 border-b border-line flex items-center justify-between bg-ink-850 flex-shrink-0">
      <div class="flex items-center gap-2.5">
        <div class="w-6 h-6 rounded-sm bg-ink-800 border border-phosphor/40 flex items-center justify-center text-phosphor">
          <Cpu class="w-3.5 h-3.5" />
        </div>
        <div>
          <h3 class="text-[11px] font-mono font-bold text-bone tracking-[0.14em] uppercase">Goals</h3>
          <span class="text-[9px] font-mono text-phosphor block uppercase tracking-wider">
            Role playbook
          </span>
        </div>
      </div>
      <button type="button" onclick={onClose} class={btn({ variant: 'ghost', size: 'icon' })} aria-label="Close goals">
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-4 space-y-5">
      <div>
        <label for="supervisor-goal-input" class="block text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-phosphor mb-2">
          Goal
        </label>
        <textarea
          id="supervisor-goal-input"
          bind:value={goalInput}
          rows={4}
          placeholder="What should this crew finish? Lead implements, Review waits for a diff, Shell runs tests."
          class="w-full p-3 bg-ink-850 border border-line rounded-sm text-xs text-bone font-mono placeholder:text-dim focus:outline-none focus:border-phosphor resize-none leading-relaxed"
          onkeydown={(e) => {
            if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              void runPlaybook();
            }
          }}
        ></textarea>
        <button
          type="button"
          onclick={() => void runPlaybook()}
          disabled={!goalInput.trim() || busy || live.length === 0}
          class={cn(btn({ variant: 'primary', size: 'sm' }), 'w-full mt-2')}
          title={live.length === 0 ? 'Launch a running session first' : 'Ctrl+Enter'}
        >
          <Zap class="w-3.5 h-3.5 fill-current" />
          <span>{busy ? 'Sending…' : 'Run playbook'}</span>
        </button>
        {#if live.length === 0}
          <p class="mt-2 text-[10px] font-mono text-dim leading-relaxed">
            Launch Pair (or Workbench / Swarm). Goals types a different contract into each role — not the same line to everyone.
          </p>
        {/if}
      </div>

      <div>
        <span class="text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-fog block mb-2">
          Crew ({live.length} live)
        </span>
        {#if sessions.length === 0}
          <p class="font-mono text-[11px] text-fog p-3 bg-ink-850 rounded-sm border border-line leading-relaxed">
            No session in this group.
          </p>
        {:else}
          <div class="space-y-1.5">
            {#each sessions as s (s.id)}
              {@const lane = laneOf(s)}
              <div class="flex items-center justify-between gap-2 p-2 rounded-sm bg-ink-850 border border-line text-xs">
                <span class="font-mono text-bone truncate">{s.role ?? s.name}</span>
                <span class="flex items-center gap-1.5 flex-shrink-0">
                  <span class={badge({ variant: lane === 'review' ? 'warning' : lane === 'shell' ? 'default' : 'success' })}>
                    {lane}
                  </span>
                  <span class="text-[9px] font-mono text-dim uppercase">{s.status}</span>
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="space-y-2">
        <span class="text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-fog block">
          After they work
        </span>
        <button
          type="button"
          onclick={() => void sendReviews()}
          disabled={busy || pairs.length === 0}
          class="w-full flex items-center justify-between px-3 py-2 rounded-sm bg-ink-850 border border-line hover:border-phosphor/50 text-fog hover:text-bone text-xs font-mono transition-colors disabled:opacity-40"
          title={pairs.length === 0 ? 'Needs a builder and a Review pane' : `Send git packet × ${pairs.length}`}
        >
          <div class="flex items-center gap-2">
            <GitPullRequest class="w-3.5 h-3.5 text-phosphor" />
            <span>Send review</span>
          </div>
          <span class="flex items-center gap-1 text-dim">
            {#if pairs.length > 0}
              <span class="text-[10px]">{pairs.length}</span>
            {/if}
            <ArrowRight class="w-3.5 h-3.5" />
          </span>
        </button>
        <button
          type="button"
          onclick={() => void runTests()}
          disabled={busy || !testCmd || !verify}
          class="w-full flex items-center justify-between px-3 py-2 rounded-sm bg-ink-850 border border-line hover:border-phosphor/50 text-fog hover:text-bone text-xs font-mono transition-colors disabled:opacity-40"
          title={!verify
            ? 'Add a Shell pane (+) to run tests'
            : testCmd
              ? `${testCmd} → ${verify.role ?? verify.name}`
              : 'No test command in this folder'}
        >
          <div class="flex items-center gap-2 min-w-0">
            <ShieldCheck class="w-3.5 h-3.5 text-phosphor flex-shrink-0" />
            <span class="truncate">{!verify ? 'Add a Shell to run tests' : testCmd ? `Run ${testCmd}` : 'No tests found'}</span>
          </div>
          <ArrowRight class="w-3.5 h-3.5 text-dim flex-shrink-0" />
        </button>
      </div>

      {#if lastGoal || log.length > 0}
        <div>
          {#if lastGoal}
            <p class="text-[10px] font-mono text-dim mb-2 truncate" title={lastGoal}>Goal: {lastGoal}</p>
          {/if}
          <div class="p-3 rounded-sm bg-ink-950 border border-line space-y-1.5 max-h-40 overflow-y-auto font-mono text-[11px]">
            {#each log as row, i (i)}
              <div class="flex items-start gap-2 leading-relaxed">
                <span class="text-dim select-none flex-shrink-0 text-[10px]">{row.t}</span>
                <span class="text-fog">{row.msg}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
