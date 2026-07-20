# Agents Guidance

## Project overview

Agentic OS is a local-first desktop control plane for personal agents, skills, routines, and usage data. It runs entirely on the user's machine with no exposed ports or public gateway.

**Stack:** Tauri 2 (Rust core) + React 19 + TypeScript + Vite, with TanStack Query, Zustand, and rusqlite for local database access.

**Primary goal:** a single local system that discovers skills/agents/routines, executes tasks through harness adapters (Codex, Claude, ACP), maintains memory, and provides observability — all without cloud dependencies for the control plane itself.

## Architecture principles

- **Always local.** No listening ports, no public gateway. Tauri IPC is the only trust boundary. Outbound calls are limited to model providers and approved connectors.
- **Deterministic shell, agentic core.** Authorization, credentials, approvals, persistence, and budgets are deterministic Rust services. LLMs handle interpretation, planning, and synthesis only.
- **Single user, multiple domains.** Strict data separation between `work`, `planphysique`, `personal`, `family`, `finance`, `research`.
- **Harness adapters, not a custom agent loop.** Codex, Claude, and ACP are spawned as child processes; Agentic OS orchestrates them rather than reimplementing agent runtimes.
- **SQLite as system of record.** WAL mode, append-only events, hash-chained audit. No external databases or brokers at desktop scale.

## Repository layout

```
src/                        React UI
  app/                      router + providers
  components/               layout and UI primitives
  features/                 feature modules (catalog, runner, usage, approvals, audit, memory, dashboard, control)
  lib/                      formatting and platform helpers
  store/                    persisted Zustand state (task-events, workbench)

src-tauri/                  Rust core
  src/
    commands.rs             Tauri commands exposed to the UI
    orchestrator.rs         task state machine
    policy.rs               deterministic policy engine
    approval.rs             approval queue management
    audit.rs                hash-chained audit trail
    discovery.rs            catalog discovery (skills, plugins, routines)
    snapshot.rs             composed dashboard snapshot from SQLite
    harness/                harness adapters (codex child process)
    db.rs                   SQLite connection wrapper
    error.rs                AppError enum with thiserror
    models.rs               shared response payloads (DashboardSnapshot)
    control_models.rs       control-plane types (TaskSummary, TaskDetail, etc.)

docs/                       architecture and UI spec
policies/                   versioned TOML policy rules (future)
agents/                     domain agent configurations (future)
workflows/                  workflow definitions (future)
```

## Development commands

```bash
pnpm install               # install JS dependencies
pnpm dev                   # Vite dev server on port 1420
pnpm dev:desktop           # Tauri dev (Rust + web)
pnpm build                 # tsc + vite build
pnpm build:desktop         # Tauri production build
pnpm check:native          # cargo check on src-tauri
pnpm lint                  # eslint
```

Frontend dev server runs on `localhost:1420`. Tauri desktop uses that as `devUrl`.

## Code conventions

### TypeScript / React

- **React 19** with functional components and hooks only.
- **Zustand** for client state. Persisted stores use `zustand/middleware/persist` with `partialize` to select what hits localStorage. Store files live in `src/store/`.
- **TanStack Query** for server state and Tauri command hydration. Query hooks live alongside their feature in `src/features/*/hooks.ts`.
- **Feature modules** in `src/features/` — each feature owns its page component, API layer, schema, and hooks. Keep cross-feature imports minimal.
- **Path alias:** `@/` maps to `src/` (configured in tsconfig and vite).
- **Zod** for runtime schema validation where data crosses the Tauri IPC boundary.
- **Lucide React** for icons. Use lucide icons in buttons; avoid hand-drawn SVGs.
- **CSS:** Tailwind-style utility classes. No CSS modules or styled-components.
- **No mock data in production paths.** Mock data files (`mock-data.ts`) exist for feature development but must not be imported in production query hooks.

### Rust

