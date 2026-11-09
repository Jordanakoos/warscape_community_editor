use crate::{MapDocument, document::MapSource};
use wce_editor_core::document::{Document, SaveTarget};
use wce_tile::battle_map::{context::PlacedMap, tile_db::TileDatabase};
use std::{collections::BTreeMap, path::{Path, PathBuf}};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
        let sequence=NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
        let p=std::env::temp_dir().join(format!("wce-campaign-hardening-{}-{}-{sequence}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&p).unwrap();Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let target=self.0.canonicalize().unwrap();let root=std::env::temp_dir().canonicalize().unwrap();
        assert!(target.starts_with(root)&&target.file_name().unwrap().to_string_lossy().starts_with("wce-campaign-hardening-"));
        for p in std::fs::read_dir(&target).unwrap().flatten().map(|p|p.path()) {
            if p.is_file() { let mut perm=std::fs::metadata(&p).unwrap().permissions();if perm.readonly(){perm.set_readonly(false);std::fs::set_permissions(p,perm).unwrap();} }
        }
        std::fs::remove_dir_all(target).unwrap();
    }
}
fn doc(m:MapModel)->MapDocument {
    let db=Arc::new(TileDatabase::default());let placed=PlacedMap::from_tile_list("test",m.tile_list.as_ref().unwrap().get(),&db,None);
    MapDocument::new("test".into(),MapSource::Kit(PathBuf::new()),Some(m),Arc::new(placed),None,db)
}
fn begin(d:&mut MapDocument,x:f64,amount:f64)->u64 {
    let ticket=d.begin_campaign_stroke(&GameProfile::ROME_2,None).unwrap();d.campaign_dab(ticket,&brush(x,x,3.),amount).unwrap();ticket
}
fn complete(d:&mut MapDocument,x:f64,amount:f64) { let t=begin(d,x,amount);d.finish_campaign_stroke(t).unwrap().unwrap(); }
fn saved(base:u16,amount:f64)->MapFiles {
    let mut d=doc(model(&fixture_with(base)));complete(&mut d,64.,amount);authority::project_files(d.model().unwrap(),None).unwrap()
}
fn fixture_with(base:u16)->MapFiles {
    let mut f=fixture();if base==10000{return f;}
    let mut c=ChmfFile::parse(&f.folder["lf_height_map.data"].bytes().unwrap()).unwrap();
    for r in &mut c.regions { let(_,offsets,data)=encode_region(16,16,&[base;256]);r.tag=[base,65535];r.offsets=offsets;r.data=data; }
    f.folder.insert("lf_height_map.data".into(),Blob::Bytes(Arc::from(c.write())));
    for(n,w)in[("lf_height_map.dds",192),("lf_height_map_lq.dds",96)] {f.folder.insert(n.into(),Blob::Bytes(Arc::from(wce_terrain_io::attila::heights::lf_height_dds(&PagedU16::filled(w,w,base)))));}
    f
}
fn assert_valid(d:&MapDocument) { authority::prepared(d.model().unwrap()).unwrap(); }
fn disk(p:&Path)->BTreeMap<String,Vec<u8>> { MapFiles::from_dir(p).unwrap().folder.into_iter().map(|(n,b)|(n,b.bytes().unwrap().to_vec())).collect() }
fn raw(d:&MapDocument)->(Vec<u8>,Vec<u8>) { (bits(d.model().unwrap()),d.model().unwrap().lf(LfRole::Land).unwrap().get().to_le_rows()) }

