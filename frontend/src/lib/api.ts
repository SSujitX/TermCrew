import type {
  AgentMeta,
  DetectedSkill,
  LaunchPresetRequest,
  MarketplaceSearch,
  PresetType,
  Session,
  SkillContent,
  SkillsCatalog,
} from './types';

const BASE_URL = '';

/** Shape returned by the backend GET /api/sessions (session::SessionInfo). */
interface BackendSessionInfo {
  id: string;
  name: string;
  engine: string;
  preset: string;
  role?: string | null;
  working_dir: string;
  worktree_path?: string | null;
  created_at: string;
  is_alive: boolean;
  group_id?: string | null;
  group_label?: string | null;
  task?: string | null;
}

/** Shape returned by the backend GET /api/agents (registry::DetectedAgent). */
interface BackendDetectedAgent {
  id: string;
  name: string;
  binary: string;
  description: string;
  category: string;
  is_installed: boolean;
  binary_path: string | null;
  install_cmd?: string | null;
  update_cmd?: string | null;
  uninstall_cmd?: string | null;
  manage_hint?: string | null;
  docs_url?: string | null;
}

function mapSession(b: BackendSessionInfo): Session {
  const preset = (b.preset || 'solo').toLowerCase() as PresetType;
  const group_id = b.group_id?.trim() || b.id;
  const group_label =
    b.group_label?.trim() ||
    `${b.preset || 'Solo'}`;
  return {
    id: b.id,
    name: b.name,
    engine: b.engine,
    preset,
    working_dir: b.working_dir,
    worktree_path: b.worktree_path ?? undefined,
    status: b.is_alive ? 'running' : 'exited',
    created_at: b.created_at,
    role: b.role ?? undefined,
    group_id,
    group_label,
    task: b.task?.trim() || undefined,
  };
}

const PROVIDERS: Record<string, string> = {
  claude: 'Anthropic',
  opencode: 'OpenCode',
  openclaw: 'OpenClaw',
  pi: 'pi-mono',
  hermes: 'Nous Research',
  aider: 'Aider',
  gemini: 'Google',
  agy: 'Google',
  'cursor-agent': 'Anysphere',
  codex: 'OpenAI',
  amp: 'Sourcegraph',
  goose: 'Block / LF',
  cline: 'Cline',
  crush: 'Charmbracelet',
  qwen: 'Alibaba',
  kimi: 'Moonshot',
  plandex: 'Plandex',
  openhands: 'All Hands',
  interpreter: 'Open Interpreter',
  continue: 'Continue.dev',
  kilo: 'Kilo',
  vibe: 'Mistral',
  kiro: 'AWS',
  forge: 'ForgeCode',
  gptme: 'gptme',
  codewhale: 'Codewhale',
  deepseek: 'DeepSeek',
  grok: 'xAI',
  shell: 'System Runtime',
  cmd: 'Windows',
  'git-bash': 'Git',
  wsl: 'Microsoft',
  bash: 'GNU',
  zsh: 'Zsh',
  sh: 'POSIX',
};

