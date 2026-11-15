//! Immutable native battle height-family inputs. Authoring storage is separate
//! from MapFiles: none of these records is a native resource or a pack member.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use wce_foundation::publication::{Output, Source, Staged, resource_path};
use wce_terrain::{LfRole, MapKind, MapModel};

mod fingerprint;
pub use fingerprint::digest;

const MAGIC: &str = "WCE-NATIVE-BATTLE-BASELINE-1";
const LF: &str = "lf_height_map.dds";
const CHMF: &str = "lf_height_map.data";
// Explicit calculation/encoder revision AND source fingerprints. A future
// algorithm change needs a deliberate compatibility/migration decision.
fn pipeline() -> String {
    format!(
        "native-delta-dxt5-1:{}:{}:{}",
        digest(include_bytes!("../attila/normals.rs")),
        digest(include_bytes!("../../../wce_tile_io/src/attila/bob_tile.rs")),
        digest(include_bytes!("../../../wce_tile_io/src/attila/ati_dxt.rs"))
    )
}
pub fn normal(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("normals/normal_") && key.ends_with(".dds")
}
pub fn family(key: &str) -> bool {
    key.eq_ignore_ascii_case(LF) || key.eq_ignore_ascii_case(CHMF) || normal(key)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    key: String,
    len: usize,
    hash: String,
}
#[derive(Clone, Debug)]
pub struct Descriptor {
    pub id: String,
    game: String,
    shape: [u32; 4],
    entries: Vec<Entry>,
    body: String,
}
#[derive(Clone, Debug)]
pub struct Baseline {
    pub descriptor: Descriptor,
    pub(super) files: BTreeMap<String, Arc<[u8]>>,
}
fn shape(m: &MapModel) -> Result<[u32; 4], String> {
    if !m.built || m.kind != MapKind::Battle || m.tile_list.as_ref().is_some_and(|t| t.get().is_campaign) {
        return Err("native baseline supports built battle terrain only".into());
    }
    let f = m.lf(LfRole::Land).ok_or("missing land LF")?.get();
    let (cx, cy) = super::cells(m).ok_or("unknown native battle geometry")?;
    if [f.width(), f.height(), cx, cy].contains(&0) {
        return Err("empty native battle geometry".into());
    }
    Ok([f.width(), f.height(), cx, cy])
}
fn inventory(files: &BTreeMap<String, Arc<[u8]>>) -> Vec<Entry> {
    files.iter().map(|(key, b)| Entry { key: key.clone(), len: b.len(), hash: digest(b) }).collect()
}
fn entries_text(entries: &[Entry]) -> String {
    entries.iter().map(|e| format!("{}\t{}\t{}\n", e.key, e.len, e.hash)).collect()
}
impl Baseline {
    /// Explicitly start a lineage at the supplied native resources. This does
    /// not assert that a legacy map's resources predate earlier editing.
    pub fn capture(m: &MapModel, game: &str) -> Result<Self, String> {
        if !matches!(game, "rome_2" | "attila") {
            return Err("unsupported native battle profile".into());
        }
        let s = shape(m)?;
        let lf = m.lf(LfRole::Land).unwrap();
        let mut files = BTreeMap::new();
        files.insert(LF.into(), lf.origin.bytes().map_err(|e| e.to_string())?);
        for (key, blob) in &m.carried {
            if normal(key) || key.eq_ignore_ascii_case(CHMF) {
                let key = key.to_ascii_lowercase();
                if files.insert(key, blob.bytes().map_err(|e| e.to_string())?).is_some() {
                    return Err("duplicate baseline key".into());
                }
            }
        }
        let entries = inventory(&files);
        let text = entries_text(&entries);
        parse_entries(&text)?; // Never create a context that its bounded reader rejects.
        let body = format!("{MAGIC}\n{}\n{game}\n{} {} {} {}\n{}", pipeline(), s[0], s[1], s[2], s[3], text);
        let b = Self { descriptor: Descriptor { id: digest(body.as_bytes()), game: game.into(), shape: s, entries, body }, files };
        b.validate()?;
        Ok(b)
    }
    fn validate(&self) -> Result<(), String> {
        let [w, h, _, _] = self.descriptor.shape;
        let lf = self.files.get(LF).ok_or("missing baseline LF")?;
        let payload = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(128))
            .ok_or("baseline LF size overflow")?;
        if payload > lf.len() || lf.len() < 128 {
            return Err("truncated baseline LF".into());
        }
        let u = |at| u32::from_le_bytes(lf[at..at + 4].try_into().unwrap());
        if (u(16), u(12)) != (w, h) {
            return Err("baseline LF dimensions mismatch".into());
        }
        if lf.get(..4) != Some(b"DDS ") || lf.get(80..84) != Some(&0x20000u32.to_le_bytes()) || lf.get(88..92) != Some(&16u32.to_le_bytes())
        {
            return Err("unsupported baseline LF encoding/geometry".into());
        }
        let chmf = self.files.get(CHMF).ok_or("missing baseline CHMF")?;
        let c = wce_formats::chmf::ChmfFile::parse(chmf).map_err(|e| e.to_string())?;
        let [_, _, cx, cy] = self.descriptor.shape;
        let (rx, ry) = (cx.div_ceil(16), cy.div_ceil(16));
        if c.width != w
            || c.height != h
            || c.cells_x != (w / cx).max(1)
            || c.cells_y != (h / cy).max(1)
            || c.spacing != 1
            || c.regions.len() as u64 != u64::from(rx) * u64::from(ry)
            || c.regions.iter().enumerate().any(|(i, r)| {
                let (x, y) = (i as u32 % rx * 16, i as u32 / rx * 16);
                r.rect != [x, y, x + 16, y + 16]
            })
        {
            return Err("unsupported baseline CHMF region layout".into());
        }
        let count = self.files.keys().filter(|k| normal(k)).count();
        let mut side = 0;
        for i in 0..count {
            let key = format!("normals/normal_{i}.dds");
            let bytes = self.files.get(&key).ok_or("noncontiguous baseline normals")?;
            if bytes.len() < 128 || bytes.get(..4) != Some(b"DDS ") {
                return Err("invalid baseline normal DDS".into());
            }
            let d = wce_formats::dds_decode::dds_info(bytes);
            if i == 0 {
                side = d.width;
                if super::super::attila::normals::Layout::of(count, i64::from(side), i64::from(w), i64::from(h)).is_none() {
                    return Err("unsupported native normal layout".into());
                }
            }
            if d.width != side || d.height != side || d.format != "DXT5" || d.mips == 0 || d.mips > 32 {
                return Err("unsupported normal encoding".into());
            }
            // Validate every retained mip, with bounded one-mip temporary memory.
            for mip in 0..d.mips {
                wce_formats::dds_decode::dds_decode_mip(bytes, mip).map_err(str::to_owned)?;
            }
        }
        if super::super::attila::normals::Layout::of(count, i64::from(side), i64::from(w), i64::from(h)).is_none() {
            return Err("unsupported native normal layout".into());
        }
        Ok(())
    }
    pub fn check_model(&self, m: &MapModel) -> Result<(), String> {
        if shape(m)? != self.descriptor.shape {
            return Err("native baseline geometry differs from document".into());
        }
        if m.carried.keys().any(|key| normal(key) && !self.files.keys().any(|original| original.eq_ignore_ascii_case(key))) {
            return Err(
                "native normal inventory differs from the verified baseline; preserve and resolve the extra resources explicitly".into()
            );
        }
        Ok(())
    }
    pub fn original_lf_hash(&self) -> String {
        let [w, h, _, _] = self.descriptor.shape;
        digest(&self.files[LF][128..128 + w as usize * h as usize * 2])
    }
    pub fn bytes(&self) -> usize {
        self.files.values().map(|b| b.len()).sum()
    }
}

