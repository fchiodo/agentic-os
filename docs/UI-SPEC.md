# Agentic OS — UI Specification v1.0 (developer handoff)

Companion to `docs/ARCHITECTURE.md` v1.1. Scope: everything the frontend needs to build Phases 1–3. Stack is unchanged: React 19 + TypeScript + Vite, TanStack Query, Zustand, react-router (hash router), Tauri 2 IPC. No new UI framework; extend the existing design vocabulary in `src/index.css`.

## 0. Design ground rules

1. **Reuse the existing vocabulary.** Palette (ivory/clay/olive/sky), fonts (Anthropic Sans/Serif/Mono), `MetricCard`, `StatusBadge`, `InfoTooltip`, `SectionEmptyState`, `.surface`, `.code-panel`, `.source-list` rows, `.segmented-control`, `.search-field`, toast stack. New components must compose these tokens, not introduce parallel ones.
2. **Status tone mapping (single source of truth, put in `src/lib/status.ts`):**
   - `created/classified/planned` → neutral · `running/resuming/verifying/waiting_for_tool` → accent · `waiting_for_approval` → warning · `completed` → success · `failed` → danger · `cancelled/partially_completed` → neutral.
3. **Serif for content, sans for chrome.** Agent-produced prose (brief, summaries, reports) renders in `.body-copy` (serif). All controls, labels, statuses stay sans.
4. **Every page has explicit empty / loading / error states.** Reuse `SectionEmptyState`; never show a blank surface. Placeholder pages currently using `ComingSoonPage` are replaced, and `ComingSoonPage` is deleted at the end of Phase 3.
5. **No blocking modals for agent work.** Approvals and diffs are inline cards; the only modal remains the Catalog detail (existing pattern).
6. **Feature folder conventions unchanged:** kebab-case files, `*-page.tsx` per route, colocated components in `src/features/{feature}/components/`.

## 1. Information architecture

Routes (hash router), sidebar order:

| Route | Label | Icon (lucide) | Phase | Purpose |
|---|---|---|---|---|
| `/today` | Today | `Sunrise` | 3 | Daily brief + attention + activity digest. Default route once Phase 3 lands (until then default stays `/catalog`) |
| `/runner` | Runner | `Play` | 1 | Submit tasks, watch live execution, history |
| `/approvals` | Approvals | `ShieldCheck` | 1 | Every action waiting on a human decision |
| `/memory` | Memory | `BrainCircuit` | 2 | Vault browser + pending write proposals (diffs) |
| `/catalog` | Catalog | `FolderOpen` | done | Existing page + "Run" action + provenance badge |
| `/usage` | Usage | `Activity` | 1 (costs) / 3 (ontology) | Cost rollups + time-mix vs ontology targets |
| `/audit` | Audit | `History` | 1 | Run traces, hash-chain verification |
| `/settings` | Settings | `Settings2` | 1 (read-only) | Policies, agents, connectors, schedules |

Sidebar nav items gain an optional numeric badge: Approvals (pending count) and Memory (pending write proposals). Badge data comes from `control_status` (see §3). Style: 18px pill, `background: var(--clay); color: var(--ivory-light);` right-aligned in the nav row.

The global metric strip in `AppShell` becomes route-aware: keep it on `/catalog` and `/usage` as-is; on other routes the page owns its own header metrics. Implementation: move the strip into an `AppShellMetrics` slot rendered per-route.

## 2. Data contracts (TypeScript, zod-validated like `features/dashboard/schema.ts`)

