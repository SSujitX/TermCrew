//! Discover and manage Agent Skills (`SKILL.md`) across harness roots.
//!
//! Hybrid model: native scan / prefs / copy / delete / local+git install;
//! marketplace browse via skills.sh; marketplace install via `npx skills`
//! in a hidden setup PTY (same pattern as agent install).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::info;

use crate::worktree;

const PREFS_FILE: &str = "skills-prefs.json";
const MAX_PREVIEW_BYTES: usize = 256 * 1024;
const SCAN_CACHE_TTL: Duration = Duration::from_secs(8);
const MARKET_CACHE_TTL: Duration = Duration::from_secs(600);
const MARKET_CACHE_MAX: usize = 24;
const BROWSE_QUERY: &str = "skill";
const MAX_SKILL_NAME_LEN: usize = 80;
const CONFIRM_DELETE: &str = "DELETE";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsPrefs {
    /// Absolute skill directory paths that TermCrew treats as disabled.
    #[serde(default)]
    pub disabled: Vec<String>,
}

impl Default for SkillsPrefs {
    fn default() -> Self {
        Self {
            disabled: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillRootInfo {
    pub id: String,
    pub label: String,
    pub harness: String,
    pub scope: String,
    pub path: String,
    pub exists: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub skill_md: String,
    pub harness: String,
    pub scope: String,
    pub root_id: String,
    pub readonly: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillsCatalog {
    pub roots: Vec<SkillRootInfo>,
    pub skills: Vec<DetectedSkill>,
    pub workdir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillContent {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Deserialize)]
pub struct PrefsUpdate {
    pub path: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CopyRequest {
    pub source_path: String,
    pub target_root_id: String,
    /// Optional override for destination folder name.
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub path: String,
    /// Must be exactly `DELETE`.
    pub confirm: String,
}

#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    /// `local` | `git`
    pub source: String,
    pub path_or_url: String,
    pub target_root_id: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MarketplaceInstallRequest {
    /// skills.sh `source` field, e.g. `owner/repo`
    pub source: String,
    pub skill: Option<String>,
    /// TermCrew harness id: `global` | `claude` | `codex` | `cursor` | `opencode` | …
    pub harness: String,
    pub global: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceSkill {
    pub id: String,
    pub skill_id: String,
    pub name: String,
    pub source: String,
    pub installs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceSearch {
    pub query: String,
    pub view: String,
    pub page: u32,
    pub per_page: u32,
    pub total: usize,
    pub has_more: bool,
    pub skills: Vec<MarketplaceSkill>,
}

#[derive(Debug, Clone)]
struct RootDef {
    id: &'static str,
    label: &'static str,
    harness: &'static str,
    scope: &'static str,
    writable: bool,
    path: PathBuf,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn prefs_path() -> PathBuf {
    worktree::app_data_dir().join(PREFS_FILE)
}

fn normalize_path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

pub fn load_prefs() -> SkillsPrefs {
    let path = prefs_path();
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => SkillsPrefs::default(),
    }
}

fn save_prefs(prefs: &SkillsPrefs) -> Result<(), String> {
    let root = worktree::app_data_dir();
    fs::create_dir_all(&root).map_err(|e| format!("Cannot create app data dir: {e}"))?;
    let path = prefs_path();
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(prefs).map_err(|e| format!("prefs encode: {e}"))?;
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("Cannot write prefs: {e}"))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("Cannot write prefs: {e}"))?;
        f.flush().map_err(|e| format!("Cannot flush prefs: {e}"))?;
    }
    fs::rename(&tmp, &path).map_err(|e| format!("Cannot finalize prefs: {e}"))?;
    Ok(())
}

fn is_reserved_segment(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "skills-cursor" || lower == ".system" || lower == "node_modules" || lower == ".git"
}

fn is_safe_skill_name(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_SKILL_NAME_LEN {
        return false;
    }
    if name == "." || name == ".." {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !is_reserved_segment(name)
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("Cannot resolve {}: {e}", path.display()))
}

fn path_contained(child: &Path, parent: &Path) -> bool {
    let Ok(c) = child.canonicalize() else {
        return false;
    };
    let Ok(p) = parent.canonicalize() else {
        return false;
    };
    c.starts_with(&p)
}

fn reject_traversal(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err("Path is empty".into());
    }
    for c in path.components() {
        if matches!(c, Component::ParentDir) {
            return Err("Path must not contain '..'".into());
        }
    }
    Ok(path)
}

fn parse_skill_md(content: &str) -> (String, String) {
    let mut name = String::new();
    let mut description = String::new();
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (name, description);
    }
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (name, description);
    };
    let Some(end) = rest.find("\n---") else {
        return (name, description);
    };
    let front = &rest[..end];
    for line in front.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = v.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }
    (name, description)
}

