//! `.terry` edits of a map document: an attribute set (the Inspector, the gizmo: merged while one gesture lasts) and a structural edit
//! (entities added or removed: the text splices that did it). Both find their elements by entity id when they run, so they stay right
//! whatever the other edits did to the element table. Every one moves the document's `.terry` revision (the views draw the entities again).

use std::any::Any;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use wce_editor_core::command::Command;
use wce_formats::xml_doc::XmlSplice;
use wce_terrain::MapModel;

/// Moves with every `.terry` edit, undo and redo included.
pub type TerryRevision = Arc<TerryChanges>;

/// A bounded preview invalidation log. Unknown edits retain the ordinary full-refresh path.
/// This is editor state only; no authoring data or native representation is changed.
#[derive(Default)]
pub struct TerryChanges {
    revision: AtomicU64,
    positions: Mutex<VecDeque<(u64, u64)>>,
}

impl TerryChanges {
    pub fn load(&self, ordering: Ordering) -> u64 {
        self.revision.load(ordering)
    }

    /// An unclassified edit leaves a gap in the position log, forcing consumers to rebuild.
    pub fn fetch_add(&self, value: u64, ordering: Ordering) -> u64 {
        self.revision.fetch_add(value, ordering)
    }

    fn moved(&self, id: u64) {
        let mut log = self
            .positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        log.push_back((revision, id));
        while log.len() > 128 {
            log.pop_front();
        }
    }

    /// Only leaf-prop position edits since `after`, or none when any edit needs a full refresh.
    pub fn positions_since(&self, after: u64) -> Option<Vec<u64>> {
        let now = self.load(Ordering::Relaxed);
        if after > now || now - after > 128 {
            return None;
        }
        let log = self
            .positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut next = after;
        let mut ids = vec![];
        for &(revision, id) in log
            .iter()
            .filter(|(revision, _)| *revision > after && *revision <= now)
        {
            if revision != next + 1 {
                return None;
            }
            next = revision;
            ids.push(id);
        }
        if next != now {
            return None;
        }
        ids.sort_unstable();
        ids.dedup();
        Some(ids)
    }
}

fn leaf_prop(p: &wce_terrain::terry::TerryProject, id: u64) -> bool {
    let Some(e) = p.entity(id) else { return false };
    if !e.children.is_empty() || !matches!(p.kind(e), wce_terrain::terry::EntityKind::Prop(_)) {
        return false;
    }
    let Some(t) = p.component(e, "ECTransform") else {
        return false;
    };
    let elements = p.doc.elements();
    // Transform children are distinct from entity children; a parent move must still refresh descendants.
    elements[t]
        .children
        .iter()
        .all(|&c| elements[c].name != "children" || elements[c].children.is_empty())
}

#[cfg(test)]
mod preview_changes_tests {
    use super::*;

    #[test]
    fn position_log_deduplicates_and_unknown_or_expired_edits_fall_back() {
        let changes = TerryChanges::default();
        changes.moved(7);
        changes.moved(8);
        changes.moved(7);
        assert_eq!(changes.positions_since(0), Some(vec![7, 8]));
        assert_eq!(changes.positions_since(3), Some(vec![]));
        changes.fetch_add(1, Ordering::Relaxed);
        changes.moved(7);
        assert_eq!(changes.positions_since(3), None);
        assert_eq!(changes.positions_since(4), Some(vec![7]));
        for _ in 0..129 {
            changes.moved(7);
        }
        assert_eq!(changes.positions_since(5), None);
        assert_eq!(
            changes.positions_since(changes.load(Ordering::Relaxed) - 2),
            Some(vec![7])
        );
    }

    #[test]
    fn only_leaf_prop_positions_are_incremental_including_undo_redo() {
        use wce_terrain::{Item, MapModel, terry::TerryProject};
        use wce_tile_model::Blob;
        let xml = br#"<project><scene>
          <entity id="1"><components><ECPropMesh/><ECMesh model_path="p"/><ECTransform position="0 0 0" rotation="0 0 0"/></components><children/></entity>
          <entity id="2"><components><ECPropMesh/><ECMesh model_path="p"/><ECTransform position="0 0 0"><children><child id="1"/></children></ECTransform></components><children/></entity>
          <entity id="3"><components><ECTransform position="0 0 0"/></components><children/></entity>
        </scene></project>"#;
        let p = TerryProject::parse(xml.to_vec()).unwrap();
        assert!(leaf_prop(&p, 1));
        assert!(!leaf_prop(&p, 2));
        assert!(!leaf_prop(&p, 3));
        let mut model = Some(MapModel {
            kind: wce_terrain::MapKind::Campaign,
            project: Some(Item::new("test.terry", p, Blob::Bytes(Arc::from(&xml[..])))),
            ..Default::default()
        });
        let revision = Arc::new(TerryChanges::default());
        let mut command = TerryAttrCommand {
            id: 1,
            component: "ECTransform".into(),
            attr: "position".into(),
            before: "0 0 0".into(),
            after: "1 2 3".into(),
            key: 1,
            revision: revision.clone(),
        };
        command.apply(&mut model);
        assert_eq!(revision.positions_since(0), Some(vec![1]));
        command.revert(&mut model);
        assert_eq!(revision.positions_since(1), Some(vec![1]));
        assert_eq!(
            model
                .as_ref()
                .unwrap()
                .project
                .as_ref()
                .unwrap()
                .get()
                .save(),
            xml
        );
        command.apply(&mut model);
        assert_eq!(revision.positions_since(2), Some(vec![1]));
        command.id = 2;
        command.apply(&mut model);
        assert_eq!(revision.positions_since(3), None);
        command.id = 1;
        command.attr = "rotation".into();
        command.apply(&mut model);
        assert_eq!(revision.positions_since(4), None);
        model.as_mut().unwrap().kind = wce_terrain::MapKind::Battle;
        command.attr = "position".into();
        command.apply(&mut model);
        assert_eq!(revision.positions_since(5), None);
        model.as_mut().unwrap().kind = wce_terrain::MapKind::Campaign;
        model.as_mut().unwrap().built = true;
        command.apply(&mut model);
        assert_eq!(revision.positions_since(6), None);
    }
}

