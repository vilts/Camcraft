//! The egui shell's demo document and per-frame scene collectors, graduated
//! from the M1.4.1 spike (world.rs) — the mirror of the retired app.rs's
//! private collectors, now the only collectors in the tree.
//!
//! `seed` builds what the panels and viewport show without a file dialog:
//! a 60x60x30 block, fitted stock, a 6 mm end mill from the catalog, and one
//! Contour operation picked on the block's top face - the same command walk
//! `crates/ui/tests/operations.rs` proves, through the same `State` the
//! imgui app runs. Nothing here pokes world state ad hoc: every change is a
//! `CommandKind` through `State::drain_actions`, so the egui shell exercises
//! the real validation, undo and staleness machinery (AGENTS.md: everything
//! is a command - the migration must not become an exception). Since M-d the
//! window does NOT seed at startup - it opens on a plain `State::new()`,
//! exactly like imgui's `App::new`; the ribbon's New/Model/Project/Save are
//! the real file ops (rfd dialog, shared command machinery). This seed is
//! what the frame harnesses and the headless proof call directly.
//!
//! `collect` builds the world-space `View3D` buffers: parts here (models,
//! stock, region fills - the same walk app.rs does), overlay by delegation
//! to `crate::interact` since M-e, the single law both shells draw from.
//! The gates are copied line for line, so a plane the imgui shell would
//! refuse to draw is a plane this shell does not draw either.
//! Provenance: our own code, mirrored from app.rs.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature, transform_point};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::tools::{Tool, catalog};

use crate::interact::ftoz;
use crate::state::{CommandKind, State, UiTarget};
use crate::viewport::GpuPart;

/// The shell's whole scene for one frame: what `View3D::set_document` and
/// `set_overlay` take.
pub struct Scene {
    pub parts: Vec<GpuPart>,
    pub overlay: Vec<[f32; 6]>,
}

impl Scene {
    /// Content digest for the upload gate: the scene is rebuilt every frame
    /// (it must track camera-independent world truth), but re-uploading
    /// unchanged vertex buffers is traffic for nothing. `GpuPart` derives
    /// nothing, so the digest walks the floats' bits - exact, no hash-skein.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for p in &self.parts {
            eat(u64::from(p.selected) | (u64::from(p.stock) << 1) | (u64::from(p.fill) << 2));
            eat(p.verts.len() as u64);
            for v in &p.verts {
                for f in v {
                    eat(u64::from(f.to_bits()));
                }
            }
            // The deviation classes are part of what the buffer holds -
            // a re-color must re-upload exactly like a re-mesh.
            eat(p.classes.len() as u64);
            for f in &p.classes {
                eat(u64::from(f.to_bits()));
            }
        }
        eat(!0);
        eat(self.overlay.len() as u64);
        for e in &self.overlay {
            for f in e {
                eat(u64::from(f.to_bits()));
            }
        }
        h
    }
}

/// Build the demo document: block, fitted stock, catalog tool, one Contour
/// op on the top face, selected and generated, with the Properties pane
/// open on it. Idempotent by construction - call once after `State::new`.
pub fn seed(state: &mut State) {
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("Block", cube(Point3::ZERO, Point3::new(60.0, 60.0, 30.0))),
    );
    let Some(tool) = catalog_tool("EM-0600-4FL") else {
        state.status = String::from("seed: catalog has no EM-0600-4FL - panels show an empty doc");
        return;
    };
    let Some(top) = top_face_pick(state, "Block") else {
        state.status = String::from("seed: the block has no top face - panels show an empty doc");
        return;
    };
    state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
    state.pending.push_back(CommandKind::AddTool { name: "EM-0600-4FL".into(), tool });
    // The seed's Contour bottoms out at the WCS (hand-test 4's
    // fixture law), so the WCS must exist before generate or the command is
    // refused by name. The block sits z 0..30; the offset at the world
    // origin puts Cut Z at the model bottom.
    state.pending.push_back(CommandKind::SetWcs {
        pos: [0.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Contour1".into(),
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(vec![top]),
        params: contour_params(),
    });
    // Generate through the command machinery, not a raw command: a refusal
    // would then land in the status strip exactly where a user would see it
    // (an ungenerated seed still shows every panel mechanism, minus the
    // green path - the honest degraded case).
    state.pending.push_back(CommandKind::GenerateOperation("Contour1".into()));
    state.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    state.settle();
    state.ui_target = UiTarget::Operation("Contour1".into());
    state.sync_ops_ui();
}

fn catalog_tool(name: &str) -> Option<Tool> {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t)
}

/// The top horizontal planar face as a pick (the test helper's law: face
/// normals are sign-canonical, so the signed offset alone tells top from
/// bottom).
fn top_face_pick(state: &State, name: &str) -> Option<GeoSel> {
    let entity = model::find_by_name(&state.world, name)?;
    let f = model::features_of(&state.world, &entity)?;
    let mut hs: Vec<_> = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
        .collect();
    hs.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let fa = hs.pop()?;
    Some(GeoSel { entity: name.to_string(), feature: FeatureSel::Face(fa.id) })
}

