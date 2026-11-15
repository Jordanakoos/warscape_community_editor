# F-B Phase 4 — production native battle continuity

Implemented locally on **`feature/native-battle-baseline`**, tested implementation **`da264468f6449b427ee13ea97b214eb1517d91e6`** (15 November 2026). Ready for independent review of the bounded native battle workflows below. Nothing has been pushed or submitted; owner acceptance of Phase 4 is still pending. Final documentation/evidence commits do not change production code.

## Provenance and scope

The isolated branch starts at `dbc40166bd8cc0f098cb71b335ecd9772870c6e4`, composing accepted self-contained Terrain Save/Export **`4c4351e2eb5a9e969c7a7a83b5eda68149957b06`** and native v9 safeguards **`c91f91005fabc84a8c6338fe1eb80340c8abc58f`**. It does not import D3C, held #52, or the Phase 3 test adapter. Shared publication/freshness infrastructure is unchanged from the accepted Save foundation; upstream Save As copy/no-retarget and recovery overwrite semantics are retained.

Prototype reconciliation: `1e01a61b44b1e97b62695ab31ec44d9746dd7ded` differs from tested `aae161ccb71cda69ad9f760e65e5ff45b4d792fb` only in five report/evidence files. No Rust or dependency manifest changes invalidate its test evidence. The production implementation replaces that adapter rather than merging it.

