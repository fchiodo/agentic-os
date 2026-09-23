# Second Brain milestone 2 benchmark

Measured on 2026-09-23 on Apple M2 / arm64 / macOS 26.4.1 with the Rust debug
test build. The fixture contains six realistic, human-confirmed question/source
pairs (one per domain) and 1,200 indexed distractor memories. It uses the same
retrieval and composite scoring functions as Search and Ask; no outbound model
call is part of this measurement.

| Profile | Top-1 | Hit@5 | Recall@5 | MRR | p50 | p95 | Outbound cost |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| FTS5 baseline | 100% | 100% | 100% | 1.00 | 1.40 ms | 3.47 ms | $0 |
| Candidate: FTS5 + aliases + fuzzy trigram | 100% | 100% | 100% | 1.00 | 11.26 ms | 13.39 ms | $0 |
| Production flags | 100% | 100% | 100% | 1.00 | 1.35 ms | 1.42 ms | $0 |

The fuzzy candidate scanned all 1,206 eligible rows. A separate regression test
uses 2,006 rows and puts the only matching source behind 2,005 newer rows; it is
still retrieved, proving that the former 2,000-row truncation is gone.

The real-registry orbital composition test on the same machine discovered 625
skills and 208 applications, produced 857 typed relations, and completed in
691.84 ms. The isolated demo vault correctly reported zero memories and the
registry reported zero routines; the corresponding radial guides remain
visible without inventing placeholder records. Registry discovery dominates
that cold composition time and is the next optimization target.

Interpretation: on this fixture the fuzzy lane adds no correctness benefit and
adds roughly 10 ms p95, so it remains off by default. This result validates the
measurement path and the corpus-limit fix; it is not evidence that embeddings
are unnecessary. Semantic search remains `not_configured` until a real local
embedding backend has its own quality, latency, storage and cost comparison.
Six questions also do not establish general Ask quality, and the empty Memory
and Routines rings do not validate expansion fluidity for a complete working
vault. These remain explicit milestone-closure gates rather than inferred
successes.

Reproduce with:

```bash
cargo test memory::retrieval::tests::benchmark_uses_confirmed_questions_and_the_production_pipeline -- --nocapture
```

The deterministic Ask suite additionally rejects uncited claims, polarity
reversals, changed subjects/numbers/dates, same-token role reversals, a number
attached to the wrong subject, and role reversal for predicates outside the
recognized relation list. It also verifies that attribution and uncertainty are
retained by returning the full source sentence for unknown predicates. The
orbital suite proves that text-derived catalog references remain inferred and
includes an execution-to-audit-to-observed-relation test for a completed MCP
tool event. Full native and frontend verification for this run: 62 Rust tests
and 8 Vitest tests passed; TypeScript build and ESLint also passed.
