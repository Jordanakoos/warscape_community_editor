//! Native battle authoring authority owned by the document, independent of its
//! history and the current native output. Recovery slots refer to this durable
//! owner; deleting a session never deletes B0.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use wce_terrain::{LfRole, MapKind, MapModel};
use wce_terrain_io::{
    MapFiles,
    built::{
        self,
        baseline::{Baseline, Context, digest},
    },
};

pub const RECOVERY: &str = "terrain-authoring.ref";
pub struct Authoring {
    location: Option<Location>,
    initial_lf: Option<String>,
    expected_id: Option<String>,
    revision: std::sync::atomic::AtomicU64,
    state: Mutex<State>,
}
#[derive(Clone)]
struct Location {
    root: PathBuf,
    map: String,
    game: String,
    native: String,
}
enum State {
    Legacy,
    Blocked(String),
    Pending(Baseline),
    Bound(Context),
    Other,
}
pub fn lf_hash(m: &MapModel) -> Option<String> {
    m.lf(LfRole::Land).map(|f| digest(&f.get().to_le_rows()))
}
impl Authoring {
    pub fn new(model: Option<&MapModel>) -> Self {
        let applicable = model.is_some_and(|m| m.built && m.kind == MapKind::Battle && m.lf(LfRole::Land).is_some());
        Self {
            location: None,
            expected_id: None,
            revision: Default::default(),
            initial_lf: model.and_then(lf_hash),
            state: Mutex::new(if applicable { State::Legacy } else { State::Other }),
        }
    }
    /// Packs supplied as immutable bytes are trusted as the explicitly selected
    /// source, not assumed to be vanilla. Existing loose projects need a record
    /// or explicit adoption, even if their files resemble the game's originals.
    pub fn discover(root: &Path, game: &str, map: &str, native: &Path, m: &MapModel, trusted: bool) -> Self {
        let mut result = Self::new(Some(m));
        if matches!(*result.state.lock().unwrap(), State::Other) {
            return result;
        }
        let (root, native) = match (std::path::absolute(root), std::path::absolute(native)) {
            (Ok(root), Ok(native)) => (root, native),
            _ => {
                *result.state.lock().unwrap() = State::Blocked("cannot resolve authoring owner".into());
                return result;
            }
        };
        let (root, native) = (root.as_path(), native.as_path());
        let rel = match native.strip_prefix(root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                *result.state.lock().unwrap() = State::Blocked("native output is outside its authoring owner".into());
                return result;
            }
        };
        result.location = Some(Location { root: root.into(), map: map.into(), game: game.into(), native: rel });
        let state = (|| {
            let path = Context::record_path(root, map)?;
            if path.exists() {
                let c = Context::open(root, map)?;
                result.expected_id = Some(c.id().to_owned());
                c.check_game(game)?;
                if wce_foundation::save_path::key(native)? != wce_foundation::save_path::key(&c.native_dir())? {
                    return Err("authoring record names a different native destination".into());
                }
                c.check_generation(m)?;
                c.resolve()?.check_model(m)?;
                Ok(State::Bound(c))
            } else if trusted {
                Ok(State::Pending(Baseline::capture(m, game)?))
            } else {
                Ok(State::Legacy)
            }
        })();
        *result.state.lock().unwrap() = state.unwrap_or_else(State::Blocked);
        result
    }
    fn context_locked(&self, state: &mut State) -> Result<Option<Context>, String> {
        if let State::Pending(b) = state {
            let l = self.location.as_ref().ok_or("no authoring owner")?;
            let c = Context::create(&l.root, &l.map, &l.native, b)?;
            *state = State::Bound(c);
        }
        match state {
            State::Bound(c) => Ok(Some(c.clone())),
            State::Other | State::Legacy => Ok(None),
            State::Blocked(e) => Err(e.clone()),
            State::Pending(_) => unreachable!(),
        }
    }
    pub fn context(&self) -> Result<Option<Context>, String> {
        self.context_locked(&mut *self.state.lock().map_err(|e| e.to_string())?)
    }
    pub fn resolve(&self, m: &MapModel) -> Result<Option<Baseline>, String> {
        match self.context()? {
            Some(c) => {
                let b = c.resolve()?;
                b.check_model(m)?;
                Ok(Some(b))
            }
            None => {
                if m.built && m.kind == MapKind::Battle && lf_hash(m) != self.initial_lf {
                    return Err("native height publication needs verified original LF/normals: adopt a new lineage explicitly or recover the recorded originals".into());
                }
                Ok(None)
            }
        }
    }
    pub fn write(&self, m: &MapModel, db: Option<&wce_tile::battle_map::tile_db::TileDatabase>, project: bool) -> Result<MapFiles, String> {
        if !m.built {
            return wce_terrain_io::attila::write(m).map_err(|e| e.to_string());
        }
        if m.kind != MapKind::Battle {
            return if project { built::write_project(m, db) } else { built::write(m, db) }.map_err(|e| e.to_string());
        }
        if m.lf.iter().any(|(role, item)| *role != LfRole::Land && item.edited()) {
            return Err("unsupported LF-affecting native battle operation; edits retained for recovery".into());
        }
        let b = self.resolve(m)?;
        match b {
            Some(ref b) => built::write_baseline(m, db, project, b),
            None => built::write_draft(m, db, project),
        }
        .map_err(|e| e.to_string())
    }
    pub fn status(&self) -> String {
        match &*self.state.lock().unwrap() {
            State::Other => "Native baseline: not applicable".into(),
            State::Legacy => "Native baseline: unverified; explicit new-lineage adoption required for height Save/Export".into(),
            State::Blocked(e) => format!("Native baseline blocked: {e}"),
            State::Pending(_) => "Native baseline: immutable source captured; pinned on first publication/recovery copy".into(),
            State::Bound(c) => format!("Native baseline: {}", c.id()),
        }
    }
    /// Explicit legacy adoption. The initially loaded LF must still be the
    /// donor's LF, so pending edits are retained but a later compressed output
    /// cannot silently become their source.
    pub fn adopt(&self, m: &MapModel) -> Result<(), String> {
        let l = self.location.as_ref().ok_or("no authoring owner configured")?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if Context::record_path(&l.root, &l.map)?.exists() {
            return Err("an authoring record already exists; recover it rather than replacing its lineage".into());
        }
        if let Some(source) = &m.disk_source
            && wce_foundation::disk_changes::Folder::capture(&source.root)?.files != source.files
        {
            return Err("source files changed since Open; reopen before explicit adoption".into());
        }
        let b = Baseline::capture(m, &l.game)?;
        if Some(b.original_lf_hash()) != self.initial_lf {
            return Err("source LF changed since Open; reopen before explicit adoption".into());
        }
        *state = State::Bound(Context::create(&l.root, &l.map, &l.native, &b)?);
        self.revision.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    pub fn accept_completion(&self, c: Context) {
        *self.state.lock().unwrap() = State::Bound(c);
    }
    /// A recovery record contains a durable-owner reference plus the initially
    /// opened LF fingerprint. It is outside map/, and never a native resource.
    pub fn save_recovery(&self, slot: &Path) -> Result<(), String> {
        if matches!(*self.state.lock().unwrap(), State::Other) {
            return Ok(());
        }
        let context = self.context();
        let (decision, reason, id) = match &*self.state.lock().unwrap() {
            State::Bound(c) => ("verified", String::new(), c.id().to_owned()),
            State::Blocked(e) => ("blocked", e.clone(), self.expected_id.clone().unwrap_or_default()),
            State::Pending(b) => {
                ("blocked", context.err().unwrap_or_else(|| "original baseline is not durably pinned".into()), b.descriptor.id.clone())
            }
            _ => ("legacy", String::new(), String::new()),
        };
        let (root, map, game, native) = self
            .location
            .as_ref()
            .map(|l| (l.root.to_string_lossy().into_owned(), l.map.clone(), l.game.clone(), l.native.clone()))
            .unwrap_or_default();
        let reason = reason.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
        let fields = [root, map, game, native, id, self.initial_lf.clone().unwrap_or_default(), decision.into(), reason];
        if fields.iter().any(|s| s.contains(['\r', '\n'])) {
            return Err("recovery authoring path contains a newline".into());
        }
        fs::create_dir_all(slot).map_err(|e| e.to_string())?;
        fs::write(slot.join(RECOVERY), format!("WCE-TERRAIN-RECOVERY-2\n{}\n", fields.join("\n"))).map_err(|e| e.to_string())
    }
    pub fn restore(slot: &Path, m: &MapModel) -> Result<Self, String> {
        let v = read_recovery(slot)?;
        let mut result = Self::new(Some(m));
        result.initial_lf = (!v[6].is_empty()).then(|| v[6].clone());
        result.expected_id = (!v[5].is_empty()).then(|| v[5].clone());
        let prior_block = if v[0] == "WCE-TERRAIN-RECOVERY-2" && v[7] == "blocked" {
            let bytes = v[8]
                .as_bytes()
                .chunks_exact(2)
                .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).map_err(|_| "bad recovery refusal encoding"))
                .collect::<Result<Vec<_>, _>>()?;
            Some(String::from_utf8(bytes).map_err(|e| e.to_string())?)
        } else {
            None
        };
        if !v[1].is_empty() {
            let l = Location { root: PathBuf::from(&v[1]), map: v[2].clone(), game: v[3].clone(), native: v[4].clone() };
            let state = if let Some(reason) = prior_block {
                State::Blocked(reason)
            } else if v[5].is_empty() {
                if Context::record_path(&l.root, &l.map)?.exists() {
                    State::Blocked("recovery refers to an unverifiable existing authoring record; recover explicitly".into())
                } else {
                    State::Legacy
                }
            } else {
                match Context::open(&l.root, &l.map).and_then(|c| {
                    c.check_game(&l.game)?;
                    if c.id() != v[5] {
                        return Err("recovery baseline identity changed".into());
                    }
                    // Check the OWNER, never compare a dirty draft with saved LF.
                    // This also safely handles old v1 slots with no recorded verdict.
                    c.check_owner_generation()?;
                    c.resolve()?.check_model(m)?;
                    Ok(c)
                }) {
                    Ok(c) => State::Bound(c),
                    Err(e) => State::Blocked(e),
                }
            };
            result.location = Some(l);
            *result.state.lock().unwrap() = state;
        }
        Ok(result)
    }
    /// An absent pinned normal is optional for opening an editable draft, but
    /// not for compiling it. All other source-reference failures remain fatal.
    pub fn original_recovery_ref(slot: &Path, key: &str, source: &Path) -> bool {
        let Ok(v) = read_recovery(slot) else {
            return false;
        };
        if v[1].is_empty() || v[5].is_empty() || !built::baseline::normal(key) {
            return false;
        }
        let root = PathBuf::from(&v[1]).join("authoring/terrain/baselines").join(&v[5]);
        let Ok(expected) = wce_foundation::publication::resource_path(&root, &key.to_ascii_lowercase()) else {
            return false;
        };
        wce_foundation::save_path::key(&expected).ok().zip(wce_foundation::save_path::key(source).ok()).is_some_and(|(a, b)| a == b)
    }
    /// Pure preflight: never pin or repair originals inside either input map.
    pub fn check_copy_owner(&self, root: &Path) -> Result<(), String> {
        if let Some(l) = &self.location {
            for protected in [l.root.join(&l.native), l.root.join("authoring")] {
                if built::baseline::overlaps(root, &protected)? {
                    return Err("authoring copy overlaps the native map or authoring store".into());
                }
            }
        }
        Ok(())
    }
    pub fn check_destination(&self, dir: &Path) -> Result<(), String> {
        if let Some(l) = &self.location {
            let metadata = wce_foundation::save_path::key(&l.root.join("authoring"))?;
            let target = wce_foundation::save_path::key(dir)?;
            let sep = std::path::MAIN_SEPARATOR;
            if target == metadata || target.starts_with(&format!("{metadata}{sep}")) || metadata.starts_with(&format!("{target}{sep}")) {
                return Err("native publication may not overlap authoring storage".into());
            }
        }
        Ok(())
    }
    pub fn map_key(&self) -> Option<&str> {
        self.location.as_ref().map(|l| l.map.as_str())
    }
    pub fn owned_dir(&self) -> Option<PathBuf> {
        self.location.as_ref().map(|l| l.root.join(&l.native))
    }
    pub fn recover(&self, donor: &MapModel, m: &MapModel) -> Result<(), String> {
        let l = self.location.as_ref().ok_or("no authoring owner")?;
        let c = Context::open(&l.root, &l.map)?;
        c.check_game(&l.game)?;
        if self
            .context()
            .ok()
            .flatten()
            .map(|c| c.id().to_owned())
            .or_else(|| self.expected_id.clone())
            .as_ref()
            .is_some_and(|id| id != c.id())
        {
            return Err("authoring baseline identity changed; reopen before recovery".into());
        }
        c.recover(donor)?;
        c.resolve()?.check_model(m)?;
        // Deliberate reconciliation of a partial native publication is safe:
        // subsequent output is recomputed from B0 and this document's final LF.
        *self.state.lock().unwrap() = State::Bound(c);
        self.revision.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    pub fn recover_with(&self, m: &MapModel, read: impl FnMut(&str) -> Result<Vec<u8>, String>) -> Result<(), String> {
        let l = self.location.as_ref().ok_or("no authoring owner")?;
        let c = Context::open(&l.root, &l.map)?;
        c.check_game(&l.game)?;
        if self
            .context()
            .ok()
            .flatten()
            .map(|c| c.id().to_owned())
            .or_else(|| self.expected_id.clone())
            .as_ref()
            .is_some_and(|id| id != c.id())
        {
            return Err("authoring baseline identity changed; reopen before recovery".into());
        }
        c.recover_with(read)?;
        c.resolve()?.check_model(m)?;
        *self.state.lock().unwrap() = State::Bound(c);
        self.revision.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    pub fn revision(&self) -> u64 {
        self.revision.load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn read_recovery(slot: &Path) -> Result<Vec<String>, String> {
    let path = slot.join(RECOVERY);
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("oversized terrain recovery authoring record".into());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: Vec<String> = text.lines().map(str::to_owned).collect();
    let valid = match v.first().map(String::as_str) {
        Some("WCE-TERRAIN-RECOVERY-1") => v.len() == 7,
        Some("WCE-TERRAIN-RECOVERY-2") => v.len() == 9,
        _ => false,
    };
    if !valid {
        return Err("unknown terrain recovery authoring record".into());
    }
    if v.len() == 9
        && (!matches!(v[7].as_str(), "verified" | "blocked" | "legacy")
            || v[8].len() % 2 != 0
            || !v[8].bytes().all(|b| b.is_ascii_hexdigit())
            || (v[7] == "blocked" && v[8].is_empty())
            || (v[7] != "blocked" && !v[8].is_empty())
            || (v[7] == "verified" && v[5].is_empty()))
    {
        return Err("invalid terrain recovery authority verdict".into());
    }
    for hash in [&v[5], &v[6]] {
        if !hash.is_empty() && (hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit())) {
            return Err("invalid terrain recovery identity".into());
        }
    }
    Ok(v)
}
