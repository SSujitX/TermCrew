<script lang="ts">
  import { onMount } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebglAddon } from '@xterm/addon-webgl';
  import { CanvasAddon } from '@xterm/addon-canvas';
  import type { AgentMeta, Session } from '../types';
  import { cn, statusColor, phosphorXtermTheme, agentDisplayName } from '../ui';
  import AgentMark from './AgentMark.svelte';
  import { bindTerminalClipboard, copyTerminalText } from '../termClipboard';
  import { openPtySocket } from '../ptySocket';
  import { CLEAR_DRAFT } from '../draftClear';
  import { RotateCcw, X, ArrowRightFromLine, Eraser, Maximize2, Minimize2, Copy, Check, Folder } from 'lucide-svelte';

  interface Props {
    agents?: AgentMeta[];
    session: Session;
    isMaximized?: boolean;
    /** When false (tabbed/hidden), skip fit until shown — avoids 0-size cols breaking TUIs. */
    isVisible?: boolean;
    clearEpoch?: number;
    clearSessionId?: string | null;
    onToggleMaximize?: () => void;
    onKill: (sessionId: string) => void;
    onRestart: (sessionId: string) => void;
    onHandoff: (sessionId: string) => void;
  }

  let {
    agents = [],
    session,
    isMaximized = false,
    isVisible = true,
    clearEpoch = 0,
    clearSessionId = null,
    onToggleMaximize,
    onKill,
    onRestart,
    onHandoff,
  }: Props = $props();

  let terminalContainer: HTMLDivElement | null = $state(null);
  let wsConnected = $state<boolean>(false);
  let sessionStatus = $state<'running' | 'exited' | 'paused' | 'error'>('running');

  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let sendResize: ((cols: number, rows: number) => void) | null = null;
  let sendInput: ((data: string) => void) | null = null;
  let lastClearEpoch = 0;
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let dirCopied = $state(false);
  let dirCopyTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => { sessionStatus = session.status || 'running'; });

  let isLive = $derived(wsConnected && sessionStatus === 'running');
  let sessionDir = $derived(session.worktree_path || session.working_dir);
  let agentLabel = $derived(agentDisplayName(session.engine, agents));

  $effect(() => {
    if (clearEpoch <= 0 || clearEpoch === lastClearEpoch) return;
    lastClearEpoch = clearEpoch;
    if (clearSessionId && clearSessionId !== session.id) return;
    sendInput?.(CLEAR_DRAFT);
  });

  function refit() {
    if (!term || !fitAddon || !terminalContainer) return;
    if (!isVisible) return;
    if (terminalContainer.clientWidth < 16 || terminalContainer.clientHeight < 16) return;
    try {
      fitAddon.fit();
      sendResize?.(term.cols, term.rows);
      term.refresh(0, term.rows - 1);
    } catch {
      /* noop */
    }
  }

  $effect(() => {
    if (!isVisible) return;
    const t0 = requestAnimationFrame(() => {
      refit();
      requestAnimationFrame(refit);
    });
    const t1 = setTimeout(refit, 50);
    const t2 = setTimeout(refit, 200);
    return () => {
      cancelAnimationFrame(t0);
      clearTimeout(t1);
      clearTimeout(t2);
    };
  });

  onMount(() => {
    if (!terminalContainer) return;
    const container = terminalContainer;

    const xterm = new Terminal({
      scrollback: 3000,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: '"IBM Plex Mono", "Chivo Mono", monospace',
      fontSize: 13,
      lineHeight: 1.35,
      allowTransparency: false,
      theme: { ...phosphorXtermTheme },
    });

    const fit = new FitAddon();
    xterm.loadAddon(fit);
    xterm.open(terminalContainer);

    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      xterm.loadAddon(webgl);
    } catch {
      try { xterm.loadAddon(new CanvasAddon()); } catch { /* DOM fallback */ }
    }

    term = xterm;
    fitAddon = fit;

    xterm.writeln(`\x1b[38;2;57;211;83m▶ [TTY]\x1b[0m \x1b[1;37m${session.name}\x1b[0m \x1b[38;2;107;143;110m(${session.engine})\x1b[0m`);
    xterm.writeln(`\x1b[38;2;107;143;110mNode directory: ${session.working_dir} · Preset: ${session.preset}\x1b[0m`);
    xterm.writeln(`\x1b[38;2;107;143;110mConnecting to PTY bridge...\x1b[0m\r\n`);

    let sawConnected = false;
    let clearedForReplay = false;
    let sock: ReturnType<typeof openPtySocket> | null = null;
    let disposed = false;

    const attachSocket = () => {
      if (disposed || sock) return;
      // Fit before connect so the first WS message can be a real resize
      // (backend waits for it before dumping TUI scrollback).
      try {
        fit.fit();
      } catch {
        /* noop */
      }

      sock = openPtySocket(session.id, {
        onLive: (isLive) => {
          wsConnected = isLive;
        },
        onOpen: () => {
          // Backend waits for this resize before dumping TUI scrollback.
          try {
            fit.fit();
          } catch {
            /* noop */
          }
          sock?.send({ type: 'resize', cols: xterm.cols, rows: xterm.rows });
        },
        onBytes: (bytes, onParsed) => {
          if (!clearedForReplay) {
            clearedForReplay = true;
            xterm.reset();
          }
          xterm.write(bytes, onParsed);
        },
        onPayload: (payload) => {
          if (payload.type === 'connected') {
            clearedForReplay = false;
            if (sawConnected) xterm.reset();
            sawConnected = true;
            try {
              fit.fit();
            } catch {
              /* noop */
            }
            sock?.send({ type: 'resize', cols: xterm.cols, rows: xterm.rows });
            return;
          }
          if (payload.type === 'error' && payload.message) {
            xterm.writeln(`\r\n\x1b[38;2;232;93;93m✖ [TTY ERROR] ${payload.message}\x1b[0m`);
            sessionStatus = 'error';
            sock?.dispose();
            return;
          }
          if (payload.type === 'status' && payload.data) {
            sessionStatus = payload.data as typeof sessionStatus;
          }
        },
      });

      sendResize = (cols, rows) => {
        sock?.send({ type: 'resize', cols, rows });
      };
      sendInput = (data) => {
        sock?.send({ type: 'input', data });
      };

      xterm.onData((data) => {
        if (!sock?.send({ type: 'input', data })) {
          if (data === '\r') xterm.write('\r\n');
          else if (data === '\u007F') xterm.write('\b \b');
          else xterm.write(data);
        }
      });

      xterm.onResize(({ cols, rows }) => {
        sock?.send({ type: 'resize', cols, rows });
      });
    };

    const waitForLayout = (tries = 0) => {
      if (disposed) return;
      try {
        fit.fit();
      } catch {
        /* noop */
      }
      const ready =
        container.clientWidth >= 40 &&
        container.clientHeight >= 40 &&
        xterm.cols >= 40;
      if (ready || tries >= 30) {
        attachSocket();
        return;
      }
      requestAnimationFrame(() => waitForLayout(tries + 1));
    };
    waitForLayout();

    const unbindClipboard = bindTerminalClipboard(xterm, terminalContainer);
    let roTimer: ReturnType<typeof setTimeout> | null = null;
    const ro = new ResizeObserver(() => {
      if (roTimer) clearTimeout(roTimer);
      roTimer = setTimeout(refit, 16);
    });
    ro.observe(terminalContainer);

    return () => {
      disposed = true;
      if (roTimer) clearTimeout(roTimer);
      sendResize = null;
      sendInput = null;
      sock?.dispose();
      unbindClipboard();
      ro.disconnect();
      xterm.dispose();
      term = null;
      fitAddon = null;
    };
  });

  async function copyOutput() {
    if (!term) return;
    const ok = await copyTerminalText(term);
    if (!ok) return;
    copied = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copied = false;
    }, 1400);
  }

  async function copyDirPath() {
    try {
      await navigator.clipboard.writeText(sessionDir);
      dirCopied = true;
      if (dirCopyTimer) clearTimeout(dirCopyTimer);
      dirCopyTimer = setTimeout(() => {
        dirCopied = false;
      }, 1400);
    } catch {
      /* ignore */
    }
  }

  function clearDraft() {
    sendInput?.(CLEAR_DRAFT);
  }

  function handleRestart() {
    term?.writeln('\r\n\x1b[38;2;57;211;83m▶ [TTY] Re-initializing agent stream...\x1b[0m\r\n');
    sessionStatus = 'running';
    onRestart(session.id);
  }

  function handleKill() {
    onKill(session.id);
  }