fn parse_entries(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut total = 0usize;
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let [key, len, hash] = fields[..] else {
            return Err("invalid baseline inventory".into());
        };
        resource_path(Path::new("authoring"), key)?;
        if !family(key) || key != key.to_ascii_lowercase() || entries.iter().any(|e: &Entry| e.key == key) {
            return Err("invalid/duplicate baseline key".into());
        }
        let len: usize = len.parse().map_err(|_| "invalid baseline length")?;
        total = total.checked_add(len).ok_or("baseline size overflow")?;
        if total as u64 > 8 * 1024 * 1024 * 1024
            || entries.len() >= 4096
            || hash.len() != 64
            || !hash.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        {
            return Err("unsupported baseline inventory".into());
        }
        entries.push(Entry { key: key.into(), len, hash: hash.into() });
    }
    if !entries.iter().any(|e| e.key == LF) || !entries.iter().any(|e| e.key == CHMF) {
        return Err("incomplete baseline inventory".into());
    }
    Ok(entries)
}
impl Descriptor {
    fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.splitn(5, '\n');
        if lines.next() != Some(MAGIC) || lines.next() != Some(pipeline().as_str()) {
            return Err("unknown authoring format/calculation/encoder version".into());
        }
        let game = lines.next().ok_or("missing baseline profile")?;
        if !matches!(game, "rome_2" | "attila") {
            return Err("unknown baseline profile".into());
        }
        let s: Vec<u32> = lines
            .next()
            .ok_or("missing geometry")?
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "invalid geometry")?;
        let shape: [u32; 4] = s.try_into().map_err(|_| "invalid geometry")?;
        if shape.contains(&0) {
            return Err("empty baseline geometry".into());
        }
        let entries = parse_entries(lines.next().ok_or("missing inventory")?)?;
        Ok(Self { id: digest(text.as_bytes()), game: game.into(), shape, entries, body: text.into() })
    }
}

