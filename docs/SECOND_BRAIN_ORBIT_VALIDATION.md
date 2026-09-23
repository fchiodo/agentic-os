# Second Brain orbital map — implementation and validation

Validated branch: `fix/second-brain-orbit-review`

Base: `ec05f36` (`master`, initial orbital-map milestone). The branch is intentionally local and is not merged or pushed.

## Review corrections after `ec05f36`

- Memory-sector guide anchors are transparent, renderable Sigma nodes instead of hidden nodes, so all six separators remain visible in WebGL.
- Activity task selection and inspected-node selection are independent. Inspecting a linked memory, skill, routine, or application retains the task and its observed links.
- Evidence and relation-type filters apply to selected-node links, advanced Structure links, and Activity links. Filtering an edge also closes its stale edge detail.
- Evidence actions are routed by provenance: audit references open Audit, catalog references open Catalog, vault paths open Library documents, and document-import references open their governed original source.
- Audit no longer falls back to an unrelated run when a requested trace is absent; it shows an explicit not-found message with no trace selected.
- Search reports the complete authorized match count and reveals results in pages of 40 instead of silently truncating.
- “Today” starts at local midnight and converts that boundary to UTC. Tests cover both the Italian UTC+1 and UTC+2 offsets; “Last 7 days” remains a rolling 168-hour interval.

## Delivered behavior

- Four persistent concentric bands around Agentic OS: Skills, Memory, Routines, and Applications.
- Six deterministic Memory sectors (`work`, `planphysique`, `personal`, `family`, `finance`, `research`), including explicit empty sectors.
- Aggregate-first rendering with bounded group sizes, a 48-item initial expansion limit, and incremental “show more”.
- Separate selection and expansion actions, breadcrumb, back/reset/center controls, Escape and background dismissal, and an equivalent keyboard-accessible list.
- Search across the full authorized payload, including children of closed groups; selecting a result opens its group, detail, and focus.
- No `registers` starburst in the overview. Incident relations appear on selection; advanced relation rendering is opt-in, evidence/type-filtered, and capped at 60/120/240 edges.
- Declared, observed, and inferred evidence remain labeled in the detail and legend. Observed links retain their audit reference, timestamp, operation, and outcome.
- Structure and Activity are separate modes. Activity supports Today and Last 7 days and only promotes structured executor/memory references. A task without trace telemetry displays “Data not available”.
- User-first detail panel with technical IDs and paths moved into a disclosure; application catalog, connection, and observed-usage states remain distinct.
- Governed navigation to Library documents/import sources, Catalog entries, and Audit traces. Graph clicks never execute tools or routines.
- Dark map canvas with a maintained light variant, fixed category icons, bounded glow, responsive drawer detail, visible focus, and reduced-motion parity.
- Renderer reuse, stable hashed coordinates, camera preservation on data refresh, and 300 ms action-triggered focus/expand transitions.
- Runtime performance disclosure for backend payload composition, first useful render, graph update, search, expansion, and selection.

The Markdown/Git vault remains authoritative, SQLite remains a rebuildable index, and no graph database, 3D engine, or retrieval rewrite was introduced.

## Visibility and evidence boundaries

- Domain, lifecycle, and sensitivity filters run in Rust before nodes, relations, activity, previews, and counts are built.
- Original imported-source metadata is excluded unless sensitive content is explicitly enabled. Even then, source bodies are never included in the graph payload.
- Activity ignores free-text command/path matches and `catalogRefs`; those can remain inferred in Structure but can never become observed Activity.
- Context events are described as “inserted into context”; the UI does not claim that the memory caused the answer.
- Related-memory, original-source, and supersession links come only from governed Markdown frontmatter.

## Measurements

Reference device: MacBook Air, Apple M2 (8 cores), 8 GB RAM, arm64, macOS 26.4.1.

| Stage | Dataset | Result |
| --- | --- | ---: |
| Rust payload composition | Real local registries; 625 skills, 0 indexed memories, 0 routines, 209 applications, 858 relations, 25 top-level nodes | 1,031.672 ms |
| Deterministic layout | Clearly separated synthetic fixture: 671 nodes (561 skills and 100 applications plus aggregates/sectors) | 0.508 ms |
| First useful render after payload | Browser preview, authorized one-node fallback | 11.3 ms |
| Graph update including intentional transition | Browser preview, one-node fallback, 300 ms transition enabled | 306.0 ms |
| Search | Browser preview, one-node fallback | below displayed 0.01 ms precision |

The representative layout benchmark has a test budget of 100 ms. Interaction tests cover selection, expansion/search, complete search counts, Escape dismissal, refresh camera stability, evidence filtering, provenance routing, and Activity telemetry/task-selection states. The in-app performance disclosure is the source for measurements on a populated vault because it measures the currently authorized payload without exporting its contents.

The live desktop database used for the final smoke run contained no memories, tasks, imports, or audit events. Therefore the Tauri run validated the real catalog and explicit empty states, while populated group/search/activity behavior was validated with the clearly isolated test fixture. No data was seeded into the user's vault to make the demonstration look fuller.

## Verification completed

- `pnpm build` — passed.
- `pnpm lint` — passed.
- `pnpm vitest run` — 24 tests passed across 5 files.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` — 89 tests passed.
- `pnpm check:native` — passed.
- `pnpm dev:desktop` — Tauri debug binary compiled and launched against the real local database.
- Browser WebGL visual QA — the real Sigma renderer showed all concentric rings and all six Memory separators at desktop width in both map themes. The visual captures belong to the review session; the browser preview remains clearly identified as non-Tauri data.
- Release application bundle — `pnpm exec tauri build --bundles app` passed; a local ad-hoc signature passes `codesign --verify --deep --strict`. The bundle is not Developer ID signed or notarized, so it is a local test artifact rather than a distributable release.
- Release smoke — the compiled app launched successfully against an isolated temporary database and vault, leaving the user's real empty vault unchanged.

Run the desktop app with:

```bash
pnpm dev:desktop
```

Compiled app:

```text
/Users/fabiochiodo/Documents/agentic-os/src-tauri/target/release/bundle/macos/Agentic OS.app
```

On a machine that still reports an unaccepted Xcode license, accept it once with `sudo xcodebuild -license`, then rerun the command.

## Current telemetry limits

- Completed MCP calls emit structured application execution references and can appear as observed.
- Memory context/output links appear only when the event contains a structured `memoryRefs` or `memoryId` field.
- Skill and routine use appears as observed only when their real executor emits a complete `executionRefs` envelope. Text/path matches remain inferred in Structure and are absent from Activity.
- Historical Activity is limited to the audit rows currently retained by the desktop backend (the map scans at most the latest 5,000 relevant rows and returns at most 100 tasks per interval).
- Browser preview intentionally has no access to Tauri desktop registries; its empty authorized fallback is not evidence of desktop counts.
