# Document Converter runbook

## Architecture

```text
React Document Converter / Memory import
                │ typed Tauri commands + events
                ▼
Rust DocumentConversionService
  ├─ Job Manager (one active OCR job, SQLite history, cancellation)
  ├─ Model Manager (HTTPS, size/SHA-256, atomic install)
  ├─ Document Classifier (digital / OCR / hybrid)
  └─ OcrEngine → PaddleOcrEngine
                    │ protocol v1 JSON Lines over stdio
                    ▼
             OCR sidecar 0.2.1
                    │ direct local library calls, no server
                    ▼
       MLX-VLM 0.7.2 + MLX 0.32.2
                    │ verified local snapshot only
                    ▼
              PaddleOCR-VL 1.6
```

Rust is the control plane. React never starts Python, sees a PID, chooses an
engine, or writes conversion packages. The sidecar never downloads models and
never chooses a final output path. The existing MarkItDown sidecar remains
separate and unchanged.

Document data, rendered pages, OCR output and assets stay inside this local
pipeline. The only Document Converter network operation is the explicit model
installation performed by the Rust Model Manager. There is no cloud fallback.

## Requirements

Developer builds require:

- Apple Silicon (`arm64`);
- macOS 26.2 or newer for the pinned MLX wheel selected by this build;
- Node 20.19+ or 22.12+ and pnpm 11.25.0;
- repository-pinned Rust/Cargo 1.98.1;
- Internet access during the first bootstrap to fetch the pinned private
  CPython 3.10.19 arm64 build runtime and locked Python dependencies.

The distributed `.app` does not require Node, pnpm, Rust, Python, pip,
Homebrew, Git, Codex, or a listening service.

Generic GitHub-hosted macOS CI clears `externalBin` only for Rust compile/test,
because those runners are Intel and cannot package the v1 MLX runtime. The
self-hosted ARM64 workflow performs the actual sidecar and desktop bundle
build; it never substitutes an Intel OCR binary.

## Bootstrap and daily development

```bash
./scripts/bootstrap-macos.sh
pnpm dev:desktop
```

Bootstrap is idempotent, verifies prerequisites, installs locked JavaScript
dependencies and prepares both sidecars. It never installs system software or
packages into global Python. It downloads the build-only CPython runtime
declared in `tools/ocr-sidecar/python-runtime.json`, verifies its byte size and
SHA-256, and installs it under `.build/ocr-python/`. No Homebrew Python is
required. `AGENTIC_OS_OCR_PYTHON` remains an optional advanced override and is
accepted only when it points to native arm64 CPython 3.10.19.

Sidecar builds use `src-tauri/target/ocr-sidecar/venv/`. A fingerprint covers
source files, lockfile, build configuration, architecture and Python version.
An unchanged run prints `OCR sidecar up to date — skipping build`.

## Model lifecycle

The source of truth is `tools/ocr-sidecar/model-manifest.json`. The model is
installed under the Tauri application-data directory:

```text
models/paddleocr-vl/1.6/
```

Installation performs disk preflight, HTTPS-only per-file download, declared
size and SHA-256 validation, a verified marker write, and an atomic rename.
Partial files live under `models/.downloads/` and never count as installed.
Cancel, Remove and Repair operate only below the managed model root and reject
symlinks/path traversal. Removing the `.app` does not remove model data;
“Remove Document AI” does.

## Commands and events

Primary IPC commands use the `document_converter_*` prefix and cover status,
model install/cancel/remove/repair, file inspection, job create/get/list/retry,
cancellation, history, preview, assets, Finder and Memory import.

Events:

- `document-converter:model-progress`
- `document-converter:conversion-progress`
- `document-converter:conversion-completed`
- `document-converter:conversion-failed`

Sidecar commands are `health`, `capabilities`, `load-model`, `convert`,
`convert-image` (developer smoke), `cancel` and `shutdown`. Conversion
cancellation is enforced by Rust terminating the process, so it interrupts
active inference even when the MLX call itself is not cooperative.

## Output and persistence

For `AnnualReport.pdf`, the service writes into a UUID staging directory on
the destination filesystem, validates files, then atomically renames it:

```text
AnnualReport/
  AnnualReport.md
  document.json
  assets/
```

Collisions use `AnnualReport-2`, `AnnualReport-3`, and so on. SQLite table
`document_conversion_jobs` stores provenance, state, progress, errors and
paths, never source blobs or duplicate Markdown. On startup active jobs become
`failed / CONVERSION_INTERRUPTED`, queued jobs become cancelled, and known
temporary roots are cleaned.

## Tests

