# Native battle authoring context, version 1

WCE keeps a battle map's original LF, original encoded normal chunks (all mip levels), and original CHMF outside the native map. The native writer takes these immutable inputs directly. For the same verified baseline, supported geometry and final LF samples, session boundaries do not change the normal output. This contract concerns the existing native battle delta-normal calculation, not campaign-normal reconstruction or game-runtime certification.

## Ownership and layout

An explicit owner root contains:

```
authoring/terrain/maps/<map>.wce
authoring/terrain/baselines/<baseline-sha256>/lf_height_map.dds
authoring/terrain/baselines/<baseline-sha256>/lf_height_map.data
authoring/terrain/baselines/<baseline-sha256>/normals/normal_<n>.dds
```

The map entry holds an owner-relative native-map path. The normal project loader uses the configured per-game projects root and its existing `assembly_kit/working_data/terrain/battles/<map>` destination. Explicit portable projects use `native/<map>`. Copying the owner root preserves this relationship. Native files and game export packages contain none of the above records.

The baseline store is project data, not a disposable cache. WCE does not automatically delete it when a document closes, a session ends, an autosave is discarded, or a newer Save completes. Identical baseline IDs within one owner share storage. No cross-project hard links or automatic garbage collection are introduced. External manual backup/deletion must include the authoring directory.

## Encoding and verification

Records are UTF-8 with LF separators. Resource keys are lowercase, reader-relative paths accepted by the publication path validator. Schema/version, game profile, geometry and resource inventory are bounded and validated; unrecognized versions are refused. Resource inventories contain tab-separated key, decimal byte length and lowercase SHA-256. Keys, lengths and hashes cover the complete encoded bytes, including DDS headers, tails and all mip levels.

The map entry is:

```
WCE-TERRAIN-CONTEXT-1
<owner-relative native path>
<baseline id>
WCE-NATIVE-BATTLE-BASELINE-1
<calculation/encoder compatibility identity>
<game key>
<LF width> <LF height> <cells x> <cells y>
<baseline resource inventory, each line terminated by LF>

--generation--
<last completed native height-family inventory, or empty before first Save>
```

The baseline ID is SHA-256 of the baseline descriptor body, starting at its magic and ending after the inventory's final LF. The compatibility identity starts with `native-delta-dxt5-1` and fingerprints the existing normal calculation and DXT5 encoder sources. It is intentionally conservative: a changed identity needs an explicit compatibility/migration decision, never automatic recapture. The generation inventory records current compiled LF/CHMF/normal bytes; it does not replace B0.

Verification checks exact resource bytes, the supported L16 LF encoding, LF/cell dimensions, CHMF region geometry, contiguous DXT5 chunks, supported chunk layout, and decodability of every mip. Current map source/destination freshness remains a separate Save safeguard. Overwrite consent never authorizes replacing B0 or accepting an unknown authoring version.

## Lifecycle and failure behavior

Opening a supported battle from effective pack bytes captures those explicitly selected source resources; this does not assert that modded pack data is vanilla. The first Save, Export or recovery copy pins them durably. A loose native project without a record is legacy/unverified. Viewing and unchanged resource copying remain possible; LF-affecting compilation requires explicit new-lineage adoption. Adoption uses the originally opened native resources and verifies that their LF still matches the initially loaded LF. Pending edits/history survive adoption. An existing record cannot be replaced through adoption.

Save stages native output as before. It publishes supporting resources before LF and publishes the authoring completion entry last. The record is checked for external changes before publication. This extends the existing per-file protocol; it is not a directory transaction or power-loss journal. A partial height-family publication detected at a subsequent Open blocks compilation. Explicit verified-original recovery/reconciliation, or restoration of an intact project backup, is required. In-process retries retain B0 and the pending edits.

Missing/corrupt originals refuse compiled Save/Export before native replacement. Recovery checks the entire supplied donor against the recorded baseline identity before repairing any stored resource. Current edited compressed normals are never silently adopted as historical originals. Unknown versions and geometry remain blocked. An explicit new project root can start a deliberately chosen new lineage without overwriting the old one.

Ordinary **Save As remains a standalone native copy**, without retargeting or clearing dirty history. It is usable by the game, but its native files alone do not carry authoring continuity. **Save authoring-aware copy** also copies the verified baseline and creates a destination-owned entry, while leaving the source document and dirty state intact. Moving that complete owner root is portable; moving only its native directory is not.

Autosaves use the existing native `map/` draft and reference protocol. A separate `terrain-authoring.ref` in the recovery slot records the durable authoring owner, baseline identity and initially opened LF fingerprint. Unchanged original normal chunks use the existing reference mechanism against the pinned baseline store, avoiding a full original normal-set copy per slot; modified encoded chunks and editable LF samples remain in the draft. Recovery materializes referenced files through the existing protocol when opened. Restoring resolves the original durable owner and retains the existing overwrite prompt. Missing originals still allow an editable recovery draft; they do not grant permission to compile normals. Repeated recovery copies copy session-local native inputs before the previous session can be removed.

## Entry points and limits

The Terrain panel exposes authoring status, opening an owner root, explicit adoption, authoring-aware copying and verified-original recovery. Automation uses the same document/loader paths:

```
terrain_project_create <owner-root> <native-source> <map>
terrain_project_open <owner-root> <map>
terrain_project_copy <owner-root>
terrain_save_copy <standalone-native-folder>
terrain_baseline_adopt
terrain_baseline_recover <recorded-original-native-folder>
terrain_baseline_recover_packs
terrain_save_answer overwrite|cancel
terrain_recovery_copy <slot>
terrain_recovery_open <slot>
```

Normal `save`, File Save As, `terrain_export`, Undo/Redo and the existing autosave commands continue to use production document paths. Creating a project or adopting legacy data explicitly selects the start of a new lineage. Recovery never makes that choice implicitly.

Supported scope is built/native battle terrain for the Attila and Rome II profiles and layouts recognized by the existing shipped-chunk updater. Unsupported LF-affecting operations refuse without discarding edits; recovery drafts retain their samples. Raw/kit terrain and campaign workflows retain their existing behavior. Historical originals of arbitrary legacy projects cannot be reconstructed by this mechanism. Recovery can verify the currently mounted effective pack sources, but never guesses historical pack versions. Automatic historical-source searches, global baseline deduplication, version migration, directory transactions and campaign-normal reconstruction are outside this change.

Implementation: `wce_terrain_io::built::{baseline, write_baseline}`, `wce_terrain_editor::{authoring, document}`, `wce_terrain_tool::{tool::authoring, export}`. Permanent synthetic continuity tests live in `wce_terrain_editor::baseline_tests`; hashing vectors live beside the baseline fingerprint implementation.

## Reproduction

Run `cargo test --release -p wce_terrain_editor baseline_tests` for the permanent regression (including its fresh-process child). `cargo run --release -p wce_terrain_editor --example native_baseline_cost -- <new-root>` generates the shared synthetic source at `<new-root>/source` and records bounded heap/storage costs. Build `wce_app`, set `WCE_FB_SMOKE_GAME` to the installed Rome II directory and `WCE_FB_SMOKE_CACHE` to an existing cache, then run `python scripts/terrain_native_baseline_smoke.py <wce-executable> <synthetic-source> <new-output-root>`. The script creates only independent synthetic outputs, runs non-activating and ends every session with `quit`. It verifies the original Phase 1 hash, LF/CHMF equality, copies/relocation, Export, real autosave restoration and Cancel/Overwrite behavior.
