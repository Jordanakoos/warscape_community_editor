//! Asset-free fixture generator and bounded allocation receipt. Run with a NEW
//! destination root; uses the same fixture and production paths as the tests.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use wce_editor_core::document::{Document, SaveTarget};
use wce_terrain::LfRole;
use wce_terrain_editor::{MapDocument, MapSource};
use wce_terrain_io::{MapFiles, built};
use wce_tile::battle_map::{context::PlacedMap, tile_db::TileDatabase};

#[path = "../tests/support/native_battle.rs"]
mod native_fixture;
#[global_allocator]
static ALLOC: wce_foundation::heap::CountingAlloc = wce_foundation::heap::CountingAlloc;

fn write(files: &MapFiles, path: &Path) -> Result<(), String> {
    use wce_foundation::publication::{Output, Source, Staged, resource_path};
    let outputs = files
        .folder
        .iter()
        .map(|(key, blob)| Ok(Output::new(resource_path(path, key)?, Source::Bytes(blob.bytes().map_err(|e| e.to_string())?))))
        .collect::<Result<Vec<_>, String>>()?;
    Staged::prepare(outputs).and_then(Staged::publish).map_err(|e| e.to_string())?;
    Ok(())
}
fn size(root: &Path) -> u64 {
    fs::read_dir(root).unwrap().flatten().map(|e| if e.path().is_dir() { size(&e.path()) } else { e.metadata().unwrap().len() }).sum()
}
fn main() -> Result<(), String> {
    use wce_foundation::heap;
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("usage: native_baseline_cost <new-root>")?);
    if root.exists() {
        return Err("destination must be new".into());
    }
    let map = "fb_phase1_battle";
    let native = root.join(format!("native/{map}"));
    let source = root.join("source");
    write(&native_fixture::fixture(), &source)?;
    let model = built::read(map, &MapFiles::from_dir(&source).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let db = Arc::new(TileDatabase::default());
    let placed = Arc::new(PlacedMap::from_tile_list(map, model.tile_list.as_ref().unwrap().get(), &db, None));
    let mut d = MapDocument::new(map.into(), MapSource::Kit(source), Some(model), placed, None, db);
    d.save_dir = Some(native.clone());
    d.observe_save_destination()?;
    let before = heap::live();
    d.configure_authoring(&root, "rome_2", map, &native, true);
    d.authoring.context()?; // Pin originals, releasing the pending payload.
    let bound_idle_delta = heap::live() as i64 - before as i64;
    let metadata = d.authoring.context()?.unwrap().metadata_bytes();
    let f = d.lf_field_mut(LfRole::Land).unwrap();
    f.set(63, 64, 10200);
    f.set(64, 64, 10300);
    f.set(65, 64, 10100);
    let idle = heap::live();
    heap::reset_peak();
    let files = d.authoring.write(d.model().unwrap(), Some(&d.db), true)?;
    let writer_peak = heap::peak() - idle;
    let normal = files.folder["normals/normal_0.dds"].bytes().map_err(|e| e.to_string())?;
    let hash = built::baseline::digest(&normal);
    assert_eq!(hash, "2ffa5f8f4bf4d8ed5dace39b67058b513c65a2d039f8b00c83ca278dbb32fe1e");
    drop(normal);
    drop(files);
    let idle = heap::live();
    heap::reset_peak();
    d.save(&SaveTarget::default())?;
    let save_peak = heap::peak() - idle;
    let record = fs::metadata(built::baseline::Context::record_path(&root, map)?).map_err(|e| e.to_string())?.len();
    let payload = size(&root.join("authoring/terrain/baselines"));
    println!(
        "{{\"payload_bytes\":{payload},\"record_bytes\":{record},\"bound_idle_allocation_delta\":{bound_idle_delta},\"context_metadata_capacity_bytes\":{metadata},\"writer_peak_extra_heap_bytes\":{writer_peak},\"save_peak_extra_heap_bytes\":{save_peak},\"normal_sha256\":\"{hash}\"}}"
    );
    Ok(())
}