```ts
export type Domain = 'work' | 'planphysique' | 'personal' | 'family' | 'finance' | 'research'
export type Harness = 'codex' | 'claude' | 'acp'
export type RiskLevel = 'low' | 'medium' | 'high' | 'critical'

export type TaskStatus =
  | 'created' | 'classified' | 'planned' | 'running' | 'waiting_for_tool'
  | 'waiting_for_approval' | 'resuming' | 'verifying'
  | 'completed' | 'failed' | 'cancelled' | 'partially_completed'

export interface TaskSummary {
  id: string
  title: string            // short, derived from goal
  goal: string
  domain: Domain
  agentId: string | null   // registry agent configuration
  harness: Harness
  status: TaskStatus
  originKind: 'manual' | 'workflow' | 'schedule'
  ontologyCategoryId: string | null
  currentStep: number
  stepCount: number
  costTokens: number
  costUsd: number | null
  pendingApprovalId: string | null
  createdAt: string
  updatedAt: string
}

export interface TaskStep {
  index: number
  title: string
  status: 'pending' | 'active' | 'done' | 'failed' | 'skipped'
}

export interface TaskDetail extends TaskSummary {
  planVersion: number
  steps: TaskStep[]
  artifacts: ArtifactRef[]
  lastEventSeq: number
}

export interface ArtifactRef { id: string; label: string; path: string; kind: 'file' | 'diff' | 'report' | 'draft' }

export interface TaskEvent {
  taskId: string
  seq: number              // monotonic per task; UI uses it to dedupe/re-sync
  ts: string
  kind: 'status_changed' | 'plan_updated' | 'step_started' | 'step_completed'
      | 'tool_call' | 'tool_result' | 'agent_message' | 'file_change'
      | 'approval_required' | 'cost_update' | 'error'
  payload: Record<string, unknown>   // discriminated per kind, see §3
}

export interface ApprovalRequest {
  id: string
  taskId: string
  taskTitle: string
  domain: Domain
  toolName: string           // e.g. "git.push_branch"
  actionSummary: string      // one sentence, human-readable
  riskLevel: RiskLevel
  preview: { kind: 'diff' | 'command' | 'text'; content: string } | null
  requestedAt: string
}

export interface MemoryWriteProposal {
  id: string
  taskId: string | null
  vaultPath: string          // e.g. "vault/work/newsletter-ai.md"
  domain: Domain
  sensitivity: 'normal' | 'sensitive'
  unifiedDiff: string        // computed by backend; UI renders, never diffs
  provenance: { source: string; ts: string }
  kind: 'memory' | 'skill'   // 'skill' = distillation output
  status: 'pending' | 'approved' | 'discarded'
}

export interface DigestEntry {
  id: string; ts: string; taskId: string; domain: Domain
  toolName: string; actionSummary: string; outcome: 'ok' | 'failed'
}

export interface TodayBrief {
  generatedAt: string | null
  briefMarkdown: string | null       // null => routine not yet run today
  attention: { approvals: number; failedRuns: number; memoryProposals: number }
  digest: DigestEntry[]
}

export interface OntologyCategory {
  id: string; domain: Domain; label: string
  direction: 'automate' | 'assist' | 'human'
  minutesThisWeek: number; minutesPrevWeek: number
}

export interface UsageRollup {
  period: 'day' | 'week' | 'month'
  byDomain: { domain: Domain; tokens: number; usd: number; runs: number }[]
  byModel: { model: string; tokens: number; usd: number }[]
  topTasks: { taskId: string; title: string; usd: number }[]
}

export interface TraceEntry {
  runId: string; seq: number; ts: string
  kind: 'input' | 'routing' | 'context' | 'model_call' | 'tool_call' | 'policy_decision'
      | 'approval' | 'output' | 'feedback'
  summary: string
  detail: Record<string, unknown>    // args/results, collapsed by default
  tokens: number | null; costUsd: number | null
}

export interface ControlStatus {
  pendingApprovals: number
  pendingMemoryProposals: number
  runningTasks: number
  spentTodayUsd: number
  auditChainOk: boolean
}
```

## 3. Tauri command surface and event streaming

Commands (Rust, thin wrappers in `src/lib/tauri.ts`, one TanStack Query hook each in the owning feature):