</script>

<div class="bezel flex flex-col h-full w-full bg-ink-950 rounded-sm border border-line-strong overflow-hidden group">
  <!-- Hardware title bar -->
  <div class="h-8 flex items-center px-3 gap-2.5 border-b border-line-strong bg-ink-900 flex-shrink-0 select-none">
    <span class={cn('w-2 h-2 rounded-sm flex-shrink-0', statusColor[sessionStatus] || statusColor.exited)}></span>

    <div class="flex items-center gap-2 truncate flex-1 min-w-0">
      <AgentMark id={session.engine} name={agentLabel} installed={true} size="xs" />
      <span class="font-mono text-xs font-semibold text-bone truncate">{agentLabel}</span>
      {#if session.role}
        <span class="eyebrow text-[9px] px-1.5 py-0.5 border border-phosphor/30 bg-phosphor/5 flex-shrink-0">
          {session.role}
        </span>
      {/if}
      {#if sessionDir}
        <button
          type="button"
          class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors flex-shrink-0"
          title={dirCopied ? 'Copied' : `${sessionDir} — click to copy`}
          aria-label="Copy folder path"
          onclick={copyDirPath}
        >
          {#if dirCopied}<Check class="w-3.5 h-3.5 text-phosphor" />{:else}<Folder class="w-3.5 h-3.5" />{/if}
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-1.5 text-[10px] font-mono text-fog mr-1">
      <span
        class={cn(
          'w-1.5 h-1.5 rounded-sm flex-shrink-0',
          isLive ? 'bg-phosphor pulse-glow' : 'bg-ink-600'
        )}
      ></span>
      <span class="hidden sm:inline tracking-wider">{isLive ? 'LIVE' : 'OFFLINE'}</span>
    </div>

    <div class="flex items-center gap-0.5">
      <button type="button" onclick={copyOutput} title="Copy selection, or the full buffer"
        class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors">
        {#if copied}<Check class="w-3.5 h-3.5 text-phosphor" />{:else}<Copy class="w-3.5 h-3.5" />{/if}
      </button>
      <button type="button" onclick={() => onHandoff(session.id)} title="Send review — compact git packet to the next pane in this session"
        class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors">
        <ArrowRightFromLine class="w-3.5 h-3.5" />
      </button>
      <button type="button" onclick={clearDraft} title="Clear the chat box — does not send"
        class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors">
        <Eraser class="w-3.5 h-3.5" />
      </button>
      <button type="button" onclick={handleRestart} title="Restart session"
        class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors">
        <RotateCcw class="w-3.5 h-3.5" />
      </button>
      {#if onToggleMaximize}
        <button type="button" onclick={onToggleMaximize} title={isMaximized ? 'Restore grid' : 'Maximize'}
          class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-phosphor hover:bg-ink-800 transition-colors">
          {#if isMaximized}<Minimize2 class="w-3.5 h-3.5" />{:else}<Maximize2 class="w-3.5 h-3.5" />{/if}
        </button>
      {/if}
      <button type="button" onclick={handleKill} title="Terminate session"
        class="h-6 w-6 flex items-center justify-center rounded-sm text-fog hover:text-alert hover:bg-alert/10 transition-colors">
        <X class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>

  <!-- CRT canvas -->
  <div class="scanlines flex-1 overflow-hidden relative bg-ink-950">
    <div bind:this={terminalContainer} class="absolute inset-0 p-2"></div>
  </div>
</div>

<style>
  :global(.xterm) { height: 100%; }
  :global(.xterm-viewport) { background-color: #050805 !important; }
</style>