fn project_root_from_workdir(workdir: Option<&Path>) -> Option<PathBuf> {
    let start = workdir?;
    if !start.exists() {
        return None;
    }
    let mut cur = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if cur.join(".git").exists() {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    Some(start.canonicalize().unwrap_or_else(|_| start.to_path_buf()))
}

fn build_roots(workdir: Option<&Path>) -> Vec<RootDef> {
    let mut roots = Vec::new();
    let home = home_dir();

    if let Some(ref h) = home {
        roots.push(RootDef {
            id: "global",
            label: "Global (agents)",
            harness: "global",
            scope: "user",
            writable: true,
            path: h.join(".agents").join("skills"),
        });
        roots.push(RootDef {
            id: "agents-universal",
            label: "Global (universal)",
            harness: "global",
            scope: "user",
            writable: true,
            path: h.join(".config").join("agents").join("skills"),
        });
        roots.push(RootDef {
            id: "claude-user",
            label: "Claude · user",
            harness: "claude",
            scope: "user",
            writable: true,
            path: h.join(".claude").join("skills"),
        });
        roots.push(RootDef {
            id: "codex-user",
            label: "Codex · user",
            harness: "codex",
            scope: "user",
            writable: true,
            path: h.join(".codex").join("skills"),
        });
        roots.push(RootDef {
            id: "cursor-user",
            label: "Cursor · user",
            harness: "cursor",
            scope: "user",
            writable: true,
            path: h.join(".cursor").join("skills"),
        });
        roots.push(RootDef {
            id: "opencode-user",
            label: "OpenCode · user",
            harness: "opencode",
            scope: "user",
            writable: true,
            path: h.join(".config").join("opencode").join("skills"),
        });
        roots.push(RootDef {
            id: "openclaw-user",
            label: "OpenClaw · user",
            harness: "openclaw",
            scope: "user",
            writable: true,
            path: h.join(".openclaw").join("skills"),
        });
        roots.push(RootDef {
            id: "hermes-user",
            label: "Hermes · user",
            harness: "hermes",
            scope: "user",
            writable: true,
            path: h.join(".hermes").join("skills"),
        });
        roots.push(RootDef {
            id: "gemini-user",
            label: "Gemini · user",
            harness: "gemini",
            scope: "user",
            writable: true,
            path: h.join(".gemini").join("skills"),
        });
        roots.push(RootDef {
            id: "kiro-user",
            label: "Kiro · user",
            harness: "kiro",
            scope: "user",
            writable: true,
            path: h.join(".kiro").join("skills"),
        });
        roots.push(RootDef {
            id: "goose-user",
            label: "Goose · user",
            harness: "goose",
            scope: "user",
            writable: true,
            path: h.join(".config").join("goose").join("skills"),
        });
        // Read-only system caches under Codex / agents
        roots.push(RootDef {
            id: "codex-system",
            label: "Codex · system",
            harness: "codex",
            scope: "system",
            writable: false,
            path: h.join(".codex").join("skills").join(".system"),
        });
        roots.push(RootDef {
            id: "agents-system",
            label: "Global · system",
            harness: "global",
            scope: "system",
            writable: false,
            path: h.join(".agents").join("skills").join(".system"),
        });
    }

    if let Some(proj) = project_root_from_workdir(workdir) {
        roots.push(RootDef {
            id: "claude-project",
            label: "Claude · project",
            harness: "claude",
            scope: "project",
            writable: true,
            path: proj.join(".claude").join("skills"),
        });
        roots.push(RootDef {
            id: "agents-project",
            label: "Agents · project",
            harness: "global",
            scope: "project",
            writable: true,
            path: proj.join(".agents").join("skills"),
        });
        roots.push(RootDef {
            id: "cursor-project",
            label: "Cursor · project",
            harness: "cursor",
            scope: "project",
            writable: true,
            path: proj.join(".cursor").join("skills"),
        });
        roots.push(RootDef {
            id: "codex-project",
            label: "Codex · project",
            harness: "codex",
            scope: "project",
            writable: true,
            path: proj.join(".codex").join("skills"),
        });
    }

    roots
}

fn root_info(def: &RootDef) -> SkillRootInfo {
    SkillRootInfo {
        id: def.id.to_string(),
        label: def.label.to_string(),
        harness: def.harness.to_string(),
        scope: def.scope.to_string(),
        path: def.path.to_string_lossy().to_string(),
        exists: def.path.is_dir(),
        writable: def.writable,
    }
}

fn find_root_by_id<'a>(roots: &'a [RootDef], id: &str) -> Result<&'a RootDef, String> {
    roots
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Unknown skill root '{id}'"))
}

fn skill_is_readonly(skill_dir: &Path, root: &RootDef) -> bool {
    if !root.writable {
        return true;
    }
    for c in skill_dir.components() {
        if let Component::Normal(os) = c {
            if is_reserved_segment(&os.to_string_lossy()) {
                return true;
            }
        }
    }
    false
}

fn scan_root(root: &RootDef, disabled: &HashSet<String>) -> Vec<DetectedSkill> {
    let mut out = Vec::new();
    if !root.path.is_dir() {
        return out;
    }
    let Ok(entries) = fs::read_dir(&root.path) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().to_string();
        if is_reserved_segment(&folder) && root.scope != "system" {
            // Skip reserved dirs in writable roots; system roots are the reserved tree itself.
            if folder.eq_ignore_ascii_case(".system") || folder.eq_ignore_ascii_case("skills-cursor")
            {
                continue;
            }
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            // One level of nesting (e.g. catalogs)
            if let Ok(nested) = fs::read_dir(&path) {
                for nest in nested.flatten() {
                    let np = nest.path();
                    if !np.is_dir() {
                        continue;
                    }
                    let nname = nest.file_name().to_string_lossy().to_string();
                    if is_reserved_segment(&nname) {
                        continue;
                    }
                    let nmd = np.join("SKILL.md");
                    if nmd.is_file() {
                        out.push(make_skill(&np, &nmd, root, disabled));
                    }
                }
            }
            continue;
        }
        out.push(make_skill(&path, &skill_md, root, disabled));
    }
    out
}