/// One map's authoring context. The native path and original resource store are
/// relative to an explicit owner root, so copying that root preserves authority.
/// Immutable resources are pinned for the lifetime of the project, never the
/// lifetime of an autosave session. There is intentionally no automatic GC.
#[derive(Clone, Debug)]
pub struct Context {
    pub root: PathBuf,
    pub map: String,
    pub native: String,
    pub descriptor: Descriptor,
    record: Arc<[u8]>,
    generation: Vec<Entry>,
}
impl Context {
    pub fn record_path(root: &Path, map: &str) -> Result<PathBuf, String> {
        if map.contains('/') {
            return Err("authoring map must be a single resource name".into());
        }
        resource_path(&root.join("authoring/terrain/maps"), &format!("{map}.wce"))
    }
    pub fn native_dir(&self) -> PathBuf {
        self.root.join(&self.native)
    }
    pub fn resource_dir(&self) -> PathBuf {
        self.root.join("authoring/terrain/baselines").join(&self.descriptor.id)
    }
    /// Recovery slots may refer to pinned original encoded chunks rather than
    /// copy all untouched normals on every autosave. Editable LF samples always
    /// remain in the recovery draft, independently of baseline availability.
    pub fn original_normal_ref(&self, key: &str, bytes: &[u8]) -> Option<PathBuf> {
        if !normal(key) {
            return None;
        }
        let e = self.descriptor.entries.iter().find(|e| e.key.eq_ignore_ascii_case(key))?;
        if e.len != bytes.len() || e.hash != digest(bytes) {
            return None;
        }
        let path = self.resource_dir().join(&e.key);
        (fs::metadata(&path).ok()?.len() == e.len as u64).then_some(path)
    }
    fn encode(native: &str, desc: &Descriptor, generation: &[Entry]) -> Arc<[u8]> {
        Arc::from(
            format!("WCE-TERRAIN-CONTEXT-1\n{native}\n{}\n{}\n--generation--\n{}", desc.id, desc.body, entries_text(generation))
                .into_bytes(),
        )
    }
    pub fn create(root: &Path, map: &str, native: &str, b: &Baseline) -> Result<Self, String> {
        let root = std::path::absolute(root).map_err(|e| e.to_string())?;
        let root = root.as_path();
        let record_path = Self::record_path(root, map)?;
        let native_dir = resource_path(root, native)?;
        let authoring = wce_foundation::save_path::key(&root.join("authoring"))?;
        let dest = wce_foundation::save_path::key(&native_dir)?;
        if dest.starts_with(&authoring) || authoring.starts_with(&dest) {
            return Err("authoring context must be outside native map directories".into());
        }
        let record = Self::encode(native, &b.descriptor, &[]);
        let context = Self {
            root: root.into(),
            map: map.into(),
            native: native.into(),
            descriptor: b.descriptor.clone(),
            record: record.clone(),
            generation: vec![],
        };
        if record_path.exists() {
            let existing = Self::open(root, map)?;
            if existing.descriptor.id != context.descriptor.id || existing.native != native {
                return Err("authoring destination already owns a different lineage".into());
            }
            existing.resolve()?;
            return Ok(existing);
        }
        context.persist_resources(b, false)?;
        // The anchor is durable before the first native replacement. A crash
        // can leave an unreferenced content-addressed store, never a fake B0.
        let staged = Staged::prepare(vec![Output::new(record_path.clone(), Source::Bytes(record))]).map_err(|e| e.to_string())?;
        staged
            .publish_checked(
                |_| if record_path.exists() { Err("authoring context appeared concurrently".into()) } else { Ok(()) },
                |_| Ok(()),
            )
            .map_err(|e| e.to_string())?;
        Ok(context)
    }
    pub fn open(root: &Path, map: &str) -> Result<Self, String> {
        let root = std::path::absolute(root).map_err(|e| e.to_string())?;
        let root = root.as_path();
        let path = Self::record_path(root, map)?;
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
            return Err("oversized authoring context".into());
        }
        let record: Arc<[u8]> = Arc::from(fs::read(path).map_err(|e| e.to_string())?);
        let text = std::str::from_utf8(&record).map_err(|e| e.to_string())?;
        let mut lines = text.splitn(4, '\n');
        if lines.next() != Some("WCE-TERRAIN-CONTEXT-1") {
            return Err("unknown authoring context version".into());
        }
        let native = lines.next().ok_or("missing native path")?.to_owned();
        resource_path(root, &native)?;
        let id = lines.next().ok_or("missing baseline id")?;
        let (body, generation) =
            lines.next().ok_or("missing baseline descriptor")?.split_once("\n--generation--\n").ok_or("missing generation marker")?;
        let descriptor = Descriptor::parse(body)?;
        if descriptor.id != id {
            return Err("baseline descriptor digest mismatch".into());
        }
        let generation = if generation.is_empty() { vec![] } else { parse_entries(generation)? };
        let result = Self { root: root.into(), map: map.into(), native, descriptor, record, generation };
        let dest = wce_foundation::save_path::key(&result.native_dir())?;
        let authoring = wce_foundation::save_path::key(&root.join("authoring"))?;
        if dest.starts_with(&authoring) || authoring.starts_with(&dest) {
            return Err("invalid authoring/native ownership".into());
        }
        Ok(result)
    }
    pub fn check_record(&self) -> Result<(), String> {
        if fs::read(Self::record_path(&self.root, &self.map)?).map_err(|e| e.to_string())?.as_slice() != self.record.as_ref() {
            return Err("authoring context changed on disk; reopen before saving".into());
        }
        Ok(())
    }
    pub fn check_game(&self, game: &str) -> Result<(), String> {
        if game == self.descriptor.game { Ok(()) } else { Err("authoring context belongs to another game".into()) }
    }
    pub fn check_generation(&self, m: &MapModel) -> Result<(), String> {
        if self.generation.is_empty() {
            return Ok(());
        }
        for e in &self.generation {
            let b = if e.key == LF {
                m.lf(LfRole::Land).ok_or("missing LF")?.origin.bytes()
            } else {
                m.carried
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(&e.key))
                    .ok_or("missing published height-family resource")?
                    .1
                    .bytes()
            }
            .map_err(|e| e.to_string())?;
            if b.len() != e.len || digest(&b) != e.hash {
                return Err(
                    "native height family differs from its completed Save; recover/reconcile explicitly before height publication".into()
                );
            }
        }
        Ok(())
    }
    pub fn resolve(&self) -> Result<Baseline, String> {
        self.check_record()?;
        let mut files = BTreeMap::new();
        for e in &self.descriptor.entries {
            let path = resource_path(&self.resource_dir(), &e.key)?;
            if fs::metadata(&path).map_err(|err| format!("baseline {}: {err}; recover verified originals", e.key))?.len() != e.len as u64 {
                return Err("baseline length mismatch; recover verified originals".into());
            }
            let bytes: Arc<[u8]> = Arc::from(fs::read(path).map_err(|e| e.to_string())?);
            if digest(&bytes) != e.hash {
                return Err(format!("baseline hash mismatch: {}; recover verified originals", e.key));
            }
            files.insert(e.key.clone(), bytes);
        }
        let b = Baseline { descriptor: self.descriptor.clone(), files };
        b.validate()?;
        Ok(b)
    }
    fn persist_resources(&self, b: &Baseline, repair: bool) -> Result<(), String> {
        if b.descriptor.id != self.descriptor.id {
            return Err("recovery donor is not the recorded original baseline".into());
        }
        let mut outputs = vec![];
        for (key, bytes) in &b.files {
            let path = resource_path(&self.resource_dir(), key)?;
            if path.exists() && !repair && fs::read(&path).map_err(|e| e.to_string())?.as_slice() != bytes.as_ref() {
                return Err("content-addressed baseline store is corrupt; explicit recovery required".into());
            }
            outputs.push(Output::new(path, Source::Bytes(bytes.clone())));
        }
        Staged::prepare(outputs).and_then(Staged::publish).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn recover(&self, donor: &MapModel) -> Result<(), String> {
        self.check_record()?;
        let baseline = Baseline::capture(donor, &self.descriptor.game)?;
        // Verify the COMPLETE donor before changing any stored resource.
        self.persist_resources(&baseline, true)?;
        drop(baseline);
        self.resolve().map(|_| ())
    }
    /// Recover from an existing source provider (for example effective game
    /// packs). Names/provenance alone are insufficient: every encoded byte is
    /// verified against the retained descriptor before any repair is staged.
    pub fn recover_with(&self, mut read: impl FnMut(&str) -> Result<Vec<u8>, String>) -> Result<(), String> {
        self.check_record()?;
        let mut files = BTreeMap::new();
        for e in &self.descriptor.entries {
            let bytes: Arc<[u8]> = Arc::from(read(&e.key)?);
            if bytes.len() != e.len || digest(&bytes) != e.hash {
                return Err(format!("recovery source is not the recorded original: {}", e.key));
            }
            files.insert(e.key.clone(), bytes);
        }
        let baseline = Baseline { descriptor: self.descriptor.clone(), files };
        baseline.validate()?;
        self.check_record()?;
        self.persist_resources(&baseline, true)?;
        drop(baseline);
        self.resolve().map(|_| ())
    }
    /// Retained variable-size metadata allocations, excluding allocator headers
    /// and inline structs. Original payload is loaded only for an operation.
    pub fn metadata_bytes(&self) -> usize {
        let entries = |v: &Vec<Entry>| {
            v.capacity() * std::mem::size_of::<Entry>() + v.iter().map(|e| e.key.capacity() + e.hash.capacity()).sum::<usize>()
        };
        self.root.capacity()
            + self.map.capacity()
            + self.native.capacity()
            + self.record.len()
            + self.descriptor.id.capacity()
            + self.descriptor.game.capacity()
            + self.descriptor.body.capacity()
            + entries(&self.descriptor.entries)
            + entries(&self.generation)
    }
    pub fn copy_to(&self, root: &Path, native: &str) -> Result<Self, String> {
        Self::create(root, &self.map, native, &self.resolve()?)
    }
    pub fn completion(&self, files: &crate::MapFiles) -> Result<(Output, Self), String> {
        self.check_record()?;
        let mut family_files = BTreeMap::new();
        for (key, blob) in &files.folder {
            if family(key) {
                family_files.insert(key.to_ascii_lowercase(), blob.bytes().map_err(|e| e.to_string())?);
            }
        }
        let generation = inventory(&family_files);
        let record = Self::encode(&self.native, &self.descriptor, &generation);
        let mut output = Output::new(Self::record_path(&self.root, &self.map)?, Source::Bytes(record.clone()));
        output.priority = u8::MAX;
        let mut next = self.clone();
        next.record = record;
        next.generation = generation;
        Ok((output, next))
    }
    pub fn id(&self) -> &str {
        &self.descriptor.id
    }
}
