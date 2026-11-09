//! Preservation boundary for built campaign heights. A built full LF pixel averages source samples while the CHMF stores a source
//! corner. A full-pixel edit alone cannot recover that corner, nor the source fractions needed by LQ quantization. The battle
//! reconstruction and normal-chunk updater must never silently run on these inputs. This is a guard, not an implemented resampler.

use wce_terrain::{MapKind, MapModel};

use crate::{Result, file_error};

/// Validate edited heights, returning whether to retain the campaign's original LF blobs. Unknown profiles remain readable and
/// unmodified resources remain writable. Companion markers protect incomplete folders/callers without a native campaign flag too.
pub(super) fn check_heights(m: &MapModel) -> Result<bool> {
    if super::campaign_authority::edited(m)? {
        return Err(file_error("lf_height_map.dds", "edited campaign export is unsupported until native campaign-normal regeneration is verified; save the WCE authoring project instead"));
    }
    let campaign = m.kind == MapKind::Campaign
        || m.tile_list.as_ref().is_some_and(|t| t.get().is_campaign)
        || m.carried.keys().any(|p| p.eq_ignore_ascii_case("lf_height_map_lq.dds") || p.eq_ignore_ascii_case("lf_normal.dds"));
    if !campaign {
        return Ok(false);
    }

    for (_, item) in &m.lf {
        if !item.edited() {
            continue;
        }

        let f = item.get();
        let original = item.origin.bytes()?;
        // Item::edited stays true after undo (and after merely taking a mutable reference). Compare actual samples and dimensions,
        // keeping every original header/mip/opaque byte when the user has restored the field.
        let rows = f.to_le_rows();
        if original.get(..4) == Some(b"DDS ")
            && original.get(12..16) == Some(f.height().to_le_bytes().as_slice())
            && original.get(16..20) == Some(f.width().to_le_bytes().as_slice())
            && original.get(128..128 + rows.len()) == Some(rows.as_slice())
        {
            continue;
        }

        return Err(file_error(&item.path, "saving or exporting edited built campaign heights is unsupported: campaign-normal regeneration and the mapping of arbitrary full-DDS edits are not verified. Undo the height changes to save or export the original height resources"));
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wce_terrain::lf_edit::LfStroke;
    use wce_terrain::paged::PagedU16;
    use wce_terrain::LfRole;
    use wce_tile::battle_map::tile_list::TileList;
    use wce_tile_model::Blob;

    use crate::{MapFiles, built};
    use super::*;

    // Synthetic resources only. Opaque tails deliberately exercise whole-blob preservation.
    fn files() -> MapFiles {
        let mut dds = crate::attila::heights::lf_height_dds(&PagedU16::filled(68, 36, 12000));
        dds.extend_from_slice(b"opaque DDS tail");
        MapFiles::from_bytes([
            ("lf_height_map.dds".into(), dds),
            ("lf_height_map.data".into(), b"unknown CHMF layout kept on no-op".to_vec()),
            ("Lf_Height_Map_Lq.DDS".into(), b"unknown LQ format kept on no-op".to_vec()),
            ("LF_NORMAL.DDS".into(), b"unknown normal format kept on no-op".to_vec()),
            ("unrelated.bin".into(), vec![7, 9, 0, 255]),
        ])
    }

    #[test]
    fn folder_classification_does_not_depend_on_a_map_name_or_tile_list() {
        let files = files();
        let campaign = built::read_as("arbitrary", MapKind::Campaign, &files).unwrap();
        assert_eq!(campaign.kind, MapKind::Campaign);
        assert!(built::changed(&files, &built::write(&campaign, None).unwrap()).is_empty());
        let battle = built::read_as("arbitrary", MapKind::Battle, &MapFiles::default()).unwrap();
        assert_eq!(battle.kind, MapKind::Battle);
    }

    #[test]
    fn native_campaign_flag_identifies_a_bare_name() {
        for campaign in [false, true] {
            let t = TileList { version: 9, is_campaign: campaign, ..Default::default() };
            let files = MapFiles::from_bytes([("terrain.tile_list".into(), t.write())]);
            let m = built::read("bare-name", &files).unwrap();
            assert_eq!(m.kind, if campaign { MapKind::Campaign } else { MapKind::Battle });
            assert!(built::changed(&files, &built::write(&m, None).unwrap()).is_empty());
        }
    }

    #[test]
    fn mutable_access_and_undo_preserve_every_original_blob_and_redo_refuses() {
        let files = files();
        let mut m = built::read_as("any", MapKind::Campaign, &files).unwrap();
        let field = m.lf_mut(LfRole::Land).unwrap().edit();
        let before = field.at(64, 16);
        let mut stroke = LfStroke::default();
        assert!(built::changed(&files, &built::write(&m, None).unwrap()).is_empty());
        let field = m.lf_mut(LfRole::Land).unwrap().edit();
        stroke.set(field, 64, 16, before + 100);
        let record = stroke.finish(field).unwrap();
        assert!(built::write(&m, None).is_err());
        record.revert(m.lf_mut(LfRole::Land).unwrap().edit());
        for _ in 0..3 {
            let written = built::write(&m, None).unwrap();
            assert!(built::changed(&files, &written).is_empty());
            let reopened = built::read_as("renamed", MapKind::Campaign, &written).unwrap();
            assert!(built::changed(&files, &built::write(&reopened, None).unwrap()).is_empty());
        }
        record.apply(m.lf_mut(LfRole::Land).unwrap().edit());
        assert_eq!(m.lf(LfRole::Land).unwrap().get().at(64, 16), before + 100);
        assert!(built::write(&m, None).is_err());
    }

    #[test]
    fn unsupported_edits_fail_at_interior_boundaries_edges_and_extrema() {
        let files = files();
        for (x, y) in [(1, 1), (15, 15), (16, 16), (63, 16), (64, 16), (67, 17), (0, 35), (67, 35)] {
            for value in [0, 1, 11999, 12001, 65534, 65535] {
                let mut m = built::read_as("any", MapKind::Campaign, &files).unwrap();
                m.lf_mut(LfRole::Land).unwrap().edit().set(x, y, value);
                let error = built::write(&m, None).unwrap_err().to_string();
                assert!(error.contains("lf_height_map.dds") && error.contains("unsupported") && error.contains("Undo"));
            }
        }
        let mut m = built::read_as("any", MapKind::Campaign, &files).unwrap();
        for (x, y) in [(15, 15), (16, 15), (16, 16), (17, 16)] {
            m.lf_mut(LfRole::Land).unwrap().edit().set(x, y, 65535);
        }
        assert!(built::write(&m, None).is_err());
        *m.lf_mut(LfRole::Land).unwrap().edit() = PagedU16::filled(34, 72, 12000);
        assert!(built::write(&m, None).is_err(), "same sample count with different dimensions is not a no-op");
    }

    #[test]
    fn each_companion_marker_protects_an_incomplete_unclassified_folder() {
        for marker in ["LF_HEIGHT_MAP_LQ.DDS", "Lf_Normal.dds"] {
            let mut files = files();
            files.folder.retain(|p, _| p == "lf_height_map.dds");
            files.folder.insert(marker.into(), Blob::Bytes(Arc::from(&b"opaque"[..])));
            let mut m = built::read("bare-name", &files).unwrap();
            assert!(built::write(&m, None).is_ok());
            m.lf_mut(LfRole::Land).unwrap().edit().set(3, 3, 0);
            assert!(built::write(&m, None).is_err());
        }
    }

    #[test]
    fn unrelated_native_edits_remain_writable_with_unchanged_heights() {
        let mut files = files();
        // This unrelated-edit control needs a recognized v9 trailer after combining P0-01 and P0-02B.
        // There are zero placements, so retain a valid zero-row opaque metadata table.
        let t = TileList { version: 9, is_campaign: true, strings: ["before".into(), "kept".into()],
            tail: vec![4, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0], ..Default::default() };
        files.folder.insert("terrain.tile_list".into(), Blob::Bytes(Arc::from(t.write())));
        let mut m = built::read("bare-name", &files).unwrap();
        m.tile_list.as_mut().unwrap().edit().strings[0] = "after".into();
        // Mutable height access alone must not prevent a safe unrelated operation.
        m.lf_mut(LfRole::Land).unwrap().edit();
        let written = built::write(&m, None).unwrap();
        let diff = built::changed(&files, &written);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].0, "terrain.tile_list");
        assert_eq!(TileList::parse(&diff[0].1).unwrap().strings[0], "after");
    }

    #[test]
    fn unedited_blobs_need_not_be_read_for_preservation() {
        let mut m = MapModel { kind: MapKind::Campaign, built: true, ..Default::default() };
        m.lf.push((LfRole::Land, wce_terrain::Item::new("lf_height_map.dds", PagedU16::filled(8, 8, 0), Blob::File("missing-original.dds".into()))));
        let written = built::write(&m, None).unwrap();
        assert_eq!(written.folder["lf_height_map.dds"], m.lf[0].1.origin);
    }

    #[test]
    fn raw_campaign_full_dds_does_not_determine_the_chmf_corner() {
        use crate::attila::heights::{campaign_lf_data, campaign_lf_dds};
        use wce_formats::chmf::{ChmfFile, decode_block};

        let a = PagedU16::filled(16, 16, 12000);
        let mut b = a.clone();
        b.set(0, 0, 11999);
        b.set(1, 0, 12001);
        assert_eq!(campaign_lf_dds(&a, 1), campaign_lf_dds(&b, 1), "the actual BOB-compatible reducer loses the corner information");
        let corner = |field: &PagedU16| {
            let c = ChmfFile::parse(&campaign_lf_data(field, (2, 2))).unwrap();
            let mut v = vec![];
            decode_block(&c.regions[0].data, 256, &mut v).unwrap();
            v[0]
        };
        assert_ne!(corner(&a), corner(&b), "equal built full DDS inputs require different source-corner CHMF samples");
    }
}
