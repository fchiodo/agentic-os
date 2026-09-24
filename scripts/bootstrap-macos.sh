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

macos_major="$(sw_vers -productVersion | cut -d. -f1)"
[[ "$macos_major" -ge 14 ]] || fail "Document AI requires macOS 14 or newer. Found $(sw_vers -productVersion)."

require_command node "Node.js is missing. Install a supported Node.js release (20 or newer), then rerun this script."
node_major="$(node -p 'process.versions.node.split(".")[0]')"
[[ "$node_major" -ge 20 ]] || fail "Node.js 20 or newer is required. Found $(node --version)."

require_command pnpm "pnpm is missing. Enable Corepack or install pnpm, then rerun this script."
require_command rustc "Rust is missing. Install Rust 1.77.2 or newer from https://rustup.rs, then rerun this script."
require_command cargo "Cargo is missing. Install the Rust toolchain, then rerun this script."

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
