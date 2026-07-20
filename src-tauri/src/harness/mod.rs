pub mod codex;

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
