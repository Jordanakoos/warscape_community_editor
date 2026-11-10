use std::sync::Arc;
use wce_editor_core::document::Document;
use wce_foundation::game::GameProfile;
use wce_terrain::{LfRole, MapKind};
use wce_terrain_editor::{MapDocument, document::MapSource};
use wce_terrain_io::{MapFiles, built};
use wce_tile::battle_map::tile_list::{TileList, TileListPlacement};
use super::validate::{self, Category, Finding, Severity};

fn list(rows: u32) -> TileList {
    let mut tail = 4u32.to_le_bytes().to_vec();
    tail.extend(3u32.to_le_bytes());
    for name in ["one", "two", "three"] { tail.extend((name.len() as u16).to_le_bytes()); tail.extend(name.bytes()); }
    tail.extend(rows.to_le_bytes());
    tail.extend(vec![0xa5; rows as usize * 32]);
    TileList { version: 9, is_campaign: true, names: vec!["shared".into()],
        // Deliberate coincident placements: campaigns share tile geometry.
        placements: vec![TileListPlacement::default(); 2], tail, ..Default::default() }
}

fn files(list: &TileList) -> MapFiles {
    MapFiles::from_bytes([
        ("terrain.tile_list".into(), list.write()),
        ("lf_height_map.dds".into(), wce_terrain_io::attila::heights::lf_height_dds(&wce_terrain::paged::PagedU16::filled(16, 16, 12000))),
        ("lf_normal.dds".into(), b"unknown opaque normal profile".to_vec()),
        ("unknown.bin".into(), vec![0, 255, 128, 13, 10]),
    ])
}

fn document(m: wce_terrain::MapModel) -> MapDocument {
    let mut d = MapDocument::new("campaigns/control".into(), MapSource::Packs("test".into()), Some(m), Arc::default(), None, Arc::default());
    d.history_mut().mark_saved(); d
}

fn check(d: &MapDocument) -> Vec<Finding> { validate::check(d, &[], &GameProfile::ROME_2, None) }
fn state(d: &MapDocument) -> (usize, usize, u64, bool, u64) {
    (d.history().size(), d.history().position(), d.history().version(), d.modified(), d.terry_revision.load(std::sync::atomic::Ordering::Relaxed))
}

#[test]
fn validation_shared_geometry_and_opaque_resources_are_valid_and_read_only() {
    let original = files(&list(2));
    let d = document(built::read_as("campaigns/control", MapKind::Campaign, &original).unwrap());
    let before = state(&d);
    let native = built::write(d.model().unwrap(), None).unwrap();
    let pointer = d.placed.clone();
    for _ in 0..3 {
        let findings = check(&d);
        assert!(findings.iter().all(|f| f.category != Category::InvalidData && f.severity != Severity::Error), "{findings:?}");
        assert!(findings.iter().any(|f| f.text.contains("shared geometry")));
        assert_eq!(state(&d), before);
        assert!(Arc::ptr_eq(&pointer, &d.placed));
        assert_eq!(built::write(d.model().unwrap(), None).unwrap(), native);
        assert!(built::changed(&original, &native).is_empty());
    }
}

#[test]
fn validation_missing_resource_is_invalid_not_an_unsupported_format() {
    let mut source = files(&list(0)); source.folder.remove("lf_height_map.dds");
    let d = document(built::read("campaigns/control", &source).unwrap());
    let f = check(&d).into_iter().find(|f| f.resource == "lf_height_map.dds" && f.severity == Severity::Error).unwrap();
    assert_eq!(f.category, Category::InvalidData);
    assert_eq!((f.game, f.kind), ("rome_2", MapKind::Campaign));
    assert!(f.display().contains("native Export"));
}

#[test]
fn validation_v9_variable_names_zero_rows_and_paired_count_edit_use_checked_rules() {
    for rows in [0, 2] {
        let source = files(&list(rows));
        let mut m = built::read("campaigns/control", &source).unwrap();
        m.tile_list.as_mut().unwrap().edit().placements.pop();
        let d = document(m);
        let findings = check(&d);
        assert!(!findings.iter().any(|f| f.category == Category::InvalidData), "{findings:?}");
        let blocked = findings.iter().any(|f| f.category == Category::UnsupportedOperation && f.affects == "Save / native Export");
        assert_eq!(blocked, rows != 0, "{findings:?}");
        assert_eq!(built::write(d.model().unwrap(), None).is_err(), blocked);
    }
}

#[test]
fn validation_original_metadata_inconsistency_and_unknown_layout_are_distinct() {
    let mut bad = list(1);
    let d = document(built::read("campaigns/control", &files(&bad)).unwrap());
    assert!(check(&d).iter().any(|f| f.category == Category::InvalidData && f.text.contains("paired rows")));
    let mut m = built::read("campaigns/control", &files(&bad)).unwrap();
    let _ = m.tile_list.as_mut().unwrap().edit(); // Taking a mutable reference is not an effective native edit.
    assert!(check(&document(m)).iter().any(|f| f.category == Category::InvalidData && f.text.contains("paired rows")));
    bad.tail = vec![99, 1, 2, 3];
    let source = files(&bad);
    let mut m = built::read("campaigns/control", &source).unwrap();
    let findings = check(&document(m.clone()));
    assert!(!findings.iter().any(|f| f.category == Category::InvalidData));
    assert!(findings.iter().any(|f| f.severity == Severity::Information && f.text.contains("Unknown v9")));
    assert!(built::changed(&source, &built::write(&m, None).unwrap()).is_empty());
    m.tile_list.as_mut().unwrap().edit().placements[0].x += 1;
    assert!(check(&document(m)).iter().any(|f| f.severity == Severity::Error && f.category == Category::UnsupportedOperation));
}