Upstream was assessed at [`1786457a4f671e1336775a09cfb89910b8da4791`](https://github.com/MrJox/warscape_community_editor/commit/1786457a4f671e1336775a09cfb89910b8da4791). Since the foundation's upstream ancestor `45fa5fe8`, changes concern Tile variations, their source resolution/validation, UI and launch behavior. Save/normal/autosave architecture and normal calculation/encoder sources have no intervening change requiring F-B adaptation. Those unrelated Tile features are not copied into this branch. This is an accepted-foundation composition, **not a claim that current master is an ancestor**; Work 3 owns eventual upstream packaging/rebase and must retain the newer Tile-source validation when composing it.

Scope is supported built/native **battle** LF/normal handling. Strategic campaign-normal reconstruction, raw terrain algorithms, Tile Editor Save architecture and game-runtime certification remain separate.

## Implemented contract and lifecycle

The [version 1 authoring-context contract](../formats/terrain_authoring.md) defines the format and operator commands. Each owner-relative native map locator references a content-addressed immutable original LF DDS, original encoded normal chunks including every mip, and original CHMF. SHA-256 inventories, profile, supported geometry/layout and calculation/encoder compatibility are verified. Original CHMF is retained so accumulated bounds and return-to-original behavior also remain deterministic.

Records live under `authoring/terrain/`, outside native map directories and export inputs. Effective pack sources can establish the explicitly selected original lineage; loose legacy resources require explicit adoption. Original data is pinned before publication and retained for the project lifetime. A completion inventory publishes after native height resources; partial or externally altered generations refuse compilation after reopen. Recovery verifies every donor before repair, from a supplied native source or currently mounted effective packs. It never substitutes intermediate compressed output while claiming historical continuity.

The native writer receives B0 explicitly and performs one normal calculation. No normal calculation or encoder mathematics changed. Save, Export and recovery share this authority. Ordinary Save As remains a standalone native copy, preserving dirty state and the active destination. Authoring-aware copies additionally carry verified originals and an independent relative locator; moving the complete owner root preserves continuity.

Autosave refers to a durable owner outside its session. Unchanged original normal chunks use existing file references; editable LF and modified chunks remain in the draft. Adoption/recovery changes advance its autosave stamp. Missing authority still permits an editable draft, but refuses compiled output. Deleting recovery sessions cannot delete the project's originals.

Relevant sources: [baseline store/verification](../../crates/wce_terrain_io/src/built/baseline.rs), [single-pass native writer](../../crates/wce_terrain_io/src/built.rs), [document authority](../../crates/wce_terrain_editor/src/authoring.rs), [Save/autosave publication](../../crates/wce_terrain_editor/src/document.rs), [production loader](../../crates/wce_terrain_tool/src/tool.rs), [project commands/UI](../../crates/wce_terrain_tool/src/tool/authoring.rs), [Export worker](../../crates/wce_terrain_tool/src/export.rs).

## Verification

The [sanitized evidence receipt](fb_phase4/evidence.json) records exact identities, outcomes and costs. All final production tests, builds, costs and application smoke use `da264468f6449b427ee13ea97b214eb1517d91e6`.

- **434 tests passed; 8 existing opt-in tests ignored**, across foundation, editor core, terrain, terrain I/O, terrain editor, terrain tool and tile. The parent additionally invokes and verifies **one fresh-process child**. [Permanent F-B regressions](../../crates/wce_terrain_editor/src/baseline_tests.rs) reuse the [shared synthetic Phase 1 fixture](../../crates/wce_terrain_editor/tests/support/native_battle.rs).
- Continuous A→B→Save, fresh-process reopen, Save between edits, Undo/Redo and original-LF return produce identical intended height-family output. Normal comparison covers encoded bytes and decoded components at **all eight mips**; LF samples and CHMF bytes also match. Edit A demonstrably changes normals. The unbound Phase 1 counterexample remains a negative control.
- Save As, authoring-aware copying/relocation, Export preparation, repeated autosave restoration and successful retry pass. Missing/corrupt originals, unknown versions/pipelines, geometry changes, extra normal chunks, partial publication, legacy adoption/refusal, key spelling and untouched resources are covered. Failures preserve pending LF/history/output.
- Production libraries pass Clippy with warnings denied. Terrain I/O/editor tests pass with the existing scratch-permission cleanup lint allowance; the new cost example also passed Clippy. Application plus Terrain `--all-targets` checks and the release application build pass. Broader Clippy attempts expose pre-existing UI-format, example and Terrain test style lints; those unrelated sources were not rewritten.
- Required `cargo fmt --all` encounters Windows error 206; the equivalent formatter/check runs cover all **31 members** with upstream configuration. `git diff --check` passes.

The non-activating [application smoke](../../scripts/terrain_native_baseline_smoke.py) passed **eight real WCE processes**, using the generated synthetic source, installed Rome II resources and an existing cache. It exercises project creation/open/reopen, actual brush/history edits, Save, standalone Save As, authoring-aware copy and relocation, loose/pack Export, real `autosave_now`/abandon/discovery/restoration, Cancel/Overwrite, missing-baseline refusal and verified repair. Every session ends with `quit`; no user-opened process is killed. All compared native outputs retain the original Phase 1 normal SHA-256:

`2ffa5f8f4bf4d8ed5dace39b67058b513c65a2d039f8b00c83ca278dbb32fe1e`

Final LF values at `(63,64)`, `(64,64)`, `(65,64)` are `10200`, `10300`, `10100`; final CHMF is 1,402 bytes. Tile-list and opaque-resource bytes remain unchanged. This is application integration evidence, not a game-runtime or rendering certification. Reproduction commands are in the [format document](../formats/terrain_authoring.md#reproduction); no private archive is required.

## Costs and remaining limits

Measured by the [counting-allocator example](../../crates/wce_terrain_editor/examples/native_baseline_cost.rs), excluding OS mappings, allocator overhead and GPU memory:

| Small fixture measurement | Bytes |
|---|---:|
| Original LF + normal + CHMF payload | 162,700 |
| Completed authoring record | 932 |
| Persistent authoring total | **163,632** |
| Extra retained heap after initial pin, relative to the unbound document | 2,040 |
| Context variable-size metadata capacity at initial pin | 1,849 |
| Extra heap peak during writer preparation | 1,342,700 |
| Extra heap peak during Save | 1,417,686 |

Metadata grows with inventory/path lengths and completed-generation entries; the idle figure is the initial-pin measurement, not a maximum for every project. Original payload is loaded temporarily for verified operations, not kept as a permanent second grid after pinning. The existing updater additionally caches decoded reached chunks. Using the previously available 8128×5748 / 736-chunk geometry gives **399.8 MiB for original LF plus encoded normals**, plus original CHMF/metadata, by arithmetic only. Large-map peak RAM/performance is unmeasured and can be substantially higher. No full-sized map collection or new research archive was created.

No known feature defect remains in the tested bounded workflows. Limits remain explicit: project-held originals consume disk space; historical originals cannot be inferred from arbitrary legacy edited maps; mounted-pack recovery requires exact matching bytes; unsupported profiles/layouts and unknown versions refuse compilation; no automatic migration, garbage collection, historical-pack search or whole-directory/power-loss transaction is introduced. A standalone native copy alone does not carry continuity. The original normal model remains an approximation, and full-size performance, additional real-map profiles and actual game-runtime output are not certified by this fixture.

The contribution is ready for independent code/format review. Maintainer review should assess the version 1 ownership/compatibility contract and packaging boundary; it is not a prerequisite left blocking this implementation. Work 3 retains publication coordination. All pre-existing accepted, published and prototype refs were compared with the start snapshot and remain unchanged; only the new feature branch was added. No PR was opened or modified.
