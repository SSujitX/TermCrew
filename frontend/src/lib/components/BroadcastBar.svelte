<script lang="ts">
  import type { Session } from '../types';
  import { Send, Check, Trash2, Eraser, ChevronUp } from 'lucide-svelte';
  import { btn } from '../ui';

  interface Props {
    activeSessionsCount: number;
    sessionCount?: number;
    activeSession?: Session | null;
    groupSessions?: Session[];
    onBroadcast: (message: string, sessionIds?: string[]) => void;
    onCloseAll?: () => void;
    onClear?: (sessionIds: string[]) => void;
  }

  let {
    activeSessionsCount = 0,
    sessionCount = 0,
    activeSession = null,
    groupSessions = [],
    onBroadcast,
    onCloseAll,
    onClear,
  }: Props = $props();

  let message = $state('');
  let isSent = $state(false);
  let target = $state<'active' | 'all'>('active');
  let cmdsOpen = $state(false);

  const QUICK_PROMPTS = [
    { label: 'git status', cmd: 'git status' },
    { label: 'git diff', cmd: 'git diff' },
    { label: 'run tests', cmd: 'pnpm test' },
    { label: 'stop', cmd: 'exit' },
  ];

  let groupIds = $derived(groupSessions.map((s) => s.id));
  let multi = $derived(groupIds.length > 1);
  let groupName = $derived(
    activeSession?.group_label?.split(' · ')[0] ??
      groupSessions[0]?.group_label?.split(' · ')[0] ??
      'Session'
  );
  let nodeLabel = $derived(activeSession?.role ?? activeSession?.name ?? 'selected');
  let hasSession = $derived(Boolean(activeSession) || groupIds.length > 0);
  let canSend = $derived(target === 'all' ? groupIds.length > 0 : Boolean(activeSession));

  let destLabel = $derived(
    target === 'all' ? `${groupName} · ${groupIds.length} terminals` : `${groupName} · ${nodeLabel}`
  );

  function targetIds(): string[] {
    if (target === 'all') return groupIds;
    return activeSession ? [activeSession.id] : [];
  }

  function send(cmd: string) {
    const ids = targetIds();
    if (!canSend || ids.length === 0) return;
    onBroadcast(cmd, ids);
    cmdsOpen = false;
    isSent = true;
    setTimeout(() => {
      isSent = false;
    }, 1200);
  }

  function handleSubmit() {
    if (!message.trim() || !canSend) return;
    send(message.trim());
    message = '';
  }
</script>

<footer
  class="h-12 px-3 py-1.5 bg-ink-900 border-t border-line-strong flex items-center flex-shrink-0 overflow-hidden z-20"
>
  {#if !hasSession}
    <div class="flex-1 flex items-center gap-2.5 min-w-0">
      <span class="font-mono text-phosphor/70 select-none">$</span>
      <span class="text-xs text-fog truncate">Start a session to run commands here</span>
    </div>
  {:else}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
      class="flex-1 flex items-center gap-2 min-w-0 h-full"
    >
      <div class="flex items-center gap-2 pr-2 border-r border-line flex-shrink-0">
        <span
          class="w-1.5 h-1.5 rounded-full {canSend
            ? 'bg-phosphor neon-dot'
            : 'bg-dim'}"
          aria-hidden="true"
        ></span>
        <div class="min-w-0 hidden sm:block">
          <span class="block text-[10px] font-mono text-phosphor truncate max-w-[180px]">{destLabel}</span>
        </div>
        {#if multi}
          <div class="flex border border-line rounded-sm overflow-hidden" role="group" aria-label="Command target">
            <button
              type="button"
              onclick={() => (target = 'active')}
              class="px-2 h-6 text-[10px] font-mono uppercase tracking-wider {target === 'active'
                ? 'bg-phosphor text-ink-950'
                : 'text-fog hover:text-bone'}"
            >
              This
            </button>
            <button
              type="button"
              onclick={() => (target = 'all')}
              class="px-2 h-6 text-[10px] font-mono uppercase tracking-wider {target === 'all'
                ? 'bg-phosphor text-ink-950'
                : 'text-fog hover:text-bone'}"
            >
              All
            </button>
          </div>
        {/if}
      </div>

      <div class="relative flex items-center flex-shrink-0">
        <button
          type="button"
          onclick={() => (cmdsOpen = !cmdsOpen)}
          disabled={!canSend}
          class="h-7 w-7 flex items-center justify-center text-fog hover:text-phosphor disabled:opacity-40"
          aria-expanded={cmdsOpen}
          aria-haspopup="menu"
          title="Quick commands"
        >
          <ChevronUp class="w-3.5 h-3.5 {cmdsOpen ? '' : 'opacity-70'}" />
        </button>
        {#if cmdsOpen}
          <div
            class="absolute bottom-[calc(100%+6px)] left-0 min-w-[160px] bg-ink-900 border border-line-strong rounded-sm p-1 z-30"
            role="menu"
          >
            {#each QUICK_PROMPTS as item}
              <button
                type="button"
                role="menuitem"
                onclick={() => send(item.cmd)}
                class="w-full text-left px-2.5 py-1.5 text-[11px] font-mono text-fog hover:text-phosphor hover:bg-ink-800 rounded-sm"
              >
                {item.label}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <label class="flex-1 min-w-0 flex items-center gap-2">
        <span class="font-mono text-phosphor select-none">$</span>
        <input
          type="text"
          bind:value={message}
          disabled={!canSend}
          onfocus={() => (cmdsOpen = false)}
          placeholder={target === 'all'
            ? `Run on all ${groupName} terminals`
            : `Run on ${nodeLabel}`}
          class="cmd-input w-full min-w-0 bg-transparent border-0 rounded-none text-xs font-mono text-bone placeholder:text-dim disabled:opacity-40"
        />
      </label>

      <div class="flex items-center gap-1.5 flex-shrink-0">
        <button
          type="submit"
          disabled={!message.trim() || !canSend}
          class={btn({ variant: 'primary', size: 'sm' })}
        >
          {#if isSent}
            <Check class="w-3.5 h-3.5 stroke-[3]" />
            <span>Sent</span>
          {:else}
            <Send class="w-3.5 h-3.5" />
            <span>Run</span>
          {/if}
        </button>
        <button
          type="button"
          onclick={() => onClear?.(targetIds())}
          disabled={!canSend}
          title={target === 'all' ? 'Clear chat boxes in this group' : 'Clear the chat box in this pane'}
          class={btn({ variant: 'ghost', size: 'icon' })}
        >
          <Eraser class="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          onclick={() => onCloseAll?.()}
          disabled={sessionCount === 0 && activeSessionsCount === 0}
          title={multi ? 'Close this session' : 'Close session'}
          class={btn({ variant: 'ghost', size: 'icon' })}
        >
          <Trash2 class="w-3.5 h-3.5" />
        </button>
      </div>
    </form>
  {/if}
</footer>

<style>
  .cmd-input {
    appearance: none;
    outline: none;
    box-shadow: none;
  }
  .cmd-input:focus,
  .cmd-input:focus-visible {
    outline: none;
    box-shadow: none;
  }
</style>
