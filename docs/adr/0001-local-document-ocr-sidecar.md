# ADR 0001: Local OCR sidecar behind an engine abstraction

- Status: Proposed, validated by Phase 1/2 spike
- Date: 2026-09-24

## Context

Agentic OS needs scanned-document conversion without sending document content
to Codex, OpenAI, or an OCR service. The existing MarkItDown sidecar is useful
for digital documents but does not provide OCR/document understanding. MLX and
PaddleOCR-VL have a large Python/native dependency graph and should not become
part of the Rust control plane or the Memory feature.

## Decision

Use a dedicated, self-contained local OCR sidecar over a versioned JSON Lines
stdio protocol. Rust owns its lifecycle and exposes one public
`DocumentConversionService`. A Rust `OcrEngine` boundary will adapt engine
output into a versioned Canonical Document Model before rendering Markdown,
JSON, and assets.

The first adapter is `PaddleOcrEngine`. MarkItDown remains a separate digital
extraction implementation selected by the backend classifier. React and
Memory never choose between them directly.

Do not run the MLX-VLM HTTP server described by the upstream Apple Silicon
integration. Direct in-process MLX-VLM calls preserve Agentic OS's no-listening
ports invariant. Models remain outside the `.app` and are installed by a Rust
Model Manager only after HTTPS size and SHA-256 verification.

## Consequences

- Conversion can run offline after the model is installed.
- The end-user bundle can include a controlled runtime without requiring
  Python, pip, Conda, or Homebrew.
- Sidecar startup and memory must be managed explicitly, with one active OCR
  job and an idle shutdown policy in v1.
- JSONL is an internal versioned API; incompatible protocol/model versions
  must fail closed.
- Paddle-specific fields stay in namespaced metadata rather than leaking into
  the public IPC or UI.
- Signing/notarization must cover the sidecar and all embedded native code.
- Layout-model provenance and clean-machine packaging remain release gates.

## Rejected alternatives

- Cloud OCR or silent cloud fallback: violates privacy and offline operation.
- OCR inside Memory: duplicates the pipeline and couples source conversion to
  one consumer.
- Extending the MarkItDown sidecar into a monolith: mixes a small digital
  extractor with a large, warmable ML runtime and makes failures harder to
  isolate.
- A localhost MLX service: conflicts with the repository's Tauri-IPC-only
  trust boundary.
- Installing Python packages at application startup: not reproducible or
  acceptable for the `.app`/`.dmg` user experience.
