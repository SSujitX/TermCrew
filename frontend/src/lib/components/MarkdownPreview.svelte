<script lang="ts">
  import { btn, cn } from '../ui';
  import { renderSkillMarkdown } from '../markdown';
  import { Check, Copy, Eye, FileCode } from 'lucide-svelte';

  interface Props {
    content: string;
    truncated?: boolean;
  }

  let { content, truncated = false }: Props = $props();

  let mode = $state<'preview' | 'raw'>('preview');
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  let html = $derived(renderSkillMarkdown(content));

  async function copyRaw() {
    try {
      await navigator.clipboard.writeText(content);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }
</script>

<div class="flex items-center justify-between gap-2 px-3 py-1.5 border-b border-line bg-ink-850">
  <div class="flex items-center border border-line rounded-sm p-0.5 bg-ink-950">
    <button
      type="button"
      onclick={() => (mode = 'preview')}
      class={cn(
        'inline-flex items-center gap-1.5 h-7 px-2.5 rounded-sm font-mono text-[11px] uppercase tracking-wider',
        mode === 'preview' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone',
      )}
    >
      <Eye class="w-3.5 h-3.5" />
      Preview
    </button>
    <button
      type="button"
      onclick={() => (mode = 'raw')}
      class={cn(
        'inline-flex items-center gap-1.5 h-7 px-2.5 rounded-sm font-mono text-[11px] uppercase tracking-wider',
        mode === 'raw' ? 'bg-ink-800 text-phosphor' : 'text-fog hover:text-bone',
      )}
    >
      <FileCode class="w-3.5 h-3.5" />
      Raw
    </button>
  </div>
  <button
    type="button"
    onclick={copyRaw}
    class={btn({ variant: copied ? 'primary' : 'outline', size: 'xs' })}
  >
    {#if copied}
      <Check class="w-3 h-3" />
      Copied
    {:else}
      <Copy class="w-3 h-3" />
      Copy raw
    {/if}
  </button>
</div>

{#if mode === 'preview'}
  <div class="flex-1 overflow-auto bg-ink-950">
    <article class="md-preview">{@html html}</article>
  </div>
{:else}
  <pre
    class="flex-1 overflow-auto p-4 text-[12px] font-mono text-bone/90 whitespace-pre-wrap leading-relaxed bg-ink-950"
  >{content}</pre>
{/if}

{#if truncated}
  <div class="px-4 py-2 border-t border-warning/40 bg-warning/10 text-[10px] font-mono text-warning">
    Preview truncated
  </div>
{/if}
