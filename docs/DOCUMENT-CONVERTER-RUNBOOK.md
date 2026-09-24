# Document Converter runbook

Status: Phase 1/2 spike only. The Tauri feature is intentionally not enabled
until the critical gates in `docs/ocr-spike-results.md` are closed.

## Current architecture

```text
test/client
    │ versioned JSONL over stdio
    ▼
OCR sidecar (private Python 3.10 runtime)
    │ direct library call; offline mode forced
    ▼
MLX-VLM 0.7.2 + MLX 0.32.2
    │ verified local path only
    ▼
PaddleOCR-VL 1.6 model snapshot
```

The planned production control plane is React → Tauri IPC → Rust
`DocumentConversionService` → engine adapter/sidecar. The sidecar never
downloads a model and is not allowed to decide output paths.

## Developer prerequisites

- Apple Silicon Mac, currently macOS 14 or newer for source builds. The final
  deployment target is not approved yet; the spike host selected native MLX
  libraries with minimum OS 26.2.
- Node.js 20 or newer and pnpm.
- Rust/Cargo 1.77.2 or newer for Agentic OS.
- Native arm64 CPython 3.10.19 for building the OCR sidecar. The exact version
  is pinned in `tools/ocr-sidecar/.python-version`; set
  `AGENTIC_OS_OCR_PYTHON` if it is not on `PATH`.

The final `.app` user will not need any of these developer tools.

## Bootstrap

```bash
./scripts/bootstrap-macos.sh
```

The script is designed to be idempotent: it verifies prerequisites, installs
the locked project dependencies, and prepares both sidecars. It does not
install system software silently. On a cache hit the OCR build is skipped. A
full second bootstrap could not be tested on this host because Rust is absent.

To build only OCR:

```bash
AGENTIC_OS_OCR_PYTHON=/absolute/path/to/python3.10 pnpm prepare:ocr
pnpm check:ocr
```

Build state is under `src-tauri/target/ocr-sidecar/`. The generated Tauri
binary is `src-tauri/binaries/ocr-sidecar-aarch64-apple-darwin` and is ignored
by Git.

## Tests

Fast protocol tests do not load the model:

```bash
pnpm test:ocr
```

The opt-in real test requires an already installed, verified snapshot:

```bash
AGENTIC_OS_OCR_MODEL=/absolute/path/to/model pnpm test:ocr:integration
```

It creates a synthetic non-sensitive image in a temporary directory, launches
the packaged sidecar with a minimal environment, performs real inference, and
checks expected text. It does not download anything.

## Model lifecycle (production target)

The source-of-truth manifest is
`tools/ocr-sidecar/model-manifest.json`. A future Rust Model Manager must:

1. check disk space for download, temporary data, installation, and margin;
2. download every file from the pinned HTTPS revision into an app-controlled
   temporary directory;
3. validate declared size and SHA-256 for every file;
4. fsync/close and atomically rename the verified directory into
   `~/Library/Application Support/Agentic OS/models/paddleocr-vl/<version>/`;
5. never load a partial or mismatched version;
6. remove only paths proven to be descendants of its managed root and never
   follow an untrusted symlink during repair/cleanup.

The sidecar itself accepts only the resulting local directory. Removing the
`.app` will not remove models; the future `Remove Document AI` action must do
that explicitly.

## Clean rebuild

Generated state may be removed from the narrow build directory and rebuilt:

```bash
rm -rf src-tauri/target/ocr-sidecar
pnpm prepare:ocr
```

Do not remove broader workspace, application-support, or model directories as
part of a build cleanup.

## Protocol diagnostics

`pnpm check:ocr` reports architecture, protocol, sidecar version, MLX-VLM
version, MLX version, and required model revision. stdout is JSONL only;
warnings and diagnostics use stderr. Full OCR text must never be written to
normal application logs.

Current commands are `health`, `capabilities`, `load-model`, `convert-image`,
and `shutdown`. `cancel` deliberately returns `NOT_IMPLEMENTED` in the spike;
real cancellation and lifecycle ownership belong to the Rust Job Manager.

## Minimum threat model

| Threat | Required mitigation |
|---|---|
| Malicious filename / traversal | Rust-generated UUID temp paths, filename sanitization, descendant checks, no shell interpolation. |
| Symlink attack during cleanup | Operate only below canonical managed roots; reject symlinks and never recursively remove an unresolved user path. |
| Malicious/corrupt PDF | Bounded page/image inspection, explicit encrypted/invalid errors, isolated rendering, resource limits. |
| Malicious Markdown | Disable raw HTML, sanitize output, block implicit remote loads and arbitrary `file://` access. |
| Corrupted model download | Pinned HTTPS revision plus size and SHA-256 for every file; atomic install only after validation. |
| Unexpected sidecar output | Versioned schema validation, bounded line/output sizes, fail closed on unknown/incompatible messages. |
| Resource exhaustion | One OCR job, page-oriented processing, disk preflight, configurable page/pixel/asset limits, idle unload. |
| Sidecar compromise | Signed bundle, structured stdio, no shell, no listening port, local verified model only, narrow filesystem inputs. |

The spike implements the versioned stdio boundary, local-only model path,
offline environment, and fixed prompt set. Remaining mitigations belong to the
Rust control plane and are Phase 3 work.

## Offline test

The full acceptance test remains:

1. install the model while online;
2. disconnect the Mac from the Internet;
3. restart Agentic OS;
4. convert a scanned PDF;
5. validate Markdown, JSON, and assets.

For the spike, the packaged sidecar was additionally run under a macOS sandbox
profile denying all network access and completed real OCR from the local model.

## Mac A → Mac B test

This is not complete until it is performed on a genuinely separate clean Mac:

```bash
git clone <repository>
cd agentic-os
./scripts/bootstrap-macos.sh
pnpm dev:desktop
```

No `.venv`, binary, model cache, hidden environment, or local build output may
be copied from Mac A. Record OS/hardware, command logs, model verification,
offline conversion, and failure details in `docs/ocr-spike-results.md`.

## Signing notes

The spike binary is not a release artifact. Before `.app`/`.dmg` release:

- enable the sidecar in `externalBin` only after the backend lifecycle exists;
- recursively inspect and sign the sidecar and extracted native libraries with
  Hardened Runtime-compatible entitlements;
- inspect `LC_BUILD_VERSION` for every bundled Mach-O. The outer executable's
  minimum version is insufficient evidence because embedded MLX libraries can
  require a newer macOS;
- run `codesign --verify --deep --strict`, notarize, staple, and test on a Mac
  without developer tools;
- verify model files remain external persistent data and are never modified by
  an application update.

## Troubleshooting

- `requires native Apple Silicon`: do not build through Rosetta.
- `requires Python 3.10`: use an arm64 3.10 interpreter and set
  `AGENTIC_OS_OCR_PYTHON`; never install packages into global Python.
- `MODEL_NOT_INSTALLED`: the passed directory is missing one of the required
  model files. Repair through the future Model Manager, not pip/Hugging Face
  auto-download.
- `PROTOCOL_MISMATCH`: rebuild the sidecar and verify the app/model versions.
- Registry HTTP 429 during model research is not an OCR failure; the production
  downloader needs explicit retry/backoff and resumable, verified artifacts.
