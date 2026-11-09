//! Campaign-specific continuous raise/lower authority composed with the existing LF page command and mirror.
use std::{any::Any, sync::Arc};
use wce_editor_core::command::Command;
use wce_foundation::game::GameProfile;
use wce_terrain::{LfRole, MapModel, lf_edit::{LfBrush, LfStroke}};
use wce_terrain_io::built::campaign_authority::{self as authority, Authority};
use wce_tile_model::Blob;
use crate::lf::{LfMirror, LfStrokeCommand};

pub struct CampaignStroke { authority: Authority, stroke: LfStroke, before: Option<Blob>, after: Option<Blob>, mirror: Option<Arc<LfMirror>>, version: u64 }
impl CampaignStroke {
    pub fn begin(m: &MapModel, game: &GameProfile, mirror: Option<Arc<LfMirror>>, version: u64) -> Result<Self, String> {
        // Built load already captures sidecars. Also freeze here for callers constructing models directly.
        let before = authority::state_blob(m).map(|b| b.bytes().map(|b| Blob::Bytes(Arc::from(b.to_vec())))).transpose().map_err(|e| e.to_string())?;
        Ok(Self { authority: Authority::begin(m, game).map_err(|e| e.to_string())?, stroke: LfStroke::default(), after: before.clone(), before, mirror, version })
    }
    fn check(&self, m: &MapModel, version: u64) -> Result<(), String> {
        if version != self.version || authority::state_blob(m) != self.after {
            return Err("Campaign stroke is stale after history or authority changed; release it and start a new stroke".into());
        }
        // Check preview too, including arbitrary DDS edits. Never restore a stale stroke over newer state.
        authority::prepared(m).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn changed(&self) -> bool { self.before != self.after }
    pub fn dab(&mut self, m: &mut MapModel, brush: &LfBrush, amount: f64, version: u64) -> Result<Option<[u32; 4]>, String> {
        self.check(m, version)?;
        let prepared = self.authority.raise(brush, amount).map_err(|e| e.to_string())?;
        let bytes = prepared.height_files.folder["lf_height_map.dds"].bytes().map_err(|e| e.to_string())?;
        let field = m.lf_mut(LfRole::Land).ok_or("no campaign LF")?.edit();
        let before = field.to_le_rows();
        let w = field.width();
        let mut rect: Option<[u32; 4]> = None;
        for (i, (a, b)) in before.as_chunks::<2>().0.iter().zip(bytes[128..].as_chunks::<2>().0).enumerate() {
            if a != b {
                let (x, y) = (i as u32 % w, i as u32 / w);
                self.stroke.set(field, x, y, u16::from_le_bytes(*b));
                rect = Some(rect.map_or([x, y, x + 1, y + 1], |r| [r[0].min(x), r[1].min(y), r[2].max(x + 1), r[3].max(y + 1)]));
            }
        }
        if let (Some(mirror), Some(rect)) = (&self.mirror, rect) { mirror.copy(field, rect); }
        let state = if self.authority.is_empty() && self.before.is_none() { None } else { Some(self.authority.snapshot()) };
        self.after = state.clone();
        authority::put_state(m, state);
        Ok(rect)
    }
    // The document retains the stroke until this succeeds. All fallible validation precedes
    // taking the page record; after that only an infallible command handoff remains.
    pub(crate) fn resolve(&mut self, m: &MapModel, version: u64) -> Result<Option<CampaignStrokeCommand>, String> {
        self.check(m, version)?;
        let field = m.lf(LfRole::Land).ok_or("no campaign LF")?.get();
        let record = std::mem::take(&mut self.stroke).finish(field);
        if self.after == self.before && record.is_none() { return Ok(None); }
        Ok(Some(CampaignStrokeCommand { preview: record.map(|record| LfStrokeCommand { role: LfRole::Land, record: Arc::new(record), mirror: self.mirror.clone() }), before: self.before.clone(), after: self.after.clone() }))
    }
    pub fn finish(mut self, m: &MapModel, version: u64) -> Result<Option<CampaignStrokeCommand>, String> {
        self.resolve(m, version)
    }
    pub fn cancel(self, m: &mut MapModel, version: u64) -> Result<(), String> {
        self.check(m, version)?;
        if let Some(record) = self.stroke.finish(m.lf(LfRole::Land).ok_or("no campaign LF")?.get()) {
            let field = m.lf_mut(LfRole::Land).ok_or("no campaign LF")?.edit();
            record.revert(field);
            if let Some(mirror) = self.mirror { mirror.copy(field, record.rect); }
        }
        authority::put_state(m, self.before);
        Ok(())
    }
}

pub struct CampaignStrokeCommand { preview: Option<LfStrokeCommand>, before: Option<Blob>, after: Option<Blob> }
impl CampaignStrokeCommand {
    pub fn bytes(&self) -> usize {
        self.preview.as_ref().map_or(0, |p| p.record.bytes()) + [&self.before, &self.after].into_iter().map(|b| match b { Some(Blob::Bytes(b)) => b.len(), _ => 0 }).sum::<usize>()
    }
}
impl Command<Option<MapModel>> for CampaignStrokeCommand {
    fn name(&self) -> String { "Campaign raise/lower".into() }
    fn apply(&mut self, m: &mut Option<MapModel>) {
        if let Some(p) = &mut self.preview { p.apply(m); }
        if let Some(m) = m { authority::put_state(m, self.after.clone()); }
    }
    fn revert(&mut self, m: &mut Option<MapModel>) {
        if let Some(p) = &mut self.preview { p.revert(m); }
        if let Some(m) = m { authority::put_state(m, self.before.clone()); }
    }
    fn as_any(&self) -> &dyn Any { self }
}

#[cfg(test)]
mod tests {
    use crate::map_fixture::FixtureWrite;
    use super::*;
    mod hardening { use super::*; include!("campaign_lf_hardening_tests.rs"); }
    use wce_editor_core::command::{CommandStack, History};
    use wce_formats::chmf::{ChmfFile, ChmfRegion, encode_region};
    use wce_terrain::{MapKind, paged::PagedU16};
    use wce_terrain_io::{MapFiles, built};
    use wce_tile::battle_map::tile_list::TileList;

