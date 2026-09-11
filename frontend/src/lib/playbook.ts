import type { Session } from './types';
import { listDir, readFile } from './api';

export type Lane = 'build' | 'review' | 'shell';

const SHELL_ENGINES = new Set(['shell', 'cmd', 'git-bash', 'wsl', 'bash', 'zsh', 'sh']);

export function laneOf(session: Session): Lane {
  const role = (session.role ?? '').toLowerCase();
  const engine = session.engine.toLowerCase();
  if (role === 'shell' || SHELL_ENGINES.has(engine)) return 'shell';
  if (role.startsWith('review')) return 'review';
  return 'build';
}

export function running(crew: Session[]): Session[] {
  return crew.filter((s) => s.status === 'running');
}

/** Builders → each live reviewer. Empty when the crew has no reviewer. */
export function reviewSends(crew: Session[]): { source: Session; target: Session }[] {
  const live = running(crew);
  const builders = live.filter((s) => laneOf(s) === 'build');
  const reviewers = live.filter((s) => laneOf(s) === 'review');
  if (builders.length === 0 || reviewers.length === 0) return [];
  const out: { source: Session; target: Session }[] = [];
  for (const source of builders) {
    for (const target of reviewers) out.push({ source, target });
  }
  return out;
}

/** Live shell only — never type a test command into an agent chat box. */
export function testTarget(crew: Session[]): Session | null {
  return running(crew).find((s) => laneOf(s) === 'shell') ?? null;
}

/**
 * One contract per role. Reviewers do not implement. Shells get nothing
 * (they receive the test command later). Builders get a slice when N>1.
 */
export function playbookLine(session: Session, goal: string, crew: Session[]): string | null {
  const g = goal.trim();
  if (!g) return null;
  const lane = laneOf(session);
  if (lane === 'shell') return null;
  const folder = session.worktree_path || session.working_dir;
  if (lane === 'review') {
    return [
      `GOAL: ${g}`,
      '',
      'You review only. Do not implement.',
      'Wait for a git review packet (role, folder, diff).',
      'Then list bugs and missing tests. Do not push.',
    ].join('\n');
  }
  const builders = crew.filter((s) => laneOf(s) === 'build');
  const n = builders.length;
  const i = builders.findIndex((s) => s.id === session.id);
  if (n > 1 && i >= 0) {
    return [
      `GOAL: ${g}`,
      '',
      `You are builder ${i + 1} of ${n} (${session.role ?? 'Lead'}).`,
      `Implement one non-overlapping slice in ${folder}.`,
      'Do not merge or push. Stop when your slice works.',
    ].join('\n');
  }
  return [
    `GOAL: ${g}`,
    '',
    `Implement this in ${folder}.`,
    'Do not push. Stop when it works.',
  ].join('\n');
}

function joinUnder(root: string, name: string): string {
  const slash = root.includes('\\') ? '\\' : '/';
  return root.endsWith('/') || root.endsWith('\\') ? `${root}${name}` : `${root}${slash}${name}`;
}

/** Infer a test command from the folder. Null if nothing obvious. */
export async function detectTestCommand(root: string): Promise<string | null> {
  if (!root.trim()) return null;
  let names: Set<string>;
  try {
    const listing = await listDir(root);
    names = new Set(listing.entries.map((e) => e.name.toLowerCase()));
  } catch {
    return null;
  }
  if (names.has('cargo.toml')) return 'cargo test';
  if (names.has('go.mod')) return 'go test ./...';
  if (names.has('pytest.ini') || names.has('conftest.py')) return 'pytest';
  if (names.has('package.json')) {
    try {
      const file = await readFile(joinUnder(root, 'package.json'));
      const pkg = JSON.parse(file.content) as { scripts?: Record<string, string> };
      if (!pkg.scripts?.test) return null;
    } catch {
      return null;
    }
    if (names.has('bun.lock') || names.has('bun.lockb')) return 'bun run test';
    if (names.has('pnpm-lock.yaml')) return 'pnpm test';
    if (names.has('yarn.lock')) return 'yarn test';
    return 'npm test';
  }
  if (names.has('pyproject.toml')) return 'pytest';
  return null;
}
