// Verifies every icon name in src/lib/fileIcons.ts exists in the bundled
// vscode-icons collection. A missing name renders as a blank icon, so this
// guard fails the check instead. Run: pnpm icons:check
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const collection = require('@iconify-json/vscode-icons/icons.json');
const known = new Set(Object.keys(collection.icons));
for (const [prefix, chars] of Object.entries(collection.aliases ?? {})) {
  known.add(prefix);
  void chars;
}

const source = readFileSync(new URL('../src/lib/fileIcons.ts', import.meta.url), 'utf8');

// Every icon reference is a quoted string in one of the maps (prefix stripped).
const refs = [...source.matchAll(/'([a-z0-9._-]+)'/g)].map((m) => m[1]);
const iconNames = new Set(
  refs.filter((n) => n.startsWith('file-type-') || n.startsWith('folder-type-') || n.startsWith('default-'))
);

if (iconNames.size === 0) {
  console.error('No icon names found in fileIcons.ts — did the map move?');
  process.exit(1);
}

const missing = [...iconNames].filter((n) => !known.has(n));
for (const name of missing) {
  console.error('MISSING from vscode-icons collection:', name);
}

const unknownExt = refs.filter(
  (n) => !n.startsWith('file-type-') && !n.startsWith('folder-type-') && !n.startsWith('default-') && !n.startsWith('.')
);
void unknownExt;

console.log(`Checked ${iconNames.size} icon names against the collection.`);
if (missing.length > 0) {
  console.error(`${missing.length} icon name(s) do not exist — fix fileIcons.ts.`);
  process.exit(1);
}
console.log('All icon names valid.');
