//! Read-only native check of the built campaign preservation boundary. All edits stay in memory.
//! cargo run --release -p wce_terrain_io --example built_campaign_height_check -- <native folder>

use std::path::Path;

use wce_formats::chmf::ChmfFile;
use wce_terrain::lf_edit::LfStroke;
use wce_terrain::{LfRole, MapKind};
use wce_terrain_io::{MapFiles, built};

fn main() {
    let dir = std::env::args().nth(1).expect("read-only native campaign reference folder");
    let files = MapFiles::from_dir(Path::new(&dir)).unwrap();
    let mut m = built::read_as("native-reference", MapKind::Campaign, &files).unwrap();
    assert_eq!(m.kind, MapKind::Campaign);
    assert!(built::changed(&files, &built::write(&m, None).unwrap()).is_empty());
    let c = ChmfFile::parse(&files.folder["lf_height_map.data"].bytes().unwrap()).unwrap();
    let field = m.lf_mut(LfRole::Land).unwrap().edit();
    let (w, h) = (field.width(), field.height());
    let (x, y) = (1000.min(w - 1), 1000.min(h - 1));
    let before = field.at(x, y);
    let after = if before < 61000 { before + 4096 } else { before - 4096 };
    let mut stroke = LfStroke::default();
    stroke.set(field, x, y, after);
    let record = stroke.finish(field).unwrap();
    let error = built::write(&m, None).unwrap_err().to_string();
    assert!(error.contains("unsupported"));
    eprintln!("{error}");
    record.revert(m.lf_mut(LfRole::Land).unwrap().edit());
    let restored = built::write(&m, None).unwrap();
    assert!(built::changed(&files, &restored).is_empty());
    let reopened = built::read_as("renamed", MapKind::Campaign, &restored).unwrap();
    assert!(built::changed(&files, &built::write(&reopened, None).unwrap()).is_empty());
    record.apply(m.lf_mut(LfRole::Land).unwrap().edit());
    assert_eq!(m.lf(LfRole::Land).unwrap().get().at(x, y), after);
    assert!(built::write(&m, None).is_err());
    println!("{{\"campaign_kind\":true,\"no_op_all_files_identical\":true,\"input_files\":{},\"full_dimensions\":[{},{}],\"chmf_dimensions\":[{},{}],\"chmf_parameters\":[{},{}],\"chmf_regions\":{},\"changed_sample\":[{},{},{},{}],\"edited_output_refused\":true,\"undo_all_files_identical\":true,\"reopen_all_files_identical\":true,\"redo_refused\":true,\"native_files_written\":0}}",
        files.folder.len(), w, h, c.width, c.height, c.cells_x, c.cells_y, c.regions.len(), x, y, before, after);
}