    fn fixture() -> MapFiles {
        let mut c = ChmfFile { version: 5, width: 48, height: 48, cells_x: 1, cells_y: 8, min: 0.0, max: 65535.0, spacing: 1, regions: vec![] };
        for y in (0..48).step_by(16) { for x in (0..48).step_by(16) {
            let (_, offsets, data) = encode_region(16, 16, &[10000;256]);
            c.regions.push(ChmfRegion { rect:[x,y,x+16,y+16], tag:[10000,65535], offsets, data });
        } }
        MapFiles::from_bytes([
            ("lf_height_map.data".into(), c.write()),
            ("lf_height_map.dds".into(), wce_terrain_io::attila::heights::lf_height_dds(&PagedU16::filled(192,192,10000))),
            ("lf_height_map_lq.dds".into(), wce_terrain_io::attila::heights::lf_height_dds(&PagedU16::filled(96,96,10000))),
            ("terrain.tile_list".into(), TileList { version:9,is_campaign:true,battle_size:[48,48],..Default::default() }.write()),
            ("normals/normal_0.dds".into(), b"opaque unsupported normals stay original".to_vec()),
        ])
    }
    fn brush(x:f64,y:f64,r:f64)->LfBrush { LfBrush { brush:wce_tile::height_field::Brush {cx:x,cy:y,radius:r,strength:1.0,falloff:0.0,..Default::default()},aspect:1.0,rotation:0.0 } }
    fn model(f:&MapFiles)->MapModel { built::read_as("arbitrary",MapKind::Campaign,f).unwrap() }
    fn paint(stack:&mut CommandStack<Option<MapModel>>, b:&LfBrush, amount:f64) {
        let version=stack.version();
        let mut stroke=CampaignStroke::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2,None,version).unwrap();
        stroke.dab(stack.model_mut().as_mut().unwrap(),b,amount,version).unwrap();
        if let Some(c)=stroke.finish(stack.model().as_ref().unwrap(),version).unwrap(){stack.push(Box::new(c));stack.break_merge();}
    }
    fn bits(m:&MapModel)->Vec<u8>{authority::state_blob(m).map(|b|b.bytes().unwrap().to_vec()).unwrap_or_default()}

