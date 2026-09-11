import { clsx, type ClassValue } from 'clsx';
import { tv } from 'tailwind-variants';

export function cn(...inputs: ClassValue[]) {
  return clsx(inputs);
}

/** Phosphor console button variants — flat, sharp, no gradient glow. */
export const btn = tv({
  base: 'inline-flex items-center justify-center gap-1.5 font-mono font-medium transition-colors duration-150 cursor-pointer select-none disabled:opacity-40 disabled:cursor-not-allowed whitespace-nowrap active:brightness-90',
  variants: {
    variant: {
      default:
        'bg-ink-850 text-bone hover:bg-ink-700 border border-line hover:border-line-strong hover:text-phosphor-bright',
      primary:
        'bg-phosphor text-ink-950 hover:bg-phosphor-bright border border-phosphor font-semibold',
      violet:
        'bg-ink-800 text-phosphor border border-phosphor-dim hover:border-phosphor hover:bg-ink-700',
      ghost: 'text-fog hover:text-phosphor hover:bg-ink-800/60',
      danger: 'text-alert hover:text-bone hover:bg-alert/15 border border-alert/40',
      outline:
        'border border-line hover:border-phosphor text-fog hover:bg-ink-850 hover:text-phosphor',
    },
    size: {
      icon: 'h-7 w-7 rounded-sm',
      xs: 'h-6 px-2 text-[11px] rounded-sm',
      sm: 'h-8 px-3 text-xs rounded-sm',
      md: 'h-9 px-4 text-xs rounded-sm',
    },
  },
  defaultVariants: { variant: 'default', size: 'sm' },
});

export const badge = tv({
  base: 'inline-flex items-center rounded-sm px-2 py-0.5 text-[10px] font-semibold border font-mono tracking-wide',
  variants: {
    variant: {
      default: 'bg-ink-800 text-fog border-line',
      success: 'bg-phosphor/10 text-phosphor border-phosphor/35',
      warning: 'bg-warning/10 text-warning border-warning/35',
      error: 'bg-alert/10 text-alert border-alert/35',
      cyan: 'bg-phosphor/10 text-phosphor-bright border-phosphor/35',
      purple: 'bg-ink-800 text-fog border-line-strong',
    },
  },
  defaultVariants: { variant: 'default' },
});

export const statusColor = {
  running: 'bg-phosphor text-phosphor neon-dot pulse-glow',
  exited: 'bg-ink-600 text-ink-600',
  paused: 'bg-warning text-warning neon-dot',
  error: 'bg-alert text-alert neon-dot',
} as const;

/** xterm.js phosphor CRT palette — share across panes */
export const phosphorXtermTheme = {
  background: '#050805',
  foreground: '#a8d4aa',
  cursor: '#39d353',
  cursorAccent: '#050805',
  selectionBackground: 'rgba(57, 211, 83, 0.28)',
  black: '#050805',
  brightBlack: '#2a3a2a',
  red: '#e85d5d',
  brightRed: '#ff7a7a',
  green: '#39d353',
  brightGreen: '#7cff8a',
  yellow: '#d4a017',
  brightYellow: '#f0c040',
  blue: '#4a8f5a',
  brightBlue: '#6bb87a',
  magenta: '#6b8f6e',
  brightMagenta: '#8faf90',
  cyan: '#39d353',
  brightCyan: '#7cff8a',
  white: '#c8e6c9',
  brightWhite: '#e8ffe9',
} as const;

/** Compact chrome label: short binary (`agy` → `Agy`), else catalog name minus ` CLI`. */
export function agentDisplayName(
  engine: string,
  agents: { id: string; name: string; binary?: string }[],
): string {
  const a = agents.find((x) => x.id === engine);
  const binary = (a?.binary || engine).trim();
  if (binary && binary.length <= 3 && !binary.includes(' ')) {
    return binary[0].toUpperCase() + binary.slice(1);
  }
  if (!a?.name.trim()) {
    return binary ? binary[0].toUpperCase() + binary.slice(1) : engine;
  }
  return a.name.replace(/\s+CLI$/i, '').trim();
}

/** Hide usernames in UI paths: `%LOCALAPPDATA%\…` / `~/…` (full path stays in title/copy). */
export function displayHomePath(path: string): string {
  let n = path.replace(/\\/g, '/');
  n = n
    .replace(/^[A-Za-z]:\/Users\/[^/]+\/AppData\/Local/i, '%LOCALAPPDATA%')
    .replace(/^[A-Za-z]:\/Users\/[^/]+\/AppData\/Roaming/i, '%APPDATA%')
    .replace(/^[A-Za-z]:\/Users\/[^/]+/i, '~')
    .replace(/^\/c\/Users\/[^/]+\/AppData\/Local/i, '%LOCALAPPDATA%')
    .replace(/^\/c\/Users\/[^/]+\/AppData\/Roaming/i, '%APPDATA%')
    .replace(/^\/c\/Users\/[^/]+/i, '~')
    .replace(/^\/Users\/[^/]+/, '~')
    .replace(/^\/home\/[^/]+/, '~');
  if (n.includes('%LOCALAPPDATA%') || n.includes('%APPDATA%') || /^[A-Za-z]:\//.test(n)) {
    n = n.replace(/\//g, '\\');
  }
  return n;
}
