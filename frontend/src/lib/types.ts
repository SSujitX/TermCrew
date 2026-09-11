export type PresetType = 'solo' | 'pair' | 'workbench' | 'swarm';

export interface AgentMeta {
  id: string;
  name: string;
  provider: string;
  binary: string;
  icon?: string;
  install_cmd?: string;
  update_cmd?: string;
  uninstall_cmd?: string;
  manage_hint?: string;
  docs_url?: string;
  is_installed: boolean;
  path?: string;
  description?: string;
  category?: 'coding' | 'general' | 'terminal';
}

export interface Session {
  id: string;
  name: string;
  engine: string;
  preset: PresetType;
  working_dir: string;
  worktree_path?: string;
  status: 'running' | 'exited' | 'paused' | 'error';
  created_at: string;
  exit_code?: number;
  role?: string;
  /** Launch group — all nodes from one New Session share this. */
  group_id: string;
  /** e.g. "Pair 1" */
  group_label: string;
  /** Launch task, when set — used in compact review packets. */
  task?: string;
}

export interface WsMessage {
  type: 'input' | 'stdout' | 'resize' | 'status';
  data?: string;
  cols?: number;
  rows?: number;
}

export interface LaunchPresetRequest {
  preset: PresetType;
  agentId: string;
  /** Selected engines in click order. Backend opens them serially for the layout. */
  agentIds?: string[];
  count: number;
  task?: string;
  workingDir?: string;
}

export interface BroadcastRequest {
  message: string;
  sessionIds?: string[];
}

export interface HandoffRequest {
  sourceSessionId: string;
  targetSessionId: string;
  summary?: string;
  includeDiff?: boolean;
}

export interface DiffResult {
  sessionId?: string;
  diff: string;
  stats?: {
    filesChanged: number;
    insertions: number;
    deletions: number;
  };
}

export interface SupervisorLog {
  id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'success' | 'action';
  message: string;
  agentId?: string;
  sessionId?: string;
}

export interface SkillRoot {
  id: string;
  label: string;
  harness: string;
  scope: string;
  path: string;
  exists: boolean;
  writable: boolean;
}

export interface DetectedSkill {
  id: string;
  name: string;
  description: string;
  path: string;
  skill_md: string;
  harness: string;
  scope: string;
  root_id: string;
  readonly: boolean;
  enabled: boolean;
}

export interface SkillsCatalog {
  roots: SkillRoot[];
  skills: DetectedSkill[];
  workdir?: string | null;
}

export interface SkillContent {
  path: string;
  content: string;
  truncated: boolean;
}

export interface MarketplaceSkill {
  id: string;
  skill_id: string;
  name: string;
  source: string;
  installs: number;
}

export interface MarketplaceSearch {
  query: string;
  view: string;
  page: number;
  per_page: number;
  total: number;
  has_more: boolean;
  skills: MarketplaceSkill[];
}
