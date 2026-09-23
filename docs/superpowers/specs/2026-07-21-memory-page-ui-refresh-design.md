# Memory Page UI Refresh Design

Date: 2026-07-21
Status: approved in chat for implementation planning

## Goal

Refresh the `Memory` page UI so the main user-facing surfaces feel more
intentional and editorial, using the reference direction provided in chat,
without changing the underlying Memory behavior or introducing new backend
contracts.

The redesign covers three surfaces only:

1. the page-owned metric strip
2. the `Ask/Search` control panel
3. the `Governance` rail

The rest of the page keeps the current information architecture and current
feature set.

## Constraints

- Reuse the existing visual vocabulary in `src/index.css` and the repo-level UI
  guidance from `docs/UI-SPEC.md`.
- Stay inside the current feature boundary:
  - [src/features/memory/memory-page.tsx](/Users/FCHIODO/Documents/projects/agentic-os/src/features/memory/memory-page.tsx)
  - [src/index.css](/Users/FCHIODO/Documents/projects/agentic-os/src/index.css)
  - [src/features/memory/memory-page.test.tsx](/Users/FCHIODO/Documents/projects/agentic-os/src/features/memory/memory-page.test.tsx)
- No new backend command for metrics in this pass.
- No behavior change to `Ask`, `Search`, imports, approvals, or proposal
  decisions.
- Metric strip is look-only: use data already available to the page or existing
  frontend queries, but do not add a new summary endpoint.

## Recommended approach

Use a conservative editorial refresh:

- preserve the existing data flow and interactions
- increase hierarchy, spacing, and state clarity
- keep the page recognizably part of Agentic OS rather than a pasted mockup

This is preferred over a literal mock replica because it minimizes behavioral
risk while still materially improving the page.

## Scope

### In scope

- add a page-level metric strip for Memory
- redesign the mode toolbar and `Ask/Search` entry surface
- redesign the right-hand Governance rail header, tabs, banner, cards, and
  footer
- improve empty/loading composition where needed so the three refreshed areas
  feel cohesive
- add focused tests for the refreshed page structure

### Out of scope

- adding graph view or new Memory features
- changing retrieval, synthesis, governance, or approval logic
- adding a new Rust command or schema for a memory summary object
- rewriting the sidebar or reader/importer flows beyond local visual alignment

## Information architecture

The page keeps the current three-column structure:

- left: vault sidebar
- center: search / ask / reader / importer content
- right: Governance rail

The refresh adds a page-owned metric strip above the main layout, then applies a
shared visual language to the center control surface and the right rail.

## Metrics strip design

### Purpose

Give the Memory page a first-scan summary similar in density to the provided
reference, without creating new backend requirements.

### Data sources

Use existing frontend data only. The exact values can be derived from:

- `treeQuery`: total visible memory files in the vault tree
- `proposalsQuery`: total writes, pending writes, reviewed writes
- optional existing imports query if a fourth metric is needed and can be read
  without changing behavior

The recommended initial four metrics are:

1. `Vault items`
2. `Pending review`
3. `Reviewed writes`
4. `Total writes`

This keeps the strip honest and fully available from current data.

### Visual structure

- one continuous horizontal surface, not four separate cards
- warm white background with a soft border
- internal 1px vertical dividers between metrics
- each metric uses:
  - small muted label
  - large numeric value
  - optional small secondary descriptor on the same baseline
- desktop: 4 equal columns
- tablet: 2x2 grid while preserving fixed inner rhythm
- mobile: stacked cards or 2-column wrap, whichever keeps numbers readable

### Accessibility

- wrap in a labeled section, e.g. `aria-label="Memory metrics"`
- values remain plain text, not decorative counters

## Ask/Search panel design

### Purpose

Turn the current utilitarian mode bar plus form into a single composed control
surface that better matches the provided reference.

### Structure

The center control area becomes one top-level panel with two rows:

1. top row: mode switch + secondary controls
2. bottom row: primary entry field and submit actions

### Top row

- left: segmented control for `Search` and `Ask`
- right:
  - real `Include stale` checkbox rendered as a cleaner compact control
  - `Import document` secondary button
  - `Save memory` stays available, but should be visually subordinate to the
    main ask/search action

### Bottom row: Ask mode

- large question field inside a warm input container
- leading icon treatment with a dedicated circular or rounded icon seat
- domain selector remains inline with the form
- primary `Ask` button uses the darkest fill on the row
- when pending:
  - `Stop` replaces `Ask`
  - stop styling remains clearly secondary-danger rather than looking like a
    new primary action

