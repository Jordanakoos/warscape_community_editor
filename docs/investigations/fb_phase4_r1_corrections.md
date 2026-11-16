# F-B Phase 4 — independent R1 corrections

Local correction branch: **`fix/native-battle-baseline-r1`**, based directly on rejected candidate **`11ff9ce74a12e94a933dcc9f0f445b9ec47656f7`**. Tested implementation: **`4cdfe730559167a417a2f546bacb5b45c06febd5`** (16 November 2026). Subsequent documentation/evidence changes do not alter executable sources. Ready for focused independent **R2 by Work 8**, not owner acceptance or upstream publication.

The accepted Save/Export and #50 composition remains the candidate's foundation. Normal calculation, encoding, native writer mathematics and shared publication/freshness infrastructure are unchanged. No campaign-normal reconstruction, held #52, unrelated Tile work or general Save redesign is included. The [authoring-context contract](../formats/terrain_authoring.md) documents the corrected lifecycle; the [sanitized evidence receipt](fb_phase4_r1/evidence.json) records source identities, assertion outcomes, log fingerprints, application receipts and measurements.

## R1–R5 closure

The eight desired-behavior regressions are frozen in [baseline_r1_tests.rs](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs), registered under `baseline_tests::r1_corrections`. The names below are suffixes within that module. Each fails on the rejected implementation and passes on the correction. The original review probes asserted the defects; their passing results are **not** correction acceptance.

