//! On-demand Terrain checks for kit and native maps. These are diagnostics, not a second Save/Export permission system.
//! Native preservation decisions stay with the existing checked readers/writers. No history, scene or native output is changed.
//!
//! Not checked yet: painted settlement sizes against `tile_upgrades.xml` (docs/plans/tile.md item 3; its rules to be settled), BCAs whose
//! infield holds restricted tiles (Phase 6).

use std::collections::BTreeMap;
use std::path::PathBuf;

use wce_foundation::game::GameProfile;
use wce_terrain::{LayerRole, MapKind, MapModel};
use wce_terrain_editor::MapDocument;
use wce_terrain_io::built;
use wce_tile_model::Blob;
use wce_vfs::files::Files;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The fill or the game will not do what the map says.
    Error,

    /// Works, but likely not as meant.
    #[default]
    Warning,
    Information,
}

/// Distinguish a known invalid relationship from an edit WCE cannot safely write, and a capability notice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Category { #[default] InvalidData, UnsupportedOperation, Limitation }

impl Category {
    fn label(self) -> &'static str {
        match self { Self::InvalidData => "invalid data", Self::UnsupportedOperation => "unsupported operation", Self::Limitation => "limitation" }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Finding {
    pub severity: Severity,
    pub text: String,
    pub cell: Option<(i32, i32)>,
    pub game: &'static str,
    pub kind: MapKind,
    pub resource: String,
    pub entity: Option<String>,
    pub category: Category,
    /// Explanation only: existing Save/Export guards remain authoritative.
    pub affects: &'static str,
}

impl Finding {
    pub fn title(&self) -> String {
        let at = self.entity.as_ref().map(|s| format!(" [{s}]")).unwrap_or_default();
        format!("{:?}: {}{at}", self.severity, self.resource)
    }

    pub fn explanation(&self) -> String {
        format!("{} {:?}, {}: {} (affects: {})", self.game, self.kind, self.category.label(), self.text, self.affects)
    }