/** Official vendor docs. Live values come from the backend; this is the offline fallback. */
const DOCS_URLS: Record<string, string> = {
  claude: 'https://code.claude.com/docs/en/overview',
  opencode: 'https://opencode.ai/docs',
  openclaw: 'https://docs.openclaw.ai',
  pi: 'https://github.com/badlogic/pi-mono',
  hermes: 'https://hermes-agent.nousresearch.com/docs/getting-started/installation',
  aider: 'https://aider.chat/docs/install.html',
  gemini: 'https://github.com/google-gemini/gemini-cli',
  agy: 'https://antigravity.google/docs/cli/install/',
  'cursor-agent': 'https://cursor.com/docs/cli/overview',
  codex: 'https://developers.openai.com/codex/cli',
  amp: 'https://ampcode.com/manual',
  goose: 'https://block.github.io/goose/docs/getting-started/installation',
  cline: 'https://docs.cline.bot/cline-cli/installation',
  crush: 'https://github.com/charmbracelet/crush',
  qwen: 'https://qwenlm.github.io/qwen-code-docs/en/users/quickstart/',
  kimi: 'https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html',
  plandex: 'https://docs.plandex.ai/install',
  openhands: 'https://docs.openhands.dev/openhands/usage/cli/installation',
  interpreter: 'https://docs.openinterpreter.com',
  continue: 'https://docs.continue.dev/cli/quickstart',
  kilo: 'https://kilo.ai/docs/code-with-ai/platforms/cli',
  vibe: 'https://docs.mistral.ai/getting-started/quickstarts/vibe-code/install-cli',
  kiro: 'https://kiro.dev/docs/cli',
  forge: 'https://forgecode.dev/docs',
  gptme: 'https://gptme.org/docs/getting-started.html',
  codewhale: 'https://github.com/Hmbown/CodeWhale',
  deepseek: 'https://deepseek-harness.github.io/deepseek-harness/',
  grok: 'https://docs.x.ai/docs',
  shell: 'https://learn.microsoft.com/powershell/scripting/overview',
  cmd: 'https://learn.microsoft.com/windows-server/administration/windows-commands/cmd',
  'git-bash': 'https://git-scm.com/downloads/win',
  wsl: 'https://learn.microsoft.com/windows/wsl/install',
  bash: 'https://www.gnu.org/software/bash/manual',
  zsh: 'https://zsh.sourceforge.io/Doc/',
  sh: 'https://pubs.opengroup.org/onlinepubs/9699919799/utilities/sh.html',
};

/** Offline fallback only. Live commands come from the backend (official vendor docs). */
function isWindowsHost(): boolean {
  return typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);
}

const INSTALL_CMDS_WIN: Record<string, string> = {
  claude: 'irm https://claude.ai/install.ps1 | iex',
  opencode: 'npm i -g opencode-ai@latest',
  openclaw: 'iwr -useb https://openclaw.ai/install.ps1 | iex',
  pi: 'npm install -g --ignore-scripts @earendil-works/pi-coding-agent',
  hermes: 'iex (irm https://hermes-agent.nousresearch.com/install.ps1)',
  aider: 'python -m pip install aider-install; aider-install',
  gemini: 'npm install -g @google/gemini-cli',
  agy: 'irm https://antigravity.google/cli/install.ps1 | iex',
  'cursor-agent': "irm 'https://cursor.com/install?win32=true' | iex",
  codex: 'powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"',
  amp: 'powershell -c "irm https://ampcode.com/install.ps1 | iex"',
  goose: 'irm https://github.com/block/goose/raw/main/download_cli.ps1 | iex',
  cline: 'npm install -g cline',
  crush: 'winget install --id charmbracelet.crush -e',
  qwen: 'irm https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.ps1 | iex',
  kimi: 'irm https://code.kimi.com/kimi-code/install.ps1 | iex',
  openhands: 'uv tool install openhands --python 3.12',
  interpreter: 'irm https://www.openinterpreter.com/install.ps1 | iex',
  continue: 'irm https://raw.githubusercontent.com/continuedev/continue/main/extensions/cli/scripts/install.ps1 | iex',
  kilo: 'npm install -g @kilocode/cli',
  vibe: 'uv tool install mistral-vibe',
  kiro: "irm 'https://cli.kiro.dev/install.ps1' | iex",
  forge: "bash -lc 'curl -fsSL https://forgecode.dev/cli | sh'",
  gptme: 'pipx install gptme',
  codewhale: 'npm install -g codewhale',
  deepseek: 'npm install -g @deepseek-ai/dsh',
  grok: 'irm https://x.ai/cli/install.ps1 | iex',
};

