//! Permanent F-B regression: synthetic Phase 1 input, production document,
//! writer, publication and recovery paths. No adapter and no game resources.
use crate::{MapDocument, MapSource, publication_tests::FixtureWrite};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use wce_editor_core::document::{Document, SaveTarget};
use wce_terrain::{LfRole, MapModel, lf_edit::LfStroke, paged::PagedU16};
use wce_terrain_io::{
    MapFiles,
    built::{
        self,
        baseline::{Context, digest},
    },
};
use wce_tile::battle_map::{context::PlacedMap, tile_db::TileDatabase, tile_list::TileList};
const LF: &str = "lf_height_map.dds";
const CHMF: &str = "lf_height_map.data";
const N: &str = "normals/normal_0.dds";
const MAP: &str = "fb_phase1_battle";
const GOLDEN: &str = "2ffa5f8f4bf4d8ed5dace39b67058b513c65a2d039f8b00c83ca278dbb32fe1e";
const A: [(u32, u32, u16); 2] = [(63, 64, 200), (64, 64, 200)];
const B: [(u32, u32, u16); 2] = [(64, 64, 100), (65, 64, 100)];
type Files = BTreeMap<String, Vec<u8>>;
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "wce-fb4-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        assert!(
            path.starts_with(std::env::temp_dir().canonicalize().unwrap())
                && path.file_name().unwrap().to_string_lossy().starts_with("wce-fb4-")
        );
        fs::remove_dir_all(path).unwrap();
    }
}
pub(crate) fn fixture() -> MapFiles {
    let mut lf = wce_terrain_io::attila::heights::lf_height_dds(&PagedU16::filled(192, 192, 10000));
    lf.extend(b"F-B synthetic DDS tail\0\xff");
    MapFiles::from_bytes([
        (LF.into(), lf),
        (CHMF.into(), wce_terrain_io::attila::heights::lf_height_data_rome2(&vec![10000; 192 * 192], 192, 192, (48, 48))),
        (N.into(), wce_tile_io::attila::bob_tile::dds_dxt5(&[0u8, 180, 255, 130].repeat(256 * 256), 256, 256)),
        ("terrain.tile_list".into(), TileList { version: 9, is_campaign: false, battle_size: [48, 48], ..Default::default() }.write()),
        ("opaque.bin".into(), b"F-B untouched synthetic resource\0\xff".to_vec()),
    ])
}
fn bytes(f: &MapFiles) -> Files {
    f.folder.iter().map(|(k, b)| (k.clone(), b.bytes().unwrap().to_vec())).collect()
}
fn native(root: &Path) -> PathBuf {
    root.join(format!("native/{MAP}"))
}
fn read(root: &Path) -> Files {
    bytes(&MapFiles::from_dir(&native(root)).unwrap())
}
fn doc(model: MapModel) -> MapDocument {
    let db = Arc::new(TileDatabase::default());
    let placed = Arc::new(PlacedMap::from_tile_list(MAP, model.tile_list.as_ref().unwrap().get(), &db, None));
    MapDocument::new(MAP.into(), MapSource::Kit(PathBuf::new()), Some(model), placed, None, db)
}
fn open(root: &Path, trusted: bool) -> MapDocument {
    let mut d = doc(built::read(MAP, &MapFiles::from_dir(&native(root)).unwrap()).unwrap());
    d.save_dir = Some(native(root));
    d.configure_authoring(root, "rome_2", MAP, &native(root), trusted);
    d.observe_save_destination().unwrap();
    d
}
fn project(root: &Path) -> MapDocument {
    fixture().write_to(&native(root)).unwrap();
    let mut d = open(root, true);
    save(&mut d);
    d
}
fn edit(d: &mut MapDocument, points: &[(u32, u32, u16)]) {
    let mut stroke = LfStroke::default();
    for &(x, y, delta) in points {
        let f = d.lf_field_mut(LfRole::Land).unwrap();
        let v = f.at(x, y) + delta;
        stroke.set(f, x, y, v);
    }
    let record = stroke.finish(d.lf_field(LfRole::Land).unwrap()).unwrap();
    d.push_stroke(crate::lf::LfStrokeCommand { role: LfRole::Land, record: Arc::new(record), mirror: None });
}
fn plan(d: &MapDocument) -> Files {
    bytes(&d.authoring.write(&d.publication_model().unwrap(), Some(&d.db), true).unwrap())
}
fn save(d: &mut MapDocument) -> Files {
    let expected = plan(d);
    d.save(&SaveTarget { path: PathBuf::new() }).unwrap();
    assert_eq!(bytes(&MapFiles::from_dir(d.save_dir.as_ref().unwrap()).unwrap()), expected);
    expected
}
fn assert_same(a: &Files, b: &Files) {
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (key, value) in a {
        assert_eq!(value, &b[key], "{key}");
    }
    let info = wce_formats::dds_decode::dds_info(&a[N]);
    assert_eq!(info.mips, 8);
    for mip in 0..info.mips {
        assert_eq!(
            wce_formats::dds_decode::dds_decode_mip(&a[N], mip).unwrap(),
            wce_formats::dds_decode::dds_decode_mip(&b[N], mip).unwrap()
        );
    }
}
#[test]
fn continuous_fresh_process_and_save_between_match_reference() {
    let s = Scratch::new();
    let c = s.0.join("continuous");
    let r = s.0.join("reopened");
    let k = s.0.join("kept-open");
    let mut d = project(&c);
    edit(&mut d, &A);
    let a = plan(&d);
    assert_ne!(a[N], bytes(&fixture())[N], "A must change normals");
    edit(&mut d, &B);
    let expected = save(&mut d);
    assert_eq!(digest(&expected[N]), GOLDEN, "do not regenerate the Phase 1 continuous reference");
    let mut d = project(&r);
    edit(&mut d, &A);
    save(&mut d);
    drop(d);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "baseline_tests::reopen_child", "--nocapture"])
        .env("WCE_FB4_CHILD_ROOT", &r)
        .output()
        .unwrap();
    assert!(child.status.success(), "{}\n{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
    assert!(String::from_utf8_lossy(&child.stdout).contains("test result: ok. 1 passed"));
    assert_same(&expected, &read(&r));
    let mut d = project(&k);
    edit(&mut d, &A);
    save(&mut d);
    edit(&mut d, &B);
    assert_same(&expected, &save(&mut d));
    let original = bytes(&fixture());
    for key in ["terrain.tile_list", "opaque.bin"] {
        assert_eq!(expected[key], original[key]);
    }
    let field = d.lf_field(LfRole::Land).unwrap();
    assert_eq!((field.at(63, 64), field.at(64, 64), field.at(65, 64)), (10200, 10300, 10100));
}
#[test]
fn reopen_child() {
    if let Some(root) = std::env::var_os("WCE_FB4_CHILD_ROOT") {
        let mut d = open(Path::new(&root), false);
        edit(&mut d, &B);
        let result = save(&mut d);
        assert_eq!(digest(&result[N]), GOLDEN);
    }
}
#[test]
fn undo_redo_original_lf_restores_all_original_mips_and_chmf() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    let original = read(&s.0);
    edit(&mut d, &A);
    let a = save(&mut d);
    assert!(d.history_mut().undo());
    assert_same(&original, &save(&mut d));
    assert!(d.history_mut().redo());
    assert_same(&a, &save(&mut d));
    edit(&mut d, &B);
    assert_eq!(digest(&save(&mut d)[N]), GOLDEN);
}
#[test]
fn standalone_save_as_authoring_copy_and_relocation() {
    let s = Scratch::new();
    let root = s.0.join("source");
    let copy = s.0.join("copy");
    let relocated = s.0.join("relocated");
    let mut d = project(&root);
    edit(&mut d, &A);
    let target = SaveTarget { path: s.0.join("standalone") };
    d.save(&target).unwrap();
    assert!(d.modified());
    assert_eq!(d.save_dir, Some(native(&root)));
    assert!(!target.path.join("authoring").exists());
    let mut standalone = doc(built::read(MAP, &MapFiles::from_dir(&target.path).unwrap()).unwrap());
    edit(&mut standalone, &B);
    assert!(standalone.authoring.write(standalone.model().unwrap(), None, true).is_err());
    d.save_authoring_copy(&copy).unwrap();
    assert!(d.modified());
    assert_eq!(d.save_dir, Some(native(&root)));
    drop(d);
    fs::rename(&copy, &relocated).unwrap();
    let mut d = open(&relocated, false);
    edit(&mut d, &B);
    let result = save(&mut d);
    assert_eq!(digest(&result[N]), GOLDEN);
    let record = fs::read_to_string(Context::record_path(&relocated, MAP).unwrap()).unwrap();
    assert!(!record.contains(&root.to_string_lossy().into_owned()));
    assert!(!record.contains(&copy.to_string_lossy().into_owned()));
}
#[test]
fn missing_corrupt_unknown_and_verified_recovery_preserve_edits() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    let before = read(&s.0);
    let rows = d.lf_field(LfRole::Land).unwrap().to_le_rows();
    let position = d.history().position();
    let c = d.authoring.context().unwrap().unwrap();
    let path = c.resource_dir().join(N);
    let original = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(d.save(&SaveTarget { path: PathBuf::new() }).is_err());
    assert_eq!(read(&s.0), before);
    assert_eq!(d.history().position(), position);
    assert_eq!(d.lf_field(LfRole::Land).unwrap().to_le_rows(), rows);
    let slot = s.0.join("recovery-session/slot");
    d.autosave_to(&slot).unwrap();
    assert!(slot.join(crate::authoring::RECOVERY).is_file());
    assert!(!slot.join("map/authoring").exists());
    fs::write(&path, vec![0; original.len()]).unwrap();
    assert!(d.save(&SaveTarget { path: PathBuf::new() }).is_err());
    let donor = built::read(MAP, &fixture()).unwrap();
    d.authoring.recover(&donor, d.model().unwrap()).unwrap();
    save(&mut d);
    edit(&mut d, &B);
    assert_eq!(digest(&save(&mut d)[N]), GOLDEN);
    let record = Context::record_path(&s.0, MAP).unwrap();
    let saved = fs::read(&record).unwrap();
    let unknown = String::from_utf8(saved.clone()).unwrap().replacen("WCE-TERRAIN-CONTEXT-1", "WCE-TERRAIN-CONTEXT-999", 1);
    fs::write(&record, unknown).unwrap();
    let mut blocked = open(&s.0, false);
    edit(&mut blocked, &B);
    assert!(blocked.save(&SaveTarget { path: PathBuf::new() }).is_err());
    assert!(blocked.authoring.adopt(blocked.model().unwrap()).is_err());
    fs::write(record, saved).unwrap();
}
#[test]
fn partial_publication_retry_and_reopen_refusal() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    edit(&mut d, &B);
    let expected = plan(&d);
    let position = d.history().position();
    d.publication_failure_after = Some(2);
    assert!(d.save(&SaveTarget { path: PathBuf::new() }).is_err());
    assert!(d.modified());
    assert_eq!(d.history().position(), position);
    let broken = open(&s.0, false);
    assert!(broken.authoring.resolve(broken.model().unwrap()).is_err());
    assert_same(&expected, &save(&mut d));
    assert_eq!(digest(&read(&s.0)[N]), GOLDEN);
}
#[test]
fn autosave_restoration_pins_durable_originals_and_retries() {
    let s = Scratch::new();
    let root = s.0.join("owner");
    let slot = s.0.join("old-session/slot");
    let mut d = project(&root);
    edit(&mut d, &A);
    d.autosave_to(&slot).unwrap();
    drop(d);
    let map = MapDocument::complete_autosave(&slot).unwrap();
    let mut restored = doc(built::read(MAP, &MapFiles::from_dir(&map).unwrap()).unwrap());
    restored.restore_authoring(&slot).unwrap();
    restored.restored_from = Some(slot.clone());
    restored.observe_save_destination().unwrap();
    let next = s.0.join("new-session/slot");
    restored.autosave_to(&next).unwrap();
    drop(restored);
    fs::remove_dir_all(s.0.join("old-session")).unwrap();
    let map = MapDocument::complete_autosave(&next).unwrap();
    let mut d = doc(built::read(MAP, &MapFiles::from_dir(&map).unwrap()).unwrap());
    d.restore_authoring(&next).unwrap();
    d.restored_from = Some(next);
    d.observe_save_destination().unwrap();
    edit(&mut d, &B);
    assert!(d.save_changes(&SaveTarget { path: PathBuf::new() }).unwrap().is_some());
    let expected = plan(&d);
    d.save_authoring_copy(&s.0.join("recovered-copy")).unwrap();
    assert!(d.save_changes(&SaveTarget { path: PathBuf::new() }).unwrap().is_some());
    d.publication_failure_after = Some(1);
    assert!(d.save_overwrite(&SaveTarget { path: PathBuf::new() }).is_err());
    assert!(d.modified());
    d.save_overwrite(&SaveTarget { path: PathBuf::new() }).unwrap();
    assert_same(&expected, &read(&root));
    assert!(!d.modified());
    assert_eq!(digest(&read(&root)[N]), GOLDEN);
    drop(d);
    assert_eq!(digest(&save(&mut open(&root, false))[N]), GOLDEN);
}
#[test]
fn legacy_adoption_is_explicit_and_does_not_discard_pending_edits() {
    let s = Scratch::new();
    fixture().write_to(&native(&s.0)).unwrap();
    let mut d = open(&s.0, false);
    edit(&mut d, &A);
    assert!(d.save(&SaveTarget { path: PathBuf::new() }).is_err());
    d.authoring.adopt(d.model().unwrap()).unwrap();
    save(&mut d);
    edit(&mut d, &B);
    assert_eq!(digest(&save(&mut d)[N]), GOLDEN);
    assert!(d.authoring.adopt(d.model().unwrap()).is_err());
}
#[test]
fn export_writer_uses_same_baseline_and_no_authoring_resources() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    save(&mut d);
    drop(d);
    let mut d = open(&s.0, false);
    edit(&mut d, &B);
    let exported = d.authoring.write(&d.publication_model().unwrap(), Some(&d.db), false).unwrap();
    assert_same(&bytes(&exported), &save(&mut d));
    assert_eq!(digest(&bytes(&exported)[N]), GOLDEN);
    assert!(exported.folder.keys().all(|k| !k.contains("authoring")));
    let b = d.authoring.context().unwrap().unwrap().resolve().unwrap();
    assert_eq!(b.bytes(), 162700);
}
