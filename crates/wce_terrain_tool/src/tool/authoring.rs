//! Explicit native battle project lifecycle, shared by UI and automation.
use super::*;
use wce_editor_core::document::{Document, SaveTarget};
use wce_terrain_io::{
    MapFiles,
    built::{
        self,
        baseline::{Baseline, Context},
    },
};

impl TerrainTool {
    fn native_document(&mut self, e: &Editor, source: &Path, name: &str) -> Result<Box<MapDocument>, String> {
        let m = built::read(name, &MapFiles::from_dir(source).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let list = m.tile_list.as_ref().ok_or("native project requires a supported tile list")?;
        if list.get().is_campaign {
            return Err("native authoring project supports battle maps only".into());
        }
        self.prepare_packs(e);
        let mut roots = kit_roots(e);
        if let Some(raw) = self.packs.as_ref().and_then(|p| p.raw().ok()) {
            roots.push(raw);
        }
        let db = self
            .databases_of(&roots, MapKind::Battle)
            .or_else(|err| if list.get().placements.is_empty() { Ok(Arc::new(TileDatabase::default())) } else { Err(err) })?;
        let lf = m.lf(LfRole::Land).map(|f| lf_field(f.get()));
        let placed = Arc::new(wce_tile::battle_map::context::PlacedMap::from_tile_list(name, list.get(), &db, lf));
        Ok(Box::new(MapDocument::new(name.into(), MapSource::Kit(source.into()), Some(m), placed, None, db)))
    }
    fn show_native_document(&mut self, e: &mut Editor, d: Box<MapDocument>) -> Result<String, String> {
        let rq = self.request(e, &d).ok_or("no tile database available for this native map")?;
        let status = format!("{}: {}; {}", d.name, d.summary(), d.authoring.status());
        let id = e.replace_document(TERRAIN_TOOL_ID, d)?;
        self.show(e, id, Box::new(Streamer::new(rq, e.files())));
        Ok(status)
    }
    fn open_project_now(&mut self, e: &mut Editor, root: &Path, map: &str, source: Option<&Path>) -> Result<String, String> {
        let again_root = root.to_owned();
        let again_map = map.to_owned();
        let again_source = source.map(Path::to_owned);
        let retry = Box::new(move |e: &mut Editor| {
            e.with_tool::<Self, _>(|t, e| t.open_project_now(e, &again_root, &again_map, again_source.as_deref()))
                .unwrap_or_else(|| Err("Terrain tool busy".into()))
        });
        if !e.clear_to_replace(TERRAIN_TOOL_ID, retry)? {
            return Ok("asking about unsaved changes".into());
        }
        let native = format!("native/{map}");
        let directory = if let Some(source) = source {
            source.to_owned()
        } else {
            Context::open(root, map).map(|c| c.native_dir()).unwrap_or_else(|_| root.join(&native))
        };
        let mut d = self.native_document(e, &directory, map)?;
        let target = if source.is_some() { root.join(&native) } else { directory.clone() };
        if source.is_some() {
            let b = Baseline::capture(d.model().unwrap(), e.game().key)?;
            Context::create(root, map, &native, &b)?;
        }
        d.save_dir = Some(target.clone());
        d.configure_authoring(root, e.game().key, map, &target, false);
        d.observe_save_destination()?;
        if source.is_some() {
            d.save(&SaveTarget { path: PathBuf::new() })?;
        }
        self.show_native_document(e, d)
    }
    pub(super) fn restore_project_now(&mut self, e: &mut Editor, slot: &Path) -> Result<String, String> {
        let map = MapDocument::complete_autosave(slot)?;
        let text = std::fs::read_to_string(slot.join(wce_terrain_editor::authoring::RECOVERY)).map_err(|e| e.to_string())?;
        let name = text.lines().nth(2).filter(|v| !v.is_empty()).ok_or("recovery has no native project identity")?;
        let mut d = self.native_document(e, &map, name)?;
        d.restore_authoring(slot)?;
        d.restored_from = Some(slot.into());
        d.observe_save_destination()?;
        self.show_native_document(e, d)
    }
}

fn current(e: &mut Editor) -> Result<&mut MapDocument, String> {
    e.docs.active_mut().and_then(|d| d.as_any_mut().downcast_mut::<MapDocument>()).ok_or("no active Terrain document".into())
}
fn result(r: Result<String, String>) -> CommandResult {
    match r {
        Ok(s) => CommandResult::success(s),
        Err(e) => CommandResult::fail(e),
    }
}
pub(super) fn register(c: &mut CommandRegistry<Editor>) {
    c.add(
        "terrain_project_create",
        "terrain_project_create <owner-root> <native-source> <map>",
        "start an explicit native battle authoring lineage in a separate owner root",
        |e, a| {
            if a.len() != 3 {
                return CommandResult::fail("usage: terrain_project_create <owner-root> <native-source> <map>");
            }
            with_terrain(e, |t, e| result(t.open_project_now(e, Path::new(&a[0]), &a[2], Some(Path::new(&a[1])))))
        },
    );
    c.add("terrain_project_open", "terrain_project_open <owner-root> <map>", "open a portable native battle authoring context", |e, a| {
        if a.len() != 2 {
            return CommandResult::fail("usage: terrain_project_open <owner-root> <map>");
        }
        with_terrain(e, |t, e| result(t.open_project_now(e, Path::new(&a[0]), &a[1], None)))
    });
    c.add(
        "terrain_project_copy",
        "terrain_project_copy <owner-root>",
        "save an authoring-aware copy without retargeting or clearing edits",
        |e, a| {
            result((|| {
                let root = a.first().ok_or("usage: terrain_project_copy <owner-root>")?;
                current(e)?.save_authoring_copy(Path::new(root))?;
                Ok("authoring-aware copy saved; source document retained".into())
            })())
        },
    );
    c.add(
        "terrain_save_copy",
        "terrain_save_copy <native-folder>",
        "the Terrain document's ordinary Save As native copy, without authoring metadata or retargeting",
        |e, a| {
            result((|| {
                let path = a.first().ok_or("usage: terrain_save_copy <native-folder>")?;
                let id = e.docs.active_id();
                e.docs.save_to(id, &SaveTarget { path: PathBuf::from(path) })
            })())
        },
    );
    c.add(
        "terrain_baseline_adopt",
        "terrain_baseline_adopt",
        "explicitly adopt the opened native resources as a NEW lineage, without claiming historical originals",
        |e, _| {
            result((|| {
                let d = current(e)?;
                let _lease = d.begin_io()?;
                d.authoring.adopt(d.model().ok_or("no model")?)?;
                Ok(d.authoring.status())
            })())
        },
    );
    c.add(
        "terrain_baseline_recover",
        "terrain_baseline_recover <original-native-folder>",
        "repair the existing baseline only from a completely hash-verified donor; reconcile partial Save",
        |e, a| {
            result((|| {
                let source = a.first().ok_or("usage: terrain_baseline_recover <original-native-folder>")?;
                let donor =
                    built::read("donor", &MapFiles::from_dir(Path::new(source)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let d = current(e)?;
                let _lease = d.begin_io()?;
                d.authoring.recover(&donor, d.model().ok_or("no model")?)?;
                Ok(d.authoring.status())
            })())
        },
    );
    c.add(
        "terrain_recovery_copy",
        "terrain_recovery_copy <slot>",
        "write a native project recovery draft using the document's production autosave path",
        |e, a| {
            result((|| {
                let slot = a.first().ok_or("usage: terrain_recovery_copy <slot>")?;
                current(e)?.autosave_to(Path::new(slot))?;
                Ok("native recovery copy written".into())
            })())
        },
    );
    c.add(
        "terrain_recovery_open",
        "terrain_recovery_open <slot>",
        "restore a native project recovery draft using the production recovery loader",
        |e, a| {
            let Some(slot) = a.first() else {
                return CommandResult::fail("usage: terrain_recovery_open <slot>");
            };
            with_terrain(e, |t, e| result(t.restore_project_now(e, Path::new(slot))))
        },
    );
}

pub(super) fn draw(t: &mut TerrainTool, e: &mut Editor, id: DocumentId) {
    if !gui::collapsing_header("Native battle authoring", 0) {
        return;
    }
    if let Some(d) = e.docs.find_as::<MapDocument>(id) {
        gui::text_wrapped(&d.authoring.status());
    }
    if gui::button("Open authoring project...")
        && let Some(root) = e.pick_export_folder("terrain-authoring", "Open native battle authoring project")
    {
        let maps = std::fs::read_dir(root.join("authoring/terrain/maps"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.path().file_stem().map(|v| v.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        if maps.len() == 1 {
            let map = maps[0].clone();
            let r = t.open_project_now(e, &root, &map, None);
            match r {
                Ok(s) => e.status(&s, false),
                Err(s) => e.status(&s, true),
            }
        } else {
            e.status("Choose a project root with one map, or use terrain_project_open <root> <map>", true);
        }
    }
    if gui::button("Adopt opened resources as new lineage") {
        let r = (|| {
            let d = e.docs.find_as::<MapDocument>(id).ok_or("no map")?;
            let _lease = d.begin_io()?;
            d.authoring.adopt(d.model().ok_or("no model")?)?;
            Ok::<_, String>(d.authoring.status())
        })();
        match r {
            Ok(s) => e.status(&s, false),
            Err(s) => e.status(&s, true),
        }
    }
    if gui::is_item_hovered_allow_disabled() {
        gui::set_tooltip(
            "Explicitly chooses the resources loaded at Open as the new original baseline. This does not recover earlier historical originals. Existing records cannot be replaced.",
        );
    }
    if gui::button("Save authoring-aware copy...")
        && let Some(root) = e.pick_export_folder("terrain-authoring-copy", "Save native battle authoring-aware copy")
    {
        let r = e.docs.find_as_mut::<MapDocument>(id).ok_or("no map".into()).and_then(|d| d.save_authoring_copy(&root));
        match r {
            Ok(()) => e.status("Authoring-aware copy saved; source retained", false),
            Err(s) => e.status(&s, true),
        }
    }
    if gui::button("Recover verified originals...")
        && let Some(root) = e.pick_export_folder("terrain-originals", "Folder holding the recorded original native resources")
    {
        let r = (|| {
            let donor = built::read("donor", &MapFiles::from_dir(&root).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let d = e.docs.find_as::<MapDocument>(id).ok_or("no map")?;
            let _lease = d.begin_io()?;
            d.authoring.recover(&donor, d.model().ok_or("no model")?)
        })();
        match r {
            Ok(()) => e.status("Verified original baseline recovered", false),
            Err(s) => e.status(&s, true),
        }
    }
}
