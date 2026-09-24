# Agentic OS

Desktop control plane for local agents, skills, routines, memory, and document processing.

## Stack

- Tauri 2 for the desktop shell and native commands
- React + TypeScript + Vite for the UI
- TanStack Query for native data hydration
- Zustand for persisted workbench state
- Rusqlite for the local Agentic OS system of record and Codex data reads

## What is already wired

- Catalog discovery for:
  - `~/.codex/skills`
  - `~/.codex/.tmp/plugins/plugins`
  - `~/.codex/routines`
  - workspace `skills/`, `agents/`, `routines/`, and `prompts/`
- Primary views:
  - `Catalog`
  - `Memory`
  - `Document Converter`

## Project layout

```text
src/
  app/                  router + providers
  components/           layout and UI primitives
  features/
    catalog/            inventory browsing
    dashboard/          native data contract and query
    memory/             governed memory and Second Brain
    document-converter/ local conversion UI, typed IPC schemas, preview
  lib/                  formatting and platform helpers
  store/                persisted UI state

src-tauri/
  src/
    commands.rs         Tauri commands exposed to the UI
    document_converter/ jobs, models, classifier, OCR engine, storage
    discovery.rs        local file and plugin inventory
    models.rs           shared response payloads
    snapshot.rs         database reads + composed dashboard snapshot
```

## Commands

```bash
pnpm install
pnpm dev
pnpm dev:desktop
pnpm build
pnpm check:native
pnpm build:desktop
```

## Document Converter

Document Converter is a native feature with a React page, typed Tauri IPC, a
Rust control plane, SQLite history, an atomic model manager, and a separate
self-contained OCR sidecar. It routes good digital PDFs through local text
extraction and scanned/image/poor-text documents through PaddleOCR-VL on MLX.
The output is a versioned package containing Markdown, `document.json`, and
optional local assets.

The v1 runtime is Apple Silicon-only. The currently pinned MLX wheel requires
macOS 26.2 or newer; that constraint is explicit in the Tauri bundle.

```bash
./scripts/bootstrap-macos.sh
pnpm prepare:ocr
pnpm check:ocr
pnpm test:ocr
```

The bootstrap downloads a pinned, checksum-verified CPython 3.10.19 arm64
runtime into `.build/ocr-python/`. It does not install Python globally and does
not require Homebrew. `pnpm dev:desktop` performs the same preparation on
demand when the private runtime is not present.

The model is not committed or bundled. It is installed on demand from the
Document Converter page and verified file-by-file against the pinned manifest.
The direct real-inference smoke test remains available for developers:

```bash
AGENTIC_OS_OCR_MODEL=/absolute/path/to/model pnpm test:ocr:integration
pnpm benchmark:ocr -- --model /absolute/path/to/model
```

See [the evidence report](docs/ocr-spike-results.md) and
[Document Converter runbook](docs/DOCUMENT-CONVERTER-RUNBOOK.md). The current
[implementation report](docs/DOCUMENT-CONVERTER-IMPLEMENTATION-REPORT.md)
records executed tests and unresolved external gates. Clean-Mac,
M4 Pro/24 GB, Developer ID signing/notarization, and true Mac A → Mac B checks
remain external release gates and are never reported as passed without
execution. A local ad-hoc Hardened Runtime `.app` and DMG are validated by the
current build workflow.

## Next increments

1. Add a secure routine execution adapter with explicit allowlists.
2. Persist run history in an app-owned SQLite database.
3. Add pricing tables so token usage can roll up into estimated and billed cost.
4. Add writable settings for source roots and workspace-specific launch targets.