#[test]fn partial_publication_keeps_lf_baseline_and_ordinary_retry_regenerates_normals(){
    for via_copy in [false,true] {
    let dir=Scratch::new();let mut f=fixture();f.folder.remove("lf_height_map_lq.dds");
    let mut t=TileList::parse(&f.folder["terrain.tile_list"].bytes().unwrap()).unwrap();t.is_campaign=false;
    f.folder.insert("terrain.tile_list".into(),Blob::Bytes(Arc::from(t.write())));
    let pixels=[0u8,180,255,130].repeat(256*256);
    f.folder.insert("normals/normal_0.dds".into(),Blob::Bytes(Arc::from(wce_tile_io_for_test_dds(&pixels))));
    f.write_to(&dir.0).unwrap();
    let source=MapFiles::from_dir(&dir.0).unwrap();let m=built::read_as("battle",MapKind::Battle,&source).unwrap();let mut d=doc(m);
    d.history_mut().mark_saved();let mut stroke=wce_terrain::lf_edit::LfStroke::default();
    stroke.set(d.lf_field_mut(LfRole::Land).unwrap(),64,64,10200);
    let record=stroke.finish(d.model().unwrap().lf(LfRole::Land).unwrap().get()).unwrap();
    d.push_stroke(crate::lf::LfStrokeCommand{role:LfRole::Land,record:Arc::new(record),mirror:None});
    let expected=built::write(d.model().unwrap(),None).unwrap();let normal="normals/normal_0.dds";
    assert_ne!(expected.folder[normal].bytes().unwrap(),f.folder[normal].bytes().unwrap());
    d.publication_failure_after=Some(1);let error=d.save(&SaveTarget{path:dir.0.clone()}).unwrap_err();
    assert!(error.contains("1 file(s) published"));assert!(Document::modified(&d));
    for n in ["lf_height_map.dds","lf_height_map.data"]{assert_eq!(std::fs::read(dir.0.join(n)).unwrap(),f.folder[n].bytes().unwrap().as_ref());}
    if via_copy{let copy=Scratch::new();d.save(&SaveTarget{path:copy.0.clone()}).unwrap();for n in [normal,"lf_height_map.dds","lf_height_map.data"]{assert!(std::fs::read(copy.0.join(n)).unwrap().as_slice()==expected.folder[n].bytes().unwrap().as_ref(),"independent copy: {n}");}}
    d.save(&SaveTarget{path:dir.0.clone()}).unwrap();assert!(!Document::modified(&d));
    for n in [normal,"lf_height_map.dds","lf_height_map.data"]{assert!(std::fs::read(dir.0.join(n)).unwrap().as_slice()==expected.folder[n].bytes().unwrap().as_ref(),"{n} differs from a clean one-pass Save");}
    }
}
fn wce_tile_io_for_test_dds(pixels:&[u8])->Vec<u8>{
    let mut b=vec![0u8;128];b[..4].copy_from_slice(b"DDS ");
    for (at,value) in [(4,124u32),(8,0x100f),(12,256),(16,256),(20,1024),(76,32),(80,0x41),(88,32),(92,0x00ff0000),(96,0x0000ff00),(100,0x000000ff),(104,0xff000000),(108,0x1000)]{b[at..at+4].copy_from_slice(&value.to_le_bytes());}
    b.extend_from_slice(pixels);b
}

#[test]fn staging_failure_keeps_campaign_stroke_history_and_native_files(){
    let dir=Scratch::new();let mut d=doc(model(&fixture()));d.history_mut().mark_saved();let ticket=begin(&mut d,64.,0.1);
    let blocked=dir.0.join("lf_height_map.dds.wce_tmp");std::fs::create_dir(&blocked).unwrap();
    let before=(raw(&d),d.history().position(),d.history().size(),Document::modified(&d));
    assert!(d.save(&SaveTarget{path:dir.0.clone()}).is_err());
    assert_eq!((raw(&d),d.history().position(),d.history().size(),Document::modified(&d)),before);assert!(d.campaign_stroke_active(ticket));
    assert!(disk(&dir.0).is_empty());assert!(blocked.is_dir());
    std::fs::remove_dir(blocked).unwrap();d.save(&SaveTarget{path:dir.0.clone()}).unwrap();assert!(!Document::modified(&d));
}

#[test]
fn placement_handoff_propagates_active_stroke_refusal_and_recovers() {
    let dir=Scratch::new();fixture().write_to(&dir.0).unwrap();
    let mut d=doc(model(&MapFiles::from_dir(&dir.0).unwrap()));d.history_mut().mark_saved();
    let ticket=begin(&mut d,64.,0.1);
    let original=d.model().unwrap().tile_list.as_ref().unwrap().get().clone();
    let mut edited=original.clone();edited.strings[0]="placement edit".into();
    let make=|watch| crate::cells::TileListCommand {name:"placement".into(),before:original.clone(),after:edited.clone(),watch};
    let path=dir.0.join("lf_height_map.data");let removed=std::fs::read(&path).unwrap();std::fs::remove_file(&path).unwrap();
    let before=(raw(&d),d.history().position(),d.history().size(),d.history().version(),Document::modified(&d));
    assert!(d.push_tile_list(make(d.cells.clone())).is_err());
    assert_eq!((raw(&d),d.history().position(),d.history().size(),d.history().version(),Document::modified(&d)),before);
    assert_eq!(d.model().unwrap().tile_list.as_ref().unwrap().get().write(),original.write());
    assert!(d.campaign_stroke_active(ticket));
    std::fs::write(path,removed).unwrap();
    d.push_tile_list(make(d.cells.clone())).unwrap();
    assert!(!d.campaign_stroke_active(ticket));assert_eq!(d.history().position(),2);
    assert!(d.history_mut().undo());
    assert_eq!(d.model().unwrap().tile_list.as_ref().unwrap().get().write(),original.write());
    assert!(d.history_mut().undo());assert!(!Document::modified(&d));assert_valid(&d);
}

