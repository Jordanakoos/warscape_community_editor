//! Rome II campaign raise/lower authority. Exact f64 nodal deltas live in a WCE project-only sidecar (not a native resource).
//! Native origins stay the baseline; the full LF Item is the prepared preview. No inverse of quantized DDS is performed.

use std::collections::BTreeMap;
use std::sync::Arc;
use wce_formats::chmf::ChmfFile;
use wce_foundation::game::GameProfile;
use wce_terrain::{Item, LfRole, MapKind, MapModel, lf_edit::LfBrush, paged::PagedU16};
use wce_tile_model::Blob;
use crate::{MapFiles, Result, file_error};
use super::campaign_heights::{NodalDelta, PreparedHeights, prepare_delta};

/// Application authoring state only; explicitly excluded from native export. Empty v0 is a tombstone after undo/save.
pub const PROJECT_STATE: &str = "_wce_campaign_height_edits.data";
const NAMES: [&str; 3] = ["lf_height_map.data", "lf_height_map.dds", "lf_height_map_lq.dds"];
const EMPTY: &[u8] = b"WCHE\0\0\0\0";

#[derive(Clone)]
struct State { w: u32, h: u32, hashes: [u64; 3], nodes: BTreeMap<(u32, u32), f64> }

fn error(s: &str) -> crate::MapIoError { file_error(PROJECT_STATE, s) }
// Fixed FNV-1a is a portable accidental baseline-mismatch check, not a security signature. Evidence uses SHA256 separately.
fn fingerprint(b: &[u8]) -> u64 { b.iter().fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)) }

/// Native/profile gate; the tool additionally passes the active GameProfile on starting a stroke.
pub fn is_built_campaign(m: &MapModel) -> bool { m.built && m.kind == MapKind::Campaign }
fn profile(m: &MapModel, originals: &MapFiles) -> Result<(u32, u32)> {
    if !is_built_campaign(m) || !m.tile_list.as_ref().is_some_and(|t| t.get().version == 9 && t.get().is_campaign) {
        return Err(error("nodal editing requires the supported Rome II v9 built campaign profile"));
    }
    let c = ChmfFile::parse(&originals.folder[NAMES[0]].bytes()?).map_err(|e| file_error(NAMES[0], e))?;
    if c.version != 5 || (c.cells_x, c.cells_y, c.spacing) != (1, 8, 1) || c.width < 8 || c.height < 8 || c.min.to_bits() != 0 || c.max != 65535.0 {
        return Err(error("unsupported campaign height profile"));
    }
    let f = m.lf(LfRole::Land).ok_or_else(|| error("missing full LF"))?.get();
    if c.width.checked_mul(4) != Some(f.width()) || c.height.checked_mul(4) != Some(f.height()) { return Err(error("campaign full/coarse grid mismatch")); }
    Ok((c.width, c.height))
}

fn originals(m: &MapModel) -> Result<MapFiles> {
    let mut files = MapFiles::default();
    for n in NAMES {
        let blob = if n == NAMES[1] { m.lf(LfRole::Land).map(|i| i.origin.clone()) }
        else { m.carried.iter().find(|(p, _)| p.eq_ignore_ascii_case(n)).map(|(_, b)| b.clone()) };
        files.folder.insert(n.into(), blob.ok_or_else(|| file_error(n, "missing campaign height baseline"))?);
    }
    Ok(files)
}

fn hashes(f: &MapFiles) -> Result<[u64; 3]> { Ok([fingerprint(&f.folder[NAMES[0]].bytes()?), fingerprint(&f.folder[NAMES[1]].bytes()?), fingerprint(&f.folder[NAMES[2]].bytes()?)]) }
pub fn state_blob(m: &MapModel) -> Option<Blob> { m.carried.iter().find(|(p, _)| p.eq_ignore_ascii_case(PROJECT_STATE)).map(|(_, b)| b.clone()) }
pub fn put_state(m: &mut MapModel, state: Option<Blob>) {
    m.carried.retain(|p, _| !p.eq_ignore_ascii_case(PROJECT_STATE));
    if let Some(b) = state { m.carried.insert(PROJECT_STATE.into(), b); }
}

