# Agentic OS — Second Brain / Memory Specification v1.0 (developer handoff)

Implementation status: complete. Operational behavior and release checks are
documented in [MEMORY-RUNBOOK.md](./MEMORY-RUNBOOK.md).

Companion to `docs/ARCHITECTURE.md` v1.2 (§6–§8) and `docs/UI-SPEC.md` §4.3. Scope: the complete Phase 2 memory implementation — vault, index, write pipeline, TTL/staleness/retrieval-ranking (the three v1.2 refinements), context builder integration, commands, UI deltas, tests. Stack: Rust crate `memsvc` inside `src-tauri`, SQLite (existing app DB), git, markdown. No external memory SaaS — hard constraint (corporate data, local-first).

## 0. Design invariants (do not violate)

1. **The vault is the source of truth; SQLite is a derived index.** If they disagree, the file wins; `memory_reindex` reconciles. Never store memory content only in the DB.
2. **No LLM writes to the vault directly.** Every write lands as a `MemoryWriteProposal` (unified diff) and reaches disk only through the admission pipeline (§5). Approval requirements per §5.4.
3. **Memory content is untrusted data.** When injected into prompts it is wrapped in a data block with source labels; instructions inside memory bodies are never executed. (ClawHavoc lesson — ARCHITECTURE §9.)
4. **Forgetting is a feature.** Episodes expire (TTL); facts go stale and lose retrieval rank; nothing is silently deleted — expiry archives, never destroys.
5. **Every memory has provenance.** A row without a source reference fails the write gate.
6. **The vault stays Obsidian-compatible.** Plain markdown + YAML frontmatter, `[[wikilinks]]`, no proprietary syntax, filenames `[a-z0-9-]`.

## 1. Vault layout and file format

Default root: `~/AgenticOS/vault/` (configurable at deployment through `AGENTIC_OS_VAULT_ROOT`; it must be outside any workspace the harness can write to via tasks). The vault is its **own git repository**, auto-initialized on first run — separate from the app repo.

```
vault/
├── work/
│   ├── decisions/2026-07-20-powerreviews-feed-delta.md
│   ├── projects/newsletter-ai/notes.md
│   ├── vendors/sierra.md
│   ├── people/<slug>.md
│   ├── meetings/2026-07-18-databricks-sync.md      # episodes (TTL)
│   └── preferences.md
├── planphysique/ …                                   # same substructure
├── personal/ …   family/ …   finance/ …   research/ …
└── _archive/                                         # expired episodes move here
```

Every memory file carries YAML frontmatter:

```yaml
---
id: 8f14e45f-…                # uuid v4, immutable
type: fact | decision | preference | entity | episode
domain: work                  # one of the six domains, single-valued
title: PowerReviews feed is delta, not full
created: 2026-07-20T09:12:00Z
updated: 2026-07-20T09:12:00Z
provenance:
  source: task:4b1e…          # task:<id> | meeting:<vault-path> | manual | distill:<task-id>
  ts: 2026-07-20T09:12:00Z
confidence: 0.9               # 0.0–1.0, set by pipeline, bumped by confirmations
sensitivity: normal           # normal | sensitive
valid_from: 2026-06-12        # optional temporal validity (lightweight Zep-style)
valid_until: null
stale_after_days: 180         # per-type default, overridable per file (§6.2)
last_confirmed: 2026-07-20T09:12:00Z
confirmations: 1
expires: null                 # hard TTL, episodes only (ISO date)
tags: [powerreviews, voc, sftp]
---

Delta feed daily instead of full: full files >2GB hit the SFTP timeout.
Decided with the vendor on the 2026-06-12 call. Open point: retention of
processed files. See [[2026-06-12-powerreviews-call]].
```

Body rules: facts ≤ 1 200 chars (gate-enforced); episodes unlimited; one fact per file (dedup works at file granularity); wikilinks for relations.

## 2. Data model (SQLite, migrations added to `db.rs`)