```bash
pnpm lint
pnpm test
pnpm check:native
pnpm test:ocr
pnpm build
pnpm build:desktop
```

`pnpm test:ocr` builds or reuses the self-contained sidecar, runs its health
check and exercises synthetic single-image and scanned multipage PDF paths
with a fake inference adapter. Real model inference is opt-in:

```bash
AGENTIC_OS_OCR_MODEL=/absolute/verified/model \
  pnpm test:ocr:integration
```

The reproducible performance suite uses synthetic datasets A–E and reports
model-load time, cold and warm time, seconds per page, selected processing
route, and sampled peak RSS as JSON. It never downloads a model:

```bash
pnpm benchmark:ocr -- --model /absolute/verified/model \
  --output /tmp/agentic-os-ocr-benchmark.json
```

## Offline acceptance test

1. Open Document Converter while online and install Document AI.
2. Confirm diagnostics show checksum `valid`.
3. Quit Agentic OS and disconnect all network interfaces.
4. Restart Agentic OS.
5. Convert a scanned PDF and verify `.md`, `document.json`, and assets.
6. Confirm no outbound connection was required.

The direct packaged-sidecar inference has passed an OS-level deny-network
profile. The complete UI lifecycle test must still be recorded on release
hardware; do not infer it from unit tests.

## Clean rebuild and repair

```bash
rm -rf src-tauri/target/ocr-sidecar
pnpm prepare:ocr
```

Do not manually delete broad Application Support directories. Use Repair or
Remove Document AI in the app so cleanup stays inside the managed root.

## Troubleshooting

- `MODEL_NOT_INSTALLED`: install Document AI from the page.
- `MODEL_CHECKSUM_FAILED`: use Repair; the invalid model is never loaded.
- `PROTOCOL_MISMATCH`: rebuild the sidecar and update app/model together.
- `ENCRYPTED_PDF`: password entry is not supported in v1.
- `INVALID_PDF`: the input failed PDF parsing before inference.
- `INSUFFICIENT_DISK_SPACE`: free enough space for download, installation,
  temporary files and the safety margin.
- `SIDECAR_CRASHED`: copy diagnostics; no OCR text is logged.

## Threat model

| Threat | Mitigation |
|---|---|
| Malicious filename/path traversal | Canonical inputs, sanitized stems, UUID temp paths, descendant validation, no shell interpolation. |
| Symlink cleanup attack | Managed-root proof and symlink rejection before recursive removal. |
| Corrupt/encrypted PDF | Explicit classifier errors before generic OCR failure. |
| Malicious Markdown | React-only renderer, no raw HTML, no automatic remote loads, local assets through validated IPC. |
| Corrupt model/supply chain | Pinned revision, HTTPS, per-file size/SHA-256, atomic verified marker. |
| Unexpected sidecar output | Protocol version, typed fields, maximum line size, fail-closed parsing. |
| Resource exhaustion | 2,000-page and 80 MP guards, 4 GB source guard, page rendering, one active OCR job, idle unload. |
| Data exfiltration | Offline runtime flags, no cloud fallback, no document content in logs/telemetry. |

## Signing notes

The PyInstaller one-file sidecar extracts its private Python/MLX runtime before
loading it. `src-tauri/Entitlements.plist` therefore grants the narrow
`com.apple.security.cs.disable-library-validation` entitlement to the signed
bundle executables; without it, Hardened Runtime rejects the extracted Python
framework because it does not carry the outer application's Team ID. Tauri
signs both sidecars before sealing the `.app`. Release signing still requires
the Developer ID and notarization secrets documented in the release workflow.

## Mac A → Mac B release gate

This must be executed on a genuinely separate clean Apple Silicon Mac:

```bash
git clone <repository>
cd agentic-os
./scripts/bootstrap-macos.sh
pnpm dev:desktop
```

Install the model through the UI and convert a scanned PDF. Do not copy venvs,
model caches, binaries or hidden files. Record hardware, OS, commands and
result in `docs/ocr-spike-results.md`. Until then, report this gate as NOT RUN.

## Signing and release

Generic CI runs frontend and Rust checks. Self-hosted ARM64 workflows package
the OCR sidecar and Tauri app. The release workflow expects GitHub Secrets
`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`. The build uses the pinned
private Python runtime automatically; `AGENTIC_OS_OCR_PYTHON` is only an
optional runner override. No credentials live in Git.

Before release, inspect and sign every Mach-O nested in the app/sidecar, run
`codesign --verify --deep --strict`, notarize, staple the DMG, and test it on a
Mac without developer tools. The current MLX native library declares macOS
26.2; changing supported OS requires rebuilding with a compatible pinned wheel
and re-running all packaging checks.