#[test]
fn history_facade_resolves_active_undo_redo_and_fractional_branching() {
    for amount in [64.,0.1] {
        let mut d=doc(model(&fixture()));d.history_mut().mark_saved();complete(&mut d,64.,amount);
        d.history_mut().mark_saved();let a=raw(&d);let ticket=begin(&mut d,128.,amount);
        assert!(Document::modified(&d));assert_valid(&d);
        // Merely borrowing/rendering History cannot finish a drag.
        assert_eq!(d.history_mut().position(),1);assert!(d.campaign_stroke_active(ticket));
        assert!(d.history_mut().undo());assert!(!Document::modified(&d));assert_eq!(raw(&d),a);assert_valid(&d);
        assert!(d.campaign_dab(ticket,&brush(128.,128.,3.),amount).is_err());
        assert!(d.finish_campaign_stroke(ticket).is_err());assert!(d.cancel_campaign_stroke(ticket).is_err());assert_eq!(raw(&d),a);
        assert!(d.history_mut().redo());assert_valid(&d);assert!(Document::modified(&d));
        assert!(d.history_mut().undo());let branch=begin(&mut d,96.,amount);
        // Redo first completes the new edit, which explicitly branches and invalidates the old redo tail.
        assert!(!d.history_mut().redo());assert!(!d.campaign_stroke_active(branch));assert!(!d.history().can_redo());assert_valid(&d);
        d.history_mut().jump_to(1);assert_eq!(raw(&d),a);assert!(!Document::modified(&d));assert_valid(&d);
        d.history_mut().jump_to(0);assert_valid(&d);
    }
}

#[test]
fn stale_api_rejects_dab_finish_cancel_and_history_aba_without_writes() {
    for action in ["dab","finish","cancel","aba"] {
        let mut stack=CommandStack::new(Some(model(&fixture())));paint(&mut stack,&brush(64.,64.,3.),0.1);
        let version=stack.version();let mut s=CampaignStroke::begin(stack.model().as_ref().unwrap(),&GameProfile::ROME_2,None,version).unwrap();
        if action!="aba" {s.dab(stack.model_mut().as_mut().unwrap(),&brush(128.,128.,3.),0.1,version).unwrap();}
        stack.undo();if action=="aba"{stack.redo();}
        let before=(bits(stack.model().as_ref().unwrap()),stack.model().as_ref().unwrap().lf(LfRole::Land).unwrap().get().to_le_rows());
        let now=stack.version();
        match action {
            "dab"=>{assert!(s.dab(stack.model_mut().as_mut().unwrap(),&brush(128.,128.,3.),0.1,now).is_err());
                // Even a caller reusing the captured history version cannot bypass exact authority validation.
                assert!(s.dab(stack.model_mut().as_mut().unwrap(),&brush(128.,128.,3.),0.1,version).is_err());}
            "cancel"=>assert!(s.cancel(stack.model_mut().as_mut().unwrap(),now).is_err()),
            _=>assert!(s.finish(stack.model().as_ref().unwrap(),now).is_err()),
        }
        assert_eq!(before,(bits(stack.model().as_ref().unwrap()),stack.model().as_ref().unwrap().lf(LfRole::Land).unwrap().get().to_le_rows()));
        authority::prepared(stack.model().as_ref().unwrap()).unwrap();
    }
}

