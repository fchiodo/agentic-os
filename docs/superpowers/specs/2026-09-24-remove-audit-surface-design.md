# Remove Audit Product Surface

Date: 2026-09-24

## Goal

Remove Audit from the desktop product after the retirement of Runner,
Approvals, and Usage.

## Scope

- Remove the Audit sidebar item and `/audit` route.
- Remove the Audit page, frontend query hooks, schemas, mock data, and UI-only
  trace components.
- Remove the public `audit_runs`, `audit_trace`, and `audit_verify_chain` Tauri
  commands and their response models.
- Redirect stale `/audit` URLs to `/catalog` through the existing wildcard
  route.
- Remove Audit from current-product documentation.

## Internal boundary

Keep the local hash-chained event ledger as an internal Memory service. Memory
uses it for write integrity, operation recovery, provenance, feedback, and the
Second Brain activity model. It will have no route, sidebar entry, or public
Audit IPC API.

The existing `audit` module and SQLite table remain internal implementation
details. Historical task rows can remain for compatibility, but users cannot
browse them through an Audit product surface.

## Validation

- A frontend test verifies that Audit is absent from primary navigation.
- Source scans verify that `/audit` and the three Audit IPC command names are
  absent from production code.
- Frontend lint, tests, and build pass.
- `cargo check` and the complete Rust test suite pass.
- `git diff --check` reports no whitespace errors.
