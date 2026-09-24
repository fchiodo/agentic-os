# Document Converter — Phase 1/2 spike results

Date: 2026-09-24
Status: **Phase 1 and Phase 2 evidence collected; Phase 3 is gated**

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

No Tauri command, route, migration, capability, or `externalBin` entry was
added in this phase. Enabling a half-validated binary in every desktop build
would break the critical-gate rule and enlarge existing builds by about
160 MiB before the model lifecycle exists.

The existing Vite scripts listened on `0.0.0.0` without a documented LAN use
case. They now bind to `127.0.0.1`, and Tauri's `devUrl` uses the same explicit
loopback address, preserving the repository's no-exposed-ports constraint.

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
benchmark. Digital, mixed, multipage, rotated, corrupted/encrypted, complex
tables, formulas, and multicolumn golden fixtures remain untested.

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

The one-file executable has not yet been validated on a clean second Mac,
inside a signed/notarized `.app`, or through Apple's Hardened Runtime. Native
libraries still require recursive signing validation.

The current machine selected the MLX wheel tagged for macOS 26. Inspection of
the embedded source library with `otool` reports `libmlx.dylib` minimum OS
26.2, even though the outer PyInstaller executable reports 11.0 and the Python
framework reports 14.0. The wrapper's deployment target is therefore not the
effective runtime requirement. This development binary cannot be treated as a
macOS 14 release artifact. A release build must select the oldest supported
MLX wheel on a pinned build host and inspect every bundled Mach-O before
signing. The final minimum macOS version remains unresolved.

## Local-only validation

The generated sidecar loaded the pinned local model and converted the
synthetic page while launched under a macOS sandbox profile containing
`(deny network*)`. It completed in 7.150 seconds. The sidecar also sets
`HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`, and disables Hugging Face
telemetry before importing MLX-VLM.

This proves that the tested conversion did not require network access. It is
not a substitute for the required end-to-end test with Wi-Fi disabled, app
restart, Model Manager storage, and Tauri UI.

## Model and licensing observations

The pinned PaddleOCR-VL 1.6 snapshot contains 1,930,426,592 bytes across the
files required by the spike; `model.safetensors` is 1,917,255,968 bytes. The
model card and repository declare Apache-2.0. MLX and MLX-VLM declare MIT.

The model snapshot contains executable Python modeling/processing files. The
future Model Manager must verify every pinned file before load, not only the
weights file. A complete third-party-notice and commercial redistribution
review is still required before release.

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
| C — self-contained distribution | **PARTIAL PASS** | One-file binary runs without Python on the build Mac; current MLX native code has minimum OS 26.2, and clean Mac, codesigning, notarization, `.app` and `.dmg` are not tested. |
| D — offline after model install | **SPIKE PASS** | Real inference succeeded under OS-level network denial; full app lifecycle is not implemented. |
| E — Mac A → GitHub → Mac B | **NOT RUN** | No second clean Mac/runner was available. |

Per the requested gate policy, Phase 3 backend, UI, database migration, and
Memory integration must not be described as implemented. The next safe step
is to validate PP-DocLayoutV3 (or the official direct Paddle pipeline without a
listening service), run the test corpus on the target M4 Pro/24 GB machine, and
perform a clean-machine packaging/signing trial.
