//! Working-directory selection support:
//! - a persisted list of recently used launch folders (upserted on every launch)
//! - a directory listing used by the launcher's in-app folder browser

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[cfg(any(windows, target_os = "macos"))]
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::warn;

const MAX_RECENTS: usize = 12;

/// Only one native picker dialog may be open at a time.
#[cfg(any(windows, target_os = "macos"))]
static PICKER_BUSY: AtomicBool = AtomicBool::new(false);

/// Opens a native OS folder-picker window (Explorer / Finder) on
/// the machine running the backend. `Ok(None)` means the user cancelled.
///
/// The dialog is owned by `termcrew.exe`, not the browser, so Windows/macOS
/// would otherwise leave it flashing in the taskbar. A short raise pass
/// brings it in front of the browser.
pub async fn pick_folder(title: &str) -> Result<Option<PathBuf>, String> {
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = title;
        return Err(
            "Folder picker is not available on this OS. Type or browse the path in the launcher."
                .into(),
        );
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        if PICKER_BUSY
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("A folder dialog is already open".to_string());
        }
        raise_picker_soon(title);
        let picked = rfd::AsyncFileDialog::new()
            .set_title(title)
            .pick_folder()
            .await;
        PICKER_BUSY.store(false, Ordering::SeqCst);
        Ok(picked.map(|handle| handle.path().to_path_buf()))
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn raise_picker_soon(title: &str) {
    let title = title.to_string();
    std::thread::spawn(move || {
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(40));
            if try_raise_picker(&title) {
                break;
            }
        }
    });
}

#[cfg(windows)]
fn try_raise_picker(title: &str) -> bool {
    win_raise::raise_dialog(title)
}

#[cfg(target_os = "macos")]
fn try_raise_picker(_title: &str) -> bool {
    let pid = std::process::id();
    std::process::Command::new("osascript")
        .args([
            "-e",
            &format!(
                "tell application \"System Events\" to set frontmost of (first process whose unix id is {pid}) to true"
            ),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn try_raise_picker(_title: &str) -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentWorkdir {
    pub path: String,
    /// RFC3339, used to sort the recents list.
    pub last_used_at: String,
    pub use_count: u64,
}

#[derive(Debug, Serialize)]
pub struct DirEntryMeta {
    pub name: String,
    pub path: String,
    /// `"dir"` or `"file"` — the launcher picker only consumes dirs,
    /// the sidebar file tree consumes both.
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BrowseListing {
    pub path: String,
    /// `None` at a filesystem root (drive letter on Windows, `/` on Unix).
    pub parent: Option<String>,
    pub entries: Vec<DirEntryMeta>,
}

fn default_store() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let neu_dir = home.join(".termcrew");
    let legacy_dir = home.join(".multiagent");
    // One-shot rename so recent workdirs survive the rebrand.
    if !neu_dir.exists() && legacy_dir.is_dir() {
        match std::fs::rename(&legacy_dir, &neu_dir) {
            Ok(()) => {}
            Err(e) => warn!(
                from = %legacy_dir.display(),
                to = %neu_dir.display(),
                error = %e,
                "Failed to migrate TermCrew config dir"
            ),
        }
    }
    neu_dir.join("recent_workdirs.json")
}

/// Loads recents, pruning entries whose folder no longer exists.
/// A missing or corrupt file is treated as "no history", not an error.
fn load_recents_at(file: &Path) -> Vec<RecentWorkdir> {
    let Ok(raw) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<RecentWorkdir>>(&raw) {
        Ok(entries) => entries
            .into_iter()
            .filter(|e| Path::new(&e.path).is_dir())
            .collect(),
        Err(e) => {
            warn!(file = %file.display(), error = %e, "Corrupt recent-workdirs file; resetting");
            Vec::new()
        }
    }
}

pub fn load_recents() -> Vec<RecentWorkdir> {
    let mut entries = load_recents_at(&default_store());
    entries.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));
    entries.truncate(MAX_RECENTS);
    entries
}

/// Upserts `path` into the recents store. Never fails the launch that
/// triggered it — persistence problems are logged, not propagated.
pub fn record_workdir(path: &Path) {
    record_workdir_at(&default_store(), path);
}

pub fn record_workdir_at(file: &Path, path: &Path) -> Vec<RecentWorkdir> {
    if !path.is_dir() {
        return load_recents_at(file);
    }
    let path_str = normalized(path);
    let mut entries = load_recents_at(file);
    let now = Utc::now().to_rfc3339();

    match entries.iter_mut().find(|e| dirs_equal(&e.path, &path_str)) {
        Some(existing) => {
            existing.use_count += 1;
            existing.last_used_at = now;
        }
        None => entries.insert(
            0,
            RecentWorkdir {
                path: path_str,
                last_used_at: now,
                use_count: 1,
            },
        ),
    }

    entries.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));
    entries.truncate(MAX_RECENTS);

    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&entries) {
        Ok(json) => {
            if let Err(e) = std::fs::write(file, json) {
                warn!(file = %file.display(), error = %e, "Failed to persist recent workdirs");
            }
        }
        Err(e) => warn!(error = %e, "Failed to serialize recent workdirs"),
    }
    entries
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string()
}