#[test]
fn save_active_stroke_then_reopen_history_and_cancel_newer_ticket() {
    let dir=Scratch::new();let mut d=doc(model(&fixture()));d.history_mut().mark_saved();let t=begin(&mut d,64.,0.1);
    d.save(&SaveTarget{path:dir.0.clone()}).unwrap();assert!(!Document::modified(&d));assert_eq!(d.history().position(),1);assert!(!d.campaign_stroke_active(t));
    let original=raw(&d);let mut reopened=doc(model(&MapFiles::from_dir(&dir.0).unwrap()));reopened.history_mut().mark_saved();
    let t2=begin(&mut reopened,128.,0.1);reopened.history_mut().undo();assert_eq!(raw(&reopened),original);assert!(!Document::modified(&reopened));
    reopened.history_mut().redo();reopened.history_mut().undo();assert_eq!(raw(&reopened),original);assert_valid(&reopened);
    let fresh=begin(&mut reopened,96.,0.1);let newer=raw(&reopened);
    assert!(reopened.cancel_campaign_stroke(t2).is_err());assert_eq!(raw(&reopened),newer);
    reopened.cancel_campaign_stroke(fresh).unwrap();assert_eq!(raw(&reopened),original);assert!(!Document::modified(&reopened));assert_valid(&reopened);
}

#[test]
fn loaded_snapshot_authority_stays_immutable_for_file_memory_and_mixed_sources() {
    for mode in 0..4 {
        let dir=Scratch::new();saved(10000,0.5001).write_to(&dir.0).unwrap();
        let mut files=MapFiles::from_dir(&dir.0).unwrap();
        for(n,b)in &mut files.folder {if mode==1 || (mode==2&&n==authority::PROJECT_STATE) || (mode==3&&n!=authority::PROJECT_STATE) {*b=Blob::Bytes(Arc::from(b.bytes().unwrap().to_vec()));}}
        let m=model(&files);assert!(matches!(authority::state_blob(&m),Some(Blob::Bytes(_))));let captured=m.clone();
        let mut d=doc(m);complete(&mut d,64.,-0.5001);d.save(&SaveTarget{path:dir.0.clone()}).unwrap();
        assert_eq!(std::fs::read(dir.0.join(authority::PROJECT_STATE)).unwrap(),b"WCHE\0\0\0\0");
        assert_eq!(Authority::begin(&captured,&GameProfile::ROME_2).unwrap().node_delta(16,16),0.5001);
        assert!(built::write(&captured,None).is_err());assert_eq!(authority::prepared(&captured).unwrap().stats.chmf_samples,1);
        let empty=model(&MapFiles::from_dir(&dir.0).unwrap());let no_op=empty.clone();
        std::fs::write(dir.0.join(authority::PROJECT_STATE),b"replaced after load").unwrap();
        assert!(built::write(&no_op,None).is_ok());assert!(matches!(authority::state_blob(&no_op),Some(Blob::Bytes(_))));
    }
}

#[test]
fn incompatible_generation_preflight_preserves_all_files_history_and_active_state() {
    for name in ["lf_height_map.data","lf_height_map.dds","lf_height_map_lq.dds"] {
        let dir=Scratch::new();let mut f=fixture();
        let mut b=f.folder[name].bytes().unwrap().to_vec();if name.ends_with(".data"){b=fixture_with(11000).folder[name].bytes().unwrap().to_vec();}else{b[128]^=1;}
        f.folder.insert(name.into(),Blob::Bytes(Arc::from(b)));let mut target=doc(model(&f));complete(&mut target,64.,1.25);
        authority::project_files(target.model().unwrap(),None).unwrap().write_to(&dir.0).unwrap();
        std::fs::write(dir.0.join("aaa-before-native.bin"),b"original earlier resource").unwrap();
        let before=disk(&dir.0);
        let read_only=dir.0.join("lf_height_map.data");let mut perm=std::fs::metadata(&read_only).unwrap().permissions();perm.set_readonly(true);std::fs::set_permissions(&read_only,perm).unwrap();
        let mut source=fixture();source.folder.insert("aaa-before-native.bin".into(),Blob::Bytes(Arc::from(&b"changed earlier resource"[..])));
        let mut incoming=doc(model(&source));incoming.history_mut().mark_saved();complete(&mut incoming,64.,2.5);let active=begin(&mut incoming,128.,0.1);
        let state=raw(&incoming);let history=(incoming.history().size(),incoming.history().position(),incoming.history().version());
        let err=incoming.save(&SaveTarget{path:dir.0.clone()}).unwrap_err();assert!(err.contains("independent destination folder"),"{err}");
        assert_eq!(disk(&dir.0),before);assert_eq!(raw(&incoming),state);assert_eq!((incoming.history().size(),incoming.history().position(),incoming.history().version()),history);
        assert!(Document::modified(&incoming));assert!(incoming.campaign_stroke_active(active));assert_valid(&incoming);
        let reopened=model(&MapFiles::from_dir(&dir.0).unwrap());authority::prepared(&reopened).unwrap();
    }
}

