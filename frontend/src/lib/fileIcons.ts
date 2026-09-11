/**
 * Real file-type icons (the actual JS/TS/Python/Rust/Go logos, etc.) from the
 * vscode-icons collection, served fully offline: the collection JSON is bundled
 * at build time and registered once — no network requests at runtime.
 *
 * Guard: `pnpm icons:check` verifies every icon name below exists in the
 * collection, so a typo can never ship as a blank icon.
 */
import { addCollection } from '@iconify/svelte';
import { default as vscodeIcons } from '@iconify-json/vscode-icons/icons.json';

addCollection(vscodeIcons as Parameters<typeof addCollection>[0]);

const V = 'vscode-icons:';
const FILE: Record<string, string> = {
  'file': V + 'default-file',
};

/** Extension → icon (without prefix). Every name is guard-checked. */
const EXT: Record<string, string> = {
  // ── Systems / compiled ────────────────────────────────────────────
  'rs': 'file-type-rust',
  'go': 'file-type-go',
  'c': 'file-type-c',
  'h': 'file-type-cheader',
  'cpp': 'file-type-cpp',
  'cc': 'file-type-cpp',
  'cxx': 'file-type-cpp',
  'hpp': 'file-type-cpp',
  'hh': 'file-type-cheader',
  'cs': 'file-type-csharp',
  'java': 'file-type-java',
  'kt': 'file-type-kotlin',
  'kts': 'file-type-kotlin',
  'swift': 'file-type-swift',
  'zig': 'file-type-zig',
  'm': 'file-type-objectivec',
  'mm': 'file-type-objectivec',
  's': 'file-type-assembly',
  'asm': 'file-type-assembly',
  'wasm': 'file-type-wasm',
  'pas': 'file-type-delphi',
  'dpr': 'file-type-delphi',
  'f': 'file-type-fortran',
  'for': 'file-type-fortran',
  'f90': 'file-type-fortran',
  'f95': 'file-type-fortran',
  'cbl': 'file-type-cobol',
  'cob': 'file-type-cobol',
  'a': 'file-type-ada',
  'ada': 'file-type-ada',
  'dart': 'file-type-dartlang',

  // ── Scripted ──────────────────────────────────────────────────────
  'py': 'file-type-python',
  'pyi': 'file-type-python',
  'pyw': 'file-type-python',
  'rb': 'file-type-ruby',
  'erb': 'file-type-ruby',
  'php': 'file-type-php',
  'phtml': 'file-type-php',
  'lua': 'file-type-lua',
  'pl': 'file-type-perl',
  'pm': 'file-type-perl',
  'r': 'file-type-r',
  'rmd': 'file-type-rmd',
  'jl': 'file-type-julia',
  'nim': 'file-type-nim',
  'cr': 'file-type-crystal',
  'ex': 'file-type-elixir',
  'exs': 'file-type-elixir',
  'erl': 'file-type-erlang',
  'hrl': 'file-type-erlang',
  'hs': 'file-type-haskell',
  'lhs': 'file-type-haskell',
  'clj': 'file-type-clojure',
  'cljs': 'file-type-clojurescript',
  'cljc': 'file-type-clojure',
  'edn': 'file-type-clojure',
  'lisp': 'file-type-lisp',
  'el': 'file-type-lisp',
  'fs': 'file-type-fsharp',
  'fsi': 'file-type-fsharp',
  'fsx': 'file-type-fsharp',
  'ml': 'file-type-ocaml',
  'mli': 'file-type-ocaml',
  'scala': 'file-type-scala',
  'sc': 'file-type-scala',
  'groovy': 'file-type-groovy',
  'jenkinsfile': 'file-type-jenkins',
  'sol': 'file-type-solidity',
  'vy': 'file-type-vyper',
  'v': 'file-type-verilog',
  'sv': 'file-type-systemverilog',
  'vhd': 'file-type-vhdl',
  'vhdl': 'file-type-vhdl',
  'mat': 'file-type-matlab',
  'do': 'file-type-stata',
  'dta': 'file-type-stata',
  'sas': 'file-type-sas',
  'awk': 'file-type-awk',
  'as': 'file-type-actionscript',
  'hx': 'file-type-haxe',
  'elm': 'file-type-elm',
  'gleam': 'file-type-gleam',
  'exs-config': 'file-type-elixir',

  // ── Web ───────────────────────────────────────────────────────────
  'js': 'file-type-js-official',
  'mjs': 'file-type-js-official',
  'cjs': 'file-type-js-official',
  'jsx': 'file-type-js-official',
  'ts': 'file-type-typescript-official',
  'tsx': 'file-type-typescript-official',
  'mts': 'file-type-typescript-official',
  'cts': 'file-type-typescript-official',
  'svelte': 'file-type-svelte',
  'vue': 'file-type-vue',
  'astro': 'file-type-astro',
  'html': 'file-type-html',
  'htm': 'file-type-html',
  'css': 'file-type-css',
  'scss': 'file-type-scss',
  'sass': 'file-type-sass',
  'less': 'file-type-less',
  'pug': 'file-type-pug',
  'jade': 'file-type-pug',
  'haml': 'file-type-haml',
  'ejs': 'file-type-ejs',
  'cshtml': 'file-type-razor',
  'razor': 'file-type-razor',
  'prisma': 'file-type-prisma',
  'graphql': 'file-type-graphql',
  'gql': 'file-type-graphql',
  'tex': 'file-type-tex',

  // ── Config / data ─────────────────────────────────────────────────
  'json': 'file-type-json',
  'jsonc': 'file-type-json',
  'json5': 'file-type-json',
  'toml': 'file-type-toml',
  'yaml': 'file-type-yaml',
  'yml': 'file-type-yaml',
  'xml': 'file-type-xml',
  'ini': 'file-type-ini',
  'cfg': 'file-type-ini',
  'conf': 'file-type-ini',
  'env': 'file-type-dotenv',
  'properties': 'file-type-ini',
  'gradle': 'file-type-gradle',
  'bicep': 'file-type-bicep',
  'nix': 'file-type-nix',
  'bzl': 'file-type-bazel',
  'tf': 'file-type-terraform',
  'tfvars': 'file-type-terraform',
  'hcl': 'file-type-terraform',
  'proto': 'file-type-protobuf',
  'cmake': 'file-type-cmake',

  // ── Docs ──────────────────────────────────────────────────────────
  'md': 'file-type-markdown',
  'markdown': 'file-type-markdown',
  'mdx': 'file-type-markdown',
  'txt': 'file-type-text',
  'text': 'file-type-text',
  'log': 'file-type-log',
  'pdf': 'file-type-pdf2',
  'doc': 'file-type-word',
  'docx': 'file-type-word',
  'xls': 'file-type-excel2',
  'xlsx': 'file-type-excel2',
  'ppt': 'file-type-powerpoint',
  'pptx': 'file-type-powerpoint',

  // ── Media ─────────────────────────────────────────────────────────
  'png': 'file-type-image',
  'jpg': 'file-type-image',
  'jpeg': 'file-type-image',
  'gif': 'file-type-image',
  'ico': 'file-type-image',
  'webp': 'file-type-image',
  'bmp': 'file-type-image',
  'tiff': 'file-type-image',
  'mp3': 'file-type-audio',
  'wav': 'file-type-audio',
  'flac': 'file-type-audio',
  'ogg': 'file-type-audio',
  'mp4': 'file-type-video',
  'mov': 'file-type-video',
  'avi': 'file-type-video',
  'mkv': 'file-type-video',
  'webm': 'file-type-video',
  'woff': 'file-type-font',
  'woff2': 'file-type-font',
  'ttf': 'file-type-font',
  'otf': 'file-type-font',

  // ── Shells ────────────────────────────────────────────────────────
  'sh': 'file-type-shell',
  'bash': 'file-type-shell',
  'zsh': 'file-type-shell',
  'fish': 'file-type-shell',
  'ksh': 'file-type-shell',
  'bat': 'file-type-bat',
  'cmd': 'file-type-bat',
  'ps1': 'file-type-powershell',
  'psm1': 'file-type-powershell',
  'psd1': 'file-type-powershell',

  // ── Data / archives / binaries ────────────────────────────────────
  'sql': 'file-type-sqlite',
  'sqlite': 'file-type-sqlite',
  'sqlite3': 'file-type-sqlite',
  'db': 'file-type-sqlite',
  'har': 'file-type-http',
  'ipynb': 'file-type-jupyter',
  'csv': 'file-type-excel2',
  'tsv': 'file-type-excel2',
  'parquet': 'file-type-excel2',
  'yaml-sealed': 'file-type-yaml',
  'zip': 'file-type-zip',
  '7z': 'file-type-zip',
  'rar': 'file-type-zip',
  'tar': 'file-type-zip',
  'gz': 'file-type-zip',
  'bz2': 'file-type-zip',
  'xz': 'file-type-zip',
  'exe': 'file-type-binary',
  'dll': 'file-type-binary',
  'so': 'file-type-binary',
  'dylib': 'file-type-binary',
  'bin': 'file-type-binary',
  'o': 'file-type-binary',
  'class': 'file-type-binary',
  'jar': 'file-type-jar',
};

