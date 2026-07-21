# Second Brain production runbook

This runbook covers the Phase 2 memory subsystem shipped by Agentic OS. The
control plane is local-only: Markdown is authoritative, SQLite/FTS5 is derived,
and the vault is an independent Git repository.

## Storage and startup

- Default vault: `~/AgenticOS/vault`.
- Override for managed installations: `AGENTIC_OS_VAULT_ROOT`.
- Distilled skills default to `~/.codex/skills`; override with
  `AGENTIC_OS_SKILLS_ROOT`.
- Startup creates all six domain roots, initializes the vault Git repository,
  migrates the memory tables, rebuilds FTS from Markdown, and starts the daily
  lifecycle sweep.
- Startup fails visibly when the vault, Git, or index cannot be initialized;
  the app never continues with a silently degraded memory layer.

The configured vault must be outside task workspaces. It must remain on a
local disk with enough free space for Git history. A private encrypted backup
may back up the whole vault, but the control plane never configures a public
remote.

## Admission paths

All content follows one of these three Tauri commands:

- `memory_save_manual`: one user-authored candidate.
- `memory_ingest`: 1–10 typed candidates from a read-only extractor or source
  connector. `source` must be namespaced, for example
  `outlook:<message-id>`, `slack:<thread-id>`, `meeting:<vault-path>`, or
  `confluence:<page-id>`.
- `memory_import_document`: a complete pasted, uploaded, or remotely fetched
  text document plus up to 10 locally extracted, typed candidates. The source
  snapshot is saved before extraction; candidates never auto-apply.

Both commands execute the same pure-Rust gate: domain validation, secret
detection, injection heuristics, provenance, durability, attribution,
sensitivity, deduplication, and approval policy. Connector code never writes
Markdown directly.

## Import document

Use **Memory → Import document** when a source is too long for one atomic
memory, for example API documentation, an email thread export, meeting notes,
or a Confluence page snapshot.

1. Choose the destination domain and a stable title.
2. Paste text, select a UTF-8 text file, or provide the final public URL.
3. The complete body is preserved under
   `_sources/<domain>/<date>-<slug>-<id>.md`, including capture metadata and a
   SHA-256 content hash. It is committed to the vault Git history and audited.
4. A deterministic local extractor ranks self-contained claims and creates at
   most 10 atomic fact/decision proposals. Every candidate links back to the
   source snapshot with a wiki link.
5. Review every proposed fact in Governance. Nothing extracted from a
   document reaches searchable memory before approval.

Limits and safety rules:

- Maximum decoded source size: 2 MiB; no silent truncation.
- Text/file imports must be UTF-8. Supported file picker formats include
  Markdown/MDX, TXT, JSON, YAML, XML, and HTML.
- Remote imports accept HTTP(S) text only, use a 20-second total timeout, do
  not follow redirects, and reject loopback, private, link-local, reserved,
  credential-bearing, or binary targets. Import the final redirect URL
  explicitly.
- Secret detection runs before the source is written. Documentation
  placeholders such as `YOUR_API_TOKEN` are allowed; credential-shaped values
  are rejected.
- Source text remains untrusted data. Prompt-injection-like language is
  recorded as a warning, and every extracted candidate still passes the normal
  admission gate.
- The source history is read with `memory_document_imports_list`; an exact
  snapshot is retrieved with `memory_document_source_read`. Status is
  `pending`, `partial`, `completed`, or `no_candidates` according to proposal
  decisions.

## Persistence guarantee

The apply path snapshots existing files and rows, atomically renames the new
Markdown file, creates a verified Git commit, updates SQLite and FTS, updates
the proposal, and appends a hash-chained audit row. A failure triggers
compensating restoration of the prior file/index/proposal state and a rollback
Git commit. Errors are returned to the UI.

Updates retain the immutable memory ID and vault path. Supersedes create a new
ID, close `valid_until` in the old Markdown frontmatter, and mark the old row
stale. Historical truth is never overwritten or deleted.

Pending update, supersede, and skill proposals carry the hash of the source
document they reviewed. Approval compares that hash under the serialized write
lock; a concurrent edit produces a conflict and leaves the proposal pending
instead of overwriting newer content.

## Approval policy

- Normal `work`/`research` create and update: auto-apply and show in Activity.
- Sensitive content: approval.
- `personal`, `family`, `finance`, and `planphysique`: approval.
- Supersede: approval.
- Distilled skill: approval.
- Inferred preference: confidence capped at 0.5 and approval.
- Every document-import candidate: approval, including normal `work` and
  `research` facts.

## Retrieval and answers

Search applies the domain/status filter in SQL, checks exact titles first,
then ranks FTS candidates by relevance, recency, trust, and stale penalty.
`memory_ask` is deliberately extractive: it returns cited excerpts and
abstains when no evidence exists. It never fabricates a fluent answer without
supporting vault files.

Task context loads full bodies up to the 4,000-token-equivalent character
budget, compresses overflow entries, excludes sensitive memories, escapes data
block delimiters, and labels stale entries `UNVERIFIED`. Every injected path is
recorded in the task trace.

## Lifecycle and recovery

- Run episodes expire after 30 days; other episodes default to 90 days.
- Expired files move to `_archive/<domain>/<original-subpath>`, receive a Git
  commit, leave FTS, and retain their SQLite provenance row.
- Facts stale after 180 days; entities/preferences after 365 days; decisions
  stale only through supersession.
- `Confirm still true` updates Markdown first, commits, updates the index, and
  audits the confirmation.
- Use **Reindex** after an intentional hand edit. Drift is reported; invalid
  frontmatter or a domain/path mismatch fails rather than being ignored.
- Use **Maintenance** for an immediate TTL/staleness catch-up.
- To recover operator mistakes, inspect `git log` inside the vault and restore
  the required commit, then run Reindex. Do not edit SQLite as a recovery path.

## Release verification

Run before packaging:

```bash
pnpm build
pnpm lint
pnpm vitest run
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

The native suite covers FTS escaping, domain isolation, secret rejection,
identity-preserving update, supersession, approvals, staleness, expiry/archive,
reindex durability, context safety, run capture, bounded connector ingestion,
grounded answers/abstention, optimistic approval conflicts, IPC serialization,
document snapshot durability, forced import approval, secret rejection, SSRF
address filtering, and skill approval.