const INSTALL_CMDS_UNIX: Record<string, string> = {
  claude: 'curl -fsSL https://claude.ai/install.sh | bash',
  opencode: 'curl -fsSL https://opencode.ai/install | bash',
  openclaw: 'curl -fsSL https://openclaw.ai/install.sh | bash',
  pi: 'npm install -g --ignore-scripts @earendil-works/pi-coding-agent',
  hermes: 'curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash',
  aider: 'python -m pip install aider-install; aider-install',
  gemini: 'npm install -g @google/gemini-cli',
  agy: 'curl -fsSL https://antigravity.google/cli/install.sh | bash',
  'cursor-agent': 'curl https://cursor.com/install -fsS | bash',
  codex: 'curl -fsSL https://chatgpt.com/codex/install.sh | sh',
  amp: 'curl -fsSL https://ampcode.com/install.sh | bash',
  goose: 'curl -fsSL https://github.com/block/goose/releases/download/stable/download_cli.sh | bash',
  cline: 'npm install -g cline',
  crush: 'brew install charmbracelet/tap/crush',
  qwen: 'curl -fsSL https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.sh | bash',
  kimi: 'curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash',
  openhands: 'uv tool install openhands --python 3.12',
  interpreter: 'curl -fsSL https://www.openinterpreter.com/install | sh',
  continue: 'curl -fsSL https://raw.githubusercontent.com/continuedev/continue/main/extensions/cli/scripts/install.sh | bash',
  kilo: 'npm install -g @kilocode/cli',
  vibe: 'uv tool install mistral-vibe',
  kiro: 'curl -fsSL https://cli.kiro.dev/install | bash',
  forge: 'curl -fsSL https://forgecode.dev/cli | sh',
  gptme: 'pipx install gptme',
  codewhale: 'npm install -g codewhale',
  deepseek: 'npm install -g @deepseek-ai/dsh',
  grok: 'curl -fsSL https://x.ai/cli/install.sh | bash',
};

function offlineInstallCmd(id: string): string | undefined {
  return (isWindowsHost() ? INSTALL_CMDS_WIN : INSTALL_CMDS_UNIX)[id];
}

function mapAgent(a: BackendDetectedAgent): AgentMeta {
  return {
    id: a.id,
    name: a.name,
    provider: PROVIDERS[a.id] ?? a.binary,
    binary: a.binary,
    is_installed: a.is_installed,
    path: a.binary_path ?? undefined,
    description: a.description,
    category: a.category === 'Shell' ? 'terminal' : 'coding',
    // Keep intentional backend null (no installer) — do not invent Windows cmds.
    install_cmd: a.install_cmd ?? undefined,
    update_cmd: a.update_cmd ?? undefined,
    uninstall_cmd: a.uninstall_cmd ?? undefined,
    manage_hint: a.manage_hint ?? undefined,
    docs_url: a.docs_url ?? DOCS_URLS[a.id],
  };
}

