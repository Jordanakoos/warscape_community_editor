//! Campaign authority/sidecar contracts using the accepted synthetic native profile.
use std::{path::{Path,PathBuf},sync::atomic::{AtomicU64,Ordering}};
use wce_formats::chmf::{ChmfFile,ChmfRegion,encode_region};
use wce_foundation::game::GameProfile;
use wce_terrain::{MapKind,MapModel,LfRole,lf_edit::LfBrush,paged::PagedU16};
use wce_tile::battle_map::tile_list::TileList;
use wce_tile_model::Blob;
use wce_terrain_io::{MapFiles,built::{self,campaign_authority as authority}};
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

struct Scratch(PathBuf);
impl Scratch { fn new()->Self {static NEXT:AtomicU64=AtomicU64::new(0);let p=std::env::temp_dir().join(format!("wce-authority-contract-{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir(&p).unwrap();Self(p)} }
impl Drop for Scratch { fn drop(&mut self){let p=self.0.canonicalize().unwrap();assert!(p.starts_with(std::env::temp_dir().canonicalize().unwrap())&&p.file_name().unwrap().to_string_lossy().starts_with("wce-authority-contract-"));std::fs::remove_dir_all(p).unwrap();} }
fn write_files(files:&MapFiles,dir:&Path){
    use wce_foundation::publication::{Output,Source,Staged};
    let outputs=files.folder.iter().map(|(k,b)|Output::new(wce_foundation::publication::resource_path(dir,k).unwrap(),Source::Bytes(b.bytes().unwrap()))).collect();
    Staged::prepare(outputs).and_then(Staged::publish).unwrap();
}
fn bytes(files:&MapFiles,name:&str)->Vec<u8>{files.folder[name].bytes().unwrap().to_vec()}
fn preview(m:&mut MapModel,amount:f64){
    let mut a=authority::Authority::begin(m,&GameProfile::ROME_2).unwrap();
    let brush=LfBrush{brush:wce_tile::height_field::Brush{cx:64.,cy:64.,radius:3.,strength:1.,falloff:0.,..Default::default()},aspect:1.,rotation:0.};
    let p=a.raise(&brush,amount).unwrap();let b=bytes(&p.height_files,"lf_height_map.dds");
    let u=|at|u32::from_le_bytes(b[at..at+4].try_into().unwrap());
    *m.lf_mut(LfRole::Land).unwrap().edit()=PagedU16::from_le_rows(u(16),u(12),&b[128..]);
    authority::put_state(m,Some(a.snapshot()));
}
#[test]
fn project_retains_native_baselines_and_reopens_exact_authority_and_preview(){
    let f=fixture();let mut m=built::read_as("arbitrary",MapKind::Campaign,&f).unwrap();preview(&mut m,1.25);
    let expected=m.lf(LfRole::Land).unwrap().get().to_le_rows();assert!(built::write(&m,None).is_err());
    let project=authority::project_files(&m,None).unwrap();
    for name in ["lf_height_map.data","lf_height_map.dds","lf_height_map_lq.dds","normals/normal_0.dds"]{assert_eq!(bytes(&project,name),bytes(&f,name),"{name}");}
    assert!(project.folder.contains_key(authority::PROJECT_STATE));
    let reopened=built::read_as("renamed",MapKind::Campaign,&project).unwrap();
    assert_eq!(reopened.lf(LfRole::Land).unwrap().get().to_le_rows(),expected);
    assert_eq!(authority::Authority::begin(&reopened,&GameProfile::ROME_2).unwrap().node_delta(16,16),1.25);
    assert_eq!(bytes(&authority::project_files(&reopened,None).unwrap(),authority::PROJECT_STATE),bytes(&project,authority::PROJECT_STATE));
}
#[test]
fn malformed_state_and_changed_native_baselines_refuse_without_input_mutation(){
    let f=fixture();let mut m=built::read_as("test",MapKind::Campaign,&f).unwrap();preview(&mut m,2.5);let project=authority::project_files(&m,None).unwrap();
    for bad in [vec![],b"WCHE\x01".to_vec(),b"unknown authoring state".to_vec()]{
        let mut input=project.clone();input.folder.insert(authority::PROJECT_STATE.into(),Blob::Bytes(bad.clone().into()));
        assert!(built::read_as("test",MapKind::Campaign,&input).is_err());assert_eq!(bytes(&input,authority::PROJECT_STATE),bad);
    }
    let mut changed=project.clone();let mut c=bytes(&changed,"lf_height_map.data");c.push(0x7f);changed.folder.insert("lf_height_map.data".into(),Blob::Bytes(c.clone().into()));
    assert!(built::read_as("test",MapKind::Campaign,&changed).is_err());assert_eq!(bytes(&changed,"lf_height_map.data"),c);
}
#[test]
fn destination_generation_checks_are_read_only_and_require_matching_baselines(){
    let f=fixture();let mut m=built::read_as("test",MapKind::Campaign,&f).unwrap();preview(&mut m,1.25);let project=authority::project_files(&m,None).unwrap();
    let dir=Scratch::new();write_files(&project,&dir.0);authority::check_project_destination(&project,&dir.0).unwrap();
    let path=dir.0.join("lf_height_map.dds");let mut b=std::fs::read(&path).unwrap();b[128]^=1;std::fs::write(&path,&b).unwrap();
    let state=std::fs::read(dir.0.join(authority::PROJECT_STATE)).unwrap();assert!(authority::check_project_destination(&project,&dir.0).is_err());
    assert_eq!(std::fs::read(&path).unwrap(),b);assert_eq!(std::fs::read(dir.0.join(authority::PROJECT_STATE)).unwrap(),state);
}
#[test]
fn file_backed_state_is_frozen_when_loaded_and_does_not_follow_overwrites(){
    let mut m=built::read_as("test",MapKind::Campaign,&fixture()).unwrap();preview(&mut m,1.25);let project=authority::project_files(&m,None).unwrap();
    let dir=Scratch::new();write_files(&project,&dir.0);let model=built::read_as("test",MapKind::Campaign,&MapFiles::from_dir(&dir.0).unwrap()).unwrap();
    let before=authority::state_blob(&model).unwrap();assert!(matches!(before,Blob::Bytes(_)));
    std::fs::write(dir.0.join(authority::PROJECT_STATE),b"external replacement").unwrap();
    assert_eq!(authority::state_blob(&model).unwrap(),before);assert_eq!(bytes(&authority::project_files(&model,None).unwrap(),authority::PROJECT_STATE),before.bytes().unwrap().as_ref());
}
#[test]
fn unsupported_profile_brush_and_arbitrary_preview_refuse_without_changing_authority(){
    let mut m=built::read_as("test",MapKind::Campaign,&fixture()).unwrap();assert!(authority::Authority::begin(&m,&GameProfile::ATTILA).is_err());
    let mut a=authority::Authority::begin(&m,&GameProfile::ROME_2).unwrap();let before=a.snapshot();
    let brush=LfBrush{brush:wce_tile::height_field::Brush{cx:64.,cy:64.,radius:3.,strength:1.,falloff:0.,..Default::default()},aspect:1.,rotation:0.};
    assert!(a.raise(&brush,f64::NAN).is_err());assert_eq!(a.snapshot(),before);
    m.lf_mut(LfRole::Land).unwrap().edit().set(64,64,11000);assert!(authority::project_files(&m,None).is_err());assert!(authority::state_blob(&m).is_none());
}
#[test]
fn quantized_native_noop_retains_exact_project_deltas_but_never_exports_sidecar(){
    let f=fixture();let mut m=built::read_as("test",MapKind::Campaign,&f).unwrap();preview(&mut m,0.0000001);
    let project=authority::project_files(&m,None).unwrap();let native=built::write(&m,None).unwrap();
    assert!(!native.folder.contains_key(authority::PROJECT_STATE));assert!(built::changed(&f,&native).is_empty());
    let reopened=built::read_as("test",MapKind::Campaign,&project).unwrap();
    assert_eq!(authority::Authority::begin(&reopened,&GameProfile::ROME_2).unwrap().node_delta(16,16),0.0000001);
}