fn make_skill(
    skill_dir: &Path,
    skill_md: &Path,
    root: &RootDef,
    disabled: &HashSet<String>,
) -> DetectedSkill {
    let raw = fs::read_to_string(skill_md).unwrap_or_default();
    let (fm_name, fm_desc) = parse_skill_md(&raw);
    let folder = skill_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".into());
    let name = if fm_name.is_empty() {
        folder.clone()
    } else {
        fm_name
    };
    let description = if fm_desc.is_empty() {
        "Agent skill".to_string()
    } else {
        fm_desc
    };
    let path_str = skill_dir.to_string_lossy().to_string();
    let key = normalize_path_key(skill_dir);
    let readonly = skill_is_readonly(skill_dir, root);
    DetectedSkill {
        id: path_str.clone(),
        name,
        description,
        path: path_str,
        skill_md: skill_md.to_string_lossy().to_string(),
        harness: root.harness.to_string(),
        scope: root.scope.to_string(),
        root_id: root.id.to_string(),
        readonly,
        enabled: !disabled.contains(&key),
    }
}

struct ScanCache {
    at: Instant,
    workdir_key: String,
    catalog: SkillsCatalog,
}

fn scan_cache() -> &'static Mutex<Option<ScanCache>> {
    static CACHE: OnceLock<Mutex<Option<ScanCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn list_skills(workdir: Option<&str>, fresh: bool) -> SkillsCatalog {
    let wd_key = workdir.unwrap_or("").to_string();
    if !fresh {
        if let Ok(guard) = scan_cache().lock() {
            if let Some(cached) = guard.as_ref() {
                if cached.workdir_key == wd_key && cached.at.elapsed() < SCAN_CACHE_TTL {
                    return cached.catalog.clone();
                }
            }
        }
    }

    let wd_path = workdir.map(PathBuf::from);
    let roots = build_roots(wd_path.as_deref());
    let prefs = load_prefs();
    let disabled: HashSet<String> = prefs
        .disabled
        .iter()
        .map(|p| normalize_path_key(Path::new(p)))
        .collect();

    let mut skills = Vec::new();
    let mut seen = HashSet::new();
    for root in &roots {
        for skill in scan_root(root, &disabled) {
            let key = normalize_path_key(Path::new(&skill.path));
            if seen.insert(key) {
                skills.push(skill);
            }
        }
    }
    skills.sort_by(|a, b| {
        a.harness
            .cmp(&b.harness)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let catalog = SkillsCatalog {
        roots: roots.iter().map(root_info).collect(),
        skills,
        workdir: workdir.map(|s| s.to_string()),
    };

    if let Ok(mut guard) = scan_cache().lock() {
        *guard = Some(ScanCache {
            at: Instant::now(),
            workdir_key: wd_key,
            catalog: catalog.clone(),
        });
    }
    catalog
}

fn invalidate_cache() {
    if let Ok(mut guard) = scan_cache().lock() {
        *guard = None;
    }
}

pub fn set_skill_enabled(path: &str, enabled: bool) -> Result<SkillsPrefs, String> {
    let p = reject_traversal(path)?;
    if !p.join("SKILL.md").is_file() && !p.is_dir() {
        return Err("Not a skill directory".into());
    }
    let key = normalize_path_key(&p);
    let mut prefs = load_prefs();
    prefs.disabled.retain(|d| normalize_path_key(Path::new(d)) != key);
    if !enabled {
        prefs.disabled.push(p.to_string_lossy().to_string());
    }
    save_prefs(&prefs)?;
    invalidate_cache();
    Ok(prefs)
}

fn ensure_under_writable_root(skill_dir: &Path, roots: &[RootDef]) -> Result<RootDef, String> {
    let canon = canonicalize_existing(skill_dir)?;
    for root in roots {
        if !root.writable {
            continue;
        }
        let Ok(rp) = root.path.canonicalize() else {
            // Root may not exist yet
            continue;
        };
        if canon.starts_with(&rp) {
            if skill_is_readonly(&canon, root) {
                return Err("This skill is read-only".into());
            }
            return Ok(RootDef {
                id: root.id,
                label: root.label,
                harness: root.harness,
                scope: root.scope,
                writable: root.writable,
                path: root.path.clone(),
            });
        }
    }
    Err("Skill is outside managed roots".into())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Cannot create {}: {e}", dst.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("Cannot read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("Read dir error: {e}"))?;
        let from = entry.path();
        let meta = fs::symlink_metadata(&from).map_err(|e| format!("stat: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!("Refusing to copy symlink: {}", from.display()));
        }
        let to = dst.join(entry.file_name());
        if meta.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if meta.is_file() {
            fs::copy(&from, &to).map_err(|e| format!("Copy failed: {e}"))?;
        }
    }
    Ok(())
}

pub fn copy_skill(req: &CopyRequest, workdir: Option<&str>) -> Result<DetectedSkill, String> {
    let src = reject_traversal(&req.source_path)?;
    let skill_md = src.join("SKILL.md");
    if !skill_md.is_file() {
        return Err("Source must be a skill directory with SKILL.md".into());
    }
    let roots = build_roots(workdir.map(PathBuf::from).as_deref());
    let target = find_root_by_id(&roots, &req.target_root_id)?;
    if !target.writable {
        return Err("Target root is read-only".into());
    }
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            src.file_name()
                .map(|s| s.to_string_lossy().to_string())
        })
        .ok_or_else(|| "Missing skill name".to_string())?;
    if !is_safe_skill_name(&name) {
        return Err("Invalid skill name".into());
    }
    fs::create_dir_all(&target.path)
        .map_err(|e| format!("Cannot create target root: {e}"))?;
    let dest = target.path.join(&name);
    if dest.exists() {
        return Err(format!("Destination already exists: {}", dest.display()));
    }
    // Prevent copying a tree into itself
    if path_contained(&dest, &src) || src == dest {
        return Err("Invalid copy destination".into());
    }
    copy_dir_recursive(&src, &dest)?;
    invalidate_cache();
    let disabled: HashSet<String> = load_prefs()
        .disabled
        .iter()
        .map(|p| normalize_path_key(Path::new(p)))
        .collect();
    Ok(make_skill(&dest, &dest.join("SKILL.md"), target, &disabled))
}