/** Fallback list used only while the backend is unreachable. Mirrors backend registry IDs. */
const FALLBACK_AGENTS: AgentMeta[] = [
  {
    id: 'claude',
    name: 'Claude Code',
    provider: 'Anthropic',
    binary: 'claude',
    icon: 'Brain',
    install_cmd: offlineInstallCmd('claude'),
    is_installed: false,
    description: "Anthropic's agentic CLI for coding and reasoning",
    category: 'coding',
  },
  {
    id: 'opencode',
    name: 'OpenCode',
    provider: 'OpenCode',
    binary: 'opencode',
    icon: 'Code2',
    install_cmd: offlineInstallCmd('opencode'),
    is_installed: false,
    description: 'Open-source autonomous terminal coding assistant',
    category: 'coding',
  },
  {
    id: 'aider',
    name: 'Aider',
    provider: 'Paul Gauthier',
    binary: 'aider',
    icon: 'Sparkles',
    install_cmd: offlineInstallCmd('aider'),
    is_installed: false,
    description: 'AI pair programming in your terminal',
    category: 'coding',
  },
  {
    id: 'gemini',
    name: 'Gemini CLI',
    provider: 'Google',
    binary: 'gemini',
    icon: 'Bot',
    install_cmd: offlineInstallCmd('gemini'),
    is_installed: false,
    description: 'Google Gemini command line coding agent',
    category: 'general',
  },
  {
    id: 'cursor-agent',
    name: 'Cursor Agent',
    provider: 'Anysphere',
    binary: 'agent',
    icon: 'Layers',
    install_cmd: offlineInstallCmd('cursor-agent'),
    is_installed: false,
    description: 'Headless Cursor agent CLI for automated changes',
    category: 'coding',
  },
  {
    id: 'codex',
    name: 'Codex',
    provider: 'OpenAI',
    binary: 'codex',
    icon: 'Terminal',
    install_cmd: offlineInstallCmd('codex'),
    is_installed: false,
    description: 'OpenAI Codex CLI harness',
    category: 'coding',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek Harness',
    provider: 'DeepSeek',
    binary: 'dsh',
    icon: 'Cpu',
    install_cmd: offlineInstallCmd('deepseek'),
    is_installed: false,
    description: 'DeepSeek AI open-source agent harness (dsh)',
    category: 'coding',
  },
  {
    id: 'grok',
    name: 'Grok Build',
    provider: 'xAI',
    binary: 'grok',
    icon: 'Zap',
    install_cmd: offlineInstallCmd('grok'),
    is_installed: false,
    description: 'xAI Grok official coding agent TUI',
    category: 'coding',
  },
  {
    id: 'pi',
    name: 'Pi',
    provider: 'pi-mono',
    binary: 'pi',
    install_cmd: offlineInstallCmd('pi'),
    is_installed: false,
    description: 'Minimal terminal coding harness from pi-mono',
    category: 'coding',
  },
  {
    id: 'hermes',
    name: 'Hermes Agent',
    provider: 'Nous Research',
    binary: 'hermes',
    install_cmd: offlineInstallCmd('hermes'),
    is_installed: false,
    description: 'Nous Research self-improving CLI agent',
    category: 'coding',
  },
  {
    id: 'openclaw',
    name: 'OpenClaw',
    provider: 'OpenClaw',
    binary: 'openclaw',
    install_cmd: offlineInstallCmd('openclaw'),
    is_installed: false,
    description: 'Local personal AI assistant CLI with skills and channels',
    category: 'coding',
  },
  ...fallbackShells(),
];

function fallbackShells(): AgentMeta[] {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  const win = /Windows/i.test(ua);
  const mac = /Mac/i.test(ua);
  const builtIn = { category: 'terminal' as const, manage_hint: 'Built into the system', is_installed: true };
  if (win) {
    return [
      { id: 'shell', name: 'PowerShell', provider: 'System Runtime', binary: 'powershell.exe', ...builtIn, description: 'Default system terminal shell' },
      { id: 'cmd', name: 'Command Prompt', provider: 'Windows', binary: 'cmd.exe', ...builtIn, description: 'Windows Command Prompt' },
      { id: 'git-bash', name: 'Git Bash', provider: 'Git', binary: 'bash.exe', is_installed: false, description: 'Git for Windows bash', category: 'terminal', manage_hint: 'Install Git for Windows from https://git-scm.com/downloads/win' },
      { id: 'wsl', name: 'WSL', provider: 'Microsoft', binary: 'wsl.exe', is_installed: false, description: 'Windows Subsystem for Linux', category: 'terminal', manage_hint: 'Install WSL from Microsoft docs' },
    ];
  }
  const shells: AgentMeta[] = [
    {
      id: 'shell',
      name: mac ? 'Zsh' : 'Bash',
      provider: 'System Runtime',
      binary: mac ? 'zsh' : 'bash',
      ...builtIn,
      description: 'Default system terminal shell',
    },
  ];
  if (mac) {
    shells.push({ id: 'bash', name: 'Bash', provider: 'GNU', binary: 'bash', ...builtIn, description: 'Bourne-again shell' });
  } else {
    shells.push({ id: 'zsh', name: 'Zsh', provider: 'Zsh', binary: 'zsh', is_installed: false, description: 'Z shell', category: 'terminal', manage_hint: 'Built into the system' });
  }
  shells.push({ id: 'sh', name: 'sh', provider: 'POSIX', binary: 'sh', ...builtIn, description: 'POSIX shell' });
  return shells;
}

