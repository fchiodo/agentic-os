# Remove Audit Product Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Audit from the visible desktop product and public Tauri API while preserving the internal hash-chained Memory ledger.

**Architecture:** The React route and feature module are deleted, with the existing wildcard route redirecting stale `/audit` URLs to `/catalog`. Rust keeps `audit.rs` as an internal Memory dependency, but removes its UI-oriented list/read models and all public Audit commands.

**Tech Stack:** React 19, TypeScript, React Router, Vitest, Tauri 2, Rust, rusqlite.

## Global Constraints

- Keep Memory write proposals and their review flow.
- Keep the internal SQLite `audit` table and hash-chain helpers used by Memory and the Second Brain activity model.
- Do not expose an Audit route, sidebar item, frontend feature module, or Audit IPC command.
- Preserve the existing wildcard redirect to `/catalog`.

---

### Task 1: Remove the Audit navigation and route

**Files:**
- Modify: `src/components/layout/app-shell.test.tsx`
- Modify: `src/components/layout/app-shell.tsx`
- Modify: `src/app/router.tsx`
- Modify: `src/features/memory/orbit-map.test.tsx`
- Modify: `src/features/memory/orbit-map.tsx`
- Delete: `src/features/audit/audit-page.test.tsx`
- Delete: `src/features/audit/audit-page.tsx`
- Delete: `src/features/audit/api.ts`
- Delete: `src/features/audit/hooks.ts`
- Delete: `src/features/audit/mock-data.ts`
- Delete: `src/features/audit/schema.ts`
- Delete: `src/components/ui/trace-timeline.tsx`
- Delete: `src/lib/status.ts`

**Interfaces:**
- Consumes: existing `navigation` array and React Router child routes.
- Produces: primary navigation without Audit; `/audit` falls through to the existing wildcard redirect.

- [x] **Step 1: Write the failing navigation test**

Add this assertion beside the existing negative navigation assertions:

```tsx
expect(screen.queryByRole('link', { name: /audit/i })).not.toBeInTheDocument()
```

- [x] **Step 2: Run the focused test and verify the red state**

Run: `pnpm vitest run src/components/layout/app-shell.test.tsx`

Expected: FAIL because the Audit link is still rendered.

- [x] **Step 3: Remove the frontend surface**

Remove `History` and the Audit entry from `src/components/layout/app-shell.tsx`. Remove the `AuditPage` import and `/audit` child route from `src/app/router.tsx`. Remove Second Brain buttons that navigate to `/audit`, while retaining their internal activity evidence. Delete the Audit feature files and the now-unused trace and task-status UI helpers listed above.

- [x] **Step 4: Run the focused test and verify the green state**

Run: `pnpm vitest run src/components/layout/app-shell.test.tsx`

Expected: PASS, including the assertion that Audit is absent.

### Task 2: Remove the public Audit IPC boundary

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/control_models.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/audit.rs`

**Interfaces:**
- Consumes: internal `audit::append_row` and `audit::compute_content_hash` calls used by Memory, plus test-only chain verification.
- Produces: no `audit_runs`, `audit_trace`, or `audit_verify_chain` Tauri command and no UI response models.

- [x] **Step 1: Remove public commands and response models**

Delete `audit_runs`, `audit_trace`, and `audit_verify_chain` from `commands.rs` and `lib.rs`. Delete `AuditRunSummary`, `TraceEntry`, and `AuditChainStatus` from `control_models.rs`, including the now-unused `serde_json::Value` import.

- [x] **Step 2: Narrow the internal module**

Keep chain verification as a test-only boolean helper in `audit.rs`:

```rust
#[cfg(test)]
pub fn verify_chain(db: &Db) -> AppResult<bool>
```

Remove `list_runs` and `read_trace`. Update the Memory feedback test to query its recorded event directly. Keep `append_row` and `compute_content_hash` unchanged in behavior.

- [x] **Step 3: Compile the native boundary**

Run: `pnpm check:native`

Expected: PASS with no unresolved Audit UI models or commands.

### Task 3: Remove Audit-only presentation styles and update scope documentation

**Files:**
- Modify: `src/index.css`
- Modify: `README.md`
- Modify: `docs/UI-SPEC.md`
- Modify: `docs/ARCHITECTURE.md`

**Interfaces:**
- Consumes: current active navigation documentation.
- Produces: documentation listing Catalog, Memory, and Document Converter as the active product surfaces; Audit is identified only as an internal Memory ledger where technically relevant.

- [x] **Step 1: Delete Audit-only CSS**

Remove `.audit-grid`, `.audit-run-list`, `.audit-run-row`, `.audit-trace-panel`, `.trace-timeline`, and `.trace-row*` rules, including the Audit media-query override. Preserve adjacent `.diff-*` rules used by Memory proposals.

- [x] **Step 2: Update current-product documentation**

Remove Audit from the README primary views. Update the implementation notes in `docs/UI-SPEC.md` and `docs/ARCHITECTURE.md` to state that Runner, Approvals, Usage, and Audit product surfaces are retired while the Memory ledger remains internal.

- [x] **Step 3: Verify no public Audit surface remains**

Run:

```bash
rg -n "(/audit|audit_runs|audit_trace|audit_verify_chain|features/audit)" src src-tauri/src --glob '!**/target/**'
```

Expected: no production-code matches. Test assertions may mention `/audit` or the Audit label only to verify absence.

### Task 4: Full verification

**Files:**
- Verify only; no planned source changes.

**Interfaces:**
- Consumes: completed Tasks 1–3.
- Produces: fresh evidence that frontend, native code, migrations, and formatting remain valid.

- [x] **Step 1: Run frontend checks**

Run: `pnpm lint && pnpm test && pnpm build`

Expected: exit code 0 for all commands.

- [x] **Step 2: Run native checks**

Run: `pnpm check:native && cargo test --manifest-path src-tauri/Cargo.toml`

Expected: exit code 0; the registry-dependent model test may remain ignored.

- [x] **Step 3: Check the patch**

Run: `git diff --check`

Expected: no output and exit code 0.
