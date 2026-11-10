//! The native campaign prop adapter uses the map's existing selection, Inspector, gizmo and history.
use std::{any::Any,sync::atomic::Ordering};
use wce_editor_core::command::Command;
use wce_terrain::MapModel;
use crate::terry::TerryRevision;

pub struct TranslateProps {
    pub edits: Vec<(u32,[f32;3],[f32;3])>,
    pub key: u64,
    pub revision: TerryRevision,
}
impl TranslateProps {
    fn put(&self, model:&mut Option<MapModel>, undo:bool) {
        if let Some(props)=model.as_mut().and_then(|m|m.native_props.as_mut()) {
            for &(id,before,after) in &self.edits {
                props.edit().translate(id,if undo {before}else{after}).expect("validated prop command");
            }
            self.revision.fetch_add(1,Ordering::Relaxed);
        }
    }
}
impl Command<Option<MapModel>> for TranslateProps {
    fn name(&self)->String {"Translate campaign prop".into()}
    fn apply(&mut self,m:&mut Option<MapModel>){self.put(m,false);}
    fn revert(&mut self,m:&mut Option<MapModel>){self.put(m,true);}
    fn merge_key(&self)->u64{self.key}
    fn merge_from(&mut self,next:&dyn Command<Option<MapModel>>)->bool {
        let Some(n)=next.as_any().downcast_ref::<Self>() else{return false};
        if self.edits.len()!=n.edits.len() || self.edits.iter().zip(&n.edits).any(|(a,b)|a.0!=b.0){return false;}
        for (a,b) in self.edits.iter_mut().zip(&n.edits){a.2=b.2;}true
    }
    fn dirty_items(&self)->Vec<u64>{self.edits.iter().map(|e|wce_terrain::campaign_props::PROP_KEY|u64::from(e.0)).collect()}
    fn as_any(&self)->&dyn Any{self}
}
