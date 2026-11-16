//! Desired behavior for independent R1 findings. These assertions deliberately
//! fail on candidate 11ff9ce74; the read-only review probes assert its defects.
use super::*;

fn restore(slot: &Path) -> MapDocument {
    let path = MapDocument::complete_autosave(slot).expect("intact editable draft must open");
    let mut d = doc(built::read(MAP, &MapFiles::from_dir(&path).unwrap()).unwrap());
    d.restore_authoring(slot).unwrap();
    d.restored_from = Some(slot.into());
    d.observe_save_destination().unwrap();
    d
}
fn state(d: &MapDocument) -> (Vec<u8>, usize, usize, bool, Option<PathBuf>) {
    (d.lf_field(LfRole::Land).unwrap().to_le_rows(), d.history().position(), d.history().size(), d.modified(), d.save_dir.clone())
}
fn damage(path: &Path) {
    let mut value = fs::read(path).unwrap();
    value[128] ^= 1;
    fs::write(path, value).unwrap();
}

#[test]
fn r1_known_generation_block_survives_repeated_recovery_and_overwrite() {
    for partial in [false, true] {
        let s = Scratch::new();
        let root = s.0.join("owner");
        let mut d = project(&root);
        edit(&mut d, &A);
        if partial {
            d.publication_failure_after = Some(1);
            assert!(d.save(&SaveTarget::default()).is_err());
        } else {
            save(&mut d);
            damage(&native(&root).join(N));
        }
        drop(d);
        let mut blocked = open(&root, false);
        assert!(blocked.authoring.resolve(blocked.model().unwrap()).is_err());
        edit(&mut blocked, &B);
        let expected = state(&blocked).0;
        let slot = s.0.join("slot");
        blocked.autosave_to(&slot).unwrap();
        drop(blocked);
        let mut d = restore(&slot);
        assert_eq!(state(&d).0, expected);
        assert!(d.authoring.resolve(d.model().unwrap()).is_err(), "recovery must preserve the known integrity refusal");
        let before = read(&root);
        let draft = state(&d);
        assert!(d.save_overwrite(&SaveTarget::default()).is_err(), "ordinary Overwrite is not original reconciliation");
        assert_eq!(state(&d), draft);
        assert_eq!(read(&root), before);
        let again = s.0.join("again");
        d.autosave_to(&again).unwrap();
        drop(d);
        let mut d = restore(&again);
        assert!(d.authoring.resolve(d.model().unwrap()).is_err());
        d.authoring.recover(&built::read(MAP, &fixture()).unwrap(), d.model().unwrap()).unwrap();
        d.save_overwrite(&SaveTarget::default()).unwrap();
        assert_eq!(state(&d).0, expected);
    }
}

#[test]
fn r2_interrupted_first_publication_is_blocked_until_explicit_reconciliation() {
    let s = Scratch::new();
    fixture().write_to(&native(&s.0)).unwrap();
    let mut d = open(&s.0, false);
    d.authoring.adopt(d.model().unwrap()).unwrap();
    edit(&mut d, &A);
    d.publication_failure_after = Some(1);
    assert!(d.save(&SaveTarget::default()).is_err());
    drop(d);
    let mut d = open(&s.0, false);
    let before = read(&s.0);
    assert!(d.authoring.resolve(d.model().unwrap()).is_err(), "empty anchor cannot authorize mixed first publication");
    assert!(d.save_overwrite(&SaveTarget::default()).is_err());
    assert_eq!(read(&s.0), before);
    d.authoring.recover(&built::read(MAP, &fixture()).unwrap(), d.model().unwrap()).unwrap();
    d.save_overwrite(&SaveTarget::default()).unwrap();
    assert_same(&read(&s.0), &bytes(&fixture()));
}

#[test]
fn r2_line_boundary_truncation_cannot_omit_a_normal_from_generation_verification() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    save(&mut d);
    drop(d);
    let path = Context::record_path(&s.0, MAP).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let (prefix, generation) = text.split_once("--generation--\n").unwrap();
    assert!(generation.lines().last().unwrap().starts_with(N));
    let truncated = generation.lines().filter(|line| !line.starts_with(N)).collect::<Vec<_>>().join("\n");
    fs::write(&path, format!("{prefix}--generation--\n{truncated}\n")).unwrap();
    damage(&native(&s.0).join(N));
    let mut d = open(&s.0, false);
    edit(&mut d, &B);
    let before = read(&s.0);
    let pending = state(&d);
    assert!(d.authoring.resolve(d.model().unwrap()).is_err(), "completed inventory must include every original resource key");
    assert!(d.save_overwrite(&SaveTarget::default()).is_err());
    assert_eq!(read(&s.0), before);
    assert_eq!(state(&d), pending);
    d.authoring.recover(&built::read(MAP, &fixture()).unwrap(), d.model().unwrap()).unwrap();
    d.save_overwrite(&SaveTarget::default()).unwrap();
    assert_eq!(digest(&read(&s.0)[N]), GOLDEN);
}