pub fn delete_skill(req: &DeleteRequest, workdir: Option<&str>) -> Result<(), String> {
    if req.confirm != CONFIRM_DELETE {
        return Err("Confirm must be DELETE".into());
    }
    let src = reject_traversal(&req.path)?;
    if !src.join("SKILL.md").is_file() {
        return Err("Not a skill directory".into());
    }
    let roots = build_roots(workdir.map(PathBuf::from).as_deref());
    let _root = ensure_under_writable_root(&src, &roots)?;
    fs::remove_dir_all(&src).map_err(|e| format!("Delete failed: {e}"))?;
    // Drop prefs entry if present
    let key = normalize_path_key(&src);
    let mut prefs = load_prefs();
    prefs.disabled.retain(|d| normalize_path_key(Path::new(d)) != key);
    let _ = save_prefs(&prefs);
    invalidate_cache();
    info!(path = %src.display(), "Deleted skill");
    Ok(())
}

fn find_skill_dirs(root: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_reserved_segment(&name) {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            out.push(path);
        } else {
            find_skill_dirs(&path, out, depth + 1);
        }
    }
}

pub fn install_skill(req: &InstallRequest, workdir: Option<&str>) -> Result<Vec<DetectedSkill>, String> {
    let roots = build_roots(workdir.map(PathBuf::from).as_deref());
    let target = find_root_by_id(&roots, &req.target_root_id)?;
    if !target.writable {
        return Err("Target root is read-only".into());
    }
    fs::create_dir_all(&target.path)
        .map_err(|e| format!("Cannot create target root: {e}"))?;

    let source = req.source.trim().to_ascii_lowercase();
    let installed = match source.as_str() {
        "local" => {
            let src = reject_traversal(&req.path_or_url)?;
            let mut dirs = Vec::new();
            if src.join("SKILL.md").is_file() {
                dirs.push(src);
            } else if src.is_dir() {
                find_skill_dirs(&src, &mut dirs, 0);
            } else {
                return Err("Local path must be a skill dir or folder of skills".into());
            }
            if dirs.is_empty() {
                return Err("No SKILL.md found under local path".into());
            }
            install_dirs_into(&dirs, target, req.name.as_deref())?
        }
        "git" => {
            let url = req.path_or_url.trim();
            if url.is_empty()
                || !(url.starts_with("https://")
                    || url.starts_with("git@")
                    || url.starts_with("ssh://"))
            {
                return Err("Git URL must be https:// or git@ / ssh://".into());
            }
            // Basic injection guard
            if url.chars().any(|c| c.is_whitespace() || c == ';' || c == '|' || c == '&') {
                return Err("Invalid git URL".into());
            }
            let tmp = worktree::app_data_dir()
                .join("skill-install")
                .join(uuid::Uuid::new_v4().to_string());
            fs::create_dir_all(&tmp).map_err(|e| format!("temp dir: {e}"))?;
            let hooks_null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            let hooks_flag = format!("core.hooksPath={hooks_null}");
            let clone = Command::new("git")
                .args([
                    "-c",
                    hooks_flag.as_str(),
                    "-c",
                    "fetch.fsckObjects=true",
                    "clone",
                    "--depth",
                    "1",
                    "--",
                    url,
                ])
                .arg(&tmp)
                .env("GIT_TERMINAL_PROMPT", "0")
                .output()
                .map_err(|e| format!("git clone failed to start: {e}"))?;
            if !clone.status.success() {
                let _ = fs::remove_dir_all(&tmp);
                let err = String::from_utf8_lossy(&clone.stderr);
                return Err(format!("git clone failed: {err}"));
            }
            let mut dirs = Vec::new();
            find_skill_dirs(&tmp, &mut dirs, 0);
            let result = if dirs.is_empty() {
                Err("No SKILL.md found in cloned repository".into())
            } else {
                install_dirs_into(&dirs, target, req.name.as_deref())
            };
            let _ = fs::remove_dir_all(&tmp);
            result?
        }
        _ => return Err("source must be local or git".into()),
    };
    invalidate_cache();
    Ok(installed)
}

fn validate_git_url(url: &str) -> Result<&str, String> {
    let url = url.trim();
    if url.is_empty()
        || !(url.starts_with("https://") || url.starts_with("git@") || url.starts_with("ssh://"))
    {
        return Err("Git URL must be https:// or git@ / ssh://".into());
    }
    if url.chars().any(|c| c.is_whitespace() || c == ';' || c == '|' || c == '&') {
        return Err("Invalid git URL".into());
    }
    Ok(url)
}

pub fn git_install_display(url: &str) -> String {
    format!("git clone --depth 1 {url}")
}