```sql
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    vault_path TEXT NOT NULL UNIQUE,          -- relative to vault root
    domain TEXT NOT NULL,
    mem_type TEXT NOT NULL,                   -- fact|decision|preference|entity|episode
    title TEXT NOT NULL,
    summary TEXT,                             -- first 280 chars of body, plain text
    sensitivity TEXT NOT NULL DEFAULT 'normal',
    confidence REAL NOT NULL DEFAULT 0.7,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    valid_from TEXT, valid_until TEXT,
    stale_after_days INTEGER,
    last_confirmed_at TEXT,
    confirmation_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TEXT,
    access_count INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT,                          -- episodes only
    provenance TEXT NOT NULL,                 -- JSON
    content_hash TEXT NOT NULL,               -- sha256 of file bytes at index time
    status TEXT NOT NULL DEFAULT 'active'     -- active|stale|expired
);
CREATE INDEX idx_memories_domain ON memories(domain, status);

CREATE VIRTUAL TABLE memories_fts USING fts5(
    title, summary, body, tags,
    content='',                               -- contentless; body read from file on demand
    contentless_delete=1                      -- safe per-row upsert/delete
);

CREATE TABLE memory_proposals (
    id TEXT PRIMARY KEY,
    task_id TEXT,                             -- nullable (manual/meeting sources)
    vault_path TEXT NOT NULL,
    domain TEXT NOT NULL,
    kind TEXT NOT NULL,                       -- memory|skill
    op TEXT NOT NULL,                         -- create|update|supersede
    supersedes_id TEXT,                       -- memory id when op=supersede
    sensitivity TEXT NOT NULL,
    unified_diff TEXT NOT NULL,               -- computed by backend
    new_content TEXT NOT NULL,                -- full post-apply file content
    provenance TEXT NOT NULL,
    gate_report TEXT NOT NULL,                -- JSON: which gate checks ran/passed
    requires_approval INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',   -- pending|approved|discarded|auto_applied
    created_at TEXT NOT NULL, decided_at TEXT,
    base_content_hash TEXT                    -- optimistic concurrency guard
);
```

