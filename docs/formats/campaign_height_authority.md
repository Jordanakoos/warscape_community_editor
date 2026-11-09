# Rome II built campaign raise/lower authoring

This is a profile-limited brush-to-height-preparation capability. Native edited export
remains blocked until campaign normals can be regenerated correctly. Save retains an
editable WCE project; its original native files are not an edited campaign build.

## Authority and commands

The tool requires the active Rome II profile, a built campaign with v9 campaign tile list,
and the accepted CHMF v5 (1,8), spacing1, min0/max65535, literal single-block region and
full/LQ dimensional profile. Missing/unsupported resources fail before mutation.

Only land raise/lower is supported. The existing brush radius, ellipse, rotation, cosine
falloff and map height-range conversion are reused. The explicit campaign adjustment is
to sample that kernel at full-file coordinates `(4u,4v)`, retain the accumulated **f64 raw
delta** at coarse nodes, then use `campaign_heights::prepare_delta` for all three heights.
The full preview is the prepared area mean, so a sub-grid brush is not arbitrary full-pixel
painting. A brush reaching no node is refused. File rows remain north-to-south; LfMirror
reverses them for the existing view. Existing tool scene-unit conversion is unchanged;
this work does not certify it as an engine physical-unit calibration.

Every dab prepares against original CHMF/full/LQ, never against the previous quantized
preview. No normalized-f32 baseline reduction or inverse DDS inference occurs. Existing
`LfStroke::set`/`LfStrokeCommand` retain preview pages; `CampaignStrokeCommand` composes
those with exact authority snapshots on the existing CommandStack. Fractional changes
remain undoable even if all native representations currently round to no change. Undo,
redo and branching restore exact bits. Loaded file-backed metadata is frozen before a
stroke so saving over its file cannot alter an undo snapshot. Empty/refused strokes add
no command. Shared LfStroke and other games' brush paths are unchanged.

The map document owns an active campaign gesture; the tool holds an expiring ticket.
Its History facade completes a valid active gesture before undo, redo, jump, saved marking
or clearing history. Reading or borrowing History does not complete the gesture. Undo
therefore first records the active stroke, then undoes it. Redo during a changed active
stroke completes a new branch and discards the old redo tail. Save checks the destination
generation first, then completes the gesture before writing. Active continuous changes
count as dirty even if all preview samples still quantize to the original values.

Low-level CampaignStroke APIs require the current history version and compare exact
last-authority bytes plus the validated preview before dab, finish or cancellation.
Stale calls refuse without restoring old state. Global resolution expires the tool ticket;
later drag/release cannot resurrect an undone delta. Normal completed-stroke history is
unchanged. Scripted `terrain_campaign_gesture` drives this held lifecycle and the existing
viewport release path for regression checks; it provides no additional native export mode.

No edge clipping or height saturation is introduced: invalid or non-finite input,
unsupported image/curve kernels, map-edge support and out-of-range prepared heights fail.
Nodes must be at least3 and below dimension−2. State is capped at1,000,000 nonzero nodes
and its enclosing zero-bordered field at4,000,000 nodes. These are bounded support limits,
not a new general terrain authoring format. Widely separated edits can reach that cap.
Preparation currently validates/hashes baselines and scans the full preview per dab;
large-map interactive performance has not been optimized or benchmarked for long strokes.

## Project-only state

`_wce_campaign_height_edits.data` is WCE metadata, carried in the project folder. All
integers are little-endian. v1 header44 bytes:

| Offset | Type | Meaning |
|---|---|---|
|0|8 bytes|`WCHE 01 00 00 00`|
|8,12|u32 each|coarse width, height|
|16|u32|nonzero node count|
|20,28,36|u64 each|fixed FNV-1a64 of original CHMF, full DDS, LQ DDS|
|44 onward|16 bytes/node|u32 x, u32 y, exact IEEE754 f64 delta bits|

Nodes serialize in ordered `(x,y)` order. Duplicate coordinates, non-finite/zero values,
invalid ranges, unknown versions, truncated/extra bytes and mismatched baselines fail.
FNV detects accidental source mismatch; it is not an authenticity/security signature.
Validation evidence records SHA256 independently. Eight bytes `WCHE 00 00 00 00` is an
empty tombstone, allowing undo-save to clear an older sidecar without deleting arbitrary
project files. Unedited campaign saves can add this WCE metadata; native export excludes it.

Project save validates authority against current preview, writes exact original native
height baselines plus metadata, and leaves normals as originals. Reopen verifies baselines
and rebuilds the full preview from retained node bits. This avoids double application and
quantization drift. The three identity-checked baselines are materialized for save, keeping
their identity through WCE's existing same-size file optimization. This is a narrow authoring
requirement, not a fix for the separate general save-identity issue.

Built load captures sidecar data, including tombstones, as immutable Blob::Bytes before
restoring the preview. Model clones/export snapshots retain those bytes even if a later
save replaces the file. They do not reread authority through a mutable filename.

Before project publication, an existing destination generation is checked as a whole:
its metadata must be valid and match its actual CHMF/full/LQ baselines, and all three
baselines must match the incoming originals. Tombstones use actual baseline identities.
A legacy folder without a sidecar can be adopted only when all three original baselines
already match; incomplete, corrupt or incompatible destinations fail before any resource
is written, with a recommendation to use an independent destination folder. Empty/new
folders remain supported. Rejection preserves history, dirty state and any active gesture.
This is preflight only: rollback after later I/O failure and cross-process races still
require a separate publication transaction. General file alias/size/stream handling
remains a P0-03 integration dependency; the generic save loop is unchanged.

Pack export still uses `built::write`. It strips authoring metadata and refuses any effective
prepared CHMF/full/LQ change, including a CHMF-only change with no full-DDS change. A truly
native no-op may export original blobs. Arbitrary DDS edits remain refused; per-tile patches,
smooth, flatten, noise and sea painting are not silently projected into this representation.

## Validation boundary

Focused tests cover analytic interpolation/quantization and flipped mirror rows, separate
fractional strokes, overlap/adjacency/rotation, region boundaries, undo/redo/branching,
source-preserving save/reopen, file-backed save-overwrite undo, no-op and rejection paths.
The two accepted archived native forward witnesses remain byte-identical regression gates.
Application scripting uses the same brush lifecycle and verifies save/reopen and export
refusal on disposable resources. No corrected normal generation or edited native pack is
provided by this capability.
