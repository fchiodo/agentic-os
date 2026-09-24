# Document Converter — Phase 1/2 spike results

Date: 2026-09-24
Status: **Phase 1/2 evidence collected; application integration implemented; external release gates remain**

## Decision before implementation

The repository already uses a small PyInstaller one-file executable for
MarkItDown, Tauri commands registered in `lib.rs`, a shared `Db` wrapper over
rusqlite, feature-owned React modules, React Router, TanStack Query, explicit
Tauri v2 capabilities, and debug-only `tauri-plugin-log`. The OCR work should
follow those patterns, but must remain a separate sidecar and bounded Rust
module.

The official Apple Silicon PaddleOCR-VL integration starts an MLX-VLM HTTP
service on loopback. Agentic OS explicitly treats "no listening ports" as a
security property. The spike therefore uses MLX-VLM directly inside the JSONL
sidecar instead of starting that service. This is the only architectural
deviation made in Phase 1/2. PaddleOCR's full document pipeline and
PP-DocLayoutV3 were not embedded or replaced.

After the spike evidence was reviewed, the user explicitly requested complete
application integration. The repository now includes the Tauri commands,
route, SQLite migration, model/job managers, `OcrEngine` adapter, canonical
model/renderers, lifecycle ownership and OCR `externalBin`. This does not turn
unexecuted hardware or clean-machine release gates into passes.

The existing Vite scripts listened on `0.0.0.0` without a documented LAN use
case. They now bind to `127.0.0.1`, and Tauri's `devUrl` uses the same explicit
loopback address, preserving the repository's no-exposed-ports constraint.

The repository still declared Rust 1.77.2, but its already-locked Tauri 2.11.2
dependency graph contains Edition 2024 crates that Cargo 1.77.2 cannot parse.
That exact command was tested and failed at manifest parsing before compiling
Agentic OS. The developer toolchain is therefore pinned to the actually tested
Rust/Cargo 1.98.1 in `rust-toolchain.toml`; bootstrap rejects drift instead of
silently using whichever compiler happens to be installed.

## Environment actually tested

| Item | Observed value |
|---|---|
| Hardware | Apple M2, 8 GB unified memory |
| Requested reference hardware | M4 Pro, 24 GB — **not available** |
| OS | macOS 26.4.1, arm64 |
| Python build runtime | CPython 3.10.19, native arm64 |
| MLX | 0.32.2 |
| MLX-VLM | 0.7.2 |
| Transformers | 5.17.0 |
| PyInstaller | 6.21.0 |
| PDF renderer used in spike | pypdfium2 5.13.0 |
| Sidecar protocol | 1 |
| PaddleOCR-VL model | 1.6, repository revision `c5630abae1d940eafe0697512a0325494b02ab42` |

The Python dependency graph is pinned with hashes in
`tools/ocr-sidecar/requirements.lock`. The model revision, per-file sizes, and
SHA-256 values are pinned in `tools/ocr-sidecar/model-manifest.json`.

## Inference results

### Synthetic scanned page

The test page contained English and Italian text, headings, bullets, a simple
table, and `E = mc²`. It was saved as an image-only PDF and rendered back to a
1400 × 1800 PNG.

- Whole-page OCR generation: 6.902 seconds.
- Table crop: 1.859 seconds.
- Formula crop: 0.899 seconds.
- The whole-page result retained order, headings, bullets, all table values,
  and the formula.
- The table was rendered as valid Markdown.
- One observed text error: `riproducibile` became `riproducible`.
- The table-specific prompt returned Paddle cell tokens, so it still needs an
  adapter before it can enter the Canonical Document Model.
- The formula-specific prompt returned LaTeX-like output but also included the
  nearby "Formula" label; region/layout separation remains necessary.

The generated 160 MiB sidecar from `scripts/build-ocr-sidecar.mjs` passed a
second real fixture in 4.873 seconds and returned the expected English and
Italian lines.

### Official demo image

- 1524 × 1368 image, maximum 1024 output tokens.
- OCR output was coherent and accurate on the visible Chinese newspaper text.
- First measured process duration: 68.53 seconds, including initial model
  acquisition/load work.
- Same-process generation with the model resident: 6.403 seconds cold versus
  6.178 seconds warm for the capped comparison.

These are engineering baselines, not a statistically meaningful quality
benchmark. Synthetic multipage PDF parsing, invalid/unsupported inputs,
classifier heuristics and output structure now have automated tests. Mixed,
rotated, complex tables/formulas and multicolumn quality still need the manual
release corpus.

## Memory observations

On the M2/8 GB machine:

- observed process max RSS: 1,536,458,752 bytes on the first demo run;
- observed peak memory footprint: 3,492,858,520 bytes;
- synthetic three-case run peak footprint: 3,663,011,840 bytes;
- packaged one-file process max RSS: about 1.06 GB, while the Metal/process
  peak metric from `/usr/bin/time` was not reliable enough to report as a
  comparable peak.

No M4 Pro/24 GB memory measurement was performed.

## Packaging result

PyInstaller one-file initially completed inference but emitted a Python
resource-tracker traceback during shutdown. Adding
`multiprocessing.freeze_support()` removed the failure. The final lean build:

- is a 160 MiB arm64 Mach-O executable;
- includes the private Python runtime and MLX dependencies;
- runs with `PATH=/usr/bin:/bin` and no project Python on `PATH`;
- returns machine-readable health/capabilities on stdout;
- uses stderr only for diagnostics;
- completed real OCR and exited cleanly;
- is cached by a fingerprint covering sources, lockfile, build script,
  architecture, target triple, and Python version;
- prints `OCR sidecar up to date — skipping build` on a cache hit.

The one-file executable has not yet been validated on a clean second Mac or
inside a Developer ID-signed and notarized `.app`. Native libraries still
require that release-identity validation before distribution.

Local ad-hoc Hardened Runtime validation initially reproduced a library
validation failure when the one-file sidecar extracted its private Python
framework. The final bundle applies the narrow
`com.apple.security.cs.disable-library-validation` entitlement, signs both
sidecars before sealing the app, and re-runs health from the signed `.app`.
Developer ID signing, notarization and a clean second Mac remain external
release gates.

The current machine selected the MLX wheel tagged for macOS 26. Inspection of
the embedded source library with `otool` reports `libmlx.dylib` minimum OS
26.2, even though the outer PyInstaller executable reports 11.0 and the Python
framework reports 14.0. The wrapper's deployment target is therefore not the
effective runtime requirement. This development binary cannot be treated as a
macOS 14 release artifact. The current v1 bundle now declares macOS 26.2 as
its minimum. A future attempt to lower that requirement must select an older
compatible MLX wheel on a pinned build host and inspect every bundled Mach-O
before signing.

## Local-only validation

The generated sidecar loaded the pinned local model and converted the
synthetic page while launched under a macOS sandbox profile containing
`(deny network*)`. It completed in 7.150 seconds. The sidecar also sets
`HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`, and disables Hugging Face
telemetry before importing MLX-VLM.

This proves that the tested conversion did not require network access. It is
not a substitute for the required end-to-end test with Wi-Fi disabled, app
restart, Model Manager storage, and Tauri UI.

## Application integration result

Implemented after the original spike:

- autonomous Document Converter route and sidebar entry;
- first-use model install/repair/remove UX with manifest-derived size;
- Rust model manager with HTTPS, disk preflight, per-file size/SHA-256 and
  atomic install;
- versioned typed JSONL protocol and sidecar health compatibility checks;
- one-job OCR queue, batch selection, real process cancellation, idle unload
  and startup recovery;
- digital/OCR/hybrid routing, source hash and deterministic fingerprint;
- canonical document JSON schema v1 plus Markdown renderer and atomic package;
- SQLite history, duplicates, retry, preview, Finder and Memory import;
- safe React Markdown preview without raw HTML or implicit remote loads;
- frontend, Rust and Python tests, generic CI and self-hosted ARM64 workflows.

On 2026-09-24 the updated 0.2.1 sidecar rebuilt as a 163 MiB arm64 one-file
executable. Its health check and 13 Python tests passed, including a synthetic
two-page scanned PDF. Fourteen Document Converter Rust tests and 31 frontend
tests also passed. Full commands and current limitations are in the runbook.

## Model and licensing observations

The pinned PaddleOCR-VL 1.6 snapshot contains 1,930,426,592 bytes across the
files required by the spike; `model.safetensors` is 1,917,255,968 bytes. The
model card and repository declare Apache-2.0. MLX and MLX-VLM declare MIT.

The model snapshot contains executable Python modeling/processing files. The
implemented Model Manager verifies every pinned file before load, not only the
weights file. `THIRD_PARTY_NOTICES.md` records the identified licenses; a
commercial redistribution review is still required before release.

## PP-DocLayoutV3 finding

PaddleOCR-VL alone produced useful Markdown-like text, tables, and formulas,
but the required bounding boxes, block labels, and authoritative reading order
belong to the document-layout stage. Paddle documents PP-DocLayoutV3 for this
purpose. The available MLX conversion located during the spike is
community-published, and its download attempt was rate-limited by the model
registry before validation. Its provenance, exact conversion recipe, hashes,
license chain, and quality are therefore unresolved.

This is a Gate A blocker for the requested Canonical Document Model, not a
reason to fake bounding boxes or derive structure from OCR text heuristics.

## Critical gates

| Gate | Result | Evidence / blocker |
|---|---|---|
| A — acceptable document quality | **PARTIAL** | Simple bilingual scan/table/formula succeeded; layout model and representative corpus not validated. |
| B — stable on M4 Pro 24 GB | **NOT RUN** | Only M2/8 GB was available. |
| C — self-contained distribution | **PARTIAL PASS** | One-file binary runs without Python; a Tauri `.app` and 216 MiB DMG were built, recursively ad-hoc signed with Hardened Runtime, verified, and the signed bundled sidecar passed health. Developer ID signing, notarization and a clean second Mac are not tested. |
| D — offline after model install | **SPIKE PASS** | Real inference succeeded under OS-level network denial; full app UI/restart acceptance remains NOT RUN. |
| E — Mac A → GitHub → Mac B | **NOT RUN** | No second clean Mac/runner was available. |

Backend, UI, database migration and Memory integration are now implemented by
explicit user direction. Gate A quality depth, Gate B reference hardware,
clean-Mac Gate C, full lifecycle Gate D and Gate E remain accurately marked;
the next release work is representative-corpus validation on M4 Pro/24 GB and
a clean-machine signed/notarized DMG trial.
