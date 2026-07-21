use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::error::{AppError, AppResult};

use super::VaultNode;

const DOMAIN_DIRS: [&str; 6] = [
    "work",
    "planphysique",
    "personal",
    "family",
    "finance",
    "research",
];

static VAULT_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Serialize multi-step vault/Git/index mutations. SQLite has its own mutex,
/// but Git's index and filesystem compensation also need one writer at a time.
pub fn lock_writes() -> MutexGuard<'static, ()> {
    VAULT_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Returns the vault root. Defaults to ~/AgenticOS/vault/; the
/// AGENTIC_OS_VAULT_ROOT environment variable overrides it (used by
/// tests, and the escape hatch until Settings exposes it).
pub fn vault_root() -> AppResult<PathBuf> {
    if let Ok(custom) = std::env::var("AGENTIC_OS_VAULT_ROOT") {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    let home = dirs::home_dir().ok_or(AppError::MissingHomeDirectory)?;
    let root = home.join("AgenticOS").join("vault");
    Ok(root)
}

/// Ensure the vault exists with all domain directories and is a git repo.
pub fn ensure_vault() -> AppResult<PathBuf> {
    let root = vault_root()?;
    fs::create_dir_all(&root)?;

    for domain in DOMAIN_DIRS {
        let dir = root.join(domain);
        fs::create_dir_all(&dir)?;
        // Create a .gitkeep so git tracks empty dirs
        let gitkeep = dir.join(".gitkeep");
        if !gitkeep.exists() {
            fs::write(&gitkeep, "")?;
        }
    }

    let archive = root.join("_archive");
    fs::create_dir_all(&archive)?;

    // Initialize git if not already
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        git_init(&root)?;
    }

    Ok(root)
}

/// Returns the root where distilled skills land: the directory the
/// harnesses natively consume (MEMORY-SPEC §4 source 4). Overridable via
/// AGENTIC_OS_SKILLS_ROOT (tests, non-Codex setups).
pub fn skills_root() -> AppResult<PathBuf> {
    if let Ok(custom) = std::env::var("AGENTIC_OS_SKILLS_ROOT") {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    let home = dirs::home_dir().ok_or(AppError::MissingHomeDirectory)?;
    Ok(home.join(".codex").join("skills"))
}

/// Write a distilled skill file under the skills root. Same traversal
/// defense as vault writes — a malicious slug cannot escape the root.
pub fn write_skill_file(relative_path: &str, content: &str) -> AppResult<PathBuf> {
    let root = skills_root()?;
    fs::create_dir_all(&root)?;
    let full = canonicalize_under(&root, relative_path)?;
    atomic_replace(&full, content)?;
    Ok(full)
}

pub fn read_skill_file(relative_path: &str) -> AppResult<String> {
    let root = skills_root()?;
    fs::create_dir_all(&root)?;
    let full = canonicalize_under(&root, relative_path)?;
    Ok(fs::read_to_string(full)?)
}

pub fn remove_skill_file(relative_path: &str) -> AppResult<()> {
    let root = skills_root()?;
    fs::create_dir_all(&root)?;
    let full = canonicalize_under(&root, relative_path)?;
    if full.exists() {
        fs::remove_file(full)?;
    }
    Ok(())
}

/// Read a file from the vault. Path must resolve under vault root.
pub fn read_file(relative_path: &str) -> AppResult<(String, PathBuf)> {
    let root = vault_root()?;
    let full = canonicalize_under(&root, relative_path)?;
    let content = fs::read_to_string(&full)?;
    Ok((content, full))
}

pub fn file_exists(relative_path: &str) -> AppResult<bool> {
    let root = vault_root()?;
    Ok(canonicalize_under(&root, relative_path)?.is_file())
}

/// Atomically replace a vault file. The temporary file lives beside the
/// destination, therefore `rename` cannot cross filesystems and readers
/// never observe a partially-written Markdown document.
pub fn write_file_atomic(relative_path: &str, content: &str) -> AppResult<PathBuf> {
    let root = vault_root()?;
    let full = canonicalize_under(&root, relative_path)?;
    atomic_replace(&full, content)?;
    Ok(full)
}

fn atomic_replace(full: &Path, content: &str) -> AppResult<()> {
    let parent = full
        .parent()
        .ok_or_else(|| AppError::Io(std::io::Error::other("vault file has no parent")))?;
    fs::create_dir_all(parent)?;

    let file_name = full
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Io(std::io::Error::other("invalid vault file name")))?;
    let temp = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));

    let result = (|| -> AppResult<()> {
        let mut handle = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        handle.write_all(content.as_bytes())?;
        handle.sync_all()?;
        fs::rename(&temp, full)?;
        // Best-effort directory sync makes the rename durable on filesystems
        // that support syncing directories (macOS/Linux).
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(())
}

/// Remove one validated vault file. Used only by compensating rollback for a
/// failed create; callers must provide an exact relative path.
pub fn remove_file(relative_path: &str) -> AppResult<()> {
    let root = vault_root()?;
    let full = canonicalize_under(&root, relative_path)?;
    if full.exists() {
        fs::remove_file(full)?;
    }
    Ok(())
}

/// Move a file to the archive directory (git mv).
pub fn archive_file(relative_path: &str, domain: &str) -> AppResult<PathBuf> {
    let root = vault_root()?;
    let source = canonicalize_under(&root, relative_path)?;
    let archive_dir = root.join("_archive").join(domain);
    let domain_root = fs::canonicalize(root.join(domain))?;
    let suffix = source.strip_prefix(&domain_root).map_err(|_| {
        AppError::Io(std::io::Error::other(
            "archive source does not belong to requested domain",
        ))
    })?;
    let dest = archive_dir.join(suffix);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        return Err(AppError::Io(std::io::Error::other(
            "archive destination already exists",
        )));
    }

    // Try git mv first, fall back to fs rename
    let status = Command::new("git")
        .args([
            "mv",
            source.to_str().unwrap_or(""),
            dest.to_str().unwrap_or(""),
        ])
        .current_dir(&root)
        .status();

    match status {
        Ok(s) if s.success() => Ok(dest),
        _ => {
            fs::rename(&source, &dest)?;
            Ok(dest)
        }
    }
}

