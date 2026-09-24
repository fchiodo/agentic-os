#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_dir/.." && pwd)"
cd "$repository_root"

fail() {
  echo "Bootstrap failed: $1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "$2"
}

[[ "$(uname -s)" == "Darwin" ]] || fail "Agentic OS desktop development requires macOS."
[[ "$(uname -m)" == "arm64" ]] || fail "Document Converter v1 requires an Apple Silicon Mac (arm64)."

macos_version="$(sw_vers -productVersion)"
macos_major="$(printf '%s' "$macos_version" | cut -d. -f1)"
macos_minor="$(printf '%s' "$macos_version" | cut -d. -f2)"
if [[ "$macos_major" -lt 26 ]] || { [[ "$macos_major" -eq 26 ]] && [[ "$macos_minor" -lt 2 ]]; }; then
  fail "The pinned MLX runtime requires macOS 26.2 or newer. Found $macos_version."
fi

require_command node "Node.js is missing. Install a supported Node.js release (20 or newer), then rerun this script."
node_major="$(node -p 'process.versions.node.split(".")[0]')"
node_minor="$(node -p 'process.versions.node.split(".")[1]')"
if ! { [[ "$node_major" -eq 20 ]] && [[ "$node_minor" -ge 19 ]]; } \
  && ! { [[ "$node_major" -ge 22 ]] && { [[ "$node_major" -gt 22 ]] || [[ "$node_minor" -ge 12 ]]; }; }; then
  fail "Node.js 20.19+ or 22.12+ is required by Vite 8. Found $(node --version)."
fi

require_command pnpm "pnpm is missing. Enable Corepack or install pnpm, then rerun this script."
required_pnpm="11.25.0"
actual_pnpm="$(pnpm --version)"
[[ "$actual_pnpm" == "$required_pnpm" ]] || fail "pnpm $required_pnpm is required. Found $actual_pnpm. Run: corepack prepare pnpm@$required_pnpm --activate"
require_command rustc "Rust is missing. Install rustup and the repository-pinned Rust 1.98.1 toolchain from https://rustup.rs, then rerun this script."
require_command cargo "Cargo is missing. Install the Rust toolchain, then rerun this script."
required_rust="1.98.1"
actual_rust="$(rustc --version | awk '{print $2}')"
[[ "$actual_rust" == "$required_rust" ]] || fail "Rust $required_rust is required for reproducible builds. Found $actual_rust. With rustup, run: rustup toolchain install $required_rust"

ocr_python="${AGENTIC_OS_OCR_PYTHON:-}"
if [[ -z "$ocr_python" ]]; then
  ocr_python="$(command -v python3.10 || true)"
fi
[[ -n "$ocr_python" ]] || fail "Native arm64 Python 3.10 is required to build the OCR sidecar. Set AGENTIC_OS_OCR_PYTHON to its path."
required_python="$(tr -d '[:space:]' < tools/ocr-sidecar/.python-version)"
"$ocr_python" -c 'import platform, sys; assert platform.python_version() == sys.argv[1] and platform.machine() == "arm64"' "$required_python" \
  || fail "OCR Python must be native arm64 Python $required_python."

export AGENTIC_OS_OCR_PYTHON="$ocr_python"
echo "Prerequisites verified: macOS $(sw_vers -productVersion), Node $(node --version), pnpm $(pnpm --version), $(rustc --version), Python $("$ocr_python" --version 2>&1)."
pnpm install --frozen-lockfile
pnpm prepare:sidecars
echo "Agentic OS bootstrap complete. Run: pnpm dev:desktop"
