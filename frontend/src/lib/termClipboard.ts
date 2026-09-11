import type { Terminal } from '@xterm/xterm';

function bufferText(term: Terminal): string {
  const buf = term.buffer.active;
  const lines: string[] = [];
  for (let i = 0; i < buf.length; i++) {
    const line = buf.getLine(i);
    if (line) lines.push(line.translateToString(true));
  }
  return lines.join('\n').replace(/\s+$/u, '');
}

export function selectionOrBuffer(term: Terminal): string {
  const selected = term.hasSelection() ? term.getSelection() : '';
  return selected.trim().length > 0 ? selected : bufferText(term);
}

export async function copyTerminalText(term: Terminal): Promise<boolean> {
  const text = selectionOrBuffer(term);
  if (!text) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export async function pasteIntoTerminal(term: Terminal): Promise<void> {
  try {
    const text = await navigator.clipboard.readText();
    if (text) term.paste(text);
  } catch {
    /* clipboard permission denied */
  }
}

/** Right-click paste, Ctrl/Cmd+Shift+C copy, Ctrl/Cmd+Shift+V paste, Ctrl+C copies when text is selected. */
export function bindTerminalClipboard(term: Terminal, el: HTMLElement): () => void {
  const onContext = (event: MouseEvent) => {
    event.preventDefault();
    void pasteIntoTerminal(term);
  };
  el.addEventListener('contextmenu', onContext);

  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== 'keydown') return true;
    const key = ev.key.toLowerCase();
    const copyKey = (ev.ctrlKey || ev.metaKey) && key === 'c';
    const pasteKey = (ev.ctrlKey || ev.metaKey) && key === 'v';

    if (copyKey && (ev.shiftKey || term.hasSelection())) {
      void copyTerminalText(term);
      return false;
    }
    if (pasteKey) {
      void pasteIntoTerminal(term);
      return false;
    }
    return true;
  });

  return () => el.removeEventListener('contextmenu', onContext);
}
