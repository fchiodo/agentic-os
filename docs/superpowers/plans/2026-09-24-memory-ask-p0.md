# Memory Ask P0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the SB-01/R0-R1 increment that reproduces the Movable Ink false abstention, records metadata-only evidence and claim decisions, and returns every supported list item without weakening the deterministic safety checks.

**Architecture:** Keep the existing Markdown/Git vault, SQLite index, lexical retrieval, Codex synthesis, and deterministic verification pipeline. Add chunk position metadata so the verifier can reconstruct only adjacent evidence already sent to the model, convert hard-wrapped PDF text into bullet-scoped verification units, require one list item per model claim, and record rejection codes in the existing append-only audit detail. The planner remains unchanged in this increment; adaptive planning is a separate R2 plan.

**Tech Stack:** Rust 2021, Tauri 2, rusqlite/SQLite FTS5, serde/serde_json, Tokio, existing Codex structured harness, Cargo tests, local controlled live test against the user-authorized vault.

## Global Constraints

- Baseline is commit `ac3a4ed4e6abebe6dec46d277bdae9387046305f`; the worktree also contains the repository-only worktree ignore commit.
- Preserve Markdown/Git as the source of truth and SQLite as a rebuildable derived index.
- Do not add embeddings, a graph database, a reranker, planner adaptation, or operational-map changes in this increment.
- Never store raw prompts, full source text, model drafts, credentials, or sensitive stderr in the new default trace.
- Verification may join only adjacent chunks from the same source revision that were already included in `EVIDENCE_JSON`.
- Separate bullet items must remain separate verification units.
- Preserve the existing numeric, negation, subject-direction, attribution, modality, domain, status, and cancellation checks.
- The real Movable Ink document remains local; committed fixtures must be synthetic and structurally equivalent.

---

### Task 1: Add metadata-only Ask and claim diagnostics

**Files:**
- Modify: `src-tauri/src/memory/retrieval.rs`
- Test: `src-tauri/src/memory/retrieval.rs`

**Interfaces:**
- Produces: `AskTrace`, `EvidenceTrace`, `VerificationTrace`, `ClaimVerificationTrace`, and `ClaimDecisionCode` private Rust types.
- Produces: `VerificationOutcome { answer: MemoryAnswer, trace: VerificationTrace }` from `verify_synthesis`.
- Consumes: existing `EvidencePassage`, `RawSynthesis`, `AskRunMetrics`, and append-only `audit_answer` flow.

- [ ] **Step 1: Write failing tests for rejection codes and metadata-only audit detail**

Add tests that construct one supported and one unsupported claim, call `verify_synthesis`, and assert the trace contains stable snake-case codes without claim text:

```rust
#[test]
fn verification_trace_records_claim_decisions_without_draft_text() {
    let evidence = vec![passage(
        "source:1:10",
        "_sources/work/use-cases.md",
        "● Dynamic hero selection: Choose the most relevant story for each customer.",
        0.9,
    )];
    let outcome = verify_synthesis(
        "00000000-0000-4000-8000-000000000201",
        &request(),
        "2026-09-24T12:00:00Z",
        &evidence,
        RawSynthesis {
            abstained: false,
            claims: vec![
                RawClaim {
                    text: "Dynamic hero selection: Choose the most relevant story for each customer."
                        .to_string(),
                    citations: vec![1],
                },
                RawClaim {
                    text: "Invented lunar targeting is available.".to_string(),
                    citations: vec![1],
                },
            ],
        },
        &mut Vec::new(),
    );

    assert_eq!(outcome.trace.raw_claims, 2);
    assert_eq!(outcome.trace.accepted_claims, 1);
    assert_eq!(outcome.trace.rejected_claims, 1);
    assert_eq!(outcome.trace.claims[0].code, ClaimDecisionCode::Accepted);
    assert_eq!(
        outcome.trace.claims[1].code,
        ClaimDecisionCode::InsufficientSupport
    );
    let serialized = serde_json::to_string(&outcome.trace).unwrap();
    assert!(!serialized.contains("Invented lunar targeting"));
}
```