export const DEFAULT_AGENTS: AgentMeta[] = FALLBACK_AGENTS.map((agent) => ({
  ...agent,
  docs_url: agent.docs_url ?? DOCS_URLS[agent.id],
}));

async function readJson(res: Response): Promise<any | null> {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

function backendOfflineError(err: unknown): Error {
  return new Error(
    `Cannot reach the backend on port 3001 — is it running? Start it with run-dev.ps1 or \`cargo run\` in backend/. (${
      err instanceof Error ? err.message : String(err)
    })`,
  );
}

export async function startAgentSetup(
  agentId: string,
  action: 'install' | 'update' | 'uninstall',
): Promise<Session> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/agents/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ agent_id: agentId, action }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }

  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Setup failed (HTTP ${res.status})`);
  }
  if (!data?.session) {
    throw new Error('Unexpected response from backend');
  }
  return mapSession(data.session as BackendSessionInfo);
}

export async function getAgents(fresh = false): Promise<AgentMeta[]> {
  try {
    const res = await fetch(`${BASE_URL}/api/agents${fresh ? '?fresh=true' : ''}`);
    if (res.ok) {
      const data = await res.json();
      if (Array.isArray(data) && data.length > 0) {
        return (data as BackendDetectedAgent[]).map(mapAgent);
      }
    }
  } catch {
    // Backend offline: fall back to static registry so the launcher is still usable
  }
  return DEFAULT_AGENTS;
}

function workdirQuery(workdir?: string | null, extra?: Record<string, string>): string {
  const params = new URLSearchParams();
  if (workdir?.trim()) params.set('workdir', workdir.trim());
  if (extra) {
    for (const [k, v] of Object.entries(extra)) {
      if (v !== undefined && v !== '') params.set(k, v);
    }
  }
  const q = params.toString();
  return q ? `?${q}` : '';
}

export async function getSkills(workdir?: string | null, fresh = false): Promise<SkillsCatalog> {
  let res: Response;
  try {
    const qs = workdirQuery(workdir, fresh ? { fresh: 'true' } : undefined);
    res = await fetch(`${BASE_URL}/api/skills${qs}`);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Failed to load skills (HTTP ${res.status})`);
  }
  return {
    roots: Array.isArray(data?.roots) ? data.roots : [],
    skills: Array.isArray(data?.skills) ? data.skills : [],
    workdir: data?.workdir ?? workdir ?? null,
  };
}

export async function getSkillContent(
  path: string,
  workdir?: string | null,
): Promise<SkillContent> {
  let res: Response;
  try {
    const params = new URLSearchParams();
    params.set('path', path);
    if (workdir?.trim()) params.set('workdir', workdir.trim());
    res = await fetch(`${BASE_URL}/api/skills/content?${params}`);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Failed to read skill (HTTP ${res.status})`);
  }
  return {
    path: data?.path ?? path,
    content: data?.content ?? '',
    truncated: Boolean(data?.truncated),
  };
}

export async function setSkillEnabled(path: string, enabled: boolean): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/skills/prefs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path, enabled }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Failed to update skill prefs (HTTP ${res.status})`);
  }
}

export async function copySkill(
  workdir: string | null | undefined,
  body: { source_path: string; target_root_id: string; name?: string },
): Promise<DetectedSkill> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/skills/copy${workdirQuery(workdir)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Copy failed (HTTP ${res.status})`);
  }
  return (data?.skill ?? data) as DetectedSkill;
}

export async function deleteSkill(
  workdir: string | null | undefined,
  path: string,
): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/skills/delete${workdirQuery(workdir)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path, confirm: 'DELETE' }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Delete failed (HTTP ${res.status})`);
  }
}

export type SkillInstallResult =
  | { kind: 'copied'; skills: DetectedSkill[] }
  | { kind: 'setup'; session: Session; command: string };

