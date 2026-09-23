# Memory Page UI Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refresh the Memory page metrics strip, Ask/Search control panel, and Governance rail without changing Memory behavior or backend contracts.

**Architecture:** Keep the implementation local to the existing Memory feature by reshaping `memory-page.tsx` into a few small local helper sections and adding CSS in `src/index.css`. Preserve the current hooks, mutations, and Tauri contracts; only the page composition, copy hierarchy, and local derivations change.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, global CSS in `src/index.css`, Lucide React.

## Global Constraints

- Reuse the existing visual vocabulary in `src/index.css` and the repo-level UI guidance from `docs/UI-SPEC.md`.
- Stay inside the current feature boundary: `src/features/memory/memory-page.tsx`, `src/index.css`, `src/features/memory/memory-page.test.tsx`.
- No new backend command for metrics in this pass.
- No behavior change to `Ask`, `Search`, imports, approvals, or proposal decisions.
- Metric strip is look-only: use data already available to the page or existing frontend queries, but do not add a new summary endpoint.

---

### Task 1: Lock the new Memory page shell with failing tests

**Files:**
- Modify: `src/features/memory/memory-page.test.tsx`
- Test: `src/features/memory/memory-page.test.tsx`

**Interfaces:**
- Consumes: `MemoryPage`
- Produces: test coverage for the page-level metrics strip and refreshed Governance shell

- [ ] **Step 1: Write the failing test**

```tsx
it('renders the refreshed page-level memory metrics and governance shell', async () => {
  renderPage()

  expect(screen.getByLabelText('Memory metrics')).toBeInTheDocument()
  expect(screen.getByText('Vault items')).toBeInTheDocument()
  expect(screen.getByText('Pending review')).toBeInTheDocument()
  expect(screen.getByText('Reviewed writes')).toBeInTheDocument()
  expect(screen.getByText('Total writes')).toBeInTheDocument()
  expect(screen.getByText('Governance')).toBeInTheDocument()
  expect(
    screen.getByText('Sensitive and truth-changing writes appear here before they are committed.'),
  ).toBeInTheDocument()
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/features/memory/memory-page.test.tsx -t "renders the refreshed page-level memory metrics and governance shell"`

Expected: FAIL because the current page does not render the new metrics strip or Governance footer copy.

- [ ] **Step 3: Commit**

```bash
git add src/features/memory/memory-page.test.tsx
git commit -m "test: capture memory page refresh shell"
```

### Task 2: Implement the refreshed page composition in MemoryPage

**Files:**
- Modify: `src/features/memory/memory-page.tsx`
- Test: `src/features/memory/memory-page.test.tsx`

**Interfaces:**
- Consumes: `useMemoryTree`, `useMemorySearch`, `useMemoryProposals`, `useMemoryAsk`, `useMemorySaveManual`, existing Memory local state
- Produces:
  - `MemoryMetricsStrip` local helper
  - refreshed control panel markup for Ask/Search
  - refreshed Governance rail markup
  - proposal title derivation helper

- [ ] **Step 1: Write the minimal helper boundaries**

Add local helpers in `src/features/memory/memory-page.tsx`:

```tsx
function countVaultFiles(nodes: VaultNode[]): number
function proposalTitle(proposal: MemoryWriteProposal): string
function MemoryMetricsStrip(props: {
  pendingCount: number
  reviewedCount: number
  totalCount: number
  vaultItemCount: number
}): JSX.Element
```

- [ ] **Step 2: Reshape the page markup**

Implement:

```tsx
<section aria-label="Memory metrics" className="memory-metric-strip">...</section>
```

and replace the current `memory-mode-bar` + standalone search bar with a unified control surface that still uses:

```tsx
setMode('search')
setMode('ask')
setIncludeStale(event.target.checked)
setShowImporter(true)
setShowComposer(true)
```

- [ ] **Step 3: Refresh Governance markup**

Render:

```tsx
<aside className="memory-governance-rail surface">
  <div className="memory-governance-header">...</div>
  <div className="memory-governance-tabs">...</div>
  {railTab === 'pending' && pending.length > 0 ? <div className="memory-governance-banner">...</div> : null}
  <div className="memory-governance-list">...</div>
  <p className="memory-governance-footer">Sensitive and truth-changing writes appear here before they are committed.</p>
</aside>
```

- [ ] **Step 4: Run targeted tests**

Run: `pnpm vitest run src/features/memory/memory-page.test.tsx`

Expected: PASS for the refreshed shell test and no regressions in the existing Memory page tests.

- [ ] **Step 5: Commit**

```bash
git add src/features/memory/memory-page.tsx src/features/memory/memory-page.test.tsx
git commit -m "feat: refresh memory page layout"
```

### Task 3: Add the page styling and responsive behavior

**Files:**
- Modify: `src/index.css`
- Test: `src/features/memory/memory-page.test.tsx`

**Interfaces:**
- Consumes: new `memory-page.tsx` class names
- Produces: cohesive styles for the metric strip, control panel, and Governance rail across desktop/tablet/mobile breakpoints

- [ ] **Step 1: Add the new style blocks**

Add CSS blocks for:

```css
.memory-metric-strip {}
.memory-metric-cell {}
.memory-control-panel {}
.memory-control-topbar {}
.memory-control-form {}
.memory-governance-rail {}
.memory-governance-header {}
.memory-governance-banner {}
.memory-governance-footer {}
.proposal-card--editorial {}
```

- [ ] **Step 2: Update responsive rules**

Ensure the existing Memory media queries adapt the new surfaces:

```css
@media (max-width: 1100px) { ... }
@media (max-width: 768px) { ... }
```

- [ ] **Step 3: Run verification**

Run:

```bash
pnpm vitest run src/features/memory/memory-page.test.tsx
pnpm build
```

Expected:
- Vitest passes
- `pnpm build` completes without TypeScript or Vite errors

- [ ] **Step 4: Commit**

```bash
git add src/index.css src/features/memory/memory-page.tsx src/features/memory/memory-page.test.tsx
git commit -m "style: polish memory page refresh"
```