/** Exact file names (case-insensitive) that deserve a special icon. */
const NAME: Record<string, string> = {
  'cargo.lock': 'file-type-cargo',
  'cargo.toml': 'file-type-cargo',
  'package.json': 'file-type-npm',
  'package-lock.json': 'file-type-npm',
  'yarn.lock': 'file-type-npm',
  'pnpm-lock.yaml': 'file-type-pnpm',
  'tsconfig.json': 'file-type-tsconfig',
  'jsconfig.json': 'file-type-tsconfig',
  'license': 'file-type-license',
  'license.md': 'file-type-license',
  'license.txt': 'file-type-license',
  'dockerfile': 'file-type-docker',
  'docker-compose.yml': 'file-type-docker',
  'docker-compose.yaml': 'file-type-docker',
  '.gitignore': 'file-type-git',
  '.gitattributes': 'file-type-git',
  '.gitmodules': 'file-type-git',
  'vagrantfile': 'file-type-vagrant',
  'requirements.txt': 'file-type-python',
  'pyproject.toml': 'file-type-python',
  'cmakelists.txt': 'file-type-cmake',
  'readme': 'file-type-markdown',
  'readme.md': 'file-type-markdown',
  'makefile': 'file-type-config',
  '.env': 'file-type-dotenv',
  '.env.local': 'file-type-dotenv',
  '.env.development': 'file-type-dotenv',
  '.env.production': 'file-type-dotenv',
};

