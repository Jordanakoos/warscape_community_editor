//! Compare the Rust forward adapter against independently validated native height outputs, without writing any map/pack.
//! cargo run --release -p wce_terrain_io --example campaign_height_delta_check -- <original folder> <nodal text> <expected folder>
//! Nodal text: inclusive x0 y0 x1 y1, then a complete row-major rectangle of raw f64 deltas. No proprietary fixture is committed.

use std::path::Path;

use wce_terrain::paged::PagedU16;
use wce_terrain::{LfRole, MapKind};
use wce_terrain_io::built::campaign_heights::{NodalDelta, prepare_delta};
use wce_terrain_io::{MapFiles, built};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "original folder, nodal text, independently validated expected folder");
    let files = MapFiles::from_dir(Path::new(&args[0])).unwrap();
    let text = std::fs::read_to_string(&args[1]).unwrap();
    let mut numbers = text.split_whitespace();
    let bounds = std::array::from_fn(|_| numbers.next().unwrap().parse().unwrap());
    let values = numbers.map(|s| s.parse().unwrap()).collect();
    let prepared = prepare_delta(&files, &NodalDelta::new(bounds, values).unwrap()).unwrap();
    assert_eq!(prepared.height_files.folder.len(), 3);
    for (name, blob) in &prepared.height_files.folder {
        let made = blob.bytes().unwrap();
        let expected = std::fs::read(Path::new(&args[2]).join(name)).unwrap();
        if made.as_ref() != expected.as_slice() {
            let first = made.iter().zip(&expected).position(|(a, b)| a != b);
            panic!("prepared {name} differs from the independently validated output: lengths {}/{}, first mismatch {first:?}", made.len(), expected.len());
        }
    }
    // Height-only preparation must not weaken the shared save/export guard while normals remain unresolved.
    let mut m = built::read_as("reference", MapKind::Campaign, &files).unwrap();
    let bytes = prepared.height_files.folder["lf_height_map.dds"].bytes().unwrap();
    let u = |o| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    *m.lf_mut(LfRole::Land).unwrap().edit() = PagedU16::from_le_rows(u(16), u(12), &bytes[128..]);
    assert!(built::write(&m, None).is_err(), "normal-dependent save/export must remain blocked");
    let s = prepared.stats;
    println!("{{\"all_three_files_byte_identical_to_validated_outputs\":true,\"chmf_changed_regions\":{},\"chmf_changed_samples\":{},\"full_changed_samples\":{},\"lq_changed_samples\":{},\"full_lq_delta_error\":{},\"normal_export_guard_remains\":true,\"normals_regenerated\":false,\"native_files_written\":0}}",
        s.chmf_regions, s.chmf_samples, s.full_samples, s.lq_samples, s.full_lq_delta_error);
}