```
control_status() -> ControlStatus                       // fetched on demand; never polled
tasks_list(filter?: {status?, domain?, limit?}) -> TaskSummary[]
tasks_get(id) -> TaskDetail
tasks_events_since(id, sinceSeq) -> TaskEvent[]         // catch-up after reconnect/navigation
tasks_submit({goal, domain?, agentId?, workflowId?}) -> TaskSummary
tasks_cancel(id) -> TaskSummary
approvals_list() -> ApprovalRequest[]
approvals_decide({id, decision: 'approve'|'deny', note?}) -> ApprovalRequest
memory_tree(domain?) -> VaultNode[]                     // {path, name, kind: 'dir'|'file', domain}
memory_read(path) -> {markdown, lastModified}
memory_search(query) -> {path, snippet, score}[]        // FTS5
memory_proposals_list() -> MemoryWriteProposal[]
memory_proposals_decide({id, decision: 'approve'|'discard'}) -> MemoryWriteProposal
skills_distill(taskId) -> MemoryWriteProposal           // kind: 'skill'
today_brief() -> TodayBrief
usage_rollup({period}) -> UsageRollup
ontology_report(domain?) -> OntologyCategory[]
audit_runs(filter?) -> {runId, taskId, title, ts, status, costUsd}[]
audit_trace(runId) -> TraceEntry[]
audit_verify_chain() -> {ok: boolean, checkedRows: number, brokenAt?: string}
settings_policies() -> {path, toml, parsedMatrix}[]     // read-only render in Phase 1
settings_schedules() -> {workflowId, cron, lastRun, nextRun, enabled}[]
```

**Event streaming.** One global Tauri event channel: `agent-control://task-event`, payload `TaskEvent`. Frontend pattern:

1. A single top-level listener (mounted in `providers.tsx`) pushes events into a Zustand event store keyed by `taskId`, capped at 2 000 events per task in memory (older entries dropped; full log always available from `tasks_events_since`).
2. Live events update only the Runner event store. They do not invalidate TanStack queries or trigger list/status refreshes.
3. On mount of a task detail, call `tasks_events_since(id, lastSeqInStore)` once to close any gap, then rely on the live channel. `seq` deduplicates.
4. No query uses a periodic refetch interval. Explicit user mutations may invalidate the directly affected cached data after they complete.
5. Native notifications (tauri-plugin-notification) fire on `approval_required` and terminal statuses (`completed`/`failed`) for tasks the user is not currently viewing. Clicking focuses the app on `/runner` with the task selected.

## 4. Page specifications

### 4.1 Runner (`/runner`) — Phase 1, the core screen

Layout: `main-grid` two columns — task list (left, `minmax(0,1fr)`) + composer/detail (right, 380px), collapsing to one column under 1280px (existing breakpoint behavior).

**Composer (top of right column).** `field-stack` textarea for the goal, `select` for agent (registry list, default "Auto"), `select` for domain (default inferred, shown after router classification), submit via `primary-button` labeled "Run". After submit, show the classification result inline (domain, risk, read-only/full profile) before execution starts — this is the router confidence surfacing from ARCHITECTURE §3; low confidence renders a warning `StatusBadge` "Read-only mode".

**Task list.** Cards ordered: `waiting_for_approval` first, then `running`, then rest by `updatedAt` desc. Each `TaskCard`:

- Header row: title (600, 15px) + origin subtitle (`{harness} · {agentId|workflow} · {domain}`) + `StatusBadge` with tone from `status.ts`.
- Step checklist (`StepChecklist`): one row per step, icons `Check` (done, olive), filled dot (active, clay), `Circle` (pending, cloud-dark), `X` (failed, accent-red). Show max 6 rows; beyond that collapse middle with "… N more".
- If `running`: `LiveLog` panel — reuse `.code-panel`, mono 12px, renders the last ~30 `tool_call`/`agent_message`/`file_change` events as single lines (`[tool] read_file …`, `[agent] …`). Auto-scroll pinned to bottom; any manual scroll-up unpins and shows a "Jump to latest" chip; re-pin on click. Windowed rendering (slice, no virtualization lib) — 30 visible rows is the budget.
- If `waiting_for_approval`: embed the `ApprovalCard` inline (same component as `/approvals`, §4.2).
- If `completed`: artifact chips (`tag-chip` + lucide `FileText`/`Mail`/`GitBranch` per kind, click = reveal in Finder via Tauri opener) + secondary action **"Distill to skill"** (Phase 2) → calls `skills_distill`, then routes to `/memory` with the new proposal highlighted.
- Footer meta: relative time, cost (`{tokens} tok · ${usd}`), link "Trace" → `/audit?run={runId}`.

