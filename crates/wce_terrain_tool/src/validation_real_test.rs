//! Opt-in, read-only checks of existing campaign witnesses. Never copy or publish their resources.
use super::*;
use wce_editor_core::document::Document;

#[test]
#[ignore = "requires WCE_D3C_RAW, WCE_D3C_PROJECT_RAW and WCE_D3C_NATIVE existing witnesses"]
fn validation_real_campaign_witnesses() {
    let path = |name| PathBuf::from(std::env::var_os(name).expect(name));
    let kit = path("WCE_D3C_RAW");
    let project = path("WCE_D3C_PROJECT_RAW");
    let mut report = String::new();
    let expected = render_lf(&kit.join(database_dir(MapKind::Campaign)).join("_settings.xml")).unwrap();
    for (label, raw, roots) in [("Attila pristine kit", kit.clone(), vec![kit.clone()]),
                               ("Attila accepted saved project", project.clone(), vec![project, kit.clone()])] {
        let folders: Vec<_> = roots.iter().map(|r| r.join(database_dir(MapKind::Campaign))).filter(|p| p.is_dir()).collect();
        let input = OpenInputs { name: "campaigns/main_attila_map".into(), game_dir: "terrain/battles/main_attila_map", raw,
            db: Arc::new(TileDatabase::load_layered(&folders.iter().map(PathBuf::as_path).collect::<Vec<_>>()).unwrap()),
            render_lf: campaign_render_lf(&roots, MapKind::Campaign), files: None, packs: None, built: None,
            game: &wce_foundation::game::GameProfile::ATTILA, install: None };
        let d = open_map(&input).unwrap().document();
        assert_eq!((d.placed.lf_height, d.placed.vertical_offset), expected);
        let heights: Vec<_> = [[50.,177.], [200.,200.], [335.,234.], [500.,300.]].into_iter().map(|[x,z]| d.lf_metres_at(x,z)).collect();
        let revision = d.terry_revision.load(std::sync::atomic::Ordering::Relaxed);
        let history = (d.history().version(), d.history().size(), d.history().position(), d.modified());
        let placed = d.placed.clone();
        let start = std::time::Instant::now();
        let findings = crate::validate::check(&d, &roots, input.game, None);
        assert!(!findings.iter().any(|f| f.severity == crate::validate::Severity::Error), "{findings:?}");
        assert_eq!(history, (d.history().version(), d.history().size(), d.history().position(), d.modified()));
        assert_eq!(revision, d.terry_revision.load(std::sync::atomic::Ordering::Relaxed));
        assert!(Arc::ptr_eq(&placed, &d.placed));
        assert_eq!(heights, [[50.,177.], [200.,200.], [335.,234.], [500.,300.]].into_iter().map(|[x,z]| d.lf_metres_at(x,z)).collect::<Vec<_>>());
        report.push_str(&format!("{label}: validation {:.3}s; scale/offset {expected:?}; heights {heights:?}; history/scene revision/placed identity unchanged\n", start.elapsed().as_secs_f64()));
        for f in findings { report.push_str(&format!("{}\n", f.display())); }
    }
    let files = wce_terrain_io::MapFiles::from_dir(&path("WCE_D3C_NATIVE")).unwrap();
    let model = wce_terrain_io::built::read_as("campaigns/main_rome_map", MapKind::Campaign, &files).unwrap();
    let d = MapDocument::new("campaigns/main_rome_map".into(), MapSource::Packs("witness".into()), Some(model), Arc::default(), None, Arc::default());
    let before = (d.history().version(), d.modified(), d.terry_revision.load(std::sync::atomic::Ordering::Relaxed));
    let start = std::time::Instant::now();
    let findings = crate::validate::check(&d, &[], &wce_foundation::game::GameProfile::ROME_2, None);
    let validation_seconds = start.elapsed().as_secs_f64();
    assert!(!findings.iter().any(|f| f.category == crate::validate::Category::InvalidData || f.severity == crate::validate::Severity::Error), "{findings:?}");
    assert_eq!(before, (d.history().version(), d.modified(), d.terry_revision.load(std::sync::atomic::Ordering::Relaxed)));
    assert!(wce_terrain_io::built::changed(&files, &wce_terrain_io::built::write(d.model().unwrap(), None).unwrap()).is_empty());
    report.push_str(&format!("Rome II native: validation {validation_seconds:.3}s; {} placements / {} props; no changed native bytes\n", d.model().unwrap().tile_list.as_ref().unwrap().get().placements.len(), d.native_props().unwrap().len()));
    for f in findings { report.push_str(&format!("{}\n", f.display())); }
    println!("{report}");
    if let Some(root) = std::env::var_os("WCE_DIAG_ROOT") { std::fs::write(PathBuf::from(root).join("reports/real-validation.txt"), report).unwrap(); }
}