- **Edition 2021**, MSRV 1.77.2.
- **thiserror** for error types. All commands return `Result<T, String>` to Tauri (map `AppError` via `.map_err(|e| e.to_string())`).
- **serde** for all serializable models. Derive `Serialize` on response types in `models.rs` / `control_models.rs`.
- **rusqlite** with `bundled` feature. All DB access goes through `db::Db`. Never open connections directly.
- **Commands** are registered in `lib.rs` via `tauri::generate_handler![]`. New commands must be added there.
- **Module boundary:** each Rust module (`orchestrator`, `policy`, `approval`, `audit`, etc.) is a domain boundary. Modules communicate through their public API, not by reaching into each other's internals.

### Tauri

- Capabilities are declared in `src-tauri/capabilities/`. New IPC permissions must be explicit.
- The app data directory holds `agent-control.db`. Database path is resolved in `lib.rs::setup`.
- `tauri-plugin-log` is enabled in debug builds only.

## Testing

- **Frontend:** Vitest + jsdom + @testing-library/react. Tests live next to the component they cover (e.g., `app-shell.test.tsx`).
- **Run tests:** `pnpm vitest` (or `pnpm vitest run` for single pass).
- **Rust:** `pnpm check:native` runs `cargo check`. Add `#[cfg(test)]` modules for unit tests in Rust files.
- **Coverage:** test any new Tauri command, schema validation boundary, or state machine transition. At minimum, cover the happy path and one error path per new feature.

## Review expectations

When reviewing or writing code in this repo:

1. **New Tauri commands** must be registered in `lib.rs`, return `Result<T, String>`, and use the existing `Db` state.
2. **New features** should follow the feature-module pattern: `features/<name>/` with `*-page.tsx`, `api.ts`, `schema.ts`, `hooks.ts`.
3. **Database schema changes** must be backward-compatible or include a migration path. The DB is the system of record — treat schema changes with the same care as API contracts.
4. **No secrets in prompts, env files, or the database.** Secrets live in macOS Keychain (future: Tauri keyring plugin).
5. **Policy rules are deterministic.** Never route policy decisions through an LLM. Policy evaluation is pure Rust logic.
6. **Audit trail is append-only.** Every state transition and side effect must write an event row. The hash chain must never be broken.
7. **Domain isolation.** Data from one domain must not leak into another's context without explicit cross-domain approval.
8. **Harness child processes** must be spawned with sandboxing and policy profiles. Never trust raw LLM output as a command — validate through the tool gateway.

## Key patterns

### Tauri command pattern
```rust
#[tauri::command]
pub fn my_command(db: State<'_, Db>) -> Result<MyResponse, String> {
    my_module::do_work(&db).map_err(|e| e.to_string())
}
```
Register in `lib.rs` → `tauri::generate_handler![]`.

### Frontend query pattern
```typescript
// features/my-feature/hooks.ts
export function useMyData() {
  return useQuery({
    queryKey: ['my-data'],
    queryFn: () => invoke<MyResponse>('my_command'),
  })
}
```

### Zustand store pattern
```typescript
// Persisted stores use partialize to control what goes to localStorage
// Task event stores use in-memory only (no persistence)
```

## Roadmap context

The project follows a phased build-out per `docs/ARCHITECTURE.md`:
- **Phase 1 (current):** Core OS — task model, orchestrator, Codex harness, audit, policy v0, approval inbox, token/cost capture.
- **Phase 2:** Memory and context — markdown vault, FTS, write pipeline, "distill to skill".
- **Phase 3:** Workflows and scheduler — daily brief, meeting-to-memory, newsletter QA.
- **Phase 4:** Connectors — MCP gateway, Claude/ACP adapters, GitHub, Databricks, MS Graph.
- **Phase 5:** Domain agents — PlanPhysique, Research, Finance, PersonalOps.

When working on this codebase, respect the current phase scope. Don't add infrastructure or abstractions that belong to a later phase unless explicitly asked.