/// An attribute of an entity's component (`component` empty: of the `<entity>` element itself, its name).
pub struct TerryAttrCommand {
    pub id: u64,
    pub component: String,
    pub attr: String,
    pub before: String,
    pub after: String,

    /// Commands of one gesture merge (0: never).
    pub key: u64,
    pub revision: TerryRevision,
}

impl TerryAttrCommand {
    fn put(&self, model: &mut Option<MapModel>, value: &str) {
        let Some(model) = model.as_mut() else { return };
        let kit_campaign = model.kind == wce_terrain::MapKind::Campaign && !model.built;
        let Some(item) = model.project.as_mut() else {
            return;
        };
        let p = item.edit();
        let position_only = kit_campaign
            && self.component == "ECTransform"
            && self.attr == "position"
            && leaf_prop(p, self.id);
        if self.component.is_empty() {
            if let Some(e) = p.entity(self.id).map(|e| e.elem) {
                p.doc.set(e, &self.attr, value);
                p.reindex();
            }
        } else {
            p.set_attr(self.id, &self.component, &self.attr, value);
        }

        if position_only {
            self.revision.moved(self.id);
        } else {
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Command<Option<MapModel>> for TerryAttrCommand {
    fn name(&self) -> String {
        format!("{} {}", if self.component.is_empty() { "entity" } else { &self.component }, self.attr)
    }

    fn apply(&mut self, model: &mut Option<MapModel>) {
        self.put(model, &self.after.clone());
    }

    fn revert(&mut self, model: &mut Option<MapModel>) {
        self.put(model, &self.before.clone());
    }

    fn merge_key(&self) -> u64 {
        self.key
    }

    fn merge_from(&mut self, next: &dyn Command<Option<MapModel>>) -> bool {
        let Some(n) = next.as_any().downcast_ref::<Self>() else { return false };
        if n.id != self.id || n.component != self.component || n.attr != self.attr {
            return false;
        }

        self.after.clone_from(&n.after);
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One attribute set on several entities (the Inspector on a multi-selection): one step on the history; the steps of one gesture merge.
pub struct TerryAttrsCommand {
    pub edits: Vec<TerryAttrCommand>,
    pub key: u64,
}

impl Command<Option<MapModel>> for TerryAttrsCommand {
    fn name(&self) -> String {
        format!("{} on {} entities", self.edits.first().map_or_else(String::new, |e| e.name()), self.edits.len())
    }

    fn apply(&mut self, model: &mut Option<MapModel>) {
        for e in &mut self.edits {
            e.apply(model);
        }
    }

    fn revert(&mut self, model: &mut Option<MapModel>) {
        for e in self.edits.iter_mut().rev() {
            e.revert(model);
        }
    }

    fn merge_key(&self) -> u64 {
        self.key
    }

    fn merge_from(&mut self, next: &dyn Command<Option<MapModel>>) -> bool {
        let Some(n) = next.as_any().downcast_ref::<Self>() else { return false };
        if n.edits.len() != self.edits.len() || n.edits.iter().zip(&self.edits).any(|(a, b)| a.id != b.id || a.component != b.component || a.attr != b.attr) {
            return false;
        }

        for (a, b) in self.edits.iter_mut().zip(&n.edits) {
            a.after.clone_from(&b.after);
        }

        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Entities added or removed: the splices that did it (made with `TerryProject::insert_entities` / `remove_entities`, already in the
/// document when pushed: applying them again is skipped the first time).
pub struct TerrySpliceCommand {
    pub name: String,
    pub splices: Vec<XmlSplice>,
    pub done: bool,
    pub revision: TerryRevision,
}

impl Command<Option<MapModel>> for TerrySpliceCommand {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn apply(&mut self, model: &mut Option<MapModel>) {
        if self.done {
            self.done = false;
            return;
        }

        if let Some(item) = model.as_mut().and_then(|m| m.project.as_mut()) {
            item.edit().redo(&self.splices);
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn revert(&mut self, model: &mut Option<MapModel>) {
        if let Some(item) = model.as_mut().and_then(|m| m.project.as_mut()) {
            item.edit().undo(&self.splices);
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
