/**
 * Monaco editor bootstrap: Vite `?worker` imports (paths must match
 * monaco-editor's package "exports" → `monaco-editor/<vs-relative>.js`)
 * plus the min build's aggregated CSS and an app-matched dark theme.
 * Import monaco from here, never directly.
 */
import * as monaco from 'monaco-editor';
import editorWorker from 'monaco-editor/editor/editor.worker.js?worker';
import jsonWorker from 'monaco-editor/language/json/json.worker.js?worker';
import cssWorker from 'monaco-editor/language/css/css.worker.js?worker';
import htmlWorker from 'monaco-editor/language/html/html.worker.js?worker';
import tsWorker from 'monaco-editor/language/typescript/ts.worker.js?worker';
import 'monaco-editor-css';

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    if (label === 'json') return new jsonWorker();
    if (label === 'css' || label === 'scss' || label === 'less') return new cssWorker();
    if (label === 'html' || label === 'handlebars' || label === 'razor') return new htmlWorker();
    if (label === 'typescript' || label === 'javascript') return new tsWorker();
    return new editorWorker();
  },
};

monaco.editor.defineTheme('termcrew-dark', {
  base: 'vs-dark',
  inherit: true,
  rules: [],
  colors: {
    'editor.background': '#0a0f0a',
    'editor.lineHighlightBackground': '#12201200',
    'editorLineNumber.foreground': '#3d4f3d',
    'editorLineNumber.activeForeground': '#57d163',
    'editorGutter.background': '#0a0f0a',
    'editor.selectionBackground': '#1c3a1c',
    'editorCursor.foreground': '#57d163',
    'scrollbarSlider.background': '#1c2a1c80',
  },
});

export { monaco };
