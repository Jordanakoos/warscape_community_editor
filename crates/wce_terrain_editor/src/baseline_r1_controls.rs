//! Compatibility controls, separate from the eight frozen defect regressions.
use super::*;

#[test]
fn legitimate_dirty_draft_checks_owner_generation_instead_of_draft_lf() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    let slot = s.0.join("slot");
    d.autosave_to(&slot).unwrap();
    drop(d);
    let path = MapDocument::complete_autosave(&slot).unwrap();
    let mut d = doc(built::read(MAP, &MapFiles::from_dir(&path).unwrap()).unwrap());
    d.restore_authoring(&slot).unwrap();
    d.restored_from = Some(slot);
    d.observe_save_destination().unwrap();
    assert!(d.authoring.resolve(d.model().unwrap()).is_ok());
    edit(&mut d, &B);
    d.save_overwrite(&SaveTarget::default()).unwrap();
    assert_eq!(digest(&read(&s.0)[N]), GOLDEN);
}

#[test]
fn legacy_recovery_metadata_revalidates_owner_without_promoting_a_dirty_draft() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    save(&mut d);
    drop(d);
    damage_owner(&s.0);
    let d = open(&s.0, false);
    let slot = s.0.join("slot");
    d.autosave_to(&slot).unwrap();
    drop(d);
    let meta = slot.join(crate::authoring::RECOVERY);
    let text = fs::read_to_string(&meta).unwrap();
    let mut lines = text.lines().take(7).collect::<Vec<_>>();
    lines[0] = "WCE-TERRAIN-RECOVERY-1";
    fs::write(&meta, format!("{}\n", lines.join("\n"))).unwrap();
    let path = MapDocument::complete_autosave(&slot).unwrap();
    let mut d = doc(built::read(MAP, &MapFiles::from_dir(&path).unwrap()).unwrap());
    d.restore_authoring(&slot).unwrap();
    d.restored_from = Some(slot);
    d.observe_save_destination().unwrap();
    assert!(d.authoring.resolve(d.model().unwrap()).is_err());
    assert!(d.save_overwrite(&SaveTarget::default()).is_err());
}
fn damage_owner(root: &Path) {
    let path = native(root).join(N);
    let mut b = fs::read(&path).unwrap();
    b[128] ^= 1;
    fs::write(path, b).unwrap();
}

#[test]
fn missing_unrelated_recovery_reference_is_not_silently_dropped() {
    let s = Scratch::new();
    let d = project(&s.0);
    let slot = s.0.join("slot");
    d.autosave_to(&slot).unwrap();
    drop(d);
    fs::remove_file(native(&s.0).join("opaque.bin")).unwrap();
    assert!(MapDocument::complete_autosave(&slot).is_err());
    assert!(slot.join("map").join(LF).is_file());
}
