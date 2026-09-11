<script lang="ts">
  import { Bot } from 'lucide-svelte';
  import { cn } from '../ui';

  interface Props {
    id: string;
    name: string;
    installed?: boolean;
    size?: 'xs' | 'sm' | 'md';
  }

  let { id, name, installed = false, size = 'md' }: Props = $props();
  let failed = $state(false);

  const SVG_MARKS = new Set([
    'pi',
    'shell',
    'cmd',
    'git-bash',
    'wsl',
    'interpreter',
    'codewhale',
    'kilo',
    'claude',
    'gemini',
    'opencode',
    'deepseek',
    'kiro',
  ]);
  const SHELL_MARKS = new Set(['shell', 'cmd', 'git-bash', 'wsl', 'bash', 'zsh', 'sh']);
  const src = $derived(
    SHELL_MARKS.has(id)
      ? '/agents/shell.svg'
      : SVG_MARKS.has(id)
        ? `/agents/${id}.svg`
        : `/agents/${id}.png`
  );
  const box = $derived(size === 'xs' ? 'w-4 h-4' : size === 'sm' ? 'w-8 h-8' : 'w-9 h-9');
</script>

<div
  class={cn(
    'rounded-sm border flex items-center justify-center flex-shrink-0 overflow-hidden',
    box,
    installed ? 'bg-ink-900 border-phosphor/35' : 'bg-ink-800 border-line opacity-80'
  )}
>
  {#if !failed}
    <img
      src={src}
      alt="{name} logo"
      class="w-full h-full object-contain p-0.5"
      onerror={() => (failed = true)}
    />
  {:else}
    <Bot class="w-4 h-4 {installed ? 'text-phosphor' : 'text-fog'}" />
  {/if}
</div>
