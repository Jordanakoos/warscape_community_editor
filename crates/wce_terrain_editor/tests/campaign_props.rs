use std::{sync::Arc, path::PathBuf};
use wce_editor_core::document::{Document,SaveTarget};
use wce_formats::markers::{MarkersFile,MarkerGroup,KeyPart};
use wce_foundation::math::Float4x4;
use wce_scene::render_scene::SourceRef;
use wce_terrain::{MapModel,MapKind,Item,campaign_props::{CampaignProps,PROP_KEY}};
use wce_terrain_editor::{MapDocument,MapSource};
use wce_tile::battle_map::{context::PlacedMap,tile_db::TileDatabase};
use wce_tile_model::Blob;

fn document()->MapDocument {
    let mut record=vec![0;74];for i in [0,5,10,15]{record[i*4..i*4+4].copy_from_slice(&1f32.to_le_bytes());}
    let f=MarkersFile {repository:"BASE_MARKER_REPOSITORY".into(),repository_id:645563,repository_u32:1,repository_u16:1,
        class_id:645566,class_name:"PROP_MARKER".into(),version:6,records:vec![record],
        groups:Some(vec![MarkerGroup{key:vec![KeyPart::Wide("model".encode_utf16().collect())],indices:vec![0]}])};
    let blob=Blob::Bytes(Arc::from(f.write()));
    let item=Item::new("prop_marker.markers",CampaignProps::parse(blob.bytes().unwrap()).unwrap(),blob.clone());
    let m=MapModel {kind:MapKind::Campaign,built:true,native_props:Some(item),carried:std::collections::BTreeMap::from([
        ("prop_marker.markers".into(),blob),("lf_normal.dds".into(),Blob::Bytes(Arc::from(&b"normal"[..]))),
        ("terrain.tile_list".into(),Blob::Bytes(Arc::from(wce_tile::battle_map::tile_list::TileList {version:9,is_campaign:true,..Default::default()}.write()))),
        ("unrelated".into(),Blob::Bytes(Arc::from(&b"unchanged"[..])))]),..Default::default()};
    let mut d=MapDocument::new("campaigns/test".into(),MapSource::Packs("terrain/campaigns/test".into()),Some(m),Arc::new(PlacedMap::default()),None,Arc::new(TileDatabase::default()));
    d.terry_frame=wce_terrain_editor::document::TerryFrame::campaign(16,16);d.history_mut().mark_saved();d
}

#[test]
fn gizmo_gesture_merges_undo_redo_and_refusals_preserve_state() {
    let mut d=document();let item=SourceRef {doc:d.core().id(),item:PROP_KEY};
    let mut delta=Float4x4::IDENTITY;delta.m[0][3]=1.;
    d.transform_items(&[item],&delta,10);d.transform_items(&[item],&delta,10);
    assert_eq!(d.history().size(),1);let moved=d.native_props().unwrap().position(0).unwrap();assert!(moved[0]>0.);
    let position=d.history().position();
    assert!(d.translate_native_props(&[(0,[f32::NAN,0.,0.])],12).is_err());
    assert!(d.translate_native_props(&[(0,[0.,1.,0.])],12).is_err());
    assert!(d.translate_native_props(&[(0,[1.,0.,0.]),(2,[0.;3])],12).is_err());
    assert_eq!(d.history().position(),position);assert_eq!(d.native_props().unwrap().position(0).unwrap(),moved);
    d.history_mut().undo();assert_eq!(d.native_props().unwrap().position(0).unwrap(),[0.;3]);assert!(!d.history().modified());
    d.history_mut().redo();assert_eq!(d.native_props().unwrap().position(0).unwrap(),moved);
    d.transform_items(&[item],&delta,11);assert_eq!(d.history().size(),2);
    let before=d.native_props().unwrap().write();d.scale_items(&[item],[2.;3],&delta,12);
    assert_eq!(d.native_props().unwrap().write(),before);
}

#[test]
fn save_reopen_repeat_save_as_and_failed_save_preserve_authoring() {
    let mut d=document();let original=d.native_props().unwrap().write();
    let scratch=std::env::var_os("D3B_SCRATCH").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
        .join(format!("wce-d3b-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&scratch).unwrap();
    d.translate_native_props(&[(0,[1.,0.,2.])],0).unwrap();
    let target=SaveTarget{path:scratch.join("project")};d.save(&target).unwrap();d.save(&target).unwrap();
    let files=wce_terrain_io::MapFiles::from_dir(&target.path).unwrap();
    let reopened=wce_terrain_io::built::read("campaign",&files).unwrap();
    assert_eq!(reopened.native_props.unwrap().get().position(0).unwrap(),[1.,0.,2.]);
    let copy=SaveTarget{path:scratch.join("copy")};d.save(&copy).unwrap();
    assert_eq!(std::fs::read(target.path.join("prop_marker.markers")).unwrap(),std::fs::read(copy.path.join("prop_marker.markers")).unwrap());
    d.history_mut().undo();d.save(&target).unwrap();assert_eq!(std::fs::read(target.path.join("prop_marker.markers")).unwrap(),original.as_ref());
    d.history_mut().redo();let version=d.history().version();let native=d.native_props().unwrap().write();
    std::fs::write(scratch.join("blocked"),b"file").unwrap();
    assert!(d.save(&SaveTarget{path:scratch.join("blocked/child")}).is_err());
    assert!(d.history().modified());assert_eq!(d.history().version(),version);assert_eq!(d.native_props().unwrap().write(),native);
    assert_eq!(std::fs::read(target.path.join("lf_normal.dds")).unwrap(),b"normal");
    // No recursive cleanup: bounded files only, under this unique scratch folder.
}

#[test]
fn attila_terry_campaign_prop_still_uses_its_existing_adapter() {
    let xml=br#"<project version="3"><scene version="10"><entity id="1" name="prop"><components><ECMesh model_path="model"/><ECPropMesh/><ECTransform position="0 0 0" rotation="0 0 0" scale="1 1 1"/></components></entity></scene></project>"#;
    let p=wce_terrain::terry::TerryProject::parse(xml.to_vec()).unwrap();
    let model=MapModel {kind:MapKind::Campaign,project:Some(Item::new("map.terry",p,Blob::Bytes(Arc::from(&xml[..])))),..Default::default()};
    let mut d=MapDocument::new("campaigns/test".into(),MapSource::Kit(PathBuf::new()),Some(model),Arc::new(PlacedMap::default()),None,Arc::new(TileDatabase::default()));
    d.terry_frame=wce_terrain_editor::document::TerryFrame::campaign(16,16);
    let src=SourceRef{doc:d.core().id(),item:1};let mut delta=Float4x4::IDENTITY;delta.m[0][3]=20.;
    d.transform_items(&[src],&delta,10);
    let built=wce_terrain_io::attila::campaign::prop_markers(d.project().unwrap()).unwrap();
    assert_eq!(MarkersFile::parse(&built).unwrap().records.len(),1);
    assert!(d.project().unwrap().world_position(1)[0]>0.);
    d.history_mut().undo();
    assert_eq!(wce_terrain_io::attila::write(d.model().unwrap()).unwrap().folder["map.terry"].bytes().unwrap().as_ref(),xml);
    assert!(d.native_props().is_none());
}
