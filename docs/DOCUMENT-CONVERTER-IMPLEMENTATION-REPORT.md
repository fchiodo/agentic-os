# Document Converter implementation report

Date: 2026-09-24

## IMPLEMENTED

- Document Converter route/sidebar UI, model first-run/manage states, batch
  queue, real progress events, cancellation controls, history, safe Markdown
  preview, Finder actions, diagnostics, and Memory import.
- Rust `DocumentConversionService` boundary with a dynamic `OcrEngine`,
  `PaddleOcrEngine`, page-aware classifier, one-job manager, model manager,
  canonical document model, Markdown/JSON renderers, SQLite persistence,
  recovery, collision-safe atomic output, provenance, and fingerprints.
- Separate protocol-v1 OCR sidecar, cached reproducible PyInstaller build,
  direct local MLX-VLM inference, page-oriented PDFium rendering, and no
  listening service or cloud fallback.
- Generic CI, self-hosted ARM64 packaging workflow, prepared signed release
  workflow, synthetic fixtures, benchmark command, runbook, ADR, licenses,
  and idempotent macOS bootstrap.

## ARCHITECTURE

```text
React → typed Tauri IPC/events → Rust DocumentConversionService
  → Job Manager / Model Manager / page-aware Classifier
  → dyn OcrEngine → PaddleOcrEngine → JSONL sidecar
  → PaddleOCR-VL 1.6 on MLX → Canonical Document Model
  → Markdown + document.json + deterministic assets → optional Memory import
```

The direct MLX-VLM adapter is an intentional deviation from Paddle's loopback
HTTP-server example so Agentic OS keeps its no-listening-ports property.

## VERSIONS

- PaddleOCR Python package: not used by the runtime; inference uses the
  converted PaddleOCR-VL weights directly through MLX-VLM.
- PaddleOCR-VL: 1.6, revision
  `c5630abae1d940eafe0697512a0325494b02ab42`.
- MLX: 0.32.2.
- MLX-VLM: 0.7.2.
- Python build runtime: CPython 3.10.19 arm64.
- PyInstaller: 6.21.0.
- OCR sidecar: 0.2.1.
- OCR protocol: 1.
- Canonical output schema: 1.
- Rust/Cargo: 1.98.1.
- Current minimum macOS: 26.2, imposed by the pinned MLX native library.

## MODEL

- Version: PaddleOCR-VL 1.6 at the pinned immutable revision above.
- Download size: 1,930,426,592 bytes (about 1.80 GiB).
- Installed declared size: 1,930,426,592 bytes plus the small verified marker.
- Primary weights SHA-256:
  `85a479d506a11e724e7285d395c551be69f41dbc16b6342d3cacfb189aed71db`.
- Verification: size and SHA-256 are pinned for all 15 required model files.
- License: Apache-2.0 according to the pinned model repository; engineering
  notices are in `THIRD_PARTY_NOTICES.md`. Commercial release still requires
  final legal/SBOM review.

## PERFORMANCE — M4 Pro 24 GB

- Cold start: **NOT RUN** on M4 Pro 24 GB.
- Warm start: **NOT RUN** on M4 Pro 24 GB.
- Scanned seconds/page: **NOT RUN** on M4 Pro 24 GB.
- Peak memory: **NOT RUN** on M4 Pro 24 GB.

Observed spike baseline on the available Apple M2 / 8 GB machine:

- synthetic whole-page OCR: 6.902 s;
- packaged real OCR fixture: 4.873 s;
- same-process capped comparison: 6.403 s cold / 6.178 s warm;
- observed process max RSS: 1,536,458,752 bytes;
- observed peak footprint: 3,663,011,840 bytes in the three-case run.
- complete 10-page scanned conversion through the Tauri UI: 96.34 s total
  (9.63 s/page including sidecar/model startup), 10/10 pages OCR.

Use `pnpm benchmark:ocr -- --model <verified-model>` for datasets A–E and an
archivable cold/warm JSON result on release hardware.

## TESTED

- `pnpm lint`: PASS.
- `pnpm test`: PASS, 31 frontend tests.
- `pnpm check:native`: PASS.
- `cargo test --manifest-path src-tauri/Cargo.toml`: PASS, 104 tests, including
  15 Document Converter tests.