Add an audit test using a temporary `Db`, append one result through `audit_answer`, read the newest `detail`, and assert it contains `pipelineVersion`, selected evidence identifiers, counts, and codes while excluding evidence text and draft claim text.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test memory::retrieval::tests::verification_trace_records_claim_decisions_without_draft_text -- --exact
```

Expected: compilation fails because `VerificationOutcome`, trace types, and decision codes do not exist.

- [ ] **Step 3: Implement trace types and return a verification outcome**

Add private serializable types near `AskRunMetrics`:

```rust
const ASK_PIPELINE_VERSION: &str = "ask-p0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ClaimDecisionCode {
    Accepted,
    ModelAbstained,
    EmptyClaim,
    ClaimTooLong,
    MissingCitation,
    UnknownSource,
    InsufficientSupport,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaimVerificationTrace {
    claim_index: usize,
    code: ClaimDecisionCode,
    citation_ids: Vec<usize>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationTrace {
    raw_claims: usize,
    accepted_claims: usize,
    rejected_claims: usize,
    output_truncated: bool,
    claims: Vec<ClaimVerificationTrace>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceTrace {
    evidence_id: String,
    path: String,
    source_kind: String,
    chunk_index: Option<i64>,
    score: f64,
}

#[derive(Debug, Clone, Default)]
struct AskTrace {
    queries: Vec<String>,
    evidence: Vec<EvidenceTrace>,
    verification: Option<VerificationTrace>,
}

struct VerificationOutcome {
    answer: MemoryAnswer,
    trace: VerificationTrace,
}
```

Change `verify_synthesis` to classify each claim before accepting it. Do not silently discard invalid citation numbers: any cited id outside `1..=evidence.len()` produces `UnknownSource`. Keep citation ids but never store claim text in `VerificationTrace`.

- [ ] **Step 4: Attach the trace to every terminal audit row**

Build `AskTrace` inside `ask`, populate `queries` after planning, populate `evidence` immediately after retrieval, and set `verification` from `VerificationOutcome`. Extend `audit_answer` with `trace: &AskTrace` and serialize:

```rust
"pipelineVersion": ASK_PIPELINE_VERSION,
"retrievalTrace": {
    "queries": trace.queries,
    "evidenceCandidates": trace.evidence,
},
"verificationTrace": trace.verification,
```

Use an empty/default trace for the no-evidence path. Keep the existing audit hash chain and existing answer/citation fields unchanged.

- [ ] **Step 5: Run focused and full Rust tests**

Run:

```bash
cargo test memory::retrieval::tests::verification_trace_records_claim_decisions_without_draft_text
cargo test
```

Expected: the focused test passes; the complete suite reports zero failures.

- [ ] **Step 6: Commit Task 1**

```bash
git add src-tauri/src/memory/retrieval.rs
git commit -m "feat(memory): trace Ask evidence and claim decisions"
```

---

### Task 2: Reconstruct safe verification units from cited chunks

**Files:**
- Modify: `src-tauri/src/memory/index.rs`
- Modify: `src-tauri/src/memory/retrieval.rs`
- Create: `src-tauri/src/memory/fixtures/multiline-use-cases.txt`
- Test: `src-tauri/src/memory/index.rs`
- Test: `src-tauri/src/memory/retrieval.rs`

**Interfaces:**
- Produces: `DocumentChunkHit.chunk_index: i64` from `search_document_chunks`.
- Produces: `EvidencePassage.chunk_index: Option<i64>` for ordered document evidence.
- Produces: `verification_units(citation_ids, evidence) -> Vec<String>` containing paragraph- or bullet-scoped text already sent to the model.
- Consumes: the existing 1,500-character chunks and their 220-character overlap.

- [ ] **Step 1: Add a synthetic multiline-list fixture**

Create a non-sensitive fixture with 14 bullet items. At least three items must contain hard line wraps and blank lines inside a single bullet, and one item must cross a simulated chunk boundary. Use unique titles such as `Dynamic hero selection`, `Audience targeting`, `Live inventory`, and `Regional context` so accidental fusion is detectable.

- [ ] **Step 2: Write failing tests for chunk position and conservative reflow**

Add an index test asserting `search_document_chunks` exposes the stored `chunk_index`.

Add retrieval tests:

```rust
#[test]
fn verifier_reflows_wrapped_lines_inside_one_bullet() {
    let evidence = vec![passage_with_chunk(
        "source:1:10",
        "_sources/work/use-cases.md",
        0,
        "● Dynamic hero selection: Choose the most relevant lead or secondary\n\nstory for each customer using browsing and purchase history.",
        0.9,
    )];
    let citations = BTreeSet::from([1]);
    assert!(verified_claim_text(
        "Dynamic hero selection: Choose the most relevant lead or secondary story for each customer using browsing and purchase history.",
        &citations,
        &evidence,
    )
    .is_some());
}

#[test]
fn verifier_never_fuses_adjacent_bullets() {
    let evidence = vec![passage_with_chunk(
        "source:1:10",
        "_sources/work/use-cases.md",
        0,
        "● Dynamic hero selection: Choose a story.\n\n● Regional context: Choose a location.",
        0.9,
    )];
    let citations = BTreeSet::from([1]);
    assert!(verified_claim_text(
        "Dynamic hero selection and Regional context choose a story and a location.",
        &citations,
        &evidence,
    )
    .is_none());
}
```

Add a third test with two consecutive chunks from the same source whose overlap splits one bullet; the complete claim must pass only when both chunk citations are present.

- [ ] **Step 3: Run focused tests and verify RED**

Run:

```bash
cargo test memory::retrieval::tests::verifier_reflows_wrapped_lines_inside_one_bullet
cargo test memory::retrieval::tests::verifier_never_fuses_adjacent_bullets
```

Expected: the wrapped-line test fails because each newline is currently treated as a sentence boundary; the adjacent-chunk test cannot be expressed because chunk position is missing.

- [ ] **Step 4: Carry chunk position through retrieval**

Change the document query to select `c.chunk_index`, populate `DocumentChunkHit.chunk_index`, and copy it into source `EvidencePassage` values. Memory evidence uses `None`.

- [ ] **Step 5: Implement adjacent-chunk reconstruction**

Build candidate source sequences only from cited evidence with the same `vault_path` and consecutive `chunk_index` values. Merge the known overlap by finding the longest exact suffix/prefix match; when no safe overlap exists, keep the chunks as separate units.

Normalize each candidate sequence line-by-line:

```rust
fn is_bullet_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('●')
        || trimmed.starts_with('•')
        || trimmed.starts_with("- ")
}
```

Start a new unit at every bullet marker. Within one unit, collapse whitespace and hard line wraps to one space. Do not globally replace newlines before identifying bullet boundaries. Keep paragraph units separate when a sentence ends with `.`, `!`, `?`, or `;`. Feed these units to the existing term, number, subject, negation, relation-frame, and sentence-scope checks.

- [ ] **Step 6: Run focused and complete Rust tests**

Run:

```bash
cargo test memory::retrieval::tests::verifier_reflows_wrapped_lines_inside_one_bullet
cargo test memory::retrieval::tests::verifier_never_fuses_adjacent_bullets
cargo test memory::retrieval::tests::verifier_reassembles_only_consecutive_cited_chunks
cargo test
```

Expected: all new tests and all existing adversarial verifier tests pass.

- [ ] **Step 7: Commit Task 2**

```bash
git add src-tauri/src/memory/index.rs src-tauri/src/memory/retrieval.rs src-tauri/src/memory/fixtures/multiline-use-cases.txt
git commit -m "fix(memory): verify hard-wrapped list evidence"
```

---

### Task 3: Produce atomic list claims without silent truncation

**Files:**
- Modify: `src-tauri/src/memory/retrieval.rs`
- Test: `src-tauri/src/memory/retrieval.rs`

**Interfaces:**
- Changes: transitional `MAX_CLAIMS` from `8` to `16` for the current response contract.
- Changes: `synthesis_prompt` requires exactly one source list item per claim for enumeration questions.
- Produces: `VerificationTrace.output_truncated = true` whenever the model returns more than `MAX_CLAIMS` claims.

- [ ] **Step 1: Write failing tests for a 14-item answer and prompt contract**

Create 14 `RawClaim` values from the synthetic fixture, one per bullet, and assert all 14 survive verification. Assert the generated prompt contains both of these requirements:

```text
For a list or enumeration, return exactly one source list item per claim.
Never combine two separate list items into one claim to fit the claim limit.
```

Add a 17-claim test and assert `output_truncated` is true and the user-facing warnings include an explicit partial-output warning rather than silently omitting the overflow.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test memory::retrieval::tests::fourteen_supported_list_items_are_not_merged_or_dropped
cargo test memory::retrieval::tests::claim_overflow_is_reported
```

Expected: the first test accepts only eight claims and the overflow trace/warning does not exist.

- [ ] **Step 3: Raise the transitional limit and update the prompt**

Set `MAX_CLAIMS` to `16`. Add the exact atomic-list instructions to `synthesis_prompt`. Keep `MAX_CLAIM_CHARS` at 600. Before consuming claims, compare `raw.claims.len()` with the cap; set `output_truncated` and add:

```text
La risposta del modello superava il limite operativo di 16 elementi; il risultato è parziale.
```

Do not merge overflow claims and do not claim complete coverage.

- [ ] **Step 4: Run verifier and regression tests**

Run:

```bash
cargo test memory::retrieval::tests::fourteen_supported_list_items_are_not_merged_or_dropped
cargo test memory::retrieval::tests::claim_overflow_is_reported
cargo test memory::retrieval::tests::verifier_rejects_role_reversal_with_the_same_words
cargo test memory::retrieval::tests::verifier_rejects_reversed_negation_and_changed_numbers_dates_or_subjects
cargo test
```

Expected: list tests pass and adversarial safety tests remain green.

- [ ] **Step 5: Commit Task 3**

```bash
git add src-tauri/src/memory/retrieval.rs
git commit -m "fix(memory): synthesize one claim per list item"
```

---

### Task 4: Add controlled live verification and publish the before/after report

**Files:**
- Modify: `src-tauri/src/memory/retrieval.rs`
- Create: `docs/benchmarks/MEMORY-ASK-P0-2026-09-24.md`
- Test: `src-tauri/src/memory/retrieval.rs` ignored live test

**Interfaces:**
- Consumes environment variables `AGENTIC_OS_LIVE_DB`, `AGENTIC_OS_LIVE_EXPECTED_ITEMS`, `AGENTIC_OS_LIVE_EXPECTED_SOURCE`, and optional `AGENTIC_OS_LIVE_RUNS`.
- Produces one ignored live test that runs the real local Ask pipeline and prints a JSON summary per run.

- [ ] **Step 1: Add a generic ignored live test without private fixture data**

Add:

```rust
#[tokio::test]
#[ignore = "requires the authorized local vault and corporate Codex provider"]
async fn live_ask_covers_expected_list_items() {
    let db_path = std::env::var("AGENTIC_OS_LIVE_DB").expect("AGENTIC_OS_LIVE_DB");
    let expected_path =
        std::env::var("AGENTIC_OS_LIVE_EXPECTED_ITEMS").expect("AGENTIC_OS_LIVE_EXPECTED_ITEMS");
    let expected_source =
        std::env::var("AGENTIC_OS_LIVE_EXPECTED_SOURCE").expect("AGENTIC_OS_LIVE_EXPECTED_SOURCE");
    let runs = std::env::var("AGENTIC_OS_LIVE_RUNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1);
    let expected = std::fs::read_to_string(expected_path)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let db = Db::open(std::path::Path::new(&db_path)).unwrap();

    for run in 1..=runs {
        let started = std::time::Instant::now();
        let answer = ask(
            &db,
            &MemoryAskRequest {
                question: "What are the use cases from Movable Ink?".to_string(),
                domain: "work".to_string(),
                include_stale: false,
            },
            |_| {},
            crate::harness::structured::no_cancel(),
        )
        .await
        .unwrap();
        let normalized = answer.answer.to_lowercase();
        let covered = expected
            .iter()
            .filter(|item| normalized.contains(item.as_str()))
            .count();
        println!(
            "LIVE_MEMORY_ASK={{\"run\":{run},\"durationMs\":{},\"expected\":{},\"covered\":{},\"abstained\":{},\"citations\":{}}}",
            started.elapsed().as_millis(),
            expected.len(),
            covered,
            answer.abstained,
            answer.citations.len(),
        );
        assert!(!answer.abstained);
        assert_eq!(covered, expected.len());
        assert!(answer
            .citations
            .iter()
            .any(|citation| citation.vault_path == expected_source));
    }
}
```

The expected-item file remains under ignored `output/` and is never committed.

- [ ] **Step 2: Run deterministic CI-equivalent checks**

Run:

```bash
cargo test
pnpm vitest run
pnpm lint
pnpm build
pnpm check:native
git diff --check
```

Expected: every command succeeds; the live test remains ignored in ordinary CI.

- [ ] **Step 3: Run the authorized real case five times**

Create `output/movable-ink-expected-items.txt` locally with the manually reviewed list-item headings from the real source. Then run:

```bash
AGENTIC_OS_LIVE_DB="$HOME/Library/Application Support/com.fchiodo.agentcontrol/agent-control.db" \
AGENTIC_OS_LIVE_EXPECTED_ITEMS="$PWD/output/movable-ink-expected-items.txt" \
AGENTIC_OS_LIVE_EXPECTED_SOURCE="_sources/work/2026-09-24-movable-ink-vfc-brands-use-case-submission-docx-5cdbd275.md" \
AGENTIC_OS_LIVE_RUNS=5 \
cargo test memory::retrieval::tests::live_ask_covers_expected_list_items -- --ignored --nocapture
```

Expected: five successful runs, zero abstentions, complete manually annotated item coverage, and at least one citation to the real source.

- [ ] **Step 4: Write the benchmark report**

Document the baseline audit observations and the five post-fix runs in `docs/benchmarks/MEMORY-ASK-P0-2026-09-24.md`. Include absolute counts for expected/covered items, accepted/rejected claims, citations, planner/synthesis/total duration, token availability, pipeline version, hardware/runtime context, and remaining limitations. Do not state that provider latency is solved or that the benchmark generalizes beyond the tested snapshot.

- [ ] **Step 5: Commit Task 4**

```bash
git add src-tauri/src/memory/retrieval.rs docs/benchmarks/MEMORY-ASK-P0-2026-09-24.md
git commit -m "test(memory): verify the live Movable Ink Ask path"
```

---

## Completion Gate

- [ ] `cargo test` passes with zero failures.
- [ ] `pnpm vitest run`, `pnpm lint`, `pnpm build`, and `pnpm check:native` pass.
- [ ] Existing role-reversal, negation, numeric, modality, and citation tests remain green.
- [ ] Audit trace records evidence metadata and per-claim codes without raw drafts or source text.
- [ ] Synthetic multiline bullets reflow within an item and never fuse adjacent items.
- [ ] Fourteen supported list items survive as fourteen atomic claims.
- [ ] More than sixteen items produces an explicit partial warning.
- [ ] The real Movable Ink case succeeds in five controlled runs with all manually annotated items and a real source citation.
- [ ] The report distinguishes retrieval coverage, answer coverage, accepted/rejected claims, and latency.
- [ ] Planner adaptation, semantic retrieval, persistent-memory evolution, and map changes remain outside this branch.