export async function installSkill(
  workdir: string | null | undefined,
  body: {
    source: 'local' | 'git';
    path_or_url: string;
    target_root_id: string;
    name?: string;
  },
): Promise<SkillInstallResult> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/skills/install${workdirQuery(workdir)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Install failed (HTTP ${res.status})`);
  }
  if (data?.session) {
    return {
      kind: 'setup',
      session: mapSession(data.session as BackendSessionInfo),
      command: typeof data.command === 'string' ? data.command : '',
    };
  }
  if (Array.isArray(data?.skills)) return { kind: 'copied', skills: data.skills as DetectedSkill[] };
  if (Array.isArray(data)) return { kind: 'copied', skills: data as DetectedSkill[] };
  if (data?.skill) return { kind: 'copied', skills: [data.skill as DetectedSkill] };
  return { kind: 'copied', skills: [] };
}

export async function openSkillFolder(
  path: string,
  workdir?: string | null,
): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/skills/open${workdirQuery(workdir)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Open folder failed (HTTP ${res.status})`);
  }
}

export async function searchSkillsMarketplace(
  q: string,
  opts?: { view?: string; page?: number; perPage?: number },
): Promise<MarketplaceSearch> {
  const params = new URLSearchParams();
  if (q) params.set('q', q);
  if (opts?.view) params.set('view', opts.view);
  if (opts?.page != null) params.set('page', String(opts.page));
  if (opts?.perPage != null) params.set('per_page', String(opts.perPage));
  const qs = params.toString();
  let res: Response;
  try {
    res = await fetch(
      `${BASE_URL}/api/skills/marketplace${qs ? `?${qs}` : ''}`,
    );
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Marketplace search failed (HTTP ${res.status})`);
  }
  return {
    query: data?.query ?? q,
    view: data?.view ?? opts?.view ?? 'hot',
    page: Number(data?.page) || 0,
    per_page: Number(data?.per_page) || 9,
    total: Number(data?.total) || 0,
    has_more: Boolean(data?.has_more),
    skills: Array.isArray(data?.skills) ? data.skills : [],
  };
}

export async function startSkillsMarketplaceInstall(body: {
  source: string;
  skill?: string;
  harness: string;
  global?: boolean;
}): Promise<{ session: Session; command: string }> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/skills/marketplace/install`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Marketplace install failed (HTTP ${res.status})`);
  }
  if (!data?.session) {
    throw new Error('Unexpected response from backend');
  }
  return {
    session: mapSession(data.session as BackendSessionInfo),
    command: typeof data.command === 'string' ? data.command : '',
  };
}

/** Shape returned by GET /api/workdirs (workdirs::RecentWorkdir). */
export interface RecentWorkdir {
  path: string;
  last_used_at: string;
  use_count: number;
}

/** Shape returned by GET /api/fs/dirs and GET /api/fs/list (workdirs::BrowseListing). */
export interface FsEntry {
  name: string;
  path: string;
  kind: 'dir' | 'file';
}

export interface DirListing {
  path: string;
  parent: string | null;
  entries: FsEntry[];
}

/** Recently used launch folders, newest first. Degrades to [] when offline. */
export async function getRecentWorkdirs(): Promise<RecentWorkdir[]> {
  try {
    const res = await fetch(`${BASE_URL}/api/workdirs`);
    if (res.ok) {
      const data = await res.json();
      if (Array.isArray(data)) return data as RecentWorkdir[];
    }
  } catch {
    // Recents are cosmetic — the launcher stays usable without them
  }
  return [];
}

/** Subfolder listing for the launcher's folder browser. Throws on bad paths. */
export async function browseDirs(path?: string): Promise<DirListing> {
  const url = path
    ? `${BASE_URL}/api/fs/dirs?path=${encodeURIComponent(path)}`
    : `${BASE_URL}/api/fs/dirs`;
  let res: Response;
  try {
    res = await fetch(url);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Folder listing failed (HTTP ${res.status})`);
  }
  if (!data || !Array.isArray(data.entries)) {
    throw new Error('Unexpected folder listing from backend');
  }
  return data as DirListing;
}

