# Second Brain evolution

Status: correctness hardening completed on 2026-09-23. Retrieval promotion and
final milestone closure remain gated on a larger representative corpus and an
end-to-end Tauri dataset containing real memories, routines and activity. The
detailed external brief mentioned in the request was not present in the
workspace; this document uses the requirements supplied in the task itself.

## Initial verification

| Area | Verified behavior | Result |
| --- | --- | --- |
| Reindex and restart | `memory::index::reindex` touches `memories` and `memories_fts`; it does not delete or recreate `memory_proposals`, `document_imports`, or their decision state. Startup opens the existing WAL database before reindexing. | Present. Pending proposals and import history survive an ordinary restart/reindex. |
| Rebuild boundary | Markdown can rebuild memory rows and FTS. Proposals and document-import history exist only in the app database. | Confirmed limitation. Loss of the app database cannot be repaired from the vault alone. |
| Handled write failure | Proposal persistence snapshots the target, serializes writers, checks the proposal base hash, writes atomically, commits Git, updates SQLite/FTS and audit, and performs a compensating rollback on a returned error. | Present. |
| Process crash | The previous implementation had no durable intent spanning filesystem, Git, SQLite and audit. | Fixed. `memory_operations` records intent and stage before mutation. Startup recovery rolls an untouched base back, rolls an exact journaled file forward through Git/index/state/audit, and marks a third/conflicting state `needs_attention` instead of guessing. Import source snapshots use the same journal. |
| Retrieval normalization | FTS candidates are normalized inside the result set and blended with reciprocal rank. Recency and trust are in `[0,1]`; stale has an explicit penalty. | Present, but corpus-level BM25 calibration is still unmeasured. |
| Valid decisions | Previously, every decision decayed with a 730-day half-life even if still active. | Fixed: an active decision whose validity has not ended gets full recency. |
| Ask verification | Previously, verification checked citations and a bag-of-terms subset only. A positive claim could therefore be accepted from a negative sentence, and entity/number/date substitutions were not modeled explicitly. | Fixed with sentence-level polarity, number/date and named-subject constraints. Known predicates bind arguments to their original side. Unknown predicates are no longer accepted by lexical overlap alone: they require an extractive, ordered sentence match or Ask abstains. |
| Saving Ask answers | Previously, source paths survived only as prose in the body and the multi-source confidence bonus could be persisted as fact confidence. | Fixed: original paths are written to frontmatter `sources`, and saved confidence is capped by the weakest cited evidence score. |

## Temporal model delivered

Memory frontmatter now carries:

- `valid_from` and `valid_until` for truth windows;
- `supersedes` on the new memory;
- `superseded_by` on the replaced memory;
- `sources` for an explicit derivation chain.

A supersede still requires approval. Applying it updates both Markdown files in
the same governed persistence path, closes the old truth window, marks the old
index row stale, commits Git and writes the audit event. Maintenance now marks
any active memory stale when `valid_until` has passed, including decisions.

Episode consolidation now runs in the maintenance sweep seven days before TTL.
It creates at most three typed fact/decision candidates with
`source: consolidation:<episode-id>`, inherits the episode sensitivity, writes
the episode path to `sources`, traverses the existing Rust gate, and always
requires approval. An expired episode with pending or failed consolidation is
not archived; `deferredExpirations` makes that state visible to the UI.

## Crash recovery delivered

Proposal application and document-source import now use a durable operation
journal with stages `prepared`, `files_written`, `git_committed`,
`index_updated`/`state_updated`, `audit_written`, and a terminal status.
Recovery runs before startup reindex:

- exact proposed/source bytes are rolled forward idempotently;
- an untouched proposal base is marked rolled back and remains pending;
- missing SQLite state and audit completion are rebuilt;
- an unexpected third state is retained as `needs_attention` and surfaced in
  Memory rather than overwritten;
- completed, rolled-back, and attention operations remain inspectable through
  `memory_operations_list`.

The journal does not attempt distributed ACID. It makes intent and recovery
decisions durable and auditable across local filesystem, Git and SQLite.

## Orbital map milestone 1

The map is available from **Memory → Map** and is composed by the native
`memory_orbit_map` command.

Sources:

- Skills: real catalog items with kind `skill`.
- Memory: non-expired SQLite memory rows, grouped into the six domain fences.
- Routines: real catalog items with kind `routine` or `workflow`.
- Applications: real catalog items with kind `plugin`, `mcp`, or `automation`.
- Relations: registry membership and Markdown temporal/source links are
  `declared`; context injection persists `memoryId`. Name, path and command
  matches are persisted as `catalogRefs` with `evidence: inferred` and can never
  prove execution. A catalog relation becomes `observed` only from an
  executor-emitted `executionRefs` envelope containing `catalogId`, `kind`,
  `operation`, `outcome` and a valid RFC 3339 `occurredAt`. Incomplete envelopes
  are ignored as observations.