#[test]
fn validation_invalid_placement_reference_has_location() {
    let mut l = list(0); l.placements[1].name = 8;
    let d = document(built::read("campaigns/control", &files(&l)).unwrap());
    let f = check(&d).into_iter().find(|f| f.category == Category::InvalidData).unwrap();
    assert_eq!(f.entity.as_deref(), Some("placement 1")); assert_eq!(f.cell, Some((0, 0)));
}

#[test]
fn validation_effective_campaign_height_edit_is_not_certified_for_native_export() {
    let mut m = built::read("campaigns/control", &files(&list(0))).unwrap();
    m.lf_mut(LfRole::Land).unwrap().edit().set(4, 4, 12001);
    let d = document(m.clone()); let before = state(&d);
    assert!(check(&d).iter().any(|f| f.category == Category::UnsupportedOperation && f.affects == "Save / native Export"));
    assert_eq!(state(&d), before);
    // Valid WCE metadata can be inspected without preparing heights, checking held preparation profiles, or claiming Save is safe.
    let mut b = b"WCHE\x01\0\0\0".to_vec();
    for n in [8u32, 8, 1] { b.extend(n.to_le_bytes()); }
    b.extend([0u8; 24]); b.extend(3u32.to_le_bytes()); b.extend(3u32.to_le_bytes()); b.extend(1f64.to_le_bytes());
    m.carried.insert(built::campaign_authority::PROJECT_STATE.into(), wce_tile_model::Blob::Bytes(b.into()));
    let findings = check(&document(m));
    assert!(findings.iter().any(|f| f.category == Category::UnsupportedOperation && f.affects == "native Export" && f.text.contains("baseline/profile")));
}

#[test]
fn validation_opaque_props_are_view_only_and_missing_native_pair_is_invalid() {
    let mut source = files(&list(0));
    source.folder.insert("prop_marker.markers".into(), wce_tile_model::Blob::Bytes(Arc::from(&b"unknown marker version"[..])));
    source.folder.insert("tile_map.index".into(), wce_tile_model::Blob::Bytes(Arc::from(&b"opaque index"[..])));
    let d = document(built::read("campaigns/control", &source).unwrap());
    let findings = check(&d);
    assert!(findings.iter().any(|f| f.resource == "prop_marker.markers" && f.category == Category::Limitation));
    assert!(findings.iter().any(|f| f.resource == "tile_map.index / tile_map.tiles" && f.category == Category::InvalidData));
    assert!(built::changed(&source, &built::write(d.model().unwrap(), None).unwrap()).is_empty());
}

#[test]
fn validation_terry_missing_model_and_child_references_are_identified() {
    let scene = wce_terrain::terry::TerryProject::parse(br#"<project><scene><entity id="a" name="test"><components><ECPropMesh/><ECMesh model_path=""/></components><children><child id="b"/></children></entity></scene></project>"#.to_vec()).unwrap();
    let mut m = wce_terrain::MapModel { kind: MapKind::Campaign, ..Default::default() };
    m.project = Some(wce_terrain::Item::new("test.terry", scene, wce_tile_model::Blob::Bytes(Arc::from(&b"original scene"[..]))));
    let findings = validate::check(&document(m), &[], &GameProfile::ATTILA, None);
    assert!(findings.iter().any(|f| f.resource == "test.terry" && f.entity.as_deref() == Some("entity a") && f.text.contains("model_path")));
    assert!(findings.iter().any(|f| f.text.contains("Child reference b")));
    assert!(findings.iter().any(|f| f.category == Category::Limitation && f.text.contains("native Y")));
}

#[test]
fn validation_campaign_palette_and_fill_are_not_battle_corruption_rules() {
    for kind in [MapKind::Campaign, MapKind::Battle] {
        let mut m = wce_terrain::MapModel { kind, ..Default::default() };
        let origin = wce_tile_model::Blob::Bytes(Arc::from(&b"original image"[..]));
        m.layers.push((wce_terrain::LayerRole::TileMap, wce_terrain::Item::new("tile_map.png",
            wce_terrain::cells::CellLayer::filled(4, 4, false, [30, 30, 100, 255]), origin)));
        let mut d = document(m);
        d.fill_messages.push("Failed to find tile for point 1x2".into());
        let findings = check(&d);
        let tile: Vec<_> = findings.iter().filter(|f| f.resource == "tile_map.png").collect();
        assert_eq!(tile.iter().any(|f| f.severity == Severity::Error), kind == MapKind::Battle);
        assert_eq!(tile.iter().any(|f| f.category == Category::Limitation), kind == MapKind::Campaign);
    }
}