fn dirs_equal(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Lists the subfolders of `path` for the launcher's folder browser. An empty path
/// returns the root: the first available drive letter on Windows, `/` on Unix.
pub fn browse_dirs(path: Option<&str>) -> Result<BrowseListing, String> {
    list_dir(path, false, true)
}

/// Lists folders *and* files of `path` for the sidebar file tree.
/// Dirs sort before files; an empty path falls back to the server cwd.
pub fn list_dir_with_files(path: Option<&str>) -> Result<BrowseListing, String> {
    list_dir(path, true, false)
}

fn list_dir(path: Option<&str>, include_files: bool, root_when_empty: bool) -> Result<BrowseListing, String> {
    let target: PathBuf = match path.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None if root_when_empty => root_path()?,
        None => std::env::current_dir()
            .map_err(|e| format!("Cannot resolve working directory: {e}"))?,
    };

    if !target.exists() {
        return Err(format!("Path does not exist: {}", target.display()));
    }
    if !target.is_dir() {
        return Err(format!("Not a folder: {}", target.display()));
    }

    let read = std::fs::read_dir(&target)
        .map_err(|e| format!("Cannot list {}: {e}", target.display()))?;
    let mut entries = Vec::new();
    for entry in read.flatten() {
        let p = entry.path();
        let is_dir = p.is_dir();
        if !is_dir && !include_files {
            continue;
        }
        entries.push(DirEntryMeta {
            name: entry.file_name().to_string_lossy().to_string(),
            path: p.to_string_lossy().to_string(),
            kind: if is_dir { "dir" } else { "file" },
        });
    }
    // Dirs first ("dir" < "file"), then case-insensitive by name.
    entries.sort_by(|a, b| {
        a.kind
            .cmp(b.kind)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(BrowseListing {
        path: target.to_string_lossy().to_string(),
        parent: target.parent().map(|p| p.to_string_lossy().to_string()),
        entries,
    })
}

fn root_path() -> Result<PathBuf, String> {
    if cfg!(windows) {
        // Probe drive letters; no unsafe Win32 needed for 26 metadata calls.
        for c in b'C'..=b'Z' {
            let drive = format!("{}:\\", c as char);
            if Path::new(&drive).is_dir() {
                return Ok(PathBuf::from(drive));
            }
        }
        Err("No accessible drive letters found".to_string())
    } else {
        Ok(PathBuf::from("/"))
    }
}

#[cfg(windows)]
mod win_raise {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_MENU};
    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow,
        GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
        SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SW_RESTORE,
    };

    struct FindState {
        pid: u32,
        skip: HWND,
        title: String,
        by_title: HWND,
        by_class: HWND,
        any: HWND,
    }

    pub fn raise_dialog(title: &str) -> bool {
        let _ = unsafe { AllowSetForegroundWindow(u32::MAX) };
        let Some(hwnd) = find_dialog(title) else {
            return false;
        };
        unsafe { raise_hwnd(hwnd) };
        true
    }

    fn find_dialog(title: &str) -> Option<HWND> {
        let mut state = FindState {
            pid: unsafe { GetCurrentProcessId() },
            skip: unsafe { GetConsoleWindow() },
            title: title.to_string(),
            by_title: HWND::default(),
            by_class: HWND::default(),
            any: HWND::default(),
        };
        let _ = unsafe { EnumWindows(Some(enum_cb), LPARAM(&mut state as *mut FindState as isize)) };
        for hwnd in [state.by_title, state.by_class, state.any] {
            if !hwnd.0.is_null() {
                return Some(hwnd);
            }
        }
        None
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let state = &mut *(lparam.0 as *mut FindState);
            if hwnd == state.skip {
                return BOOL(1);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != state.pid || !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            let mut title = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut title);
            if n > 0 {
                let text = String::from_utf16_lossy(&title[..n as usize]);
                if text == state.title || text.contains(&state.title) {
                    state.by_title = hwnd;
                    return BOOL(0);
                }
            }
            let mut class = [0u16; 256];
            let cn = GetClassNameW(hwnd, &mut class);
            if cn > 0 {
                let name = String::from_utf16_lossy(&class[..cn as usize]);
                if name == "#32770" && state.by_class.0.is_null() {
                    state.by_class = hwnd;
                }
            }
            if state.any.0.is_null() {
                state.any = hwnd;
            }
            BOOL(1)
        }
    }

    unsafe fn raise_hwnd(hwnd: HWND) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let fg = GetForegroundWindow();
            let fg_tid = GetWindowThreadProcessId(fg, None);
            let our_tid = GetWindowThreadProcessId(hwnd, None);
            if fg_tid != 0 && our_tid != 0 && fg_tid != our_tid {
                let _ = AttachThreadInput(fg_tid, our_tid, true);
            }
            keybd_event(VK_MENU.0 as u8, 0, Default::default(), 0);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            if fg_tid != 0 && our_tid != 0 && fg_tid != our_tid {
                let _ = AttachThreadInput(fg_tid, our_tid, false);
            }
            // Stay TOPMOST if Windows still refused foreground — otherwise
            // NOTOPMOST drops the dialog back behind the browser (taskbar flash).
            if GetForegroundWindow() == hwnd {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_NOTOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> PathBuf {
        std::env::temp_dir().join(format!("ma_workdirs_{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn record_upserts_and_sorts_recents() {
        let store = temp_store();
        let alpha = std::env::temp_dir().join(format!("ma_wd_alpha_{}", uuid::Uuid::new_v4()));
        let beta = std::env::temp_dir().join(format!("ma_wd_beta_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&beta).unwrap();

        record_workdir_at(&store, &alpha);
        record_workdir_at(&store, &beta);
        record_workdir_at(&store, &alpha); // duplicate → count bump, no new row

        let recents = load_recents_at(&store);
        assert_eq!(recents.len(), 2);
        let alpha_entry = recents
            .iter()
            .find(|r| dirs_equal(&r.path, &alpha.to_string_lossy()))
            .expect("alpha must be recorded");
        assert_eq!(alpha_entry.use_count, 2);

        let _ = std::fs::remove_dir_all(&alpha);
        let _ = std::fs::remove_dir_all(&beta);
        let _ = std::fs::remove_file(&store);
    }

    #[test]
    fn load_prunes_deleted_folders() {
        let store = temp_store();
        let gone = std::env::temp_dir().join(format!("ma_gone_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&gone).unwrap();
        record_workdir_at(&store, &gone);
        assert_eq!(load_recents_at(&store).len(), 1);

        std::fs::remove_dir_all(&gone).unwrap();
        assert!(
            load_recents_at(&store).is_empty(),
            "recents pointing at deleted folders must be pruned"
        );
        let _ = std::fs::remove_file(&store);
    }

    #[test]
    fn browse_lists_only_directories() {
        let root = std::env::temp_dir().join(format!("ma_browse_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("file.txt"), "x").unwrap();

        let listing = browse_dirs(root.to_str()).expect("browse should succeed");
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "sub");
        assert_eq!(listing.entries[0].kind, "dir");
        assert!(listing.parent.is_some());

        assert!(browse_dirs(Some("Z:\\definitely\\not\\here")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_with_files_returns_dirs_first_then_files() {
        let root = std::env::temp_dir().join(format!("ma_tree_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("zdir")).unwrap();
        std::fs::write(root.join("afile.txt"), "x").unwrap();

        let listing = list_dir_with_files(root.to_str()).expect("list should succeed");
        let kinds: Vec<&str> = listing.entries.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec!["dir", "file"], "dirs must sort before files");
        assert_eq!(listing.entries[0].name, "zdir");
        assert_eq!(listing.entries[1].name, "afile.txt");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn record_of_missing_folder_is_a_noop() {
        let store = temp_store();
        record_workdir_at(&store, Path::new("Z:\\definitely\\not\\here"));
        assert!(load_recents_at(&store).is_empty());
        let _ = std::fs::remove_file(&store);
    }
}