/** Folders and files of `path` for the sidebar file tree (dirs sort first). */
export async function listDir(path: string): Promise<DirListing> {
  const url = `${BASE_URL}/api/fs/list?path=${encodeURIComponent(path)}`;
  let res: Response;
  try {
    res = await fetch(url);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Folder listing failed (HTTP ${res.status})`);
  }
  if (!data || !Array.isArray(data.entries)) {
    throw new Error('Unexpected folder listing from backend');
  }
  return data as DirListing;
}

/** Opens a native OS folder-picker window (Explorer/Finder) via the backend.
 *  Resolves the selected path, or null when the user cancels. */
export async function pickFolderNative(title?: string): Promise<string | null> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/fs/pick-folder`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: title ?? null }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Folder picker failed (HTTP ${res.status})`);
  }
  return typeof data?.path === 'string' && data.path ? data.path : null;
}

/** Shape returned by GET /api/fs/file (file_editor::FileContent). */
export interface FileContent {
  path: string;
  content: string;
  size: number;
  modified_ms: number;
}

/** Reads a text file for the editor. Throws on binary/oversized/missing. */
export async function readFile(path: string): Promise<FileContent> {
  const url = `${BASE_URL}/api/fs/file?path=${encodeURIComponent(path)}`;
  let res: Response;
  try {
    res = await fetch(url);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Read failed (HTTP ${res.status})`);
  }
  if (typeof data?.content !== 'string') {
    throw new Error('Unexpected file content from backend');
  }
  return data as FileContent;
}

export interface CreatedEntry {
  path: string;
  kind: 'file' | 'dir';
}

/** Creates a file or folder under `root`. `relPath` may nest (`src/lib/util.ts`). */
export async function createFsEntry(
  root: string,
  relPath: string,
  kind: 'file' | 'dir'
): Promise<CreatedEntry> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/fs/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ root, rel_path: relPath, kind }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Create failed (HTTP ${res.status})`);
  }
  if (typeof data?.path !== 'string' || (data.kind !== 'file' && data.kind !== 'dir')) {
    throw new Error('Unexpected create result from backend');
  }
  return { path: data.path, kind: data.kind };
}

/** Deletes a file or folder that is inside `root`. */
export async function deleteFsEntry(root: string, path: string): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/fs/delete`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ root, path }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Delete failed (HTTP ${res.status})`);
  }
}

/** Writes editor content back. Throws on conflict (file changed on disk). */
export async function writeFile(
  path: string,
  content: string,
  expectedModifiedMs: number | null
): Promise<number> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/fs/file`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        path,
        content,
        expected_modified_ms: expectedModifiedMs,
      }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Save failed (HTTP ${res.status})`);
  }
  return typeof data?.modified_ms === 'number' ? data.modified_ms : Date.now();
}

export async function getWorkspacePath(): Promise<string> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/workspace`);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Workspace lookup failed (HTTP ${res.status})`);
  }
  const path = typeof data?.path === 'string' ? data.path.trim() : '';
  if (!path) throw new Error('Workspace path missing from backend');
  return path;
}

export interface StorageInfo {
  root: string;
  sessions: string;
  scrollback: string;
  worktrees: string;
  recents: string;
}

export async function getStorageInfo(): Promise<StorageInfo> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/storage`);
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Storage lookup failed (HTTP ${res.status})`);
  }
  return {
    root: String(data?.root ?? ''),
    sessions: String(data?.sessions ?? ''),
    scrollback: String(data?.scrollback ?? ''),
    worktrees: String(data?.worktrees ?? ''),
    recents: String(data?.recents ?? ''),
  };
}

export async function openFolder(path: string): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/fs/open`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Could not open folder (HTTP ${res.status})`);
  }
}

export async function openStorageFolder(): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/storage/open`, { method: 'POST' });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Could not open storage folder (HTTP ${res.status})`);
  }
}

export async function getSessions(): Promise<Session[]> {
  // Throws on failure so callers can keep the last known state — a transient
  // proxy error must never wipe the session grid and remount every terminal.
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions`);
  } catch (err) {
    throw backendOfflineError(err);
  }
  if (!res.ok) {
    throw new Error(`Backend responded HTTP ${res.status}`);
  }
  const data = await res.json();
  return Array.isArray(data) ? (data as BackendSessionInfo[]).map(mapSession) : [];
}

