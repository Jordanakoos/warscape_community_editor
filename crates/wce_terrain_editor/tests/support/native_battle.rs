//! Shared asset-free Phase 1 fixture; never changed to update a failing golden hash.
use wce_terrain::paged::PagedU16;
use wce_terrain_io::MapFiles;
use wce_tile::battle_map::tile_list::TileList;
const LF: &str = "lf_height_map.dds";
const CHMF: &str = "lf_height_map.data";
const N: &str = "normals/normal_0.dds";
pub fn fixture() -> MapFiles {
    let mut lf = wce_terrain_io::attila::heights::lf_height_dds(&PagedU16::filled(192, 192, 10000));
    lf.extend(b"F-B synthetic DDS tail\0\xff");
    MapFiles::from_bytes([
        (LF.into(), lf),
        (CHMF.into(), wce_terrain_io::attila::heights::lf_height_data_rome2(&vec![10000; 192 * 192], 192, 192, (48, 48))),
        (N.into(), wce_tile_io::attila::bob_tile::dds_dxt5(&[0u8, 180, 255, 130].repeat(256 * 256), 256, 256)),
        ("terrain.tile_list".into(), TileList { version: 9, is_campaign: false, battle_size: [48, 48], ..Default::default() }.write()),
        ("opaque.bin".into(), b"F-B untouched synthetic resource\0\xff".to_vec()),
    ])
}

/// The independent review's bounded 512^2 LF / four 576^2 normal chunk fixture.
pub fn multi_chunk_fixture() -> MapFiles {
    use std::sync::Arc;
    let normal: Arc<[u8]> = Arc::from(wce_tile_io::attila::bob_tile::dds_dxt5(&[0u8, 180, 255, 130].repeat(576 * 576), 576, 576));
    let mut input = MapFiles::from_bytes([
        (LF.into(), wce_terrain_io::attila::heights::lf_height_dds(&PagedU16::filled(512, 512, 10000))),
        (CHMF.into(), wce_terrain_io::attila::heights::lf_height_data_rome2(&vec![10000; 512 * 512], 512, 512, (128, 128))),
        ("terrain.tile_list".into(), TileList { version: 9, battle_size: [128, 128], ..Default::default() }.write()),
    ]);
    for i in 0..4 {
        input.folder.insert(format!("normals/normal_{i}.dds"), wce_tile_model::Blob::Bytes(normal.clone()));
    }
    input
}
