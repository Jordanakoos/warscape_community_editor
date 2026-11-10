use std::sync::Arc;
use wce_terrain::{MapKind, MapModel, Item, terry::TerryProject};
use wce_terrain_editor::{MapDocument, MapSource, document::TerryFrame};
use wce_tile::battle_map::{context::{PlacedMap, LfField, Spot}, tile_db::TileDatabase};
use wce_tile_editor::{tile_document::context::{ContextRequest, Frame}, terrain_bake::TerrainLook};
use wce_tile_model::Blob;

pub fn fixture() -> (Box<MapDocument>, Arc<Frame>) {
    let xml = br#"<project><scene>
      <entity id="1" name="a"><components><ECPropMesh/><ECMesh model_path="a"/><ECTransform position="2.56 0 2.56" rotation="0 0 0" scale="1 1 1"/></components><children/></entity>
      <entity id="2" name="b"><components><ECPropMesh/><ECMesh model_path="b"/><ECTransform position="3 0 3" rotation="0 0 0" scale="1 1 1"/></components><children/></entity>
    </scene></project>"#;
    let p = TerryProject::parse(xml.to_vec()).unwrap();
    let model = MapModel { kind: MapKind::Campaign, project: Some(Item::new("test.terry", p, Blob::Bytes(Arc::from(&xml[..])))), ..Default::default() };
    let placed = Arc::new(PlacedMap { width: 16, height: 16, lf: Some(LfField::new(2, 2, vec![10000, 20000, 30000, 40000])), ..Default::default() });
    let mut d = MapDocument::new("campaigns/test".into(), MapSource::Kit(Default::default()), Some(model), placed.clone(), None, Arc::new(TileDatabase::default()));
    d.terry_frame = TerryFrame::campaign(16, 16);
    let frame = Frame::new(&ContextRequest { map: placed, spot: Spot { x: 0, y: 0, rotation: 0 }, cells: (0, 0), unit: 2.,
        near: 1, mid: 2, ground: 3, far: 4, raw_root: Default::default(), tile_roots: vec![], game_kit: None,
        look: TerrainLook::default(), trees: false, bake_ground: false, plain: None, groups: None, cache: None, provider: None }, None);
    (Box::new(d), Arc::new(frame))
}

#[test]
fn inspector_position_edit_keeps_precise_preview_invalidation_and_history() {
    use wce_editor_core::{document::Document, inspector::{Inspector, InspectTarget}};
    use wce_scene::render_scene::SourceRef;
    struct Edit { target: InspectTarget }
    impl Inspector for Edit {
        fn target(&self) -> InspectTarget { self.target }
        fn set_target(&mut self, t: InspectTarget) { self.target = t; }
        fn section(&mut self, _: &str) {}
        fn text(&mut self, _: &str, _: &str, _: &str) {}
        fn edit_float3(&mut self, field: &str, _: &str, v: &mut [f32; 3], _: f32) -> bool {
            if field == "ECTransform.position" { *v = [2.7, 0., 2.56]; true } else { false }
        }
        fn edit_float(&mut self, _: &str, _: &str, _: &mut f32, _: f32, _: f32, _: f32) -> bool { false }
        fn edit_float2(&mut self, _: &str, _: &str, _: &mut [f32; 2], _: f32) -> bool { false }
        fn edit_int(&mut self, _: &str, _: &str, _: &mut i64, _: f32) -> bool { false }
        fn edit_color3(&mut self, _: &str, _: &str, _: &mut [f32; 3]) -> bool { false }
        fn edit_color4(&mut self, _: &str, _: &str, _: &mut [f32; 4]) -> bool { false }
        fn edit_bool(&mut self, _: &str, _: &str, _: &mut bool) -> bool { false }
        fn edit_string(&mut self, _: &str, _: &str, _: &mut String) -> bool { false }
        fn edit_choice(&mut self, _: &str, _: &str, _: &mut i32, _: &[String]) -> bool { false }
        fn gesture_ended(&mut self) -> bool { true }
    }
    let (mut d, _) = fixture(); let src = SourceRef { doc: d.core().id(), item: 1 };
    let original = d.project().unwrap().save();
    let mut edit = Edit { target: InspectTarget { doc: src.doc, item: src.item } };
    d.inspect(src, &mut edit);
    assert_eq!(d.terry_revision.positions_since(0), Some(vec![1]));
    assert_eq!(d.history().size(), 1);
    assert!((d.project().unwrap().world_position(1)[0] - 2.7).abs() < 1e-6);
    d.history_mut().undo(); assert_eq!(d.project().unwrap().save(), original);
    d.history_mut().redo(); assert!((d.project().unwrap().world_position(1)[0] - 2.7).abs() < 1e-6);
}
