# Second Brain 3D visualization — implementation and validation

The former radial Sigma map has been replaced by an in-app 3D knowledge globe
adapted from the MIT-licensed AIS-OS `3d-brain` skill. The renderer runs inside
the existing lazy-loaded Memory feature and consumes the governed
`memory_orbit_map` payload directly. It does not add a server, port, graph
database, filesystem scan, or new trust boundary.

## Delivered behavior

- A deterministic spherical composition with Agentic OS at the glowing center.
- Distinct sectors and colors for Skills, Memory, Routines, and Applications;
  Memory retains its domain colors.
- Drag-to-orbit and scroll-to-zoom controls with a bounded automatic rotation.
- A 29-second connectivity replay that reveals branches only through real
  payload edges. Disconnected nodes enter as separate roots without invented
  relationships.
- Full-brain replay: starting the animation reveals every authorized node in
  the current domain and sensitivity scope, including children hidden by the
  normal aggregate-first view.
- Cinema mode, pause/resume motion, a progress caption, and reduced-motion
  behavior that completes the replay without forced animation.
- Existing search, domain and sensitivity filters, Structure/Activity modes,
  provenance details, Catalog/Audit/Library navigation, bounded expansion,
  and the keyboard-accessible list remain available.
- Node and link clicks are ignored during replay, matching the reference
  behavior, while camera orbit and zoom remain active and do not stop growth.

The Markdown/Git vault remains authoritative and SQLite remains the system of
record. The UI receives only the data already filtered and authorized by Rust.
The 3D renderer never reads files or executes tools.

## Source and dependency record

The spherical layout, central orb, orbital accents, and growth-planning behavior
are adapted from:

```text
https://github.com/nateherkai/AIS-OS/tree/main/.agents/skills/3d-brain
```

AIS-OS is MIT licensed, copyright © 2026 Nate Herk. The runtime uses
`3d-force-graph` 1.80.0 and Three.js 0.185.1. Attribution is also recorded in
`THIRD_PARTY_NOTICES.md`.

## Automated verification

- `orbit-globe-model.test.ts` covers deterministic placement, real-edge growth,
  disconnected components, spring origin, and exact final positions.
- `orbit-map.test.tsx` covers selection versus focus, search into closed groups,
  bounded expansion, Activity evidence, relation filters, provenance routing,
  full search counts, growth controls, and Cinema mode.
- `memory-page.test.tsx` covers navigation between Library and 3D Brain.
- `pnpm lint`, `pnpm test`, and `pnpm build` are the required frontend gates.

The production build reports the existing large lazy chunk warning for the 3D
renderer. The renderer remains route-level lazy-loaded, so Library users do not
download it until they open 3D Brain.

## Manual visual checks

Run `pnpm dev` and open Memory → 3D Brain. Verify the central orb, category
colors, labels, drag/zoom, Play growth, Pause motion, Cinema, node selection,
and the narrow layout. Browser WebGL visual automation was not available in the
implementation environment, so these appearance checks are intentionally not
claimed as automated.