fn ps_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Visible setup-console command: clone + copy SKILL.md dirs into a writable root.
pub fn git_install_command(req: &InstallRequest, workdir: Option<&str>) -> Result<String, String> {
    if !req.source.trim().eq_ignore_ascii_case("git") {
        return Err("source must be git".into());
    }
    let url = validate_git_url(&req.path_or_url)?;
    let roots = build_roots(workdir.map(PathBuf::from).as_deref());
    let target = find_root_by_id(&roots, &req.target_root_id)?;
    if !target.writable {
        return Err("Target root is read-only".into());
    }
    let dest = target.path.to_string_lossy().into_owned();
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if !is_safe_skill_name(s) {
                Err(format!("Invalid skill name '{s}'"))
            } else {
                Ok(s.to_string())
            }
        })
        .transpose()?;

    if cfg!(target_os = "windows") {
        let dest_q = ps_single_quote(&dest);
        let url_q = ps_single_quote(url);
        let name_line = match name.as_deref() {
            Some(n) => format!(
                "$forced = '{}'; ",
                ps_single_quote(n)
            ),
            None => "$forced = $null; ".to_string(),
        };
        Ok(format!(
            "$ProgressPreference='SilentlyContinue'; \
             $ok=$true; \
             $tmp=Join-Path $env:TEMP ('termcrew-skill-'+[guid]::NewGuid().ToString()); \
             Write-Host 'Cloning repository…'; \
             git -c core.hooksPath=NUL -c fetch.fsckObjects=true clone --depth 1 -- '{url_q}' $tmp; \
             if ($LASTEXITCODE -ne 0) {{ Write-Host 'git clone failed.'; $ok=$false }} \
             else {{ \
               $dest='{dest_q}'; \
               New-Item -ItemType Directory -Force -Path $dest | Out-Null; \
               {name_line}\
               $dirs=@(); \
               if (Test-Path -LiteralPath (Join-Path $tmp 'SKILL.md')) {{ $dirs=@($tmp) }} \
               else {{ \
                 Get-ChildItem -LiteralPath $tmp -Recurse -Filter SKILL.md -ErrorAction SilentlyContinue | ForEach-Object {{ \
                   $p=$_.Directory.FullName; $leaf=Split-Path $p -Leaf; \
                   if ($leaf -notin @('.git','node_modules','.system','skills-cursor')) {{ $dirs += $p }} \
                 }} \
               }}; \
               if ($dirs.Count -eq 0) {{ Write-Host 'No SKILL.md found in cloned repository.'; $ok=$false }} \
               else {{ \
                 $i=0; \
                 foreach ($d in $dirs) {{ \
                   $i++; \
                   $n = if ($forced) {{ if ($dirs.Count -gt 1 -and $i -gt 1) {{ $forced+'-'+$i }} else {{ $forced }} }} else {{ Split-Path $d -Leaf }}; \
                   $out=Join-Path $dest $n; \
                   if (Test-Path -LiteralPath $out) {{ Write-Host ('Already installed: '+$n); continue }}; \
                   Copy-Item -LiteralPath $d -Destination $out -Recurse -Force; \
                   Write-Host ('Installed '+$n) \
                 }} \
               }} \
             }}; \
             if (Test-Path -LiteralPath $tmp) {{ Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }}; \
             if ($ok) {{ Write-Host 'Installed successfully.'; $global:LASTEXITCODE=0 }} else {{ $global:LASTEXITCODE=1 }}",
        ))
    } else {
        let dest_q = sh_single_quote(&dest);
        let url_q = sh_single_quote(url);
        let name_q = name
            .as_deref()
            .map(sh_single_quote)
            .unwrap_or_else(|| "''".to_string());
        Ok(format!(
            "ok=0; work=$(mktemp -d); tmp=\"$work/src\"; list=\"$work/list\"; \
             printf 'Cloning repository…\\n'; \
             if ! command -v git >/dev/null 2>&1; then printf 'git was not found. Install Git and retry.\\n'; ok=1; \
             else git -c core.hooksPath=/dev/null -c fetch.fsckObjects=true clone --depth 1 -- {url_q} \"$tmp\" || ok=1; fi; \
             dest={dest_q}; mkdir -p \"$dest\"; \
             forced={name_q}; : > \"$list\"; \
             if [ \"$ok\" -eq 0 ]; then \
               if [ -f \"$tmp/SKILL.md\" ]; then printf '%s\\n' \"$tmp\" > \"$list\"; \
               else find \"$tmp\" -name SKILL.md -type f 2>/dev/null | while IFS= read -r f; do \
                 d=$(dirname \"$f\"); leaf=$(basename \"$d\"); \
                 case \"$leaf\" in .git|node_modules|.system|skills-cursor) continue ;; esac; \
                 printf '%s\\n' \"$d\" >> \"$list\"; \
               done; fi; \
             fi; \
             if [ \"$ok\" -eq 0 ] && [ ! -s \"$list\" ]; then printf 'No SKILL.md found in cloned repository.\\n'; ok=1; fi; \
             i=0; \
             while IFS= read -r d; do \
               [ -z \"$d\" ] && continue; i=$((i+1)); \
               if [ -n \"$forced\" ]; then \
                 if [ \"$i\" -gt 1 ]; then n=\"$forced-$i\"; else n=\"$forced\"; fi; \
               else n=$(basename \"$d\"); fi; \
               out=\"$dest/$n\"; \
               if [ -e \"$out\" ]; then printf 'Already installed: %s\\n' \"$n\"; continue; fi; \
               cp -R \"$d\" \"$out\" && printf 'Installed %s\\n' \"$n\"; \
             done < \"$list\"; \
             rm -rf \"$work\"; \
             if [ \"$ok\" -eq 0 ]; then printf 'Installed successfully.\\n'; true; else false; fi",
        ))
    }
}