Domain and sensitivity filtering happens in Rust before memory nodes, edges,
previews and memory counts are returned. Sensitive memory is hidden by default.
Relations are retained only when both endpoints survive the filter. Catalog
items without governed domain tags remain global. Tags of the form
`domain:<domain>` are enforced before nodes, edges and counts are composed.
Node detail exposes declared/observed domain and capability facets with their
source references, plus operational state and last real activity. Catalog
registration, observed/inferred usage, and application connection health are
separate fields: presence in the registry never implies that an item ran or
that its connection works.

The client uses Graphology as the graph model and Sigma.js 3 as a WebGL
renderer. Positions are deterministic radial coordinates; no force layout or
continuous rotation runs. Groups expand on demand. Search can reveal a matching
child without expanding every peer. Selecting a node highlights only its
neighborhood and opens a provenance/detail panel. Memory nodes reuse the
existing reader and confirmation commands.

Collapsed child relations are rolled up onto their visible groups, preserving
relation type, evidence class, weight and up to eight provenance records. Real
task events trigger a debounced graph refresh and a 1.6-second activity pulse;
no idle or decorative loop runs. Composition time and scanned task/trace counts
are shown in the map legend. The renderer is code-split so it is not paid for
by users who remain in the library view.

Milestone 2 keeps a single Graphology graph and Sigma renderer alive across
query refreshes. Camera state is restored after data synchronization, focus is
animated over 260 ms, and newly expanded children move from their aggregate to
their stable radial coordinate over 220 ms. Highlight reducers are recomputed
from every new graph structure, so expansion, collapse and refresh preserve the
selected node and use its current neighbors. With `prefers-reduced-motion`,
layout and camera changes are applied immediately. Ring guides are graph
geometry, so they pan and zoom with the nodes instead of being an unrelated CSS
background. No continuous animation was added.

## Promotion gates for retrieval experiments

Retrieval experiments are implemented behind process-level feature flags and
do not replace the baseline by default.

Flags:

- `AGENTIC_OS_MEMORY_FUZZY=1`: local character-trigram similarity lane fused
  with FTS candidates. This is explicitly lexical typo/morphology matching,
  not semantic search;
- `AGENTIC_OS_MEMORY_ALIASES=1`: versioned Italian/English query aliases;
- `AGENTIC_OS_MEMORY_PROGRESSIVE=1`: broader second retrieval and synthesis
  pass only after grounded verification abstains or finds no evidence.

Promotion requires a representative, domain-labelled corpus and the following
measurements:

1. correctness: supported-claim precision and contradiction escape rate;
2. source coverage: answerable questions with at least one correct citation;
3. latency: p50/p95 retrieval and end-to-end Ask time;
4. total cost: model tokens, local embedding time/storage, and extra synthesis
   turns per answered question.

`AGENTIC_OS_MEMORY_SEMANTIC` no longer aliases the trigram lane. The report says
`semanticBackend: not_configured` until a real embedding backend is implemented
and evaluated. The fuzzy query no longer has a 2,000-row recency cap; a
regression test places the expected memory behind 2,005 newer rows.

The Memory sidebar exposes **Benchmark**, which compares baseline FTS5,
candidate FTS5+aliases+fuzzy, and the actual production flags through the same
scoring code used by Search/Ask. Its gold cases are realistic questions plus
human-confirmed expected source paths; a verified Ask answer can be added with
**Use as benchmark case**. It reports top-1, hit@5, recall@5, MRR, p50/p95,
corpus size, fuzzy scan count and outbound cost. With no confirmed cases it
reports that state rather than fabricating title-as-query examples.

The deterministic Ask suite separately covers citation presence, negation,
number/date and named-subject substitutions, same-token role reversals, numbers
attached to the wrong subject, and role reversal for predicates outside the
small recognized relation vocabulary. Progressive retrieval is excluded from
the zero-outbound-cost benchmark and remains measurable from audited Ask runs
because it may add model tokens.

The candidate is promoted only if it improves coverage without reducing
supported-claim precision, while keeping the agreed desktop p95 and cost
budgets. No flag is enabled automatically by this change.

## Delivery status

1. Four-ring desktop map with real registries, selection, typed/provenanced
   relations and governed detail actions: delivered.
2. Crash-safe operation intent, startup reconciliation and visible operational
   exceptions: delivered for memory proposals and document imports.
3. Explicit temporal links and governed pre-expiry episode consolidation:
   delivered.
4. Alias, explicitly fuzzy trigram and progressive retrieval experiments: delivered
   behind flags; promotion intentionally pending corpus results.
5. Real event-driven activity, collapsed relation rollups and map composition
   telemetry: delivered.
6. Production-aligned baseline/candidate/production benchmark and deterministic
   Ask verifier regression suite: delivered. Gold cases are governed local
   records populated only by explicit human confirmation.
7. Executor-qualified observed-event identifiers, distinct catalog/usage/
   connection state, persistent renderer/camera, graph-revision-safe selection,
   and reduced-motion-aware focus/expansion transitions: delivered in milestone
   2.

The milestone is not promoted on the six-question fixture alone. Closure still
requires the agreed representative evaluation of supported-claim precision,
abstention, source coverage, tokens, latency and interaction fluidity with
non-empty Memory and Routines rings.