    pub fn display(&self) -> String {
        format!("{} — {}", self.title(), self.explanation())
    }
}

fn eq(k: [f32; 3], c: [u8; 4]) -> bool {
    k[0] == f32::from(c[0]) && k[1] == f32::from(c[1]) && k[2] == f32::from(c[2])
}

/// A layer's colours that `known` does not take: (colour, cells, first cell).
fn unknown_colours(l: &wce_terrain::cells::CellLayer, known: impl Fn([u8; 4]) -> bool) -> Vec<([u8; 4], usize, (i32, i32))> {
    let mut out = vec![];
    for (i, n) in l.histogram() {
        let c = l.palette()[usize::from(i)];
        if known(c) {
            continue;
        }

        let first = l.cells().iter().position(|&k| k == i).unwrap_or(0) as u32;
        out.push((c, n, ((first % l.width()) as i32, (first / l.width()) as i32)));
    }

    out
}

fn finding(m: &MapModel, game: &GameProfile, severity: Severity, category: Category, resource: &str, affects: &'static str, text: impl Into<String>) -> Finding {
    Finding { severity, category, resource: resource.into(), text: text.into(), game: game.key, kind: m.kind, affects, ..Default::default() }
}

/// Inspect the current document without editing it or compiling/publishing native resources.
/// `files` is the mounted asset lookup, if available; its absence is never evidence of a broken model reference.
pub fn check(d: &MapDocument, roots: &[PathBuf], game: &GameProfile, files: Option<&dyn Files>) -> Vec<Finding> {
    use Category::{InvalidData, Limitation, UnsupportedOperation};
    use Severity::{Error, Information, Warning};
    let Some(m) = d.model() else {
        return vec![Finding { severity: Information, category: Limitation, game: game.key, resource: "map".into(),
            affects: "validation", text: "No authoring model is open; resource validation is unavailable.".into(), ..Default::default() }];
    };
    let mut out = if m.built { vec![] } else { check_kit(d, roots) };
    for f in &mut out { f.game = game.key; f.kind = m.kind; }
    let mut resources: BTreeMap<&str, &Blob> = m.carried.iter().map(|(p, b)| (p.as_str(), b)).collect();
    for (_, i) in &m.layers { resources.insert(&i.path, &i.origin); }
    for (_, i) in &m.lf { resources.insert(&i.path, &i.origin); }
    for i in &m.xml { resources.insert(&i.path, &i.origin); }
    if let Some(i) = &m.explicit { resources.insert(&i.path, &i.origin); }
    if let Some(i) = &m.project { resources.insert(&i.path, &i.origin); }
    if let Some(i) = &m.tile_list { resources.insert(&i.path, &i.origin); }
    let has = |name: &str| resources.keys().any(|p| p.eq_ignore_ascii_case(name));
    for (name, blob) in &resources {
        if let Blob::File(p) = blob && !p.is_file() {
            out.push(finding(m, game, Error, InvalidData, name, "Save / native Export",
                format!("The original source file is unavailable: {}. Restore it before saving or exporting.", p.display())));
        }
    }
    if m.kind == MapKind::Campaign {
        let required = if m.built { & ["lf_height_map.dds"][..] } else { & ["tile_map.png", "lf_heights.tif"][..] };
        for name in required {
            if !has(name) {
                out.push(finding(m, game, Error, InvalidData, name, "editing / native Export", "Required campaign resource is missing. Saving the remaining files cannot repair it."));
            }
        }
        if m.built {
            if !has("terrain.tile_list") && !(has("tile_map.index") && has("tile_map.tiles")) {
                out.push(finding(m, game, Error, InvalidData, "terrain.tile_list / tile_map.index + tile_map.tiles", "editing / native Export", "The native campaign has no complete placement resource."));
            }
            if has("tile_map.index") != has("tile_map.tiles") {
                out.push(finding(m, game, Error, InvalidData, "tile_map.index / tile_map.tiles", "editing / native Export", "Only one half of the native tile-map pair is present."));
            }
            if has("lf_height_map.dds") && m.lf(wce_terrain::LfRole::Land).is_none() {
                out.push(finding(m, game, Information, Limitation, "lf_height_map.dds", "height editing", "This height representation is preserved as opaque data; WCE has not decoded an editable LF field."));
            }
            check_campaign_heights(m, game, &mut out);
        } else {
            for (name, decoded) in [("tile_map.png", m.layer(&LayerRole::TileMap).is_some()), ("lf_heights.tif", m.lf(wce_terrain::LfRole::Land).is_some())] {
                if has(name) && !decoded {
                    out.push(finding(m, game, Warning, UnsupportedOperation, name, "editing / native Export", "The resource is retained, but its authoring representation is not decoded. Do not assume an edit can be exported."));
                }
            }
            out.push(finding(m, game, Information, Limitation, "campaign heights", "height editing / Save / native Export", "The raw/kit height workflow is retained. Render scale and vertical offset come from the loaded layered database; native-map height restrictions do not apply to this kit project."));
        }
        out.push(finding(m, game, Information, Limitation, "campaign placements", "validation", "Campaign shared geometry is permitted. Battle-map overlap and settlement footprint assumptions are not applied."));
    }
    if let Some(t) = &m.tile_list {
        let list = t.get();
        let placement_error = built::check_native_placements(m).err();
        // A name index is a reliable reference even when the trailer's opaque semantics are unknown.
        for (n, p) in list.placements.iter().enumerate().filter(|(_, p)| p.name >= list.names.len()) {
            let mut f = finding(m, game, Error, InvalidData, &t.path, "placement editing / native Export", format!("Placement refers to missing tile name {} ({} names available).", p.name, list.names.len()));
            f.entity = Some(format!("placement {n}")); f.cell = Some((i32::from(p.x), i32::from(p.y))); out.push(f);
        }
        if list.version == 9 {
            match list.v9_paired_metadata_count() {
                Err(e) if placement_error.is_none() => out.push(finding(m, game, Warning, InvalidData, &t.path, "placement editing", format!("Recognized v9 placement metadata is inconsistent: {e}. Original bytes remain preserved."))),
                Ok(Some(n)) if n != 0 && n as usize != list.placements.len() => {
                    // During an attempted count edit the checked writer below explains the refusal; do not call that source corruption.
                    if placement_error.is_none() { out.push(finding(m, game, Warning, InvalidData, &t.path, "placement editing", format!("The v9 metadata has {n} paired rows for {} placements. Original bytes remain preserved.", list.placements.len()))); }
                }
                Ok(None) => out.push(finding(m, game, Information, Limitation, &t.path, "placement editing", "Unknown v9 metadata is preserved unchanged. WCE cannot safely edit this placement representation.")),
                _ => {}
            }
        } else if list.version >= 11 {
            out.push(finding(m, game, Information, Limitation, &t.path, "placement editing", "This native placement version is readable and retained; the current placement editor supports Rome II versions 7–9."));
        }
        if let Some(e) = placement_error {
            out.push(finding(m, game, Error, UnsupportedOperation, &t.path, "Save / native Export", format!("The current placement edit cannot be written losslessly: {e}. Undo the unsupported edit.")));
        }
    } else if m.built && (has("terrain.tile_list") || has("tile_map.index") && has("tile_map.tiles")) {
        out.push(finding(m, game, Information, Limitation, "native placements", "placement editing", "Native placement bytes are retained, but this representation is not decoded for editing."));
    }
    if m.built {
        // Reuse the actual BLM/deployment preservation boundary, including the stricter project-Save rules.
        let save = built::check_native_scene(m, true).err();
        let export = built::check_native_scene(m, false).err();
        match (save, export) {
            (Some(e), Some(_)) => out.push(finding(m, game, Error, UnsupportedOperation, "locations.blm / deployment", "Save / native Export", e.to_string())),
            (Some(e), None) => out.push(finding(m, game, Error, UnsupportedOperation, "locations.blm / deployment", "Save", e.to_string())),
            (None, Some(e)) => out.push(finding(m, game, Error, UnsupportedOperation, "locations.blm / deployment", "native Export", e.to_string())),
            _ => {}
        }
        if has("locations.blm") && m.built_scene.is_none() {
            out.push(finding(m, game, Information, Limitation, "locations.blm", "scene editing", "No editable native BLM scene was captured. Original BLM bytes are retained; their unknown fields are not diagnosed as corruption."));
        }
    }
    check_props(d, roots, game, files, &mut out);
    out.sort_by_key(|f| f.severity);
    out
}

fn check_campaign_heights(m: &MapModel, game: &GameProfile, out: &mut Vec<Finding>) {
    use Category::{InvalidData, Limitation, UnsupportedOperation};
    use Severity::{Error, Information, Warning};
    let nodes = built::campaign_authority::authoring_nodes(m);
    if let Err(e) = &nodes {
        out.push(finding(m, game, Error, InvalidData, built::campaign_authority::PROJECT_STATE, "height authoring / Save / native Export", e.to_string()));
    }
    let authored = nodes.is_ok_and(|n| n > 0);
    if let Err(e) = built::check_campaign_preview(m) {
        out.push(finding(m, game, Error, UnsupportedOperation, "lf_height_map.dds", if authored { "native Export" } else { "Save / native Export" },
            if authored { "Effective campaign-height edits require native normal-map output that is not yet verified. Native Export is unsupported; use WCE authoring-project Save, subject to its existing baseline/profile checks.".to_owned() } else { e.to_string() }));
    } else if authored {
        out.push(finding(m, game, Warning, Limitation, built::campaign_authority::PROJECT_STATE, "native Export", "Height authoring state is present. An unchanged full-LF preview does not prove that all compiled height outputs are unchanged. This check does not certify native Export; its existing guard remains authoritative."));
    } else {
        out.push(finding(m, game, Information, Limitation, "campaign heights", "height authoring / native Export", "Unchanged native heights are preserved. Supported authoring and WCE project Save are separate from native Export: effective native campaign-height export remains restricted until normal-map output is verified. Authoring profile eligibility is checked when editing, not reconstructed by validation."));
    }
}

fn check_props(d: &MapDocument, roots: &[PathBuf], game: &GameProfile, files: Option<&dyn Files>, out: &mut Vec<Finding>) {
    use Category::{InvalidData, Limitation, UnsupportedOperation};
    use Severity::{Error, Information, Warning};
    let m = d.model().expect("checked model");
    let mut models: BTreeMap<String, String> = BTreeMap::new();
    if let Some(p) = d.project() {
        let resource = m.project.as_ref().map(|p| p.path.as_str()).unwrap_or(".terry");
        for e in &p.entities {
            if let wce_terrain::terry::EntityKind::Prop(model) = p.kind(e) {
                let id = format!("entity {:x}", e.id);
                if model.trim().is_empty() {
                    let mut f = finding(m, game, Warning, InvalidData, resource, "prop rendering / native Export", "Prop has no required ECMesh model_path reference.");
                    f.entity = Some(id); out.push(f);
                } else { models.entry(model).or_insert(id); }
            }
            for child in e.children.iter().filter(|&&id| p.entity(id).is_none()) {
                let mut f = finding(m, game, Warning, InvalidData, resource, "scene editing / native Export", format!("Child reference {child:x} has no entity in this scene."));
                f.entity = Some(format!("entity {:x}", e.id)); out.push(f);
            }
        }
        if m.kind == MapKind::Campaign && !m.built {
            out.push(finding(m, game, Information, Limitation, resource, "prop editing", "Terry campaign prop movement remains available. Landscape-bound movement recalculates native Y; its intended semantics remain an open question, not a confirmed defect."));
        }
    }
    if let Some(p) = &m.native_props {
        if let Err(e) = built::check_native_props(m) {
            out.push(finding(m, game, Error, UnsupportedOperation, &p.path, "Save / native Export", e.to_string()));
        }
        for id in 0..p.get().len() as u32 {
            if let Some(model) = p.get().model(id) { models.entry(model.into()).or_insert_with(|| format!("prop record {id}")); }
        }
        out.push(finding(m, game, Information, Limitation, &p.path, "prop editing", "Native prop records have lossless, unique model associations. Supported authoring is horizontal (X/Z) translation only; native Y is retained. Rotation, scale and record creation/deletion are unsupported."));
    } else if m.kind == MapKind::Campaign && m.built {
        let markers: Vec<_> = m.carried.iter().filter(|(p, _)| p.eq_ignore_ascii_case("prop_marker.markers")).collect();
        if markers.len() > 1 {
            out.push(finding(m, game, Warning, UnsupportedOperation, "prop_marker.markers", "prop editing", "Multiple case-insensitive marker resources make the editable source ambiguous. All originals are retained."));
        } else if let Some((path, blob)) = markers.first() {
            let reason = blob.bytes().map_err(|e| e.to_string()).and_then(wce_terrain::campaign_props::CampaignProps::parse);
            let text = match reason { Ok(_) => "Compatible marker bytes exist, but no editable source association was loaded. Reopen before editing.".into(), Err(e) => format!("Native props are view-only: {e}. Unsupported versions or ambiguous record/model associations are preserved; this is not a corruption diagnosis.") };
            out.push(finding(m, game, Information, Limitation, path, "prop editing", text));
        }
    }
    if let Some(files) = files {
        for (model, id) in models {
            if !files.exists(&model) && !roots.iter().any(|r| r.join(model.replace('\\', "/")).is_file()) {
                let mut f = finding(m, game, Warning, Limitation, &model, "prop rendering", "Referenced model is unavailable in the mounted game/project files and configured kit roots. Check the active packs and kit sources; the native reference is retained.");
                f.entity = Some(id); out.push(f);
            }
        }
    } else if !models.is_empty() {
        out.push(finding(m, game, Information, Limitation, "prop models", "validation", "No mounted asset lookup is available. Model-file availability was not checked."));
    }
}

/// The checks of a document (`roots`: the kits a placement's tile is looked for in, in their order).
fn check_kit(d: &MapDocument, roots: &[PathBuf]) -> Vec<Finding> {
    let mut out = vec![];
    let db = &d.db;
    let Some(m) = d.model() else {
        return out;
    };

    // Colours that name nothing: the fill leaves those cells empty.
    if let Some(l) = m.layer(&LayerRole::TileMap) {
        let known = |c: [u8; 4]| {
            c[..3] == [0, 0, 0] || db.sets.iter().any(|s| eq(s.colour, c)) || db.tiles.iter().any(|t| eq(t.colour, c) || t.variations.iter().any(|v| eq(v.colour, c)))
        };

        let unknown = unknown_colours(l.get(), known);
        if m.kind == MapKind::Campaign && !unknown.is_empty() {
            out.push(Finding { severity: Severity::Information, category: Category::Limitation, resource: l.path.clone(), affects: "tile authoring",
                text: format!("{} campaign palette colours are not named by the loaded tile database. Campaign background/shared-geometry semantics are not fully checked; this is not evidence of corrupt map data.", unknown.len()), ..Default::default() });
        }
        for (c, n, at) in unknown.into_iter().filter(|_| m.kind == MapKind::Battle) {
            out.push(Finding {
                severity: Severity::Error, cell: Some(at),
                text: format!("tile map: colour {}, {}, {} is no tile set, tile or variation of the database ({n} cells, the first at {}, {})", c[0], c[1], c[2], at.0, at.1),
                resource: l.path.clone(), affects: "editing / native Export", ..Default::default()
            });
        }
    }

    if let Some(l) = m.layer(&LayerRole::Climate) {
        let unknown = unknown_colours(l.get(), |c| db.climates.iter().any(|k| eq(k.colour, c)));
        if m.kind == MapKind::Campaign && !unknown.is_empty() {
            out.push(Finding { severity: Severity::Information, category: Category::Limitation, resource: l.path.clone(), affects: "climate authoring",
                text: format!("{} campaign climate colours have no exact database match. The current fill uses a fallback; campaign climate semantics are not certified by this check.", unknown.len()), ..Default::default() });
        }
        for (c, n, at) in unknown.into_iter().filter(|_| m.kind == MapKind::Battle) {
            out.push(Finding {
                severity: Severity::Warning, cell: Some(at),
                text: format!("climates: colour {}, {}, {} is no climate of the database ({n} cells, the first at {}, {}): its tiles take the first", c[0], c[1], c[2], at.0, at.1),
                resource: l.path.clone(), affects: "editing / native Export", ..Default::default()
            });
        }
    }

    // Cells the fill found no tile for.
    if m.kind == MapKind::Campaign && !d.fill_messages.is_empty() {
        out.push(Finding { severity: Severity::Warning, category: Category::Limitation, resource: "tile_map.png".into(), affects: "tile authoring / native Export",
            text: format!("The fill reported {} unfilled campaign positions. Shipped campaigns can also report unfilled positions; review tile painting before exporting. This alone does not establish corrupt resources.", d.fill_messages.len()), ..Default::default() });
    }
    for msg in d.fill_messages.iter().filter(|_| m.kind == MapKind::Battle) {
        let at = msg.rsplit(' ').next().and_then(|p| p.split_once('x')).and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)));
        out.push(Finding { severity: Severity::Error, cell: at, text: format!("the fill: {msg} (no tile of the painted set fits there)"), resource: "tile_map.png".into(), affects: "editing / native Export", ..Default::default() });
    }

    // Explicit tiles the database does not know, or off the map; lines BOB does not read.
    if let Some(x) = &m.explicit {
        for n in x.get().bad_lines() {
            out.push(Finding { severity: Severity::Error, cell: None, text: format!("explicit_tiles.txt line {n} is no x,y,tile,rotation (an empty last line too): BOB's Tilemap fails on it"), resource: x.path.clone(), affects: "native Export", ..Default::default() });
        }

        for t in x.get().tiles() {
            match crate::explicit_tiles::tile_size(db, &t.path) {
                None => out.push(Finding { severity: Severity::Error, cell: Some((t.x, t.y)), text: format!("explicit tile {} at {}, {}: not in the tile database (BOB skips it)", t.path, t.x, t.y), resource: x.path.clone(), affects: "native Export", ..Default::default() }),
                Some(s) => {
                    let r = crate::explicit_tiles::rect(t, s);
                    if m.kind == MapKind::Battle && (r[0] < 0 || r[1] < 0 || r[2] > i64::from(d.placed.width) || r[3] > i64::from(d.placed.height)) {
                        out.push(Finding { severity: Severity::Error, cell: Some((t.x, t.y)), text: format!("explicit tile {} at {}, {}: reaches off the map", t.path, t.x, t.y), resource: x.path.clone(), affects: "native Export", ..Default::default() });
                    }
                }
            }
        }
    }

    // Settlement and port battle catchment areas whose centre is on no settlement or port tile; deployment zones of no BCA.
    if let Some(p) = d.project() {
        for z in p.with_component("ECDeploymentZone") {
            let under_bca = p.parent_of(z.id).and_then(|b| p.entity(b)).is_some_and(|b| p.component(b, "ECBattleCatchmentArea").is_some());
            if !under_bca {
                out.push(Finding { severity: Severity::Warning, cell: None, text: format!("deployment zone {} ({:x}) belongs to no battle catchment area: BOB writes no deployment of it", z.name, z.id), resource: ".terry".into(), entity: Some(format!("entity {:x}", z.id)), affects: "native Export", ..Default::default() });
            }
        }

        let (w, h) = (f64::from(d.placed.width / 2), f64::from(d.placed.height / 2));
        let cell = wce_terrain_editor::document::TERRY_CELL;
        for e in p.with_component("ECBattleCatchmentArea") {
            if m.kind != MapKind::Battle { continue; }
            let wce_terrain::terry::EntityKind::Bca(t) = p.kind(e) else { continue };
            if !(t.starts_with("settlement") || t.contains("port")) {
                continue;
            }

            let pos = p.world_position(e.id);
            let (cx, cy) = ((pos[0] / cell + w).floor() as i32, (h - pos[2] / cell).floor() as i32);
            let on = d.placed.in_rect([cx, cy, cx + 1, cy + 1]).iter().any(|&i| {
                let l = d.placed.placements[i].location.to_lowercase();
                l.contains("settlement") || l.contains("port") || l.contains("city") || l.contains("town")
            });

            if !on {
                out.push(Finding {
                    severity: Severity::Warning, cell: Some((cx, cy)),
                    text: format!("{} ({t}) at cell {cx}, {cy}: on no settlement or port tile", e.name),
                    resource: ".terry".into(), entity: Some(format!("entity {:x}", e.id)), affects: "native Export", ..Default::default()
                });
            }
        }
    }

    // Tiles in no kit: the game has them only if a pack does.
    let mut used: BTreeMap<(String, String), (usize, (i32, i32))> = BTreeMap::new();
    for p in &d.placed.placements {
        used.entry((p.location.clone(), p.variation.clone())).or_insert((0, (p.x, p.y))).0 += 1;
    }

    for ((loc, var), (n, at)) in used {
        if wce_tile::sources::find_tile(roots, &loc, &var).is_none() {
            out.push(Finding {
                severity: Severity::Warning, cell: Some(at),
                text: format!("{loc}\\{var} ({n} placements, the first at {}, {}): its folder is in no kit (the packs' copy is drawn, if a pack has it)", at.0, at.1),
                resource: format!("{loc}/{var}"), affects: "editing / native Export", ..Default::default()
            });
        }
    }

    out.sort_by_key(|f| f.severity);
    out
}