/// Compensating inverse of `archive_file` used when the Git or SQLite part of
/// expiry fails. Both paths are validated under the vault root.
pub fn restore_archived_file(
    archived_relative_path: &str,
    original_relative_path: &str,
) -> AppResult<()> {
    let root = vault_root()?;
    let source = canonicalize_under(&root, archived_relative_path)?;
    let destination = canonicalize_under(&root, original_relative_path)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(source, destination)?;
    Ok(())
}

/// Git commit in the vault.
pub fn git_commit(message: &str) -> AppResult<()> {
    let root = vault_root()?;
    if !root.join(".git").exists() {
        return Ok(());
    }

    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&root)
        .status()?;
    if !add.success() {
        return Err(AppError::Io(std::io::Error::other("git add failed")));
    }

    let status = Command::new("git")
        .args(["commit", "-m", message, "--allow-empty"])
        .current_dir(&root)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(AppError::Io(std::io::Error::other("git commit failed")))
    }
}

/// Get the last git commit hash for a file.
pub fn git_last_commit(relative_path: &str) -> Option<String> {
    let root = vault_root().ok()?;
    let output = Command::new("git")
        .args(["log", "-1", "--format=%h", "--", relative_path])
        .current_dir(&root)
        .output()
        .ok()?;

    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if hash.is_empty() {
        None
    } else {
        Some(hash)
    }
}

/// Build a VaultNode tree from the vault filesystem.
pub fn tree(domain: Option<&str>) -> AppResult<Vec<VaultNode>> {
    let root = vault_root()?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let domains = match domain {
        Some(d) if DOMAIN_DIRS.contains(&d) => vec![d.to_string()],
        Some(_) => {
            return Err(AppError::Io(std::io::Error::other(
                "invalid memory domain",
            )))
        }
        None => DOMAIN_DIRS.iter().map(|s| s.to_string()).collect(),
    };

    let mut nodes = Vec::new();
    for d in &domains {
        let dir = root.join(d);
        if dir.exists() {
            nodes.push(build_node(&root, &dir)?);
        }
    }
    Ok(nodes)
}

fn build_node(root: &Path, path: &Path) -> AppResult<VaultNode> {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    if path.is_dir() {
        let mut children = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n != ".gitkeep" && !n.starts_with('.')
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            children.push(build_node(root, &entry.path())?);
        }

        Ok(VaultNode {
            name,
            path: relative,
            is_dir: true,
            children,
            memory_id: None,
            mem_type: None,
            status: None,
        })
    } else {
        Ok(VaultNode {
            name,
            path: relative,
            is_dir: false,
            children: Vec::new(),
            memory_id: None,
            mem_type: None,
            status: None,
        })
    }
}

/// Verify a relative path resolves under the vault root and return the
/// full path. Two layers of defense (MEMORY-SPEC §10):
///
/// 1. Structural: absolute paths and any `..` component are rejected
///    outright. `Path::starts_with` compares components literally, so
///    `root/../x` would pass a prefix check while escaping on write —
///    the structural rejection closes that hole for not-yet-existing
///    files where canonicalize() cannot run.
/// 2. Symlink: the deepest existing ancestor is canonicalized and must
///    still live under the canonicalized root, so a symlink inside the
///    vault cannot smuggle writes outside it.
fn canonicalize_under(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::Io(std::io::Error::other(
            "path escapes vault root",
        )));
    }

    let joined = root.join(rel);
    let root_canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    // Find the deepest existing ancestor and canonicalize it to resolve
    // any symlinks on the way.
    let mut probe = joined.clone();
    let resolved_prefix = loop {
        if probe.exists() {
            break fs::canonicalize(&probe)?;
        }
        match probe.parent() {
            Some(parent) => probe = parent.to_path_buf(),
            None => break root_canonical.clone(),
        }
    };

    if !resolved_prefix.starts_with(&root_canonical) {
        return Err(AppError::Io(std::io::Error::other(
            "path escapes vault root",
        )));
    }

    // Return the canonical form when the target exists, the joined form
    // otherwise (creation path).
    if joined.exists() {
        Ok(fs::canonicalize(&joined)?)
    } else {
        Ok(joined)
    }
}

fn git_init(root: &Path) -> AppResult<()> {
    let status = Command::new("git")
        .args(["init"])
        .current_dir(root)
        .status()?;
    if !status.success() {
        return Err(AppError::Io(std::io::Error::other("git init failed")));
    }

    // Configure local git user for the vault
    let email = Command::new("git")
        .args(["config", "user.email", "vault@agentic-os.local"])
        .current_dir(root)
        .status()?;
    let name = Command::new("git")
        .args(["config", "user.name", "Agentic OS Vault"])
        .current_dir(root)
        .status()?;
    if !email.success() || !name.success() {
        return Err(AppError::Io(std::io::Error::other(
            "git local identity configuration failed",
        )));
    }

    // Initial commit
    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .status()?;
    let commit = Command::new("git")
        .args(["commit", "-m", "mem: initialize vault"])
        .current_dir(root)
        .status()?;

    if !add.success() || !commit.success() {
        return Err(AppError::Io(std::io::Error::other(
            "initial vault commit failed",
        )));
    }

    Ok(())
}