export async function launchPreset(req: LaunchPresetRequest): Promise<Session[]> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/launch`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        preset: req.preset,
        engine: req.agentId,
        engines: req.agentIds && req.agentIds.length > 0 ? req.agentIds : [req.agentId],
        base_dir: req.workingDir?.trim() || '',
        task: req.task ?? null,
        count: req.count,
      }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }

  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Launch failed (HTTP ${res.status})`);
  }
  if (!data || !Array.isArray(data.sessions)) {
    throw new Error('Unexpected response from backend');
  }
  return (data.sessions as BackendSessionInfo[]).map(mapSession);
}

export async function addToGroup(groupId: string, engine: string): Promise<Session> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/add`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ group_id: groupId, engine }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Add pane failed (HTTP ${res.status})`);
  }
  if (!data?.session) {
    throw new Error('Unexpected add-pane result from backend');
  }
  return mapSession(data.session as BackendSessionInfo);
}

export async function killSession(sessionId: string): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/${sessionId}/kill`, {
      method: 'POST',
    });
  } catch (err) {
    throw backendOfflineError(err);
  }
  if (!res.ok) {
    const data = await readJson(res);
    throw new Error(data?.error ?? `Kill failed (HTTP ${res.status})`);
  }
}

export async function restartSession(sessionId: string): Promise<Session> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/${sessionId}/restart`, {
      method: 'POST',
    });
  } catch (err) {
    throw backendOfflineError(err);
  }

  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Restart failed (HTTP ${res.status})`);
  }
  if (!data?.session) {
    throw new Error('Unexpected response from backend');
  }
  return mapSession(data.session as BackendSessionInfo);
}

export async function renameSessionGroup(groupId: string, label: string): Promise<string> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/rename`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ group_id: groupId, label }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }

  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Rename failed (HTTP ${res.status})`);
  }
  return data?.label ?? label;
}

export async function broadcastMessage(message: string, sessionIds?: string[]): Promise<number> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/broadcast`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ input: message, session_ids: sessionIds ?? null }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }

  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Broadcast failed (HTTP ${res.status})`);
  }
  return data?.delivered_count ?? 0;
}

export async function getDiff(dir?: string): Promise<string> {
  try {
    const url = dir
      ? `${BASE_URL}/api/worktrees/diff?dir=${encodeURIComponent(dir)}`
      : `${BASE_URL}/api/worktrees/diff`;
    const res = await fetch(url);
    if (res.ok) {
      const data = await res.json();
      return data?.diff ?? '';
    }
  } catch {
    // Fall through to empty diff
  }
  return '';
}

export async function handoffReview(sourceSessionId: string, targetSessionId: string): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${BASE_URL}/api/sessions/handoff`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ source_session_id: sourceSessionId, target_session_id: targetSessionId }),
    });
  } catch (err) {
    throw backendOfflineError(err);
  }

  const data = await readJson(res);
  if (!res.ok) {
    throw new Error(data?.error ?? `Handoff failed (HTTP ${res.status})`);
  }
}

export function createSessionWs(sessionId: string): WebSocket {
  const isSecure = window.location.protocol === 'https:';
  const host = window.location.host;
  // Vite dev server on 5173 proxies /ws to http://127.0.0.1:3001 (see vite.config.ts)
  const wsUrl = `${isSecure ? 'wss:' : 'ws:'}//${host}/ws/${sessionId}`;
  return new WebSocket(wsUrl);
}