#[test]
fn compatible_generation_repeated_saves_undo_tombstone_and_new_destination() {
    let dir=Scratch::new();let new=dir.0.join("new");let mut d=doc(model(&fixture()));d.history_mut().mark_saved();complete(&mut d,64.,1.25);
    d.save(&SaveTarget{path:new.clone()}).unwrap();let first=disk(&new);d.save(&SaveTarget{path:new.clone()}).unwrap();assert_eq!(disk(&new),first);
    d.history_mut().undo();d.save(&SaveTarget{path:new.clone()}).unwrap();assert!(!Document::modified(&d));assert_eq!(std::fs::read(new.join(authority::PROJECT_STATE)).unwrap(),b"WCHE\0\0\0\0");
    assert!(built::write(&model(&MapFiles::from_dir(&new).unwrap()),None).is_ok());
    d.history_mut().redo();d.save(&SaveTarget{path:new.clone()}).unwrap();assert_eq!(disk(&new),first);
    let mut reopened=doc(model(&MapFiles::from_dir(&new).unwrap()));let other=Scratch::new();saved(10000,2.5).write_to(&other.0).unwrap();
    // Immutable loaded sidecars use the existing Bytes save path, including equal-length content replacement.
    reopened.save_overwrite(&SaveTarget{path:other.0.clone()}).unwrap();assert_eq!(bits(&model(&MapFiles::from_dir(&other.0).unwrap())),bits(reopened.model().unwrap()));
}

#[test]
fn invalid_missing_generation_metadata_and_late_conflict_fail_before_writes() {
    for mode in ["invalid","missing-native","metadata-mismatch","missing-metadata-conflict","missing-metadata-compatible"] {
        let dir=Scratch::new();saved(10000,1.25).write_to(&dir.0).unwrap();
        match mode {
            "invalid"=>std::fs::write(dir.0.join(authority::PROJECT_STATE),b"invalid generation").unwrap(),
            "missing-native"=>std::fs::remove_file(dir.0.join("lf_height_map_lq.dds")).unwrap(),
            "metadata-mismatch"=>{let mut b=std::fs::read(dir.0.join(authority::PROJECT_STATE)).unwrap();b[20]^=1;std::fs::write(dir.0.join(authority::PROJECT_STATE),b).unwrap();},
            _=>std::fs::remove_file(dir.0.join(authority::PROJECT_STATE)).unwrap(),
        }
        let before=disk(&dir.0);let mut d=doc(model(&fixture_with(if mode=="missing-metadata-conflict"{11000}else{10000})));complete(&mut d,64.,2.5);
        let state=raw(&d);let pos=d.history().position();let result=d.save_overwrite(&SaveTarget{path:dir.0.clone()});
        if mode=="missing-metadata-compatible" {result.unwrap();assert_valid(&doc(model(&MapFiles::from_dir(&dir.0).unwrap())));}else{
            assert!(result.unwrap_err().contains("independent destination folder"));assert_eq!(disk(&dir.0),before);assert_eq!(raw(&d),state);assert_eq!(d.history().position(),pos);assert!(Document::modified(&d));
        }
    }
}

