<script lang="ts">
  import { onMount } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebglAddon } from '@xterm/addon-webgl';
  import { CanvasAddon } from '@xterm/addon-canvas';
  import { phosphorXtermTheme } from '../ui';
  import { btn } from '../ui';
  import { bindTerminalClipboard, copyTerminalText } from '../termClipboard';
  import { openPtySocket } from '../ptySocket';
  import { RefreshCw, X, Copy, Check } from 'lucide-svelte';

  interface Props {
    sessionId: string;
    title: string;
    command?: string;
    onClose: () => void;
    onRescan: () => void;
  }

  let { sessionId, title, command, onClose, onRescan }: Props = $props();

  let terminalContainer: HTMLDivElement | null = $state(null);
  let live = $state(false);
  let term: Terminal | null = null;
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => {
    if (!terminalContainer) return;

    const xterm = new Terminal({
      scrollback: 4000,
      convertEol: true,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: '"IBM Plex Mono", "Chivo Mono", monospace',
      fontSize: 12,
      lineHeight: 1.3,
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
      try {
        xterm.loadAddon(new CanvasAddon());
      } catch {
        /* DOM fallback */
      }
    }
    term = xterm;
    if (command) {
      xterm.writeln(`\x1b[38;2;167;243;208m${command}\x1b[0m`);
    }
    requestAnimationFrame(() => {
      try {
        fit.fit();
      } catch {
        /* noop */
      }
    });

    let sawConnected = false;
    const sock = openPtySocket(sessionId, {
      onLive: (isLive) => {
        live = isLive;
      },
      onOpen: (ws) => {
        try {
          fit.fit();
          ws.send(JSON.stringify({ type: 'resize', cols: xterm.cols, rows: xterm.rows }));
        } catch {
          /* noop */
        }
      },
      onBytes: (bytes, onParsed) => {
        xterm.write(bytes, onParsed);
      },
      onSetupDone: () => {
        onRescan();
      },
      onPayload: (payload) => {
        if (payload.type === 'connected') {
          if (sawConnected) {
            xterm.reset();
            if (command) xterm.writeln(`\x1b[38;2;167;243;208m${command}\x1b[0m`);
          }
          sawConnected = true;
          if (payload.data) xterm.write(payload.data);
          return;
        }
        if (payload.type === 'error' && payload.message) {
          xterm.writeln(`\r\n\x1b[38;2;232;93;93m✖ ${payload.message}\x1b[0m`);
          live = false;
          sock.dispose();
        }
      },
    });

    xterm.onData((data) => {
      sock.send({ type: 'input', data });
    });
    xterm.onResize(({ cols, rows }) => {
      sock.send({ type: 'resize', cols, rows });
    });

    const unbindClipboard = bindTerminalClipboard(xterm, terminalContainer);
    const ro = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        /* noop */
      }
    });
    ro.observe(terminalContainer);

    return () => {
      sock.dispose();
      unbindClipboard();
      ro.disconnect();
      xterm.dispose();
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
</script>

<div class="border-t border-line bg-ink-950 flex flex-col h-72 flex-shrink-0">
  <div class="min-h-8 px-3 py-1.5 flex items-center gap-2 border-b border-line bg-ink-900 flex-shrink-0">
    <span class="w-1.5 h-1.5 rounded-sm {live ? 'bg-phosphor pulse-glow' : 'bg-ink-600'}"></span>
    <div class="min-w-0 flex-1">
      <div class="text-[11px] font-mono font-bold text-bone truncate uppercase tracking-wider" title={title}>
        {title}
      </div>
      {#if command}
        <div class="text-[10px] font-mono text-phosphor/80 truncate" title={command}>{command}</div>
      {/if}
    </div>
    <button type="button" onclick={copyOutput} class={btn({ variant: 'ghost', size: 'icon' })} title="Copy selection, or the full buffer">
      {#if copied}<Check class="w-3.5 h-3.5 text-phosphor" />{:else}<Copy class="w-3.5 h-3.5" />{/if}
    </button>
    <button type="button" onclick={onRescan} class={btn({ variant: 'ghost', size: 'icon' })} title="Rescan PATH">
      <RefreshCw class="w-3.5 h-3.5" />
    </button>
    <button type="button" onclick={onClose} class={btn({ variant: 'ghost', size: 'icon' })} title="Close console">
      <X class="w-3.5 h-3.5" />
    </button>
  </div>
  <div class="flex-1 relative min-h-0">
    <div bind:this={terminalContainer} class="absolute inset-0 p-1.5"></div>
  </div>
</div>

<style>
  :global(.xterm) {
    height: 100%;
  }
</style>