impl State {
    fn decode(b: &[u8]) -> Result<Option<Self>> {
        if b == EMPTY { return Ok(None); }
        if b.len() < 44 || &b[..8] != b"WCHE\x01\0\0\0" { return Err(error("unknown or truncated campaign authoring state")); }
        let u = |i| u32::from_le_bytes(b[i..i + 4].try_into().expect("four bytes"));
        let q = |i| u64::from_le_bytes(b[i..i + 8].try_into().expect("eight bytes"));
        let (w, h, n) = (u(8), u(12), u(16) as usize);
        if w < 8 || h < 8 || n > 1_000_000 || n.checked_mul(16).and_then(|n| n.checked_add(44)) != Some(b.len()) { return Err(error("invalid authoring state dimensions/count")); }
        let mut nodes = BTreeMap::new();
        for i in 0..n {
            let at = 44 + i * 16;
            let (x, y, v) = (u(at), u(at + 4), f64::from_bits(q(at + 8)));
            if x < 3 || y < 3 || x >= w - 2 || y >= h - 2 || !v.is_finite() || v == 0.0 || v.round_ties_even().abs() > 65535.0 || nodes.insert((x, y), v).is_some() {
                return Err(error("invalid or duplicate authoritative node"));
            }
        }
        Ok(Some(Self { w, h, hashes: [q(20), q(28), q(36)], nodes }))
    }
    fn blob(&self) -> Blob {
        if self.nodes.is_empty() { return Blob::Bytes(Arc::from(EMPTY)); }
        let mut b = b"WCHE\x01\0\0\0".to_vec();
        for v in [self.w, self.h, self.nodes.len() as u32] { b.extend(v.to_le_bytes()); }
        for h in self.hashes { b.extend(h.to_le_bytes()); }
        for (&(x, y), &v) in &self.nodes { b.extend(x.to_le_bytes()); b.extend(y.to_le_bytes()); b.extend(v.to_bits().to_le_bytes()); }
        Blob::Bytes(Arc::from(b))
    }
    fn prepare(&self, f: &MapFiles) -> Result<PreparedHeights> {
        let d = if self.nodes.is_empty() { NodalDelta::new([2, 2, 4, 4], vec![0.0; 9])? }
        else {
            let x0 = self.nodes.keys().map(|p| p.0).min().expect("nonempty") - 1;
            let y0 = self.nodes.keys().map(|p| p.1).min().expect("nonempty") - 1;
            let x1 = self.nodes.keys().map(|p| p.0).max().expect("nonempty") + 1;
            let y1 = self.nodes.keys().map(|p| p.1).max().expect("nonempty") + 1;
            let size = (x1 - x0 + 1) as usize * (y1 - y0 + 1) as usize;
            if size > 4_000_000 { return Err(error("authoring field exceeds the bounded profile")); }
            let mut v = vec![0.0; size];
            for (&(x, y), &d) in &self.nodes { v[((y - y0) * (x1 - x0 + 1) + x - x0) as usize] = d; }
            NodalDelta::new([x0, y0, x1, y1], v)?
        };
        prepare_delta(f, &d)
    }
}

fn loaded(m: &MapModel, f: &MapFiles) -> Result<State> {
    let (w, h) = profile(m, f)?;
    let hs = hashes(f)?;
    let state = state_blob(m).map(|b| b.bytes()).transpose()?.map(|b| State::decode(&b)).transpose()?.flatten()
        .unwrap_or(State { w, h, hashes: hs, nodes: BTreeMap::new() });
    if state.w != w || state.h != h || state.hashes != hs { return Err(error("authoring baseline identity mismatch; refusing to rebase deltas onto different native data")); }
    Ok(state)
}

fn preview_matches(m: &MapModel, p: &PreparedHeights) -> Result<()> {
    let b = p.height_files.folder[NAMES[1]].bytes()?;
    if m.lf(LfRole::Land).ok_or_else(|| error("missing preview field"))?.get().to_le_rows().as_slice() != &b[128..] {
        return Err(error("full LF contains edits outside the retained nodal authority; undo the unsupported edits"));
    }
    Ok(())
}