/// The seed's Contour draft - the numbers hand-test 4 exercises: clearance
/// anchored 25 mm above the stock top, retract above it, the full model
/// height stepped 5, one work-offset bottom, a straight lead-in.
fn contour_params() -> OpParams {
    OpParams::Contour { tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        retract_z: Height::of(HeightReference::TopOfStock, 10.0),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::WorkOffset, 0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// Rebuild the frame's scene from the world. Mirrors `App::sync_gpu`'s
/// per-frame work: parts from the document, overlay from selection and
/// draft. Returns BOTH and lets the caller's digest gate decide what to
/// upload.
#[must_use]
pub fn collect(state: &State) -> Scene {
    Scene { parts: collect_parts(state), overlay: collect_overlay(state) }
}

// ---------------------------------------------------------------- parts

fn world_verts(state: &State, entity: &flecs_ecs::prelude::Entity) -> Vec<[f32; 3]> {
    let tris = model::facets_of(&state.world, entity).unwrap_or_default();
    // #326: the part where the active setup holds it.
    let loc = camcraft_app::setup_align::view_pose(&state.world, entity).unwrap_or_default();
    let mut verts = Vec::with_capacity(tris.len() * 3);
    for t in tris.iter() {
        for p in [t.a, t.b, t.c] {
            let p = transform_point(p, loc.pos, loc.rot);
            verts.push([ftoz(p.x), ftoz(p.y), ftoz(p.z)]);
        }
    }
    verts
}

/// Models (minus browser-eye-hidden), then the stock solid, then the
/// translucent cut-region fills - `App::collect_parts`'s exact law, fills
/// included so the region reads as an area, not only an outline.
fn collect_parts(state: &State) -> Vec<GpuPart> {
    // #339 slice 4: the fixtures the selected op's warnings strike are
    // tinted (display only - the classes ride the vertex, nothing else).
    let struck = state.fixture_highlight();
    let mut parts: Vec<GpuPart> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(name, _)| !state.hidden_models.iter().any(|h| h == name))
        .map(|(name, entity)| {
            let verts = world_verts(state, &entity);
            let classes =
                if struck.contains(&name) { vec![crate::viewport::STRUCK_CLASS; verts.len()] } else { Vec::new() };
            GpuPart { verts, selected: model::is_selected(&state.world, &entity), stock: false, fill: false, classes }
        })
        .collect();
    parts.extend(sim_body_part(state));
    if !state.active_stock_hidden() {
        match (&state.program_sim, crate::machined::mesh(state)) {
            // The machined stock's extracted mesh (M13.1 owed) takes the
            // slot when the whole-program field view has one: the closed
            // shell the program leaves, through the stock pipeline like the
            // box it replaces. Judged per facet by the eye's law (#25,
            // `machined::vertex_classes`; plain material until judged), and
            // the GPU eye, when it traces, still paints over the slot.
            (_, Some(mesh)) => parts.push(GpuPart {
                verts: mesh.iter().flat_map(|t| [t.a, t.b, t.c].map(|p| [ftoz(p.x), ftoz(p.y), ftoz(p.z)])).collect(),
                selected: false,
                stock: true,
                fill: false,
                classes: crate::machined::vertex_classes(state, mesh.len()),
            }),
            // The Deviate sheet takes the stock's slot in the draw order:
            // it IS the stock's surface after the program, judged - same
            // blend law, and the solid it replaces would only veil it.
            (Some(ps), None) => parts.push(GpuPart {
                verts: ps
                    .surface
                    .verts
                    .iter()
                    .map(|p| [ftoz(p.x), ftoz(p.y), ftoz(p.z)])
                    .collect(),
                selected: false,
                stock: true,
                fill: false,
                classes: ps.surface.vcls.clone(),
            }),
            (None, None) => {
                // The drawn block is the ACTIVE setup's stock, resolved by the
                // same law the tree row and the pick use; a setup-free document
                // resolves to the legacy "Stock" name and draws exactly as before.
                let stock_name = camcraft_app::setup::stock_entity_name(
                    &state.world,
                    camcraft_app::setup::active(&state.world).as_deref(),
                );
                if let Some(entity) = model::find_by_name(&state.world, &stock_name)
                    && model::is_stock(&state.world, &entity)
                {
                    parts.push(GpuPart {
                        verts: world_verts(state, &entity),
                        selected: model::is_selected(&state.world, &entity),
                        stock: true,
                        fill: false,
                        classes: Vec::new(),
                    });
                }
            }
        }
    }
    // #406: the stock a carried setup starts from, while its settings
    // page is open - a stock-slot part painted teal (`INCOMING_CLASS`),
    // apart from the setup's own Stock node.
    if let Some(mesh) = crate::incoming_view::mesh(state) {
        let verts: Vec<[f32; 3]> =
            mesh.iter().flat_map(|t| [t.a, t.b, t.c].map(|p| [ftoz(p.x), ftoz(p.y), ftoz(p.z)])).collect();
        let classes = vec![crate::incoming_view::INCOMING_CLASS; verts.len()];
        parts.push(GpuPart { verts, selected: false, stock: true, fill: false, classes });
    }
    for (z, region) in crate::interact::shown_regions(state) {
        let Some(region) = region else { continue };
        let Ok(tris) = region.triangles() else { continue };
        if tris.is_empty() {
            continue;
        }
        let mut verts = Vec::with_capacity(tris.len() * 3);
        for t in &tris {
            for p in t {
                verts.push([ftoz(p.x), ftoz(p.y), ftoz(z)]);
            }
        }
        parts.push(GpuPart { verts, selected: false, stock: false, fill: true, classes: Vec::new() });
    }
    // The Boundary group's preview of the area the draft machines (M11.5
    // UX), filled like the committed region while the group is open.
    if let Some((z, region)) = crate::boundary_viz::preview_fill(state)
        && let Ok(tris) = region.triangles()
        && !tris.is_empty()
    {
        let verts = tris.iter().flat_map(|t| t.iter().map(|p| [ftoz(p.x), ftoz(p.y), ftoz(z)])).collect();
        parts.push(GpuPart { verts, selected: false, stock: false, fill: true, classes: Vec::new() });
    }
    parts
}

/// #371, #743: the running tool at the playhead as a revolved body -
/// the cutter and shank of the judged tool record and, above them, the
/// very band stack the fixture laws sweep (the holder cylinder, a
/// modelled body's revolved envelope, a drawn holder's bands), so what
/// is drawn is what is checked - posed by the frame's tip and axis,
/// world space. Only while the transport is up and its timeline is this
/// document's (`cached_script` refreshes it before the next frame), any
/// playhead including 0 and the end, playing or paused, the op fresh or
/// stale (display reads the cached path). A lathe-held body (#1038) is
/// drawn as modelled, posed in the holder law's turning frame; a lathe
/// insert without one yields nothing (marker only); a degenerate or
/// misfitting tool yields nothing and is logged. The mesh lives in `State::sim_tool_cache` and is
/// dropped as soon as there is no tool to draw.
pub(crate) fn sim_tool_tris(state: &State) -> Option<Vec<camcraft_app::geometry::Triangle>> {
    // A poisoned lock only means a draw panicked mid-update: the cache
    // is display data, rebuilt from the world, so it is safe to reuse.
    let mut cache = state.sim_tool_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((op, tip, axis)) = sim_tool_pose(state) else {
        cache.clear();
        return None;
    };
    cache.posed(&state.world, op, tip, axis)
}

/// The operation at the playhead with its tool tip and axis, when the
/// transport is up on this document's timeline.
fn sim_tool_pose(state: &State) -> Option<(&str, [f64; 3], [f64; 3])> {
    let sim = state.sim?;
    let (version, sc) = state.sim_script_cache.as_ref()?;
    if *version != state.doc_version || sc.is_empty() {
        return None;
    }
    let f = sc.at(sim.t);
    let op = sc.ops.get(f.op?)?;
    Some((op.as_str(), [f.pos.x, f.pos.y, f.pos.z], f.axis))
}

/// [`sim_tool_tris`] as a model part (depth-tested, shaded).
fn sim_body_part(state: &State) -> Option<GpuPart> {
    let tris = sim_tool_tris(state)?;
    let verts = tris.iter().flat_map(|t| [t.a, t.b, t.c].map(|p| [ftoz(p.x), ftoz(p.y), ftoz(p.z)])).collect();
    Some(GpuPart { verts, selected: false, stock: false, fill: false, classes: Vec::new() })
}

// -------------------------------------------------------------- overlay

/// The overlay is the SHARED law since M-e: `crate::interact::collect_overlay`
/// — cached paths with chevrons, region outlines, draft height planes,
/// committed-pick outlines, and the gizmos, built by the same functions the
/// imgui shell uploads. (The pre-M-e mirror skipped the gizmos and the
/// `show_all_paths` dimming; this delegation closes both gaps for free.)
///
/// The Z=0 ground grid goes FIRST, so a path-heavy document that fills the
/// overlay buffer loses toolpath detail at the cap, never the floor: the
/// grid is what makes the empty viewport read as a machine table rather
/// than a void, and it is depth-tested against the part like the rest of
/// the buffer.
fn collect_overlay(state: &State) -> Vec<[f32; 6]> {
    let mut out = crate::interact::ground_grid_lines(state);
    out.extend(crate::interact::collect_overlay(state));
    crate::interact::capped(out)
}
