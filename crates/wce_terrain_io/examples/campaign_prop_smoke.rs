//! Tiny synthetic editor fixture and exported-pack verification; never copies game assets.
//! campaign_prop_smoke prepare|verify <scratch folder>
use std::{path::PathBuf,sync::Arc};
use wce_formats::markers::{MarkersFile,MarkerGroup,KeyPart};
use wce_terrain::campaign_props::CampaignProps;
use wce_vfs::GameProfile;
const MAP:&str="terrain/campaigns/wce_prop_test";
fn main(){
    let args:Vec<_>=std::env::args().skip(1).collect();assert_eq!(args.len(),2);
    let root=PathBuf::from(&args[1]);
    if args[0]=="prepare" {
        assert!(!root.join("game").exists(),"use a fresh scratch folder");
        std::fs::create_dir_all(root.join("game/data")).unwrap();
        std::fs::write(root.join("game/Rome2.exe"),b"synthetic test marker").unwrap();
        let mut record=vec![0;74];for i in [0,5,10,15]{record[i*4..i*4+4].copy_from_slice(&1f32.to_le_bytes());}
        record[48..52].copy_from_slice(&1f32.to_le_bytes());record[56..60].copy_from_slice(&1f32.to_le_bytes());
        let marker=MarkersFile{repository:"BASE_MARKER_REPOSITORY".into(),repository_id:645563,repository_u32:1,repository_u16:1,
            class_id:645566,class_name:"PROP_MARKER".into(),version:6,records:vec![record],groups:Some(vec![MarkerGroup {
                key:vec![KeyPart::Wide("synthetic_missing_prop.rigid_model_v2".encode_utf16().collect())],indices:vec![0]}])};
        let list=wce_tile::battle_map::tile_list::TileList {version:9,is_campaign:true,battle_size:[16,16],..Default::default()};
        let mut files=vec![
            (format!("{MAP}/terrain.tile_list"),list.write()),
            (format!("{MAP}/prop_marker.markers"),marker.write()),
            (format!("{MAP}/lf_height_map.dds"),wce_terrain_io::attila::heights::lf_height_dds(&wce_terrain::paged::PagedU16::filled(64,64,20000))),
            (format!("{MAP}/opaque.bin"),b"retain exactly".to_vec()),
        ];
        for kind in ["battle","campaign"] {files.push((format!("terrain/tiles/{kind}/_tile_database/_settings.xml"),b"<TILE_DATABASE><TILE_SETS/><CLIMATES/></TILE_DATABASE>".to_vec()));}
        wce_vfs::mod_pack::write_new(&GameProfile::ROME_2,&root.join("game/data/terrain.pack"),&files).unwrap();
        std::fs::write(root.join("original.markers"),marker.write()).unwrap();
        println!("synthetic fixture ready: {}",root.display());
    } else {
        use rpfm_lib::{files::pack::Pack,games::supported_games::SupportedGames};
        let loose=std::fs::read(root.join("export").join(MAP).join("prop_marker.markers")).unwrap();
        let games=SupportedGames::default();let game=games.game("rome_2").unwrap();
        let mut pack=Pack::read_and_merge(&[root.join("export/terrain_wce_prop_test.pack")],game,false,false,false).unwrap();
        assert_eq!(pack.files().len(),1,"only prop changes belong in the export");
        let packed=pack.files_mut().get_mut(&format!("{MAP}/prop_marker.markers")).unwrap().encode(&None,false,false,true).unwrap().unwrap();
        assert_eq!(loose,packed);
        let props=CampaignProps::parse(Arc::from(loose)).unwrap();let at=props.position(0).unwrap();
        assert!((at[0]-1.05).abs()<1e-6 && at[1]==0. && at[2]==1.);
        let original=MarkersFile::parse(&std::fs::read(root.join("original.markers")).unwrap()).unwrap();
        let mut expected=original.clone();expected.records[0][48..52].copy_from_slice(&at[0].to_le_bytes());
        assert_eq!(MarkersFile::parse(&props.write()).unwrap(),expected);
        println!("editor output verified: position {at:?}, one prop resource only; loose/pack identical");
    }
}