// Exercise the actual document/history facade with file-backed dependencies. A failed
// resolver must retain even subquantized authority and must not consume a prior redo tail.
fn unavailable_resolution(action: &str) {
    for prior in [false, true] { for amount in [0.1, 64.] { for cancel in [false, true] {
        let dir=Scratch::new();fixture().write_to(&dir.0).unwrap();
        let files=MapFiles::from_dir(&dir.0).unwrap();assert!(matches!(files.folder["lf_height_map.data"],Blob::File(_)));
        let mut d=doc(model(&files));
        if prior {complete(&mut d,64.,64.);complete(&mut d,96.,64.);assert!(d.history_mut().undo());}
        d.history_mut().mark_saved();let before=raw(&d);
        let history=(d.history().size(),d.history().position(),d.history().version());
        let ticket=begin(&mut d,128.,amount);let edited=raw(&d);
        if amount==0.1 {assert_eq!(edited.1,before.1);assert_ne!(edited.0,before.0);}
        let path=dir.0.join("lf_height_map.data");let native=std::fs::read(&path).unwrap();std::fs::remove_file(&path).unwrap();
        for _ in 0..3 {
            match action {
                "finish"=>assert!(d.finish_campaign_stroke(ticket).is_err()),
                "cancel"=>assert!(d.cancel_campaign_stroke(ticket).is_err()),
                "undo"=>assert!(!d.history_mut().undo()),
                "redo"=>assert!(!d.history_mut().redo()),
                "jump"=>d.history_mut().jump_to(0),
                "mark_saved"=>d.history_mut().mark_saved(),
                "clear"=>d.history_mut().clear(),
                "drop_redo"=>d.history_mut().drop_redo(),
                "save"=>assert!(d.save(&SaveTarget{path:dir.0.join("output")}).is_err()),
                _=>unreachable!(),
            }
            assert_eq!(raw(&d),edited);assert!(d.campaign_stroke_active(ticket));
            assert!(Document::modified(&d));assert!(d.history().can_undo());
            assert_eq!((d.history().size(),d.history().position(),d.history().version()),history);
        }
        std::fs::write(path,native).unwrap();assert_valid(&d);
        if cancel {
            d.cancel_campaign_stroke(ticket).unwrap();assert_eq!(raw(&d),before);
            assert_eq!((d.history().size(),d.history().position(),d.history().version()),history);
            assert!(!Document::modified(&d));assert_eq!(d.history().can_redo(),prior);
        } else {
            d.finish_campaign_stroke(ticket).unwrap().unwrap();assert_eq!(raw(&d),edited);
            assert_eq!((d.history().size(),d.history().position()),(history.1+1,history.1+1));
            assert!(Document::modified(&d));assert!(d.history_mut().undo());assert_eq!(raw(&d),before);assert!(!Document::modified(&d));
            assert!(d.history_mut().redo());assert_eq!(raw(&d),edited);assert!(Document::modified(&d));
        }
        assert!(!d.campaign_stroke_active(ticket));assert!(d.finish_campaign_stroke(ticket).is_err());assert_valid(&d);
    } } }
}
#[test] fn unavailable_finish_retains_stroke_until_recovery() {unavailable_resolution("finish");}
#[test] fn unavailable_cancel_retains_stroke_until_recovery() {unavailable_resolution("cancel");}
#[test] fn unavailable_global_undo_retains_stroke_until_recovery() {unavailable_resolution("undo");}
#[test] fn unavailable_global_redo_retains_stroke_until_recovery() {unavailable_resolution("redo");}
#[test] fn unavailable_history_jump_retains_stroke_until_recovery() {unavailable_resolution("jump");}
#[test] fn unavailable_mark_saved_retains_stroke_until_recovery() {unavailable_resolution("mark_saved");}
#[test] fn unavailable_clear_retains_stroke_until_recovery() {unavailable_resolution("clear");}
#[test] fn unavailable_drop_redo_retains_stroke_until_recovery() {unavailable_resolution("drop_redo");}
#[test] fn unavailable_save_retains_stroke_until_recovery() {unavailable_resolution("save");}

fn incomplete_fixture() -> MapFiles {
    let mut f=fixture();f.folder.remove("lf_height_map.data");f.folder.remove("lf_height_map_lq.dds");f
}
fn preservation_repeated(empty_tombstone: bool) {
    let source=Scratch::new();let target=Scratch::new();let mut f=incomplete_fixture();
    if empty_tombstone {f.folder.insert(authority::PROJECT_STATE.into(),Blob::Bytes(Arc::from(&b"WCHE\0\0\0\0"[..])));}
    f.write_to(&source.0).unwrap();let expected=disk(&source.0);
    let mut d=doc(model(&MapFiles::from_dir(&source.0).unwrap()));d.history_mut().mark_saved();
    assert!(d.begin_campaign_stroke(&GameProfile::ROME_2,None).is_err());
    for reopen in 0..2 {
        for _ in 0..3 {d.save(&SaveTarget{path:target.0.clone()}).unwrap();assert_eq!(disk(&target.0),expected);assert!(!Document::modified(&d));assert_eq!(d.history().position(),0);}
        if reopen==0 {d=doc(model(&MapFiles::from_dir(&target.0).unwrap()));d.history_mut().mark_saved();}
    }
    assert_eq!(disk(&source.0),expected);
}
#[test] fn incomplete_preservation_saves_three_times_and_reopens_without_metadata() {preservation_repeated(false);}
#[test] fn incomplete_preservation_saves_three_times_and_reopens_with_old_tombstone() {preservation_repeated(true);}