fn install_dirs_into(
    dirs: &[PathBuf],
    target: &RootDef,
    forced_name: Option<&str>,
) -> Result<Vec<DetectedSkill>, String> {
    let disabled: HashSet<String> = load_prefs()
        .disabled
        .iter()
        .map(|p| normalize_path_key(Path::new(p)))
        .collect();

    // Preflight destination names so we don't leave a partial install.
    let mut planned: Vec<(PathBuf, String)> = Vec::new();
    for (i, src) in dirs.iter().enumerate() {
        let name = if let Some(n) = forced_name.filter(|s| !s.is_empty()) {
            if dirs.len() > 1 && i > 0 {
                format!("{n}-{}", i + 1)
            } else {
                n.to_string()
            }
        } else {
            src.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("skill-{}", i + 1))
        };
        if !is_safe_skill_name(&name) {
            return Err(format!("Invalid skill name '{name}'"));
        }
        let dest = target.path.join(&name);
        if dest.exists() {
            return Err(format!("Already exists: {}", dest.display()));
        }
        planned.push((src.clone(), name));
    }

    let mut out = Vec::new();
    let mut written: Vec<PathBuf> = Vec::new();
    for (src, name) in planned {
        let dest = target.path.join(&name);
        if let Err(e) = copy_dir_recursive(&src, &dest) {
            for w in &written {
                let _ = fs::remove_dir_all(w);
            }
            return Err(e);
        }
        written.push(dest.clone());
        out.push(make_skill(&dest, &dest.join("SKILL.md"), target, &disabled));
    }
    Ok(out)
}

pub fn read_skill_content(path: &str, workdir: Option<&str>) -> Result<SkillContent, String> {
    let p = reject_traversal(path)?;
    let md = if p.is_file() {
        p.clone()
    } else {
        p.join("SKILL.md")
    };
    let file_name = md
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if file_name != "skill.md" {
        return Err("Only SKILL.md can be previewed".into());
    }
    if !md.is_file() {
        return Err("SKILL.md not found".into());
    }
    if md.components().any(|c| {
        matches!(c, Component::Normal(os) if os.to_string_lossy().eq_ignore_ascii_case("skills-cursor"))
    }) {
        return Err("Reserved Cursor skills path".into());
    }

    let parent = md.parent().ok_or_else(|| "Invalid skill path".to_string())?;
    ensure_under_any_root(parent, workdir)?;

    let bytes = fs::read(&md).map_err(|e| format!("Cannot read: {e}"))?;
    let truncated = bytes.len() > MAX_PREVIEW_BYTES;
    let slice = if truncated {
        &bytes[..MAX_PREVIEW_BYTES]
    } else {
        &bytes
    };
    Ok(SkillContent {
        path: md.to_string_lossy().to_string(),
        content: String::from_utf8_lossy(slice).to_string(),
        truncated,
    })
}

fn ensure_under_any_root(path: &Path, workdir: Option<&str>) -> Result<(), String> {
    let roots = build_roots(workdir.map(PathBuf::from).as_deref());
    let canon = path
        .canonicalize()
        .or_else(|_| {
            // Parent may exist even if we just created siblings
            path.parent()
                .and_then(|p| p.canonicalize().ok())
                .map(|p| p.join(path.file_name().unwrap_or_default()))
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
        })
        .map_err(|e| format!("Cannot resolve path: {e}"))?;

    for root in &roots {
        if let Ok(rp) = root.path.canonicalize() {
            if canon.starts_with(&rp) {
                return Ok(());
            }
        } else if canon.starts_with(&root.path) {
            return Ok(());
        }
    }
    Err("Skill path is outside managed roots".into())
}

pub fn open_skill_folder(path: &str, workdir: Option<&str>) -> Result<(), String> {
    let p = reject_traversal(path)?;
    let dir = if p.is_file() {
        p.parent()
            .map(|x| x.to_path_buf())
            .ok_or_else(|| "Invalid path".to_string())?
    } else {
        p
    };
    if !dir.exists() {
        return Err("Folder does not exist".into());
    }
    ensure_under_any_root(&dir, workdir)?;
    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {e}"))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {e}"))?;
        Ok(())
    }
}

/// Map TermCrew harness id → `npx skills -a` agent slug.
pub fn skills_cli_agent(harness: &str) -> Option<&'static str> {
    match harness.trim().to_ascii_lowercase().as_str() {
        "claude" => Some("claude-code"),
        "codex" => Some("codex"),
        "cursor" | "cursor-agent" => Some("cursor"),
        "opencode" => Some("opencode"),
        "openclaw" => Some("openclaw"),
        "gemini" => Some("gemini-cli"),
        "goose" => Some("goose"),
        "kiro" => Some("kiro-cli"),
        "global" | "agents" => Some("universal"),
        _ => None,
    }
}

pub fn marketplace_install_command(req: &MarketplaceInstallRequest) -> Result<String, String> {
    let source = req.source.trim();
    if source.is_empty()
        || source.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.'))
    {
        return Err("Invalid marketplace source".into());
    }
    let mut parts = vec!["npx".to_string(), "-y".to_string(), "skills".to_string(), "add".to_string(), source.to_string()];
    parts.push("-y".into());
    // Marketplace installs target user skill dirs for the chosen harness.
    let global = req.global.unwrap_or(true);
    if global {
        parts.push("-g".into());
    }
    if let Some(agent) = skills_cli_agent(&req.harness) {
        parts.push("-a".into());
        parts.push(agent.to_string());
    } else {
        return Err(format!(
            "Harness '{}' is not supported by skills CLI install yet — use Copy instead",
            req.harness
        ));
    }
    if let Some(skill) = req.skill.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if !is_safe_skill_name(skill) {
            return Err("Invalid skill filter".into());
        }
        parts.push("-s".into());
        parts.push(skill.to_string());
    }
    parts.push("--copy".into());
    Ok(parts.join(" "))
}

