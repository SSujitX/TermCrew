//! Read/write access to individual files for the in-app editor (Monaco).
//! Guardrails: reject binary content, cap file size, and detect write
//! conflicts via the file's modification time (optimistic locking).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Files larger than this are refused: the editor is for code and configs,
/// not for multi-megabyte blobs.
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub size: u64,
    /// Millis since Unix epoch — echoed back on write for conflict detection.
    pub modified_ms: u128,
}

#[derive(Debug, Deserialize)]
pub struct FileWrite {
    pub path: String,
    pub content: String,
    /// mtime the client read; `None` creates/overwrites unconditionally.
    pub expected_modified_ms: Option<u128>,
}

#[derive(Debug, Serialize)]
pub struct FileWriteResult {
    pub path: String,
    pub modified_ms: u128,
}

fn is_probably_binary(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(8192)];
    // NUL byte is the classic tell; the UTF-8 check below catches the rest.
    probe.contains(&0) || std::str::from_utf8(probe).is_err()
}

fn modified_ms(meta: &std::fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t: SystemTime| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn read_file(path: &str) -> Result<FileContent, String> {
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return Err(format!("File does not exist: {}", target.display()));
    }
    let meta = std::fs::metadata(&target).map_err(|e| format!("Cannot stat file: {e}"))?;
    if meta.is_dir() {
        return Err(format!("'{}' is a folder, not a file", target.display()));
    }
    let size = meta.len();
    if size as usize > MAX_FILE_BYTES {
        return Err(format!(
            "File is {} MB — larger than the {} MB editor limit. Open it in a terminal instead.",
            (size as f64 / 1024.0 / 1024.0).round(),
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }

    let bytes = std::fs::read(&target).map_err(|e| format!("Cannot read file: {e}"))?;
    if is_probably_binary(&bytes) {
        return Err(
            "Binary file — the editor only opens text. Inspect it in a terminal session instead."
                .to_string(),
        );
    }

    Ok(FileContent {
        path: target.to_string_lossy().to_string(),
        content: String::from_utf8_lossy(&bytes).to_string(),
        size,
        modified_ms: modified_ms(&meta),
    })
}

pub fn write_file(req: &FileWrite) -> Result<FileWriteResult, String> {
    let target = PathBuf::from(req.path.trim());
    if !target.exists() {
        return Err(format!("File does not exist: {}", target.display()));
    }
    if req.content.len() > MAX_FILE_BYTES {
        return Err(format!(
            "Content is {} MB — larger than the {} MB editor limit.",
            (req.content.len() as f64 / 1024.0 / 1024.0).round(),
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    if is_probably_binary(req.content.as_bytes()) {
        return Err("Refusing to write: content looks binary.".to_string());
    }

    // Optimistic locking: if the file changed on disk since the client read
    // it (e.g. an agent edited it mid-flight), refuse instead of clobbering.
    if let Some(expected) = req.expected_modified_ms {
        let current = std::fs::metadata(&target)
            .map(|m| modified_ms(&m))
            .map_err(|e| format!("Cannot stat file: {e}"))?;
        if current != expected {
            return Err(
                "This file changed on disk since you opened it (an agent may have edited it). \
                 Close and reopen the file to load the latest version before saving."
                    .to_string(),
            );
        }
    }

    std::fs::write(&target, req.content.as_bytes())
        .map_err(|e| format!("Cannot write file: {e}"))?;

    let modified_ms = std::fs::metadata(&target)
        .map(|m| modified_ms(&m))
        .unwrap_or(0);
    tracing::info!(path = %target.display(), bytes = req.content.len(), "File written via editor");
    Ok(FileWriteResult {
        path: target.to_string_lossy().to_string(),
        modified_ms,
    })
}

#[derive(Debug, Deserialize)]
pub struct CreateEntry {
    pub root: String,
    pub rel_path: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct CreateResult {
    pub path: String,
    pub kind: String,
}

fn normalize_rel_path(rel: &str) -> Result<PathBuf, String> {
    let trimmed = rel.trim().replace('\\', "/");
    let trimmed = trimmed.trim_matches('/');
    if trimmed.is_empty() {
        return Err("Enter a name or path, e.g. notes.txt or src/lib/util.ts".into());
    }
    if trimmed.starts_with('/') || trimmed.contains(':') {
        return Err("Use a path relative to this folder".into());
    }
    let mut out = PathBuf::new();
    for seg in trimmed.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return Err(".. is not allowed".into());
        }
        if seg.len() > 80
            || seg.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0'))
        {
            return Err(format!("Invalid name: {seg}"));
        }
        out.push(seg);
    }
    if out.as_os_str().is_empty() {
        return Err("Enter a name or path, e.g. notes.txt or src/lib/util.ts".into());
    }
    Ok(out)
}

/// Creates a file or folder under `root`. `rel_path` may include slashes
/// (`src/lib/util.ts`) — missing parents are created. Stays inside `root`.
pub fn create_entry(req: &CreateEntry) -> Result<CreateResult, String> {
    let kind = req.kind.trim().to_ascii_lowercase();
    if kind != "file" && kind != "dir" {
        return Err("Kind must be file or dir".into());
    }
    let root = PathBuf::from(req.root.trim());
    if !root.is_dir() {
        return Err(format!("Folder does not exist: {}", root.display()));
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("Cannot resolve folder: {e}"))?;
    let rel = normalize_rel_path(&req.rel_path)?;
    let target = root.join(&rel);
    if !target.starts_with(&root) {
        return Err("Path is outside the session folder".into());
    }
    if target.exists() {
        return Err(format!("Already exists: {}", target.display()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create folders: {e}"))?;
    }
    if kind == "dir" {
        std::fs::create_dir(&target).map_err(|e| format!("Cannot create folder: {e}"))?;
    } else {
        std::fs::File::create_new(&target).map_err(|e| format!("Cannot create file: {e}"))?;
    }
    Ok(CreateResult {
        path: target.to_string_lossy().to_string(),
        kind,
    })
}

#[derive(Debug, Deserialize)]
pub struct DeleteEntry {
    pub root: String,
    pub path: String,
}

/// Deletes a file or folder that is strictly inside `root`.
pub fn delete_entry(req: &DeleteEntry) -> Result<(), String> {
    let root = PathBuf::from(req.root.trim());
    if !root.is_dir() {
        return Err(format!("Folder does not exist: {}", root.display()));
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("Cannot resolve folder: {e}"))?;
    let target = PathBuf::from(req.path.trim());
    if !target.exists() {
        return Err(format!("Does not exist: {}", target.display()));
    }
    let target = target
        .canonicalize()
        .map_err(|e| format!("Cannot resolve path: {e}"))?;
    if target == root {
        return Err("Cannot delete the session folder".into());
    }
    if !target.starts_with(&root) {
        return Err("Path is outside the session folder".into());
    }
    if target.is_dir() {
        std::fs::remove_dir_all(&target).map_err(|e| format!("Cannot delete folder: {e}"))?;
    } else {
        std::fs::remove_file(&target).map_err(|e| format!("Cannot delete file: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("{}_{}", uuid::Uuid::new_v4(), name));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn read_and_write_roundtrip() {
        let file = temp_file("edit.txt", "hello");
        let read = read_file(file.to_str().unwrap()).expect("read");
        assert_eq!(read.content, "hello");

        let result = write_file(&FileWrite {
            path: file.to_string_lossy().to_string(),
            content: "hello world".to_string(),
            expected_modified_ms: Some(read.modified_ms),
        })
        .expect("write");
        assert!(result.modified_ms >= read.modified_ms);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello world");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn write_conflict_is_detected() {
        let file = temp_file("conflict.txt", "v1");
        let read = read_file(file.to_str().unwrap()).unwrap();

        // Simulate an agent editing the file after the client read it.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&file, "v2 from agent").unwrap();

        let err = write_file(&FileWrite {
            path: file.to_string_lossy().to_string(),
            content: "stale client edit".to_string(),
            expected_modified_ms: Some(read.modified_ms),
        })
        .expect_err("stale write must be rejected");
        assert!(err.contains("changed on disk"), "{err}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2 from agent");

        // Writing with no expectation still works (explicit override).
        write_file(&FileWrite {
            path: file.to_string_lossy().to_string(),
            content: "fresh".to_string(),
            expected_modified_ms: None,
        })
        .expect("unconditional write");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn binary_files_are_rejected() {
        let file = std::env::temp_dir().join(format!("bin_{}.bin", uuid::Uuid::new_v4()));
        std::fs::write(&file, [0x4du8, 0x5a, 0x00, 0x01]).unwrap();

        let err = read_file(file.to_str().unwrap()).expect_err("binary must be rejected");
        assert!(err.contains("Binary"), "{err}");

        let err = write_file(&FileWrite {
            path: file.to_string_lossy().to_string(),
            content: "\u{0}\u{1}\u{2}".to_string(),
            expected_modified_ms: None,
        })
        .expect_err("binary write must be rejected");
        assert!(err.contains("binary"), "{err}");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn oversized_files_are_rejected() {
        let file = std::env::temp_dir().join(format!("big_{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&file, vec![b'a'; MAX_FILE_BYTES + 1]).unwrap();
        let err = read_file(file.to_str().unwrap()).expect_err("oversize must be rejected");
        assert!(err.contains("larger"), "{err}");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn create_nested_file_and_reject_escape() {
        let root = std::env::temp_dir().join(format!("tc_create_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let made = create_entry(&CreateEntry {
            root: root.to_string_lossy().into(),
            rel_path: "src/lib/util.ts".into(),
            kind: "file".into(),
        })
        .expect("create nested file");
        assert!(PathBuf::from(&made.path).is_file());
        assert_eq!(made.kind, "file");
        let dir = create_entry(&CreateEntry {
            root: root.to_string_lossy().into(),
            rel_path: "docs/notes".into(),
            kind: "dir".into(),
        })
        .expect("create nested dir");
        assert!(PathBuf::from(&dir.path).is_dir());
        let err = create_entry(&CreateEntry {
            root: root.to_string_lossy().into(),
            rel_path: "../escape.txt".into(),
            kind: "file".into(),
        })
        .expect_err(".. must be rejected");
        assert!(err.contains(".."), "{err}");
        delete_entry(&DeleteEntry {
            root: root.to_string_lossy().into(),
            path: made.path.clone(),
        })
        .expect("delete file");
        assert!(!PathBuf::from(&made.path).exists());
        delete_entry(&DeleteEntry {
            root: root.to_string_lossy().into(),
            path: dir.path.clone(),
        })
        .expect("delete folder");
        assert!(!PathBuf::from(&dir.path).exists());
        let root_err = delete_entry(&DeleteEntry {
            root: root.to_string_lossy().into(),
            path: root.to_string_lossy().into(),
        })
        .expect_err("must not delete tree root");
        assert!(root_err.contains("session folder"), "{root_err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