#[test]
fn r3_missing_original_still_opens_intact_editable_draft_and_supports_repair() {
    let s = Scratch::new();
    let mut d = project(&s.0);
    edit(&mut d, &A);
    save(&mut d);
    assert!(d.history_mut().undo());
    let wanted = state(&d).0;
    assert!(d.modified());
    let context = d.authoring.context().unwrap().unwrap();
    let slot = s.0.join("slot");
    d.autosave_to(&slot).unwrap();
    assert!(slot.join("map").join(LF).is_file());
    assert!(!slot.join("map").join(N).exists());
    fs::remove_file(context.resource_dir().join(N)).unwrap();
    drop(d);
    let mut d = restore(&slot);
    assert_eq!(state(&d).0, wanted);
    assert!(d.modified());
    assert!(d.authoring.resolve(d.model().unwrap()).is_err());
    assert!(d.save_overwrite(&SaveTarget::default()).is_err());
    let again = s.0.join("again");
    d.autosave_to(&again).unwrap();
    assert_eq!(fs::read_to_string(again.join(crate::authoring::RECOVERY)).unwrap().lines().nth(5), Some(context.id()));
    d.authoring.recover(&built::read(MAP, &fixture()).unwrap(), d.model().unwrap()).unwrap();
    d.save_overwrite(&SaveTarget::default()).unwrap();
    assert_same(&read(&s.0), &bytes(&fixture()));
}

#[test]
fn r4_rejected_nested_copy_is_read_only_for_source_destination_and_document() {
    let s = Scratch::new();
    drop(project(&s.0));
    let mut d = open(&s.0, false);
    edit(&mut d, &A);
    let before = read(&s.0);
    let pending = state(&d);
    let target = native(&s.0).join("nested-owner");
    assert!(d.save_authoring_copy(&target).is_err());
    assert_eq!(read(&s.0), before, "no authoring files may appear in the native input map");
    assert!(!target.exists(), "overlap must be rejected before any creation");
    assert_eq!(state(&d), pending);
}

#[test]
fn r4_source_map_overlap_is_checked_even_when_owned_map_is_elsewhere() {
    let s = Scratch::new();
    let source = s.0.join("source");
    drop(project(&source));
    let mut d = open(&source, false);
    d.save_dir = Some(native(&s.0.join("current")));
    edit(&mut d, &A);
    let before = read(&source);
    let pending = state(&d);
    let target = native(&source).join("nested-owner");
    assert!(d.save_authoring_copy(&target).is_err());
    assert_eq!(read(&source), before);
    assert!(!target.exists());
    assert_eq!(state(&d), pending);
}

fn retained(d: &MapDocument) -> usize {
    d.normal_inputs().lock().unwrap().values().map(|b| b.len()).sum()
}
#[test]
fn r5_single_chunk_reopen_and_noop_save_retain_no_encoded_normal_snapshot() {
    let s = Scratch::new();
    drop(project(&s.0));
    let mut d = open(&s.0, false);
    assert_eq!(retained(&d), 0);
    save(&mut d);
    assert_eq!(retained(&d), 0, "bound B0 makes the complete old-source cache unnecessary");
    save(&mut d);
    assert_eq!(retained(&d), 0);
    edit(&mut d, &A);
    save(&mut d);
    assert_eq!(retained(&d), 0);
}

#[test]
fn r5_four_chunk_reopen_and_noop_save_retain_no_duplicate_inventory() {
    let s = Scratch::new();
    let input = native_fixture::multi_chunk_fixture();
    assert_eq!(input.folder[N].bytes().unwrap().len() * 4, 1_770_560);
    input.write_to(&native(&s.0)).unwrap();
    let mut d = open(&s.0, true);
    save(&mut d);
    drop(d);
    let mut d = open(&s.0, false);
    assert_eq!(retained(&d), 0);
    save(&mut d);
    assert_eq!(retained(&d), 0, "all four original chunks must not remain duplicated after a no-op Save");
}