    #[test]
    fn raise_lower_mirror_and_analytic_native_weights() {
        let f=fixture();let mut m=model(&f);
        let mirror=Arc::new(LfMirror::of(m.lf(LfRole::Land).unwrap().get(),wce_scene::render_scene::LfSamples::new(vec![10000;192*192]),192,192).unwrap());
        let mut s=CampaignStroke::begin(&m,&GameProfile::ROME_2,Some(mirror.clone()),0).unwrap();
        s.dab(&mut m,&brush(64.,40.,3.),64.,0).unwrap();
        assert_eq!(m.lf(LfRole::Land).unwrap().get().at(64,40),10056); //64*(15/16)^2=56.25.
        assert_eq!(mirror.samples.read()[(191-40)*192+64],10056);
        assert_eq!(mirror.samples.read()[40*192+64],10000);
        let p=authority::prepared(&m).unwrap();assert_eq!(p.stats.chmf_samples,1);assert_eq!(p.stats.chmf_regions,1);
        let low=p.height_files.folder["lf_height_map_lq.dds"].bytes().unwrap();let i=128+2*(20*96+32);
        assert_eq!(u16::from_le_bytes([low[i],low[i+1]]),10042);
        let after=m.lf(LfRole::Land).unwrap().get().to_le_rows();let mut cmd=s.finish(&m,0).unwrap().unwrap();let mut mm=Some(m);
        cmd.revert(&mut mm);assert_eq!(mm.as_ref().unwrap().lf(LfRole::Land).unwrap().get().to_le_rows(),f.folder["lf_height_map.dds"].bytes().unwrap()[128..]);
        cmd.apply(&mut mm);assert_eq!(mm.as_ref().unwrap().lf(LfRole::Land).unwrap().get().to_le_rows(),after);
        let mut stack=CommandStack::new(mm);paint(&mut stack,&brush(64.,40.,3.),-64.);
        assert!(built::changed(&f,&built::write(stack.model().as_ref().unwrap(),None).unwrap()).is_empty());
    }
    #[test]
    fn fractions_survive_separate_strokes_and_history_branches() {
        let f=fixture();let mut stack=CommandStack::new(Some(model(&f)));stack.mark_saved();let mut sum=0f64;
        for _ in 0..20{paint(&mut stack,&brush(64.,64.,3.),0.1);sum+=0.1;}
        let a=Authority::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2).unwrap();
        assert_eq!(a.node_delta(16,16).to_bits(),sum.to_bits());
        assert_eq!(stack.model().as_ref().unwrap().lf(LfRole::Land).unwrap().get().at(64,64),10002);
        assert_eq!(stack.position(),20);let after=bits(stack.model().as_ref().unwrap());stack.undo();stack.redo();assert_eq!(bits(stack.model().as_ref().unwrap()),after);
        stack.undo();paint(&mut stack,&brush(68.,64.,3.),3.25);assert!(!stack.can_redo());
        let a=Authority::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2).unwrap();assert_eq!(a.node_delta(17,16),3.25);
        assert!(stack.modified());for _ in 0..20{stack.undo();}assert!(!stack.modified());
        assert!(built::changed(&f,&built::write(stack.model().as_ref().unwrap(),None).unwrap()).is_empty());
    }
    #[test]
    fn overlap_adjacent_regions_and_rotation_retain_explicit_nodes() {
        let f=fixture();let mut stack=CommandStack::new(Some(model(&f)));
        let mut b=brush(64.,64.,8.);b.aspect=2.;b.rotation=std::f64::consts::FRAC_PI_2;
        paint(&mut stack,&b,4.);paint(&mut stack,&brush(68.,64.,8.),-1.);
        let a=Authority::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2).unwrap();
        assert_eq!(a.node_delta(16,19),4.);assert_eq!(a.node_delta(19,16),0.);
        assert_eq!(a.node_delta(16,16),3.);assert_eq!(a.node_delta(17,16),3.);
        let p=authority::prepared(stack.model().as_ref().unwrap()).unwrap();assert!(p.stats.chmf_regions>=4);assert!(p.stats.full_lq_delta_error<=1.);
    }
    #[test]
    fn project_roundtrip_preserves_baselines_fractional_authority_and_tombstones() {
        let f=fixture();let mut stack=CommandStack::new(Some(model(&f)));
        paint(&mut stack,&brush(64.,64.,3.),0.1);let saved=authority::project_files(stack.model().as_ref().unwrap(),None).unwrap();
        for(n,b)in &f.folder{assert_eq!(b.bytes().unwrap(),saved.folder[n].bytes().unwrap());}
        let reopened=model(&saved);assert_eq!(bits(&reopened),bits(stack.model().as_ref().unwrap()));
        let mut new=CommandStack::new(Some(reopened));for _ in 0..19{paint(&mut new,&brush(64.,64.,3.),0.1);}
        assert_eq!(new.model().as_ref().unwrap().lf(LfRole::Land).unwrap().get().at(64,64),10002);
        for _ in 0..3{let saved=authority::project_files(new.model().as_ref().unwrap(),None).unwrap();let again=model(&saved);assert_eq!(bits(&again),bits(new.model().as_ref().unwrap()));}
        stack.undo();let empty=authority::project_files(stack.model().as_ref().unwrap(),None).unwrap();
        assert_eq!(empty.folder[authority::PROJECT_STATE].bytes().unwrap().as_ref(),b"WCHE\0\0\0\0");
        let restored=model(&empty);let native=built::write(&restored,None).unwrap();assert!(!native.folder.contains_key(authority::PROJECT_STATE));assert!(built::changed(&f,&native).is_empty());
        let mut stale=saved.clone();let mut low=stale.folder["lf_height_map_lq.dds"].bytes().unwrap().to_vec();low[128]^=1;
        stale.folder.insert("lf_height_map_lq.dds".into(),Blob::Bytes(Arc::from(low)));assert!(built::read_as("x",MapKind::Campaign,&stale).is_err());
    }
    #[test]
    fn coarse_only_and_subquantized_changes_keep_export_gate_and_history() {
        let f=fixture();let mut stack=CommandStack::new(Some(model(&f)));
        paint(&mut stack,&brush(64.,64.,3.),0.5001);
        let p=authority::prepared(stack.model().as_ref().unwrap()).unwrap();assert_eq!(p.stats.chmf_samples,1);assert_eq!(p.stats.full_samples,0);
        assert!(built::write(stack.model().as_ref().unwrap(),None).is_err());
        stack.undo();paint(&mut stack,&brush(64.,64.,3.),0.1);assert!(stack.modified());
        assert!(built::changed(&f,&built::write(stack.model().as_ref().unwrap(),None).unwrap()).is_empty());
        let size=stack.size();paint(&mut stack,&brush(64.,64.,3.),0.);assert_eq!(stack.size(),size);
    }
    #[test]
    fn unsupported_game_profile_bounds_inputs_and_arbitrary_edits_refuse() {
        let f=fixture();let mut m=model(&f);
        assert!(CampaignStroke::begin(&m,&GameProfile::ATTILA,None,0).is_err());
        for(b,amount)in[(brush(1.,1.,8.),2.),(brush(64.,64.,0.),2.),(brush(64.,64.,8.),f64::NAN),(brush(64.,64.,3.),65535.)]{
            let mut s=CampaignStroke::begin(&m,&GameProfile::ROME_2,None,0).unwrap();assert!(s.dab(&mut m,&b,amount,0).is_err());
            assert!(bits(&m).is_empty());assert!(s.finish(&m,0).unwrap().is_none());
        }
        m.lf_mut(LfRole::Land).unwrap().edit().set(4,4,11000);assert!(CampaignStroke::begin(&m,&GameProfile::ROME_2,None,0).is_err());
        assert!(authority::project_files(&m,None).is_err());
    }
    #[test]
    fn saving_over_a_loaded_sidecar_does_not_change_undo_snapshots() {
        let dir=std::env::temp_dir().join(format!("wce-campaign-authority-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let f=fixture();let mut first=CommandStack::new(Some(model(&f)));paint(&mut first,&brush(64.,64.,3.),1.25);
        let saved=authority::project_files(first.model().as_ref().unwrap(),None).unwrap();saved.write_to(&dir).unwrap();
        let before=std::fs::read(dir.join(authority::PROJECT_STATE)).unwrap();
        let on_disk=MapFiles::from_dir(&dir).unwrap();let mut stack=CommandStack::new(Some(model(&on_disk)));
        let unchanged=CampaignStroke::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2,None,0).unwrap();
        assert!(unchanged.finish(stack.model().as_ref().unwrap(),0).unwrap().is_none());
        let mut refused=CampaignStroke::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2,None,0).unwrap();
        assert!(refused.dab(stack.model_mut().as_mut().unwrap(),&brush(1.,1.,8.),2.,0).is_err());
        assert!(refused.finish(stack.model().as_ref().unwrap(),0).unwrap().is_none());
        paint(&mut stack,&brush(64.,64.,3.),2.5);
        authority::project_files(stack.model().as_ref().unwrap(),None).unwrap().write_to(&dir).unwrap();
        assert_ne!(std::fs::read(dir.join(authority::PROJECT_STATE)).unwrap(),before);
        stack.undo();assert_eq!(bits(stack.model().as_ref().unwrap()),before);
        authority::prepared(stack.model().as_ref().unwrap()).unwrap();
        authority::project_files(stack.model().as_ref().unwrap(),None).unwrap().write_to(&dir).unwrap();
        stack.redo();authority::prepared(stack.model().as_ref().unwrap()).unwrap();
        let target=dir.canonicalize().unwrap();let temp=std::env::temp_dir().canonicalize().unwrap();
        assert!(target.starts_with(&temp)&&target.file_name().unwrap().to_string_lossy().starts_with("wce-campaign-authority-"));
        std::fs::remove_dir_all(target).unwrap();
    }
}