`memories_fts` is rebuilt per-row on index upsert. Vector search (sqlite-vec + local embeddings) is explicitly **out of scope for v1** — BM25 + the ranking in §7 first; add vectors only if retrieval quality proves insufficient on real usage (ARCHITECTURE decision #3).

## 3. Rust module layout (`src-tauri/crates/memsvc/` or `src-tauri/src/memsvc/`)

| Module | Responsibility |
|---|---|
| `vault.rs` | Read/write files under vault root ONLY (path canonicalization + prefix check), git init/commit, archive moves |
| `frontmatter.rs` | Parse/serialize YAML frontmatter (serde_yaml), validation |
| `index.rs` | SQLite upserts, FTS sync, `reindex()` full scan with `content_hash` drift detection |
| `pipeline.rs` | §5: extraction → classification → gate → dedup → proposal |
| `proposals.rs` | Proposal CRUD, apply-on-approve (file write + git commit + index upsert, atomically ordered: file → commit → index) |
| `retrieval.rs` | §7 scoring, access-stat updates |
| `maintenance.rs` | §6 TTL/staleness sweeps |

Git commit message convention: `mem(<domain>): <op> <slug> [<provenance.source>]` — e.g. `mem(work): create powerreviews-feed-delta [task:4b1e]`.

## 4. Capture sources (what feeds the pipeline)

1. **Task runs** — on terminal status `completed`, the orchestrator queues an async extraction job over the run trace (only if the task's domain policy enables capture; default on for `work`/`research`, off for others until Phase 5).
2. **Meeting transcripts** — the meeting-to-memory workflow (Phase 3) feeds transcript files.
3. **Manual** — "Save to memory" action in UI; skips extraction, still passes gate + proposal.
4. **Skill distillation** — `skills_distill(taskId)` produces a `kind='skill'` proposal whose `vault_path` targets the skills directory the harnesses consume (not the vault); same pipeline, same approval.

Extraction uses `modelgw.invoke(capability=fast_extraction)` with a JSON-schema-constrained output: `[{type, domain, title, body, tags, valid_from?, valid_until?, confidence}]`, max 10 candidates per source. Extraction runs in a **read-only context**: the extractor model receives the trace text as data and cannot call tools.

## 5. Write pipeline (admission control)

```
raw source → extract candidates → classify → WRITE GATE → dedup → proposal → [approval] → persist
```

### 5.1 Classification (deterministic first, model second)
- `domain`: inherited from the task/source domain; cross-domain reclassification requires approval always.
- `sensitivity`: regex/keyword pass (names+health, salary, legal, credentials context) → `sensitive`; else extractor's suggestion.
- `type`: extractor's suggestion validated against §1 constraints (e.g. body length → cannot be `fact`).

### 5.2 Write gate (pure Rust, no model, every rule logged in `gate_report`)
| Check | Rule | On fail |
|---|---|---|
| Secrets | Regex battery: AWS/Azure keys, JWTs, `-----BEGIN`, `password=`, bearer tokens, 32+ hex/base64 runs | REJECT (never proposed, audit row written) |
| Provenance | `provenance.source` non-empty and resolvable | REJECT |
| Durability | `fact/decision/preference` body ≤ 1 200 chars; imperative-instruction heuristic ("ignore previous", "always run") → flag `injection_suspect` | REJECT if suspect; TRUNCATE-and-flag if long |
| Attribution | `type=preference` must originate from user statements (source=manual or meeting), not model inference from silence | DOWNGRADE confidence to 0.5 + require approval |
| Duplication | See §5.3 | Convert to `update`/`supersede` |
| Domain fence | `vault_path` prefix must match `domain` | REJECT |

### 5.3 Dedup
Query `memories_fts` with the candidate title+body (BM25, same domain, status != expired). If top hit normalized score > 0.82: propose `op=update` (merge bodies, keep id, bump `updated`/`confidence`) instead of `create`. If the candidate **contradicts** the hit (extractor emits `contradicts: <id>` when temporal facts clash): propose `op=supersede` — old memory gets `valid_until = new.valid_from` and `status=stale`; new file is created. Nothing is overwritten destructively.

### 5.4 Approval matrix (extends ARCHITECTURE §8)
| Case | Behavior |
|---|---|
| `work`/`research`, sensitivity=normal, op=create/update | Auto-apply (`status=auto_applied`) + entry in Today's Activity digest |
| Any `sensitive` | Approval required |
| Domains `personal`/`family`/`finance` | Approval required (always) |
| op=supersede (rewrites truth) | Approval required |
| kind=skill (procedural) | Approval required — never persist unvalidated procedures |
| Cross-domain | Approval required |

### 5.5 Persist (on approve or auto-apply)
1. Write file to vault (create/replace). 2. `git add + commit`. 3. Upsert `memories` + FTS. 4. Audit row (`kind=memory_write`). Failure at any step rolls back the previous ones (file restore from git, proposal back to pending, error surfaced).

## 6. Forgetting: TTL and staleness (v1.2 refinement #1 and #2)

`maintenance.rs` runs on app start + every 24h (in-app timer; catch-up on start).

### 6.1 TTL (episodes only)
- Default `expires = created + 90 days` (per-type default; meeting transcripts 90d, run-trace episodes 30d).
- Sweep: `expires_at < now` → `status=expired`, file moved to `vault/_archive/<domain>/…` (git mv, committed), FTS row removed, `memories` row kept for provenance chains.
- Facts extracted FROM an episode survive independently — the raw log dies, the knowledge doesn't.

### 6.2 Staleness (facts/decisions/preferences/entities)
- Defaults for `stale_after_days`: fact 180 · entity 365 · preference 365 · decision **null** (decisions never go stale by age — they are superseded, §5.3).
- Sweep: `last_confirmed_at + stale_after_days < now` → `status=stale`. Nothing moves; the memory stays readable and searchable, flagged.
- **Confirmation events** reset the clock and bump `confirmation_count`: user clicks "Confirm still true" in Memory UI; a retrieval is explicitly validated by the Reviewer step; a new extraction dedups onto it (§5.3 update).
- **Re-verification rule**: if a `stale` memory is retrieved into context for a task whose next action is side-effectful (per tool contract `side_effect != none`), the context builder tags it `UNVERIFIED` in the prompt block and the Reviewer step must either confirm it from the source or strip it before the action executes.

## 7. Retrieval and ranking (v1.2 refinement #3)

`memory_search(query, domain, opts)`:

1. **Permission filter first** (SQL `WHERE domain IN (granted)` — storage-level, never prompt-level).
2. Exact-key lane: entity/title exact matches bypass scoring.
3. Candidate set: FTS BM25 top 50, `status != 'expired'`; `stale` included only when `opts.includeStale` (default true for context building, false for UI unless toggled).
4. Score each candidate:

```
relevance  = bm25_normalized                    ∈ [0,1]
recency    = exp(-ln(2) * age_days / half_life) # half_life: episode 30d, fact 180d,
                                                # decision 730d, preference/entity 365d
trust      = confidence * min(1, 0.6 + 0.1*confirmation_count)
score      = 0.60*relevance + 0.25*recency + 0.15*trust
             - (0.30 if status == 'stale' else 0)
```

5. Return top K=8 with `vault_path` citations; update `last_accessed_at`/`access_count` (usage feeds nothing in v1 beyond observability, reserved for future decay tuning).
6. **Context budget**: the context builder caps injected memory at 4 000 tokens; overflow → summarize lowest-scored entries into one line each. Every injected memory appears in the run trace (`kind=context`) with its path — auditability of "what did the agent believe".

Prompt injection format (context builder):

```
<memory source="vault/work/decisions/2026-07-20-powerreviews-feed-delta.md" status="active" confirmed="2026-07-20">
…body…
</memory>
```

Data, not instructions; the system prompt states that `<memory>` blocks are reference material only.

## 8. Command surface (Tauri, extends UI-SPEC §3)

```
memory_tree(domain?)                       -> VaultNode[]
memory_read(path)                          -> { frontmatter, markdown, status, gitLastCommit }
memory_search({query, domain?, includeStale?}) -> ScoredMemory[]     # ScoredMemory = row + score components
memory_proposals_list()                    -> MemoryWriteProposal[]
memory_proposals_decide({id, decision})    -> MemoryWriteProposal    # approve|discard
memory_confirm(id)                         -> MemorySummary          # "still true" → reset staleness clock
memory_save_manual({domain, type, title, body, tags}) -> MemoryWriteProposal
memory_import_document({domain, inputKind, title, content?, contentEncoding?, mimeType?, sourceUrl?, fileName?}) -> DocumentImportResult
memory_document_imports_list(domain?)        -> DocumentImportRecord[]
memory_document_source_read(id)              -> { import, content, gitLastCommit }
memory_reindex()                           -> { indexed, drifted, orphaned }
memory_maintenance_run()                   -> { expired, markedStale }
skills_distill(taskId)                     -> MemoryWriteProposal    # kind=skill
```

TypeScript contracts mirror §2 (zod schemas in `src/features/memory/schema.ts`); `MemoryWriteProposal` already exists in UI-SPEC §2 — extend with `op`, `gateReport`, `requiresApproval`, `status='auto_applied'`.

## 9. UI deltas (extends UI-SPEC §4.3)

- Memory page: status chip per file (`active` success · `stale` warning · archived neutral), **"Confirm still true"** button on stale items, "Include stale" toggle on search, score breakdown on hover (relevance/recency/trust) for debuggability.
- Proposals rail: gate report visible ("checks passed: secrets ✓ provenance ✓ dedup: update of <title>"), `op` badge (create/update/supersede), auto-applied writes do NOT appear here — they appear in Today's Activity digest (three-tier visibility, ARCHITECTURE v1.1).
- Runner TaskCard (completed): "Distill to skill" wired to `skills_distill` (already spec'd).

## 10. Security summary

Path canonicalization on every vault write (must resolve under vault root; symlinks rejected) · secrets gate before anything persists · storage-level domain isolation · `<memory>` blocks are data · skill proposals always human-approved · vault git history = tamper evidence (complements the audit hash chain) · `finance`/`family` move to SQLCipher side-DB in Phase 5 (out of scope here; design keeps their index rows in a separate attached DB to make that migration mechanical).

## 11. Milestones and acceptance criteria

| Milestone | Contents | Accepted when |
|---|---|---|
| M1 — Vault + index (week 1) | `vault.rs`, `frontmatter.rs`, `index.rs`, git init, `memory_tree/read/search` (BM25 only), Memory UI browser | Vault scaffolded with the 6 domains; a hand-written file appears in UI search within 1 reindex; drift detection flags a hand-edited file |
| M2 — Write pipeline (week 2) | `pipeline.rs`, `proposals.rs`, gate, dedup, approval flow, manual save, git commits | A completed run produces ≤10 candidates; a candidate with a fake AWS key is rejected with audit row; a near-duplicate becomes `op=update`; approve → file+commit+index atomically; deny → nothing on disk |
| M3 — Forgetting + ranking (week 3, ~3 days) | `maintenance.rs`, TTL sweep, staleness, confirmations, §7 scoring, re-verification tagging | An episode older than TTL lands in `_archive/` via git mv; a 200-day-old fact shows `stale` and ranks below a fresh equivalent; "Confirm still true" resets it; stale memory entering a side-effectful task is tagged UNVERIFIED in the trace |
| M4 — Context + distill (week 4) | Context builder integration, `<memory>` injection with budget, `skills_distill`, trace citations | A run's trace lists exactly which memories were injected; distilling a completed run yields an approvable skill that then appears in Catalog with provenance |

Unit tests live next to each module; the M-level checks above are integration tests under `src-tauri/tests/memory/`. Every gate rule gets a dedicated negative test.

## 12. Explicit non-goals (v1)

Vector embeddings/semantic search (revisit after real usage) · knowledge-graph store (wikilinks + `supersedes` links suffice; Zep/Graphiti only if chronological-graph queries become a real need) · external memory SaaS (never, for corporate data) · memory sync across machines (vault is git — manual remote if ever needed, private only) · automatic deletion (archive only).