Header metrics for this page: running count, waiting approvals, spent today ($), tasks completed today — four `MetricCard`s.

**States.** Empty: `SectionEmptyState` "No tasks yet — describe a goal above or run something from Catalog". Error on submit: toast (existing toast stack) with the policy/router error verbatim.

### 4.2 Approvals (`/approvals`) — Phase 1

Single column of `ApprovalCard`s, oldest first (FIFO — the oldest block is the most expensive). Card contract:

- Header: `actionSummary` (600) + risk `StatusBadge` (`medium`→warning, `high`/`critical`→danger) + domain `tag-chip`.
- Context line: task title, link to the task in Runner.
- Preview block by `preview.kind`: `diff` → `DiffView` (§5); `command` → `.code-panel` one-liner; `text` → `.body-copy` excerpt (max 8 lines, expandable).
- Actions right-aligned: `icon-button` "Deny" + `primary-button` "Approve". Both optimistic (card fades, restores on error with danger toast). Optional note field appears on Deny (one-line input, stored in audit).
- No "always allow" shortcut. Instead a tertiary text-link "Propose policy rule…" that pre-fills a suggestion written to `policies/proposals/` for manual review — policy changes are never one click (ARCHITECTURE §9).

Keyboard: `↑/↓` moves focus between cards, `A`/`D` on the focused card. `aria-live="polite"` region announces new arrivals.

Empty state: "Nothing is waiting on you." + count of auto-approved actions today with link to Today's digest.

### 4.3 Memory (`/memory`) — Phase 2

Two-column: left = vault tree (grouped by domain, collapsible; `search-field` on top hitting `memory_search`, results replace the tree while active). Right = viewer.

- File view: rendered markdown (`react-markdown` + `remark-gfm` — the only new runtime deps approved for this spec) inside `.surface`, `.body-copy` typography; footer meta (path, last modified, domain, sensitivity). "Open in Obsidian" action (`obsidian://open?path=` URI).
- **Pending proposals rail** (top of page, always visible when count > 0): horizontal list of `ProposalCard`s — `DiffView` + provenance line ("from task {title}, {relative time}") + "Discard" / "Approve write". Skill proposals (`kind: 'skill'`) get a `Wand2` icon and the label "New skill".
- Graph view: explicitly out of scope until Phase 5; do not scaffold it.
- **Import document:** paste text, choose a PDF/UTF-8 text file, or fetch a public
  HTTP(S) URL. Show the 2 MiB limit, preserved source path, extraction warnings,
  original PDF path when applicable, extractor/version, quality
  status/score/issues, candidate count, source history/preview, and route every extracted fact to
  the existing pending-proposals rail for approval.

### 4.4 Today (`/today`) — Phase 3

Single column, three stacked sections:

1. **Brief** — `.surface` with the rendered `briefMarkdown` (serif). If `null`: empty state "The daily brief runs at {schedule}. Run it now" with a button that submits the daily-brief workflow through the normal Runner path (it appears in Runner like any task).
2. **Needs attention** — up to three `source-list` rows: pending approvals, failed runs, pending memory proposals; each row links to its page. Hidden entirely when all zeros.
3. **Activity digest** — passive list of `DigestEntry` rows for today (`[toolName] actionSummary · task · time`), max 20, "View all in Audit" link. Visual weight deliberately low: 13px, `row-subtle`. This is tier-2 visibility (ARCHITECTURE v1.1 principle): auto-approved actions are seen here, never notified.

Anti-sprawl enforcement is backend-side (routines deliver here), but the UI contract is: **no page other than Today may render routine output summaries.**

### 4.5 Usage (`/usage`) — costs Phase 1, ontology Phase 3

- Period `segmented-control` (day/week/month).
- `metric-strip`: total spend, tokens, runs, avg cost/run.
- Two `.surface` panels: by domain (rows with `token-pill` amounts), by model.
- **Time mix vs ontology (Phase 3):** one `OntologyBar` row per category: label, direction icon (`Bot` automate / `Users` assist / `Hand` human), horizontal bar of `minutesThisWeek` vs `minutesPrevWeek`, and a computed verdict chip: moving with its direction → success tone; against → warning tone with delta ("+40 min human time in an automate category").