#[test]
fn incomplete_destination_old_tombstone_accepts_metadata_free_preservation() {
    let target=Scratch::new();let f=incomplete_fixture();f.write_to(&target.0).unwrap();
    std::fs::write(target.0.join(authority::PROJECT_STATE),b"WCHE\0\0\0\0").unwrap();let before=disk(&target.0);
    let mut d=doc(model(&f));for _ in 0..3 {d.save_overwrite(&SaveTarget{path:target.0.clone()}).unwrap();assert_eq!(disk(&target.0),before);}
    let mut d=doc(model(&MapFiles::from_dir(&target.0).unwrap()));d.save(&SaveTarget{path:target.0.clone()}).unwrap();assert_eq!(disk(&target.0),before);
}

#[test]
fn complete_zero_and_fractional_authority_remain_repeatable_and_native_exact() {
    for amount in [0.,0.1] {
        let target=Scratch::new();let f=fixture();let mut d=doc(model(&f));d.history_mut().mark_saved();
        if amount!=0. {complete(&mut d,64.,amount);}
        for _ in 0..3 {d.save(&SaveTarget{path:target.0.clone()}).unwrap();assert!(!Document::modified(&d));}
        let first=disk(&target.0);for(n,b)in &f.folder {assert_eq!(first[n],b.bytes().unwrap().as_ref());}
        if amount==0. {assert_eq!(first[authority::PROJECT_STATE],b"WCHE\0\0\0\0");}
        let mut reopened=doc(model(&MapFiles::from_dir(&target.0).unwrap()));
        assert_eq!(Authority::begin(reopened.model().unwrap(),&GameProfile::ROME_2).unwrap().node_delta(16,16),amount);
        reopened.save(&SaveTarget{path:target.0.clone()}).unwrap();assert_eq!(disk(&target.0),first);assert_valid(&reopened);
    }
}

#[test]
fn incomplete_nonempty_authority_refuses_load_and_save_without_publication() {
    for missing in ["lf_height_map.data","lf_height_map_lq.dds"] {
        let target=Scratch::new();saved(10000,1.25).write_to(&target.0).unwrap();let disk_before=disk(&target.0);
        let mut f=saved(10000,0.1);f.folder.remove(missing);assert!(built::read_as("incomplete",MapKind::Campaign,&f).is_err());
        let source=Scratch::new();fixture().write_to(&source.0).unwrap();
        let mut d=doc(model(&MapFiles::from_dir(&source.0).unwrap()));d.history_mut().mark_saved();let ticket=begin(&mut d,64.,0.1);
        let path=source.0.join(missing);let native=std::fs::read(&path).unwrap();std::fs::remove_file(&path).unwrap();
        let before=raw(&d);let h=(d.history().size(),d.history().position(),d.history().version());
        assert!(d.save(&SaveTarget{path:target.0.clone()}).is_err());assert_eq!(disk(&target.0),disk_before);
        assert_eq!(raw(&d),before);assert_eq!((d.history().size(),d.history().position(),d.history().version()),h);assert!(Document::modified(&d));assert!(d.campaign_stroke_active(ticket));
        std::fs::write(path,native).unwrap();d.cancel_campaign_stroke(ticket).unwrap();assert!(!Document::modified(&d));assert_valid(&d);
    }
}

#[test]
fn preservation_exception_cannot_replace_an_incompatible_authored_generation() {
    for incomplete in [false,true] {
        let target=Scratch::new();saved(11000,1.25).write_to(&target.0).unwrap();let disk_before=disk(&target.0);
        let mut d=doc(model(&if incomplete{incomplete_fixture()}else{fixture()}));d.history_mut().mark_saved();
        if !incomplete {complete(&mut d,64.,64.);complete(&mut d,64.,-64.);assert!(Document::modified(&d));}
        let before=raw(&d);let h=(d.history().size(),d.history().position(),d.history().version());let dirty=Document::modified(&d);
        assert!(d.save(&SaveTarget{path:target.0.clone()}).is_err());assert_eq!(disk(&target.0),disk_before);assert_eq!(raw(&d),before);
        assert_eq!((d.history().size(),d.history().position(),d.history().version()),h);assert_eq!(Document::modified(&d),dirty);
    }
}