### Bottom row: Search mode

- same shell as Ask mode
- search input fills most of the row
- the input should feel like the sibling of the Ask surface, not a separate old
  component

### Below the control surface

- keep progress panel, answer card, welcome state, and errors below the control
  panel
- restyle them only as needed to stay visually consistent with the new shell
- do not change their data contract or button behavior

## Governance rail design

### Purpose

Make the Governance rail feel like a first-class review workflow rather than a
generic list of cards.

### Header

- compact shield icon
- serif `Governance` title
- pending count pill aligned to the far right

### Tabs

- retain `Pending` and `Activity`
- render as a compact segmented control inside a padded track
- active tab gets a white inset surface and small shadow

### Context banner

Show a banner under the tabs when `Pending` is selected and there are pending
items. The copy should stay generic and accurate across flows, for example:

`There are N write proposals waiting for review before they reach the vault.`

This intentionally avoids claiming the proposals came from the most recent
answer, because they may also come from imports or manual saves.

### Proposal cards

Each proposal card becomes a clearer review unit with three visible layers:

1. metadata row
2. headline and diff preview
3. actions or activity state

#### Metadata row

- sensitivity / op badge
- relative time
- optional approval-needed cue

#### Headline

- show a human-friendly title when possible
- derive from `newContent` frontmatter title if available
- fall back to `vaultPath`

#### Diff preview

- always show a compact framed preview snippet
- keep the full gate checks + full diff behind an expandable review section
- rename the toggle to read like a review action, not a developer-only control

#### Actions

- pending cards: two full-width footer actions
  - `Approve`
  - `Dismiss`
- activity cards: no footer buttons, only status presentation

### Rail footer

Add a muted explanatory footer line pinned near the bottom of the rail:

`Sensitive and truth-changing writes appear here before they are committed.`

This mirrors the reference and clarifies the purpose of the panel.

## Component boundaries

The implementation should stay close to the current file structure, but it is
reasonable to introduce a few small local UI helpers inside
`memory-page.tsx` if that reduces noise:

- `MemoryMetricsStrip`
- `MemoryControlPanel`
- `GovernanceRail`
- `proposalTitle()` helper shared with proposal rendering

No shared cross-feature abstraction is needed for this pass.

## Data flow

No new data flow is introduced.

- metrics use values already available from existing Memory queries
- `Ask/Search` keeps the current hooks and mutation flow
- Governance keeps the current proposal list and decide mutation flow

If the current mock / non-Tauri path lacks stable proposal data for tests, the
implementation may add lightweight mock values in the existing Memory frontend
API mock section. This is acceptable because it does not change production
behavior.

## Error handling and states

- existing inline errors stay inline; no modal or toast redesign is required
- empty Governance states remain explicit and are reworded only if needed for
  the new layout
- loading states should not cause layout jumps in the metrics strip or control
  panel
- responsive behavior must avoid text overlap in buttons, tabs, and metric
  cells

## Styling guidance

- use existing color tokens where possible, with only small CSS additions
- prefer warm white / ivory surfaces over saturated fills
- serif only for major titles, not for controls
- keep border radius within the current design range
- avoid nested decorative cards; use clear sections with framed repeated items
- preserve dense, work-focused SaaS ergonomics rather than building a landing
  page aesthetic

## Testing plan

Add focused frontend coverage in
[src/features/memory/memory-page.test.tsx](/Users/FCHIODO/Documents/projects/agentic-os/src/features/memory/memory-page.test.tsx).

Recommended assertions:

1. the Memory page renders the new metrics strip labels
2. the Ask/Search control surface still exposes the real `Include stale`
   checkbox
3. the Ask form still submits and renders the governed answer state
4. the Governance rail still renders its header and empty/pending messaging in
   the new structure

Tests should stay behavior-first; they do not need to assert CSS classes except
where structure would otherwise be ambiguous.

## Implementation sequence

1. add or adjust test expectations for the new page structure
2. refactor `memory-page.tsx` into clearer local sections while keeping current
   behavior
3. add the new CSS for:
   - metrics strip
   - control panel
   - governance rail
4. verify responsive layout and non-overlap
5. run focused Memory tests, then broader frontend verification as needed

## Explicit non-goals

- no new memory summary Rust command
- no changes to audit behavior
- no changes to proposal persistence rules
- no changes to the Ask synthesis flow
- no Obsidian integration changes in this pass