pub fn search_marketplace(
    query: &str,
    view: &str,
    page: u32,
    per_page: u32,
) -> Result<MarketplaceSearch, String> {
    let q = query.trim();
    let q = if q.is_empty() { BROWSE_QUERY } else { q };
    if q.len() > 120 {
        return Err("Query too long".into());
    }
    let view = normalize_marketplace_view(view);
    let per_page = per_page.clamp(1, 50);
    let mut skills = fetch_marketplace_catalog(q)?;
    sort_marketplace_skills(&mut skills, view);
    let (page_skills, page, has_more) = paginate_marketplace(&skills, page, per_page);
    Ok(MarketplaceSearch {
        query: q.to_string(),
        view: view.to_string(),
        page,
        per_page,
        total: skills.len(),
        has_more,
        skills: page_skills,
    })
}

struct MarketCache {
    entries: HashMap<String, (Instant, Vec<MarketplaceSkill>)>,
}

fn market_cache() -> &'static Mutex<MarketCache> {
    static CACHE: OnceLock<Mutex<MarketCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(MarketCache { entries: HashMap::new() }))
}

fn cache_get(q: &str) -> Option<Vec<MarketplaceSkill>> {
    let guard = market_cache().lock().ok()?;
    let (at, skills) = guard.entries.get(q)?;
    (at.elapsed() < MARKET_CACHE_TTL).then(|| skills.clone())
}

fn cache_put(q: String, skills: Vec<MarketplaceSkill>) {
    let Ok(mut guard) = market_cache().lock() else {
        return;
    };
    if guard.entries.len() >= MARKET_CACHE_MAX && !guard.entries.contains_key(&q) {
        let victim = guard
            .entries
            .iter()
            .filter(|(k, _)| k.as_str() != BROWSE_QUERY)
            .min_by_key(|(_, (at, _))| *at)
            .map(|(k, _)| k.clone());
        if let Some(k) = victim {
            guard.entries.remove(&k);
        }
    }
    guard.entries.insert(q, (Instant::now(), skills));
}