- `pnpm test:ocr`: PASS, 13 tests; the real-model checksum test is intentionally
  skipped when no installed model path is supplied. The final validation
  supplied the installed model path, so all 15 model files were hash-checked.
- `pnpm test:ocr:integration`: PASS against the packaged sidecar and real model;
  expected English/Italian fixture text was recognized in 4.054 s.
- `pnpm build:desktop`: PASS.
- Bootstrap executed twice consecutively: PASS; second run reused both
  sidecars.
- `pnpm dev:desktop` startup and shutdown: PASS; no sidecar process remained.
- UI layout/accessibility tree at the Document Converter route: PASS.
- Synthetic digital/scanned/mixed classifier routing: PASS.
- Orientation normalization at 0°, 90°, 180°, and 270°: PASS.
- Packaged OCR sidecar health from the ad-hoc Hardened Runtime `.app`: PASS.
- Model installation from the actual Document Converter UI: PASS; byte progress,
  atomic installation, and final checksum-valid Ready state were observed. A
  transient late download failure discovered during the first attempt led to
  bounded per-file retries and redacted public network errors before retest.
- Full Tauri UI scanned-PDF conversion: PASS; the 10-page package contains
  canonical Markdown and schema-v1 `document.json`, and history persisted in
  SQLite.
- Markdown preview and Copy Markdown: PASS; clipboard bytes matched the
  generated `.md` file exactly.
- Duplicate detection: PASS; a second identical request was identified before
  being explicitly queued.
- Cancellation state, temp/output cleanup, and page count: PASS at page 3/10.
  The packaged 0.2.1 process-group cancellation smoke additionally confirmed
  that the PyInstaller launcher, MLX runtime, and workers are reaped together.
- Installed `.app` restart: PASS; the verified model, completed/cancelled
  history, and canonical output remained available, and startup did not launch
  the OCR sidecar.
- Recursive ad-hoc code-sign verification: PASS.
- DMG checksum verification: PASS.
- Real local inference under OS-level deny-network sandbox: PASS with the final
  packaged 0.2.1 sidecar and installed model.

The following requested manual application scenarios are **NOT RUN** in the
complete Tauri UI on this machine: end-to-end digital, mixed, complex-table and
PNG/JPEG conversion; corrupt/encrypted PDF UX; model remove/reinstall; full
offline application restart; and recovery from an interrupted persisted job.
Automated tests cover their contracts and state transitions but are not
reported as manual passes.

## MAC A → MAC B

- Mac A commit and push to `origin/master`: **PASS**.
- Clean Mac B clone/bootstrap/model-install/conversion: **NOT RUN**. No second
  clean Apple Silicon Mac or equivalent clean ARM64 runner was available. No
  `.venv`, model cache, generated sidecar, or hidden environment was copied as
  a substitute. The bootstrap and self-hosted ARM64 workflow are prepared for
  this gate.

## OFFLINE OCR

- **PACKAGED INFERENCE PASS / FULL APP RESTART NOT RUN**. Final packaged 0.2.1
  inference passed with OS network access denied and offline runtime flags
  enabled. The exact install-model → physically disable Wi-Fi → restart app →
  convert sequence remains a release-hardware test.

## DMG BUILD

- **PASS (local ad-hoc signing)**.
- Artifact: `src-tauri/target/release/bundle/dmg/Agentic OS_0.1.0_aarch64.dmg`.
- Size: 226,325,893 bytes.
- SHA-256:
  `c8a28a937e11c0ab0e524a0582b1b345d1a26dab536ef5c1d370755029e86057`.
- Developer ID signing: **NOT RUN** (credentials unavailable).
- Apple notarization/stapling: **NOT RUN** (credentials unavailable).

## KNOWN LIMITATIONS

- Representative-corpus quality, especially authoritative bounding boxes,
  complex/continued tables, formulas, and difficult multicolumn reading order,
  remains Gate A work. PP-DocLayoutV3 was not added without a verified MLX
  artifact and license/provenance chain.
- The current pinned MLX wheel raises minimum macOS to 26.2.
- v1 is Apple Silicon-only and intentionally runs one OCR job at a time.
- Model download resume is not enabled; cancelled/incomplete staging data is
  cleaned rather than trusted.
- Global CSP remains disabled as pre-existing technical debt. The converter's
  preview independently rejects raw HTML, JavaScript, implicit remote loads,
  arbitrary local paths, and unvalidated assets.