/// A stroke's continuous authority. Each accepted dab is cumulative relative to unchanged native origins, across stroke boundaries.
pub struct Authority { state: State, originals: MapFiles }
impl Authority {
    pub fn begin(m: &MapModel, game: &GameProfile) -> Result<Self> {
        if game.key != GameProfile::ROME_2.key { return Err(error("nodal campaign raise/lower is currently verified for Rome II only")); }
        let originals = originals(m)?;
        let state = loaded(m, &originals)?;
        preview_matches(m, &state.prepare(&originals)?)?;
        Ok(Self { state, originals })
    }
    pub fn snapshot(&self) -> Blob { self.state.blob() }
    pub fn is_empty(&self) -> bool { self.state.nodes.is_empty() }
    pub fn node_delta(&self, x: u32, y: u32) -> f64 { self.state.nodes.get(&(x, y)).copied().unwrap_or(0.0) }
    /// Same raise kernel, world footprint, aspect and turn; explicitly sampled at full-coordinate (4u,4v) campaign nodes.
    /// No clamping: an unsupported border/range fails before the state or preview changes.
    pub fn raise(&mut self, b: &LfBrush, amount: f64) -> Result<PreparedHeights> {
        if ![b.brush.cx, b.brush.cy, b.brush.radius, b.brush.strength, b.brush.falloff, b.aspect, b.rotation, amount].iter().all(|v| v.is_finite())
            || b.brush.radius <= 0.0 || b.aspect <= 0.0 || !(0.0..=1.0).contains(&b.brush.strength) || !(0.0..=1.0).contains(&b.brush.falloff)
            || b.brush.curve.is_some() || b.brush.image.is_some()
        { return Err(error("unsupported or non-finite campaign raise/lower brush")); }
        let r = b.brush.radius * b.aspect.max(1.0) + 1.0;
        if !r.is_finite() || r > 4.0 * f64::from(self.state.w.max(self.state.h)) { return Err(error("campaign brush footprint is outside the bounded profile")); }
        let mut next = self.state.clone();
        let mut hit = false;
        let x0 = ((b.brush.cx - r) / 4.0).floor().max(0.0) as u32;
        let x1 = (((b.brush.cx + r) / 4.0).ceil().max(0.0) as u32).min(next.w - 1);
        let y0 = ((b.brush.cy - r) / 4.0).floor().max(0.0) as u32;
        let y1 = (((b.brush.cy + r) / 4.0).ceil().max(0.0) as u32).min(next.h - 1);
        for y in y0..=y1 { for x in x0..=x1 {
            let weight = b.weight(4.0 * f64::from(x) - b.brush.cx, 4.0 * f64::from(y) - b.brush.cy);
            if weight <= 0.0 || amount == 0.0 { continue; }
            if x < 3 || y < 3 || x >= next.w - 2 || y >= next.h - 2 { return Err(error("campaign nodal brush reaches unsupported map-edge support")); }
            hit = true;
            let v = next.nodes.get(&(x, y)).copied().unwrap_or(0.0) + amount * weight;
            if !v.is_finite() || v.round_ties_even().abs() > 65535.0 { return Err(error("campaign nodal delta is out of range")); }
            if v == 0.0 { next.nodes.remove(&(x, y)); } else { next.nodes.insert((x, y), v); }
        } }
        if !hit && amount != 0.0 { return Err(error("brush reaches no campaign nodes; use a wider brush within the map")); }
        if next.nodes.len() > 1_000_000 { return Err(error("authoring node count exceeds the bounded profile")); }
        let prepared = next.prepare(&self.originals)?;
        self.state = next;
        Ok(prepared)
    }
}

/// Rebuild a persisted project's preview from original native resources and retained node bits, not from quantized saved output.
pub fn restore(m: &mut MapModel) -> Result<()> {
    let Some(blob) = state_blob(m) else { return Ok(()) };
    // Capture once at load, including tombstones: clones/export snapshots must not reread a later replacement of this path.
    let blob = Blob::Bytes(Arc::from(blob.bytes()?.to_vec()));
    let state = State::decode(&blob.bytes()?)?;
    put_state(m, Some(blob));
    if state.is_none() { return Ok(()); }
    let f = originals(m)?;
    let state = loaded(m, &f)?;
    let p = state.prepare(&f)?;
    let b = p.height_files.folder[NAMES[1]].bytes()?;
    let field = m.lf_mut(LfRole::Land).ok_or_else(|| error("missing preview"))?.edit();
    *field = PagedU16::from_le_rows(field.width(), field.height(), &b[128..]);
    Ok(())
}

/// Validate retained authority and return height preparation for diagnostics/future normal integration.
pub fn prepared(m: &MapModel) -> Result<PreparedHeights> {
    let f = originals(m)?;
    let s = loaded(m, &f)?;
    let p = s.prepare(&f)?;
    preview_matches(m, &p)?;
    Ok(p)
}

/// Export checks the actual prepared representations, including a CHMF-only quantized change whose full preview rounds to zero.
pub(super) fn edited(m: &MapModel) -> Result<bool> {
    let Some(b) = state_blob(m) else { return Ok(false) };
    if State::decode(&b.bytes()?)?.is_none() { return Ok(false); }
    let p = prepared(m)?;
    Ok(p.stats.chmf_samples != 0 || p.stats.full_samples != 0 || p.stats.lq_samples != 0)
}