/** Special folder names; everything else gets the generic folder icon. */
const FOLDER: Record<string, string> = {
  'src': 'folder-type-src',
  'source': 'folder-type-src',
  'tests': 'folder-type-test',
  'test': 'folder-type-test',
  '__tests__': 'folder-type-test',
  'spec': 'folder-type-test',
  'docs': 'folder-type-docs',
  'doc': 'folder-type-docs',
  'dist': 'folder-type-dist',
  'build': 'folder-type-dist',
  'out': 'folder-type-dist',
  'target': 'folder-type-dist',
  '.git': 'folder-type-git',
  'node_modules': 'folder-type-node',
  '.github': 'folder-type-git',
};

export function fileIconFor(fileName: string): string {
  const lower = fileName.toLowerCase();
  const byName = NAME[lower];
  if (byName) return V + byName;
  const dot = lower.lastIndexOf('.');
  if (dot <= 0) return FILE.file;
  const ext = lower.slice(dot + 1);
  return V + (EXT[ext] ?? 'default-file');
}

export function folderIconFor(folderName: string, opened: boolean): string {
  const special = FOLDER[folderName.toLowerCase()];
  if (special) return V + special + (opened ? '-opened' : '');
  return V + (opened ? 'default-folder-opened' : 'default-folder');
}

/** Extension → Monaco language id ('plaintext' when Monaco has no grammar). */
const LANG_MAP: Record<string, string> = {
  rs: 'rust', go: 'go', c: 'c', h: 'c', cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp',
  cs: 'csharp', java: 'java', kt: 'kotlin', swift: 'swift', dart: 'dart',
  py: 'python', pyi: 'python', rb: 'ruby', php: 'php', lua: 'lua', pl: 'perl',
  pm: 'perl', r: 'r', jl: 'julia', hs: 'haskell', clj: 'clojure', cljs: 'clojure',
  fs: 'fsharp', ml: 'ocaml', scala: 'scala', groovy: 'groovy', sol: 'sol',
  js: 'javascript', mjs: 'javascript', cjs: 'javascript', jsx: 'javascript',
  ts: 'typescript', tsx: 'typescript', mts: 'typescript', cts: 'typescript',
  html: 'html', htm: 'html', css: 'css', scss: 'scss', sass: 'scss', less: 'less',
  json: 'json', jsonc: 'json', json5: 'json', md: 'markdown', markdown: 'markdown',
  mdx: 'markdown', xml: 'xml', svg: 'xml', yaml: 'yaml', yml: 'yaml',
  toml: 'ini', ini: 'ini', cfg: 'ini', conf: 'ini', properties: 'ini', sql: 'sql',
  sh: 'shell', bash: 'shell', zsh: 'shell', ksh: 'shell', ps1: 'powershell',
  psm1: 'powershell', bat: 'bat', cmd: 'bat', tex: 'latex', proto: 'proto3',
  graphql: 'graphql', gql: 'graphql', hcl: 'hcl', tf: 'hcl', tfvars: 'hcl',
  dockerfile: 'dockerfile', makefile: 'shell', exe: 'plaintext', txt: 'plaintext',
};

export function monacoLangFor(fileName: string): string {
  const lower = fileName.toLowerCase();
  if (lower === 'dockerfile') return 'dockerfile';
  const dot = lower.lastIndexOf('.');
  if (dot <= 0) return 'plaintext';
  return LANG_MAP[lower.slice(dot + 1)] ?? 'plaintext';
}