fn filter_catalog(skills: &[MarketplaceSkill], q: &str) -> Vec<MarketplaceSkill> {
    let needle = q.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return skills.to_vec();
    }
    skills
        .iter()
        .filter(|s| {
            s.name.to_ascii_lowercase().contains(&needle)
                || s.source.to_ascii_lowercase().contains(&needle)
                || s.skill_id.to_ascii_lowercase().contains(&needle)
                || s.id.to_ascii_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
}

/// Warm the browse catalog so the Marketplace tab is a cache hit.
pub fn prefetch_marketplace() {
    let _ = fetch_marketplace_catalog(BROWSE_QUERY);
}

fn fetch_marketplace_catalog(q: &str) -> Result<Vec<MarketplaceSkill>, String> {
    if let Some(hit) = cache_get(q) {
        return Ok(hit);
    }
    // Search the warm browse list locally — skip a second skills.sh round-trip.
    if q != BROWSE_QUERY {
        if let Some(browse) = cache_get(BROWSE_QUERY) {
            let filtered = filter_catalog(&browse, q);
            if !filtered.is_empty() {
                cache_put(q.to_string(), filtered.clone());
                return Ok(filtered);
            }
        }
    }

    let encoded: String = urlencoding_minimal(q);
    let url = format!("https://skills.sh/api/search?q={encoded}");
    let body = http_get_text(&url)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("marketplace JSON: {e}"))?;
    let mut skills = Vec::new();
    if let Some(arr) = parsed.get("skills").and_then(|v| v.as_array()) {
        for item in arr.iter().take(100) {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let skill_id = item
                .get("skillId")
                .or_else(|| item.get("skill_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(skill_id.as_str())
                .to_string();
            let source = item
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let installs = item
                .get("installs")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if id.is_empty() && source.is_empty() {
                continue;
            }
            skills.push(MarketplaceSkill {
                id: if id.is_empty() {
                    format!("{source}/{name}")
                } else {
                    id
                },
                skill_id,
                name,
                source,
                installs,
            });
        }
    }
    cache_put(q.to_string(), skills.clone());
    Ok(skills)
}

fn normalize_marketplace_view(view: &str) -> &'static str {
    match view.trim().to_ascii_lowercase().as_str() {
        "trending" => "trending",
        "all-time" | "all_time" | "alltime" => "all-time",
        _ => "hot",
    }
}

fn sort_marketplace_skills(skills: &mut [MarketplaceSkill], view: &str) {
    if view == "hot" {
        return;
    }
    skills.sort_by(|a, b| {
        b.installs
            .cmp(&a.installs)
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
    });
}

fn paginate_marketplace(
    skills: &[MarketplaceSkill],
    page: u32,
    per_page: u32,
) -> (Vec<MarketplaceSkill>, u32, bool) {
    let per = per_page.max(1) as usize;
    let max_page = if skills.is_empty() {
        0
    } else {
        ((skills.len() - 1) / per) as u32
    };
    let page = page.min(max_page);
    let start = page as usize * per;
    let end = (start + per).min(skills.len());
    let slice = skills.get(start..end).unwrap_or(&[]).to_vec();
    (slice, page, end < skills.len())
}

fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn http_get_text(url: &str) -> Result<String, String> {
    // Prefer curl (always present on macOS / Git-for-Windows); avoids new Rust deps.
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--compressed",
            "--connect-timeout",
            "4",
            "--max-time",
            "12",
            "-H",
            "Accept: application/json",
            "--",
            url,
        ])
        .output()
        .map_err(|e| format!("curl failed to start (needed for marketplace): {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("marketplace request failed: {err}"));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("marketplace response not utf-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter() {
        let (n, d) = parse_skill_md(
            "---\nname: demo\ndescription: Does things\n---\n\n# Body\n",
        );
        assert_eq!(n, "demo");
        assert_eq!(d, "Does things");
    }

    #[test]
    fn rejects_bad_names() {
        assert!(!is_safe_skill_name("../x"));
        assert!(!is_safe_skill_name("skills-cursor"));
        assert!(is_safe_skill_name("my-skill"));
    }

    #[test]
    fn git_install_command_rejects_bad_url() {
        let err = git_install_command(
            &InstallRequest {
                source: "git".into(),
                path_or_url: "not-a-url".into(),
                target_root_id: "global".into(),
                name: None,
            },
            None,
        )
        .unwrap_err();
        assert!(err.contains("https://") || err.contains("git@"), "{err}");
    }

    #[test]
    fn git_install_command_clones_into_writable_root() {
        let cmd = git_install_command(
            &InstallRequest {
                source: "git".into(),
                path_or_url: "https://github.com/example/skills.git".into(),
                target_root_id: "global".into(),
                name: None,
            },
            None,
        )
        .expect("valid git install command");
        assert!(cmd.contains("clone --depth 1"));
        assert!(cmd.contains("https://github.com/example/skills.git"));
        assert!(cmd.contains("SKILL.md"));
        assert!(cmd.contains("Already installed"));
        assert!(cmd.len() <= 4000, "setup PTY command budget, got {}", cmd.len());
        assert_eq!(
            git_install_display("https://github.com/example/skills.git"),
            "git clone --depth 1 https://github.com/example/skills.git"
        );
    }

    #[test]
    fn marketplace_command_is_safe() {
        let cmd = marketplace_install_command(&MarketplaceInstallRequest {
            source: "vercel-labs/agent-skills".into(),
            skill: Some("web-design".into()),
            harness: "claude".into(),
            global: Some(true),
        })
        .unwrap();
        assert!(cmd.contains("npx"));
        assert!(cmd.contains("skills add"));
        assert!(cmd.contains("-a claude-code"));
        assert!(cmd.contains("-s web-design"));
    }

    #[test]
    fn marketplace_global_uses_universal_agent() {
        let cmd = marketplace_install_command(&MarketplaceInstallRequest {
            source: "typesafe-ai/skills".into(),
            skill: Some("typesafe-ai".into()),
            harness: "global".into(),
            global: Some(true),
        })
        .unwrap();
        assert!(cmd.contains("-a universal"), "{cmd}");
        assert!(!cmd.contains("-a claude-code"));
        assert_eq!(skills_cli_agent("global"), Some("universal"));
        assert_eq!(skills_cli_agent("gemini"), Some("gemini-cli"));
        assert_eq!(skills_cli_agent("kiro"), Some("kiro-cli"));
    }

    fn mk_market(id: &str, installs: u64) -> MarketplaceSkill {
        MarketplaceSkill {
            id: id.into(),
            skill_id: id.into(),
            name: id.into(),
            source: "owner/repo".into(),
            installs,
        }
    }

    #[test]
    fn marketplace_hot_keeps_api_order() {
        let mut skills = vec![mk_market("b", 1), mk_market("a", 9), mk_market("c", 3)];
        sort_marketplace_skills(&mut skills, "hot");
        assert_eq!(
            skills.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["b", "a", "c"]
        );
    }

    #[test]
    fn marketplace_all_time_sorts_by_installs() {
        let mut skills = vec![mk_market("b", 1), mk_market("a", 9), mk_market("c", 3)];
        sort_marketplace_skills(&mut skills, "all-time");
        assert_eq!(
            skills.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["a", "c", "b"]
        );
    }

    #[test]
    fn marketplace_pages_nine() {
        let skills: Vec<_> = (0..20).map(|i| mk_market(&format!("s{i}"), i)).collect();
        let (page0, p, more) = paginate_marketplace(&skills, 0, 9);
        assert_eq!(p, 0);
        assert_eq!(page0.len(), 9);
        assert!(more);
        let (page2, p, more) = paginate_marketplace(&skills, 2, 9);
        assert_eq!(p, 2);
        assert_eq!(page2.len(), 2);
        assert!(!more);
        assert_eq!(page2[0].id, "s18");
    }

    #[test]
    fn marketplace_filter_matches_name_and_source() {
        let skills = vec![
            MarketplaceSkill {
                id: "a".into(),
                skill_id: "typesafe-ai".into(),
                name: "TypeSafe".into(),
                source: "typesafe-ai/skills".into(),
                installs: 1,
            },
            MarketplaceSkill {
                id: "b".into(),
                skill_id: "other".into(),
                name: "Other".into(),
                source: "acme/other".into(),
                installs: 2,
            },
        ];
        let hit = filter_catalog(&skills, "typesafe");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].skill_id, "typesafe-ai");
        assert!(filter_catalog(&skills, "zzzz").is_empty());
    }

    #[test]
    fn marketplace_view_aliases() {
        assert_eq!(normalize_marketplace_view(""), "hot");
        assert_eq!(normalize_marketplace_view("Trending"), "trending");
        assert_eq!(normalize_marketplace_view("all-time"), "all-time");
    }

    #[test]
    fn marketplace_rejects_injection() {
        assert!(
            marketplace_install_command(&MarketplaceInstallRequest {
                source: "foo; rm -rf /".into(),
                skill: None,
                harness: "codex".into(),
                global: Some(true),
            })
            .is_err()
        );
    }

    #[test]
    fn reject_parent_dir() {
        assert!(reject_traversal(r"C:\foo\..\bar").is_err());
    }
}
