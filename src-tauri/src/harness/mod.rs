pub mod codex;

/// Resolves the VF proxy credential for the spawned Codex process.
/// Precedence: process env (dev runs from a terminal) → macOS Keychain
/// item `VF_API_KEY` (GUI launches, which never inherit shell env).
/// The key is injected into the child process environment only — never
/// written to files, prompts, or the database (ARCHITECTURE §9). The
/// first Keychain read from the GUI app triggers a one-time macOS
/// permission prompt — answer "Always Allow".
pub fn resolve_vf_api_key() -> Option<String> {
    if let Ok(value) = std::env::var("VF_API_KEY") {
        if !value.is_empty() {
            return Some(value);
        }
    }

    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "VF_API_KEY", "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Locates the `codex` binary. Tauri's GUI process on macOS does not
/// inherit a login shell's PATH (Homebrew's /opt/homebrew/bin is often
/// missing), so we probe the common install locations before falling back
/// to relying on PATH as-is.
pub fn resolve_binary(name: &str) -> String {
    let candidates = [
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
    ];

    for candidate in candidates {
        if std::path::Path::new(&candidate).is_file() {
            return candidate;
        }
    }

    if let Some(home) = dirs::home_dir() {
        let local = home.join(".local/bin").join(name);
        if local.is_file() {
            return local.to_string_lossy().to_string();
        }
    }

    name.to_string()
}