| Finding | Production correction | Passing permanent regressions |
|---|---|---|
| **R1: authority lost through recovery** | [authoring.rs](../../crates/wce_terrain_editor/src/authoring.rs): recovery v2 retains `blocked` plus reason and expected identity, independently of LF edits. Restore checks the owner's generation, not dirty draft samples. Overwrite cannot promote blocked authority; explicit verified reconciliation can. | [`r1_known_generation_block_survives_repeated_recovery_and_overwrite`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L23): external damage and partial publication, repeated recovery, failed Overwrite, unchanged output/history and successful explicit repair. |
| **R2: incomplete generation accepted** | [baseline.rs](../../crates/wce_terrain_io/src/built/baseline.rs): `complete_family` requires exactly the descriptor's keys; `verify_generation` validates the complete current inventory. Empty anchors require B0 equality. Invalid completion retains a valid descriptor for repair but never grants authority. | [`r2_interrupted_first_publication_is_blocked_until_explicit_reconciliation`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L64); [`r2_line_boundary_truncation_cannot_omit_a_normal_from_generation_verification`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L84). Both refuse before output replacement, then reconcile explicitly. |
| **R3: missing original prevents LF recovery** | [document.rs](../../crates/wce_terrain_editor/src/document.rs): `complete_autosave` tolerates only unavailable references recognized by `Authoring::original_recovery_ref` as pinned originals. The intact LF draft opens blocked, keeps its expected identity, remains editable and can be repaired. No per-autosave original-normal duplication. | [`r3_missing_original_still_opens_intact_editable_draft_and_supports_repair`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L111): missing pinned normal, original-LF dirty draft, repeated autosave, refusal and byte-exact repair. |
| **R4: nested copy mutates source before refusal** | [document.rs](../../crates/wce_terrain_editor/src/document.rs): pure overlap checks precede context pinning. [baseline.rs](../../crates/wce_terrain_io/src/built/baseline.rs): alias-aware overlap checks, write-free `plan_copy`, then `materialize_copy` only after native staging/validation. Completion publishes last; ordinary Save As remains copy/no-retarget. | [`r4_rejected_nested_copy_is_read_only_for_source_destination_and_document`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L140); [`r4_source_map_overlap_is_checked_even_when_owned_map_is_elsewhere`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L155). Native inventory/bytes, destination absence, history, dirty state and destination identity remain unchanged. |
| **R5: unnecessary retained normal inventory** | [document.rs](../../crates/wce_terrain_editor/src/document.rs) and [export.rs](../../crates/wce_terrain_tool/src/export.rs): bound B0 bypasses legacy source snapshots; unchanged outputs do not snapshot. Actual unbound replacements retain protection. | [`r5_single_chunk_reopen_and_noop_save_retain_no_encoded_normal_snapshot`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L175); [`r5_four_chunk_reopen_and_noop_save_retain_no_duplicate_inventory`](../../crates/wce_terrain_editor/src/baseline_r1_tests.rs#L190). Separate Export control verifies required unbound snapshots. |

[Compatibility controls](../../crates/wce_terrain_editor/src/baseline_r1_controls.rs) additionally cover legitimate dirty drafts, version 1 recovery with a damaged owner, and refusal for an unrelated missing reference. Existing live partial-publication retry, recovery overwrite prompts and ordinary Save As tests remain passing.

Project context and baseline formats stay at version 1. **Recovery-slot metadata advances to version 2** to retain authority verdicts. Version 1 slots remain readable with owner revalidation; old WCE versions cannot read new slots. Missing/changed authority never silently selects intermediate compressed output as B0. An interrupted first publication reopened from disk may require explicit reconciliation; a supported live retry retains its in-memory authority.

## Verification and reproduction

Tests were committed first at **`11d5ec3db355ecd950e7ede9c367056a41c33b15`**, with production sources identical to the rejected candidate. Its only additions are the eight tests, registration and shared synthetic multi-chunk fixture. Result: **0 passed, 8 failed**. The test file remains byte-identical (SHA-256 `a816e795249941446b595964a74902758ad9fab38d63457c05111653a0266052`); corrected result: **8 passed, 0 failed**. This is a desired-behavior failure-to-pass comparison, not a reversal of the review's defect assertions.

On the tested implementation:

- **446 tests passed, 0 failed, 8 existing opt-in tests ignored**, across foundation, editor core, terrain, terrain I/O, terrain editor, terrain tool and tile. The continuity parent also verifies a fresh-process child. The total includes the eight frozen regressions, three compatibility controls and one Export snapshot control added to the previous 434-test suite.
- The original continuous-output normal SHA-256 remains **`2ffa5f8f4bf4d8ed5dace39b67058b513c65a2d039f8b00c83ca278dbb32fe1e`**. Encoded bytes and decoded components at **all eight mips**, LF samples, CHMF and untouched resources match across continuous, reopened, Save-between, Undo/Redo/original return, Save As, relocation, Export, recovery and retry sequences. Edit A still changes normal output.
- **#50 real native witness check: 1 additional opt-in test passed**, exercising existing 555,424- and 562,525-row files in place. Byte identity and paired-row safeguards pass; before/after source hashes match. No native copies were created.
- Production libraries passed Clippy with warnings denied; I/O/editor test Clippy passed with the existing scratch-permission cleanup allowance. The cost example passed Clippy. Application/Terrain all-target checks and the release application build passed. Formatting checked all 31 workspace members through the existing per-member fallback after Windows error 206 from `cargo fmt --all`; `git diff --check` passes.

One existing freshness fixture formerly changed an unpublished native normal before document creation. R2 correctly makes that an authority failure. It now changes `opaque.bin` to isolate the same enumeration-freshness behavior, retaining its original freshness assertions. The new R2 tests cover the height-family refusal explicitly; no normal reference expectation was regenerated.

The rebuilt executable (SHA-256 **`194b53229ca25ba3a715c042db6b2f0f8c7f1f612c1e9511258f8e8fe5ae23a3`**) passed:

- **Eight original application sessions**: real loader/project creation/open/reopen, brush/history edits, Save, standalone Save As, authoring-aware copy/relocation, loose/pack Export, real autosave discovery/restore, Cancel/Overwrite, missing-baseline refusal and verified repair.
- **Thirteen adversarial application sessions** using [the permanent R1 script](../../scripts/terrain_native_baseline_r1_smoke.py): blocked recovery and repeated recovery across fresh processes; missing-original editable recovery and verified repair; read-only nested-copy rejection; interrupted-first-publication and line-boundary-truncated completion refusal. All processes exit successfully with no unexpected error log entries.

These are production application paths using synthetic fixtures and existing Rome II/cache resources, background launch and `quit` in every script. They do not certify game-runtime rendering. Detailed reproduction is in the [format document](../formats/terrain_authoring.md#reproduction). Focused commands:

```text
cargo test --release --locked --offline -p wce_terrain_editor --lib r1_corrections -- --test-threads=1
cargo test --release --locked --offline -p wce_foundation -p wce_editor_core -p wce_terrain -p wce_terrain_io -p wce_terrain_editor -p wce_terrain_tool -p wce_tile
cargo run --release --locked --offline -p wce_terrain_editor --example native_baseline_cost -- <new-root>
```

## Post-reopen and no-op-Save retention

Measured by the [counting-allocator example](../../crates/wce_terrain_editor/examples/native_baseline_cost.rs), with the shared Phase 1 fixture and the existing review's 512² LF / four 576² DXT5 chunk geometry, now factored into [shared fixture code](../../crates/wce_terrain_editor/tests/support/native_battle.rs). The single case retains the original edited golden output; the four-chunk case measures unchanged output. Both drop the first document and reopen actual native files before measuring two no-op Saves.

| Measurement (bytes) | One chunk | Four chunks |
|---|---:|---:|
| Encoded normal inventory | 87,520 | 1,770,560 |
| Rejected candidate retained snapshot after no-op Save | 87,520 | 1,770,560 |
| Corrected retained snapshot after reopen / first no-op / second no-op | **0 / 0 / 0** | **0 / 0 / 0** |
| Actual net retained heap change, first / second no-op Save | 184 / 0 | 72 / 0 |
| No-op Save extra heap peak | 1,417,981 | 4,407,052 |
| Original payload + completed record on disk | 162,700 + 932 | 2,303,976 + 1,496 |
| Initial pin extra retained heap | 2,084 | 3,083 |

Net retained heap includes small bookkeeping changes. The zero figure is specifically the encoded-normal source-snapshot cache, not the entire document. Measurements exclude allocator headers, OS mappings, GPU memory and RSS. Temporary verified B0 loads and decoded reached-chunk allocations remain; there is no larger-map performance claim or new full-size resource collection.

## Preservation, limitations and R2 handoff

All **62 pre-existing refs** match the start snapshot, including the rejected candidate, accepted Save/Export, published and prototype branches. Only this correction branch was added. All **1,499 original review files** retain their inventory, lengths and SHA-256 hashes, including the review executable. Review report SHA-256: `901719870bbe256d565d2d5bdd3b789ce99c12bb9e099e007d8fd74d249fa668`. Calculation/encoder sources, native writer, shared publication implementation, #50 sources and dependency manifests are unchanged from the rejected candidate. No PR, merge, push or upstream packaging was performed.

No known R1–R5 defect remains in the tested scope. Independent R2 should focus on authority versus draft state, recovery v1/v2 compatibility, exact inventory completeness, copy preflight ordering and snapshot eligibility. Limits remain: publication is per-file rather than a power-loss transaction; I/O failure after staging can leave partial native publication or unreferenced originals; unknown/malformed authority may require verified repair or project backup; unrelated missing recovery inputs still refuse; historical originals are not inferred; no automatic store cleanup/migration is introduced. Larger-map RAM/performance and actual game-runtime output remain unmeasured. This candidate is returned for review, with publication still owned by Work 3.
