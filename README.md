# Agent Control

Desktop control plane for local agents, skills, routines, and Codex usage data.

## Stack

- Tauri 2 for the desktop shell and native commands
- React + TypeScript + Vite for the UI
- TanStack Query for native data hydration
- Zustand for persisted workbench state
- Rusqlite for read-only access to local Codex databases

## What is already wired

- Catalog discovery for:
  - `~/.codex/skills`
  - `~/.codex/.tmp/plugins/plugins`
  - `~/.codex/routines`
  - workspace `skills/`, `agents/`, `routines/`, and `prompts/`
- Usage and activity reads from:
  - `~/.codex/state_5.sqlite`
  - `~/.codex/logs_2.sqlite`
- Three primary views:
  - `Catalog`
  - `Runner`
  - `Memory`
  - `Usage`

## Project layout

```text
src/
  app/                  router + providers
  components/           layout and UI primitives
  features/
    catalog/            inventory browsing
    dashboard/          native data contract and query
    runner/             prompt/routine staging surface
    usage/              token and workspace telemetry
  lib/                  formatting and platform helpers
  store/                persisted UI state

src-tauri/
  src/
    commands.rs         Tauri commands exposed to the UI
    discovery.rs        local file and plugin inventory
    models.rs           shared response payloads
    snapshot.rs         database reads + composed dashboard snapshot
```

## Commands

```bash
pnpm install
pnpm dev
pnpm dev:desktop
pnpm build
pnpm check:native
pnpm build:desktop
```

## Next increments

1. Add a secure routine execution adapter with explicit allowlists.
2. Persist run history in an app-owned SQLite database.
3. Add pricing tables so token usage can roll up into estimated and billed cost.
4. Add writable settings for source roots and workspace-specific launch targets.