/// Save authoring state only: native height files remain original (and normals are never presented as corrected).
/// Export continues to call built::write; it cannot use this project-only path.
pub fn project_files(m: &MapModel, db: Option<&wce_tile::battle_map::tile_db::TileDatabase>) -> Result<MapFiles> {
    if !is_built_campaign(m) { return super::write_project(m, db); }
    let retained = state_blob(m);
    let blob = retained.clone().unwrap_or_else(|| Blob::Bytes(Arc::from(EMPTY)));
    let state = State::decode(&blob.bytes()?)?;
    let mut out = if state.is_some() {
        prepared(m)?; // Proves baseline identity and no unsupported DDS edits.
        let f = originals(m)?;
        let mut baseline = m.clone();
        put_state(&mut baseline, None);
        let item = baseline.lf_mut(LfRole::Land).ok_or_else(|| error("missing LF"))?;
        let bytes = item.origin.bytes()?;
        *item = Item::new(item.path.clone(), PagedU16::from_le_rows(item.get().width(), item.get().height(), &bytes[128..]), item.origin.clone());
        let mut out = super::write_project(&baseline, db)?;
        // Materialize the three identity-checked baselines, avoiding same-size skip drift for these authoring dependencies only.
        for n in NAMES { let key = out.folder.keys().find(|p| p.eq_ignore_ascii_case(n)).expect("baseline").clone(); out.folder.insert(key, Blob::Bytes(Arc::from(f.folder[n].bytes()?.to_vec()))); }
        out
    } else { super::write_project(m, db)? };
    // A new tombstone is useful for a supported authoring baseline (including undo/save),
    // but must not turn an incomplete native preservation copy into an authored generation.
    // Retain old empty tombstones: they are harmless metadata, not evidence of authoring.
    if retained.is_some() || prepared(m).is_ok() { out.folder.insert(PROJECT_STATE.into(), blob); }
    Ok(out)
}

/// Bounded pre-publication generation check. No files, directories, model or history are changed here.
/// It does not promise rollback after later I/O failures or protect against another writer racing publication.
pub fn check_project_destination(files: &MapFiles, dir: &std::path::Path) -> Result<()> {
    let conflict = |why: &str| error(&format!("Existing campaign authoring generation cannot be replaced: {why}. Save to an independent destination folder"));
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(conflict(&format!("cannot inspect destination: {e}"))),
    };
    let mut paths = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|e| conflict(&e.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if (NAMES.contains(&name.as_str()) || name == PROJECT_STATE) && paths.insert(name, entry.path()).is_some() {
            return Err(conflict("ambiguous duplicate generation resource names"));
        }
    }
    if paths.is_empty() { return Ok(()); }
    let incoming_state = files.folder.iter().find(|(p, _)| p.eq_ignore_ascii_case(PROJECT_STATE))
        .map(|(_, b)| -> Result<_> { State::decode(&b.bytes()?) }).transpose()?.flatten();
    let destination_state = paths.get(PROJECT_STATE).map(|p| {
        let bytes = std::fs::read(p).map_err(|e| conflict(&e.to_string()))?;
        State::decode(&bytes).map_err(|_| conflict("destination authoring metadata is invalid"))
    }).transpose()?.flatten();
    if incoming_state.is_none() && destination_state.is_none() {
        // Preservation-only folders need no complete authoring baseline. Still check every
        // existing native dependency so this exception cannot overwrite a different generation
        // or leave destination-only height resources coupled to an unrelated incoming full LF.
        for n in NAMES {
            if let Some(p) = paths.get(n) {
                let intended = files.folder.iter().find(|(p, _)| p.eq_ignore_ascii_case(n)).map(|(_, b)| b)
                    .ok_or_else(|| conflict("incoming project omits an existing native baseline"))?;
                if std::fs::read(p).map_err(|e| conflict(&e.to_string()))?.as_slice() != intended.bytes()?.as_ref() {
                    return Err(conflict("destination native baseline bytes differ from the incoming project"));
                }
            }
        }
        return Ok(());
    }
    let mut destination = MapFiles::default();
    let mut intended = MapFiles::default();
    for n in NAMES {
        let p = paths.get(n).ok_or_else(|| conflict("destination generation has missing native baselines"))?;
        destination.folder.insert(n.into(), Blob::Bytes(Arc::from(std::fs::read(p).map_err(|e| conflict(&e.to_string()))?)));
        intended.folder.insert(n.into(), files.folder.iter().find(|(p, _)| p.eq_ignore_ascii_case(n)).map(|(_, b)| b.clone()).ok_or_else(|| conflict("incoming native baselines are incomplete"))?);
    }
    let existing = hashes(&destination)?;
    if let Some(state) = destination_state {
        let c = ChmfFile::parse(&destination.folder[NAMES[0]].bytes()?).map_err(|_| conflict("destination CHMF is invalid"))?;
        if state.hashes != existing || (state.w, state.h) != (c.width, c.height) {
            return Err(conflict("destination metadata does not match its native baselines"));
        }
    }
    // A legacy folder without metadata can be adopted only with the complete identical baseline set.
    // Empty tombstones carry no hashes, so their actual three native baselines also define the generation.
    if existing != hashes(&intended)? { return Err(conflict("destination CHMF/full/LQ baseline identities differ from the incoming project")); }
    for n in NAMES {
        if destination.folder[n].bytes()? != intended.folder[n].bytes()? { return Err(conflict("destination native baseline bytes differ from the incoming project")); }
    }
    if let Some(state) = incoming_state {
        if state.hashes != existing { return Err(conflict("incoming authority does not match the checked native baselines")); }
    }
    Ok(())
}