### 4.6 Audit (`/audit`) — Phase 1

- Filter bar: domain select, status select, `search-field` on title.
- Run list (existing catalog-table pattern): time, title, domain, status badge, cost, chevron.
- Detail: `TraceTimeline` — chronological `TraceEntry` rows, icon per kind, `summary` visible, `detail` JSON collapsed behind a disclosure (mono, `.code-panel`). Policy decisions render the rule id that fired. Tokens/cost inline per model call.
- Header: chain status chip from `audit_verify_chain()` — "Audit chain verified · N rows" (success) or "Chain broken at {row}" (danger, always visible until resolved).

### 4.7 Catalog (`/catalog`) — additive changes only

- Row action "Run" (existing `Play` affordance) for runnable kinds (skill, routine, workflow, prompt, agent): navigates to `/runner` with the composer pre-filled (`workflowId`/`agentId`).
- Provenance badge in the detail modal for distilled skills: "Distilled from task {title}" linking to the source trace.

### 4.8 Settings (`/settings`) — Phase 1 read-only

Sections as `.surface` panels: Policies (rendered risk matrix table from `settings_policies`, link "Edit in editor" opening the TOML file), Agents (registry list), Schedules (workflow, cron, last/next run, enabled toggle — toggle is the only write), Connectors (status per MCP server/harness: available/needs-auth/disabled). Editing policies/agents in-app is out of scope until Phase 4+.

## 5. New shared components (in `src/components/ui/`)

| Component | Contract | Notes |
|---|---|---|
| `TaskCard` | `{task, events, onCancel, onDistill}` | §4.1; owns nothing global |
| `StepChecklist` | `{steps, max?: number}` | pure |
| `LiveLog` | `{events, pinned}` | windowed slice, autoscroll rules §4.1 |
| `ApprovalCard` | `{approval, onDecide}` | shared by Runner + Approvals |
| `DiffView` | `{unifiedDiff}` | renders backend-computed unified diff; `+` lines olive tint bg, `-` lines coral tint bg, hunk headers mono cloud-dark; NO client-side diffing |
| `TraceTimeline` | `{entries}` | disclosure per entry |
| `OntologyBar` | `{category}` | §4.5 |
| `NavBadge` | `{count}` | sidebar pill |
| `MarkdownView` | `{markdown}` | wraps react-markdown, serif body, sanitized (no raw HTML pass-through — agent output is untrusted content) |

`MarkdownView` sanitization is a security requirement, not a nicety: brief/memory content can contain injection-shaped or malicious markup; render inert.

## 6. State management rules

- Server state: TanStack Query exclusively (`staleTime` 5 s for lists, `Infinity` for traces). No server data in Zustand.
- Zustand (`store/workbench.ts` extended): selected task id, composer draft, log pin state, catalog filters (existing), sidebar collapsed.
- Event store: separate Zustand slice `store/task-events.ts` as described in §3; cleared per task on terminal status + 5 min.
- Optimistic updates only for `approvals_decide` and `memory_proposals_decide`; everything else re-fetches.

## 7. Accessibility and i18n

- All status changes announced via a single visually-hidden `aria-live="polite"` region in `AppShell` ("Task Newsletter QA completed", "Approval required: push branch…").
- Focus: after Approve/Deny, focus moves to the next card; after composer submit, to the new TaskCard.
- UI copy in English (consistent with current app). No i18n framework; strings inline.

## 8. Phase gates (UI acceptance)

- **Phase 1 done when:** a goal submitted in Runner streams live events, parks on an inline approval, resumes on Approve, completes with artifacts; the same approval was visible in `/approvals` and the run in `/audit` with verified chain; Usage shows its real cost. All with keyboard + notifications working.
- **Phase 2 done when:** Memory shows the vault, a run's memory write appears as an approvable diff, and "Distill to skill" on a completed run produces a proposal that lands in Catalog with provenance after approval.
- **Phase 3 done when:** Today renders brief + attention + digest from real routines, no routine has its own surface, and Usage renders the ontology time-mix with direction verdicts.