#[test]
fn preservation_preflight_rejects_invalid_metadata_and_partial_identity_conflicts() {
    for mode in ["invalid","authored-missing-native","different-full"] {
        let target=Scratch::new();let mut f=incomplete_fixture();
        if mode=="authored-missing-native" {f=saved(10000,0.1);f.folder.remove("lf_height_map_lq.dds");}
        f.write_to(&target.0).unwrap();
        if mode=="invalid" {std::fs::write(target.0.join(authority::PROJECT_STATE),b"invalid").unwrap();}
        if mode=="different-full" {std::fs::write(target.0.join("lf_height_map.dds"),fixture_with(11000).folder["lf_height_map.dds"].bytes().unwrap()).unwrap();}
        let before=disk(&target.0);let mut d=doc(model(&incomplete_fixture()));d.history_mut().mark_saved();let state=raw(&d);
        assert!(d.save(&SaveTarget{path:target.0.clone()}).is_err());assert_eq!(disk(&target.0),before);assert_eq!(raw(&d),state);
        assert_eq!((d.history().size(),d.history().position()),(0,0));assert!(!Document::modified(&d));
    }
}

fn activation_recovery(amount: f64) {
    use wce_editor_core::document_manager::DocumentManager;
    for prior in [false,true] { for recover in ["finish","cancel","undo","retry"] {
        let source=Scratch::new();fixture().write_to(&source.0).unwrap();
        let mut scene=wce_scene::render_scene::RenderScene::default();let mut docs=DocumentManager::default();
        let a=docs.add(&mut scene,Box::new(doc(model(&MapFiles::from_dir(&source.0).unwrap()))),"terrain").unwrap();
        let b=docs.add(&mut scene,Box::new(doc(model(&fixture_with(11000)))),"other-map").unwrap();
        docs.try_set_active(a).unwrap();let other=raw(docs.find_as::<MapDocument>(b).unwrap());
        let d=docs.find_as_mut::<MapDocument>(a).unwrap();
        if prior {complete(d,64.,64.);complete(d,96.,64.);assert!(d.history_mut().undo());}
        d.history_mut().mark_saved();let saved=raw(d);let pos=d.history().position();let size=d.history().size();
        let ticket=begin(d,128.,amount);let edited=raw(d);let version=d.history().version();
        if amount==0.1 {assert_eq!(edited.1,saved.1);assert_ne!(edited.0,saved.0);}
        let path=source.0.join("lf_height_map.data");let native=std::fs::read(&path).unwrap();std::fs::remove_file(&path).unwrap();
        for _ in 0..3 {
            assert!(docs.try_set_active(b).unwrap_err().contains("Cannot switch maps"));docs.set_active_tool("other-map");assert_eq!(docs.active_id(),a);
            let d=docs.find_as::<MapDocument>(a).unwrap();assert_eq!(raw(d),edited);assert!(d.campaign_stroke_active(ticket));assert!(Document::modified(d));
            assert_eq!((d.history().size(),d.history().position(),d.history().version()),(size,pos,version));assert_eq!(raw(docs.find_as::<MapDocument>(b).unwrap()),other);
        }
        std::fs::write(path,native).unwrap();
        if recover=="retry" {docs.try_set_active(b).unwrap();assert_eq!(docs.active_id(),b);} else {
            let d=docs.find_as_mut::<MapDocument>(a).unwrap();
            match recover {"finish"=>{d.finish_campaign_stroke(ticket).unwrap();},"cancel"=>d.cancel_campaign_stroke(ticket).unwrap(),_=>{assert!(d.history_mut().undo());}}
        }
        let d=docs.find_as::<MapDocument>(a).unwrap();assert!(!d.campaign_stroke_active(ticket));
        if matches!(recover,"cancel"|"undo") {assert_eq!(raw(d),saved);assert!(!Document::modified(d));}else{assert_eq!(raw(d),edited);assert!(Document::modified(d));}
        docs.try_set_active(b).unwrap();assert_eq!(raw(docs.find_as::<MapDocument>(b).unwrap()),other);
        assert!(docs.find_as_mut::<MapDocument>(b).unwrap().campaign_dab(ticket,&brush(128.,128.,3.),amount).is_err());
        docs.try_set_active(a).unwrap();let d=docs.find_as_mut::<MapDocument>(a).unwrap();
        assert!(d.campaign_dab(ticket,&brush(128.,128.,3.),amount).is_err());let fresh=begin(d,160.,0.1);d.cancel_campaign_stroke(fresh).unwrap();assert_valid(d);
    } }
}
#[test] fn fractional_map_activation_refuses_and_recovers_exactly() {activation_recovery(0.1);}
#[test] fn quantized_map_activation_refuses_and_recovers_exactly() {activation_recovery(64.);}
