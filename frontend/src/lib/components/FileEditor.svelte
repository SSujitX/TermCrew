<script lang="ts">
  import { monaco } from '../monaco';
  import { monacoLangFor } from '../fileIcons';
  import { readFile, writeFile } from '../api';
  import { cn, btn } from '../ui';
  import { Save, Check, AlertCircle } from '@lucide/svelte';

  interface Props {
    /** File shown in Monaco. Parent keeps this set while any tab is open. */
    path: string | null;
    /** Open tabs — unused models are disposed when a path drops out. */
    openFiles?: string[];
    onClose: () => void;
    /** Esc — hide the overlay without closing the tab. */
    onHide?: () => void;
    onDirty?: (path: string, dirty: boolean) => void;
    /** False when the overlay is hidden so Esc/Ctrl+S stay with the terminal. */
    active?: boolean;
  }

  let { path, openFiles = [], onClose, onHide, onDirty, active = true }: Props = $props();

  type Cached = {
    model: monaco.editor.ITextModel;
    dirty: boolean;
    expectedModifiedMs: number | null;
    sub: monaco.IDisposable;
  };
  const cache = new Map<string, Cached>();

  // Container is $state so the create effect re-runs when bind:this fires.
  // The Monaco instance itself must NOT be $state — writing it would re-trigger
  // the create effect, dispose the editor, and loop (storm of /api/fs/file reads).
  let container = $state<HTMLDivElement | null>(null);
  let editorReady = $state(false);
  let loading = $state(false);
  let error = $state('');
  let dirty = $state(false);
  let saving = $state(false);
  let savedTick = $state(false);
  let fileName = $state('');

  let editor: monaco.editor.IStandaloneCodeEditor | null = null;
  let model: monaco.editor.ITextModel | null = null;
  let openedPath: string | null = null;
  let expectedModifiedMs: number | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  async function save() {
    if (!editor || !path || !dirty || saving) return;
    saving = true;
    error = '';
    try {
      expectedModifiedMs = await writeFile(path, editor.getValue(), expectedModifiedMs);
      dirty = false;
      const hit = cache.get(path);
      if (hit) {
        hit.dirty = false;
        hit.expectedModifiedMs = expectedModifiedMs;
      }
      onDirty?.(path, false);
      savedTick = true;
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = setTimeout(() => (savedTick = false), 1500);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  // Create Monaco once the container div exists ({#if path} gates it).
  $effect(() => {
    const el = container;
    if (!el) {
      editorReady = false;
      return;
    }

    const ed = monaco.editor.create(el, {
      value: '',
      language: 'plaintext',
      theme: 'termcrew-dark',
      automaticLayout: true,
      minimap: { enabled: true },
      fontSize: 13,
      fontFamily: '"IBM Plex Mono", monospace',
      scrollBeyondLastLine: false,
      renderWhitespace: 'selection',
      tabSize: 4,
    });
    ed.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => void save());
    editor = ed;
    editorReady = true;

    return () => {
      for (const c of cache.values()) {
        c.sub.dispose();
        c.model.dispose();
      }
      cache.clear();
      model = null;
      ed.dispose();
      if (editor === ed) editor = null;
      editorReady = false;
      openedPath = null;
    };
  });

  $effect(() => {
    const keep = new Set(openFiles);
    for (const [p, c] of cache) {
      if (keep.has(p)) continue;
      c.sub.dispose();
      c.model.dispose();
      cache.delete(p);
      if (openedPath === p) {
        openedPath = null;
        model = null;
      }
    }
  });

  // Load a new file whenever `path` changes (after Monaco is ready).
  $effect(() => {
    const p = path;
    if (!p || !editorReady || !editor) {
      if (!p) openedPath = null;
      return;
    }
    if (p === openedPath) return;

    if (openedPath) {
      const prev = cache.get(openedPath);
      if (prev) prev.dirty = dirty;
    }

    openedPath = p;
    fileName = p.split(/[\\/]/).pop() ?? p;
    const hit = cache.get(p);
    if (hit) {
      model = hit.model;
      expectedModifiedMs = hit.expectedModifiedMs;
      dirty = hit.dirty;
      error = '';
      loading = false;
      editor.setModel(hit.model);
      onDirty?.(p, hit.dirty);
      return;
    }

    loading = true;
    error = '';
    dirty = false;

    const ed = editor;
    readFile(p)
      .then((file) => {
        if (openedPath !== p || editor !== ed) return;
        const next = monaco.editor.createModel(file.content, monacoLangFor(file.path));
        const sub = next.onDidChangeContent(() => {
          dirty = true;
          const c = cache.get(p);
          if (c) c.dirty = true;
          onDirty?.(p, true);
        });
        cache.set(p, {
          model: next,
          dirty: false,
          expectedModifiedMs: file.modified_ms,
          sub,
        });
        model = next;
        expectedModifiedMs = file.modified_ms;
        ed.setModel(next);
        dirty = false;
        onDirty?.(p, false);
      })
      .catch((err) => {
        if (openedPath !== p || editor !== ed) return;
        ed.setModel(null);
        model = null;
        error = err instanceof Error ? err.message : String(err);
      })
      .finally(() => {
        if (openedPath === p) loading = false;
      });
  });

  $effect(() => {
    if (!path || !active) return;
    function onKey(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
        e.preventDefault();
        void save();
      }
      if (e.key === 'Escape') (onHide ?? onClose)();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

{#if path}
  <div
    class="h-full w-full bg-ink-900 flex flex-col overflow-hidden"
    role="region"
    aria-label="File editor"
  >
    <div class="h-8 px-2 border-b border-line bg-ink-850 flex items-center justify-end gap-1.5 flex-shrink-0">
      {#if dirty}
        <span class="w-2 h-2 rounded-full bg-[#e2b93d] flex-shrink-0" title="Unsaved changes"></span>
      {/if}
      {#if savedTick}
        <span class="flex items-center gap-1 text-[10px] font-mono text-phosphor uppercase tracking-wider">
          <Check class="w-3 h-3" /> Saved
        </span>
      {/if}
      <button
        type="button"
        onclick={() => void save()}
        disabled={!dirty || saving}
        class={cn(btn({ variant: 'default', size: 'xs' }), 'disabled:opacity-40')}
        title="Save (Ctrl+S)"
      >
        <Save class="w-3.5 h-3.5" />
        <span>{saving ? 'Saving…' : 'Save'}</span>
      </button>
    </div>

    <div class="relative flex-1 min-h-0">
      <div bind:this={container} class="absolute inset-0"></div>

      {#if loading}
        <div class="absolute inset-0 bg-ink-950/70 flex items-center justify-center">
          <span class="font-mono text-xs text-fog">Loading {fileName}…</span>
        </div>
      {:else if error}
        <div class="absolute inset-x-4 top-4 bezel bg-ink-900 border border-alert/40 rounded-sm px-3 py-2.5 flex items-start gap-2.5">
          <AlertCircle class="w-4 h-4 text-alert flex-shrink-0 mt-0.5" />
          <span class="font-mono text-[11px] text-fog leading-relaxed">{error}</span>
        </div>
      {/if}
    </div>

    <div class="h-6 px-3 border-t border-line bg-ink-850 flex items-center justify-between flex-shrink-0">
      <span class="font-mono text-[9px] text-dim truncate" title={path}>{path}</span>
      <span class="font-mono text-[9px] text-dim flex-shrink-0">Ctrl+S save · Esc close</span>
    </div>
  </div>
{/if}
