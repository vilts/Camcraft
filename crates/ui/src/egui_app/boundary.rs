//! The operation page's Boundary group (M11.5; reworked on the user's
//! hand-tests - "unclear what open boundary affects .. unclear what
//! boundary actually does"): a plain-language summary of what the draft
//! does (`crate::boundary_summary`), the notices for settings this kind
//! ignores, then the Tool group (containment, offset, corner style, the
//! extension past exterior edges - each with its hover hint) and the
//! Boundaries group: ONE list of entries, each with a role dropdown
//! (Include / Exclude / Open - machine side A / B), what its picks spell,
//! which side an open boundary machines (in viewport words, with a one-
//! click flip) and pick / close / remove buttons; roles the shape cannot
//! take are greyed with the reason. It sits on the Geometry tab, under
//! the picks, because it answers the same question the selection does -
//! where the tool may go. Every field edits the page's draft
//! (`OpUi::area`, the law in `boundary_draft`); Apply commits it with the
//! parameters as one undoable edit. Viewport picks for an entry are
//! routed there by `State::boundary_pick` - transient view state, never
//! a command.
//!
//! Provenance: our own UI; no vendor material consulted.

use egui::{RichText, Ui};

use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, Containment, CornerStyle, ExtraBoundary, PickShape, area_unsupported,
};

use crate::boundary_draft::{self, Entry, EntryRef, Role};
use crate::boundary_summary;
use crate::shortcut_keys::command_key;
use crate::state::{BoundaryPick, GeoPick, State};

use super::colorvision::{ink, palette_of, text_inks};
use super::fields::{combo, num};
use super::layout::{auto_value, consequence, group, group_open, hint, prop_row, prop_row_hint};

/// One line each: what the roles do and how to pick (the user's "very
/// unclear how the open boundary selection works") - the hover of the
/// Boundaries group's rows.
/// The picking line names the platform command key (Cmd on macOS), the
/// one the pick reads (`mods.command`, #1238).
pub fn hints() -> [String; 4] {
    [
        "Include adds the area a closed boundary encloses; Exclude keeps the whole tool out of it.".to_owned(),
        "Open: an open edge chain runs on straight to the stock and splits it - side A holds more of the selection's area, side B is the rest; arrows in the viewport point into the side machined.".to_owned(),
        format!(
            "Picking: click an edge to take that one edge, {}-click to add or remove one (removing breaks a picked loop open), Shift-click to close the edges around their face loop, Alt for a single mesh segment; a face adds its loop.",
            command_key()
        ),
        "The filled area in the viewport is what the operation will machine after Apply.".to_owned(),
    ]
}

const CONTAINMENT_HINT: &str = "Inside: the tool's edge touches the boundary from inside (its centre a tool radius in). On: the tool centre rides the boundary. Outside: the tool's edge touches it from outside (its centre a tool radius out). As the kind cuts: the kind's own law.";
const OFFSET_HINT: &str = "+ grows the area outward, - shrinks it inward, before the tool containment applies.";
const CORNER_HINT: &str = "How the offset treats convex corners: Round arcs them, Sharp keeps them square.";
const EXTEND_HINT: &str = "Carries the area past its exterior edges - edges with no part material beyond them - by this distance. An open boundary is never crossed.";

/// Draw the group for the open operation's draft; a kind without a
/// machining area says why instead of drawing controls it would ignore
/// - the app's one list (`area_unsupported`), never a UI copy.
pub(super) fn boundary_group(ui: &mut Ui, state: &mut State) {
    if let Some(why) = area_unsupported(state.op_ui.params.kind()) {
        state.boundary_pick = None;
        ui.label(RichText::new(format!("Boundary settings do not apply to it: {why}.")).weak());
        return;
    }
    let u = state.units();
    let set = !state.op_ui.area.is_default();
    let r = group_open(ui, "Boundary", None, set, |ui| body(ui, state, u));
    let open = r.inner.is_some();
    state.boundary_view.shown = open;
    if !open {
        state.boundary_pick = None;
    }
}

fn body(ui: &mut Ui, state: &mut State, u: camcraft_app::units::Unit) {
    boundary_draft::warm(state);
    boundary_draft::refresh_preview(state);
    let s = boundary_summary::summarize(state);
    let inks = text_inks(ui, &state.world);
    for (k, line) in s.lines.iter().enumerate() {
        let t = RichText::new(line);
        ui.add(egui::Label::new(if k == 0 { t.strong() } else { t }).wrap());
    }
    for n in &s.notices {
        ui.colored_label(inks.warn, n);
    }
    if let Some(p) = &state.boundary_view.preview
        && let Some(why) = &p.refusal
    {
        ui.colored_label(inks.warn, why);
    }
    ui.add_space(4.0);
    let a = &mut state.op_ui.area;
    group(ui, "Tool", None, |ui| {
        let mut c = match a.containment {
            None => 0,
            Some(Containment::Inside) => 1,
            Some(Containment::On) => 2,
            Some(Containment::Outside) => 3,
        };
        prop_row_hint(ui, "Containment", CONTAINMENT_HINT, |ui| {
            combo(ui, "area_containment", &mut c, &["As the kind cuts", "Inside", "Centre on", "Outside"])
        });
        a.containment = match c {
            1 => Some(Containment::Inside),
            2 => Some(Containment::On),
            3 => Some(Containment::Outside),
            _ => None,
        };
        prop_row_hint(ui, "Boundary offset", OFFSET_HINT, |ui| num(ui, u, &mut a.offset));
        let mut corner = usize::from(a.corner == CornerStyle::Sharp);
        prop_row_hint(ui, "Offset corners", CORNER_HINT, |ui| combo(ui, "area_corner", &mut corner, &["Round", "Sharp"]));
        a.corner = if corner == 1 { CornerStyle::Sharp } else { CornerStyle::Round };
        auto_value(ui, "Extend past open edges", "Off", &mut a.extension, "off", 2.0, |ui, v| num(ui, u, v));
        hint(ui, EXTEND_HINT);
        if a.extension < 0.0 {
            a.extension = 0.0;
        }
    });

    let mut acts: Vec<Act> = Vec::new();
    group(ui, "Boundaries", None, |ui| {
        let entries = boundary_draft::entries(state);
        if entries.is_empty() {
            ui.label("None - the selection bounds the area.");
            hint(ui, &hints()[0]);
        }
        for (n, e) in entries.iter().enumerate() {
            entry_row(ui, state, n, e, &mut acts);
        }
        add_row(ui, state, &mut acts);
        collecting_line(ui, state, &mut acts);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("(i) roles, picking and the fill").small().weak())
                .on_hover_text(hints().join("\n\n"));
        });
    });
    if ui.small_button("Reset boundary").on_hover_text("every Boundary setting back to the kind's own law (Apply commits it)").clicked() {
        acts.push(Act::Reset);
    }
    for act in acts {
        apply(state, act);
    }
}

/// What a click asked for, applied after the draw (the rows read the
/// draft while they draw).
enum Act {
    Role(EntryRef, Role),
    Pick(Option<BoundaryPick>),
    Extend(EntryRef),
    Remove(EntryRef),
    Add(ExtraBoundary),
    Reset,
}

fn apply(state: &mut State, act: Act) {
    let said = match act {
        Act::Role(at, role) => boundary_draft::set_role(state, at, role).map(|()| format!("boundary is now {}", role.label())),
        Act::Pick(t) => {
            if t.is_some() && state.geo_pick == GeoPick::Model {
                state.geo_pick = GeoPick::Edge;
            }
            state.boundary_pick = t;
            Ok(if t.is_some() { "click edges or faces in the viewport".to_owned() } else { "boundary picking stopped".to_owned() })
        }
        Act::Extend(at) => boundary_draft::extend(state, at).map(|()| "closed around its face loop".to_owned()),
        Act::Remove(at) => {
            boundary_draft::remove(state, at);
            Ok("boundary removed from the draft".to_owned())
        }
        Act::Add(b) => {
            state.op_ui.area.extra.push(b);
            Ok("boundary added to the draft".to_owned())
        }
        Act::Reset => {
            state.op_ui.area = AreaControls::default();
            state.boundary_pick = None;
            Ok("Boundary settings reset in the draft".to_owned())
        }
    };
    state.status = match said {
        Ok(s) => format!("{s} - Apply commits it"),
        Err(why) => format!("boundary not changed: {why}"),
    };
}

fn role_rgb(state: &State, role: Role) -> egui::Color32 {
    let p = palette_of(&state.world);
    ink(match role {
        Role::Include => p.bound_include,
        Role::Exclude => p.bound_exclude,
        Role::OpenA | Role::OpenB => p.bound_open,
    })
}

fn entry_row(ui: &mut Ui, state: &State, n: usize, e: &Entry, acts: &mut Vec<Act>) {
    let col = role_rgb(state, e.role);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{}.", n + 1)).color(col).strong());
        egui::ComboBox::from_id_salt(("area_role", n)).selected_text(RichText::new(e.role.label()).color(col)).show_ui(ui, |ui| {
            for role in Role::ALL {
                let block = boundary_draft::role_block(state, e, role);
                let r = ui.add_enabled(block.is_none(), egui::Button::selectable(role == e.role, role.label()));
                if let Some(why) = block {
                    r.on_disabled_hover_text(why);
                } else if r.clicked() && role != e.role {
                    acts.push(Act::Role(e.at, role));
                }
            }
        });
        ui.label(&e.summary);
    });
    if e.role.is_open() {
        // The side is the user's choice: said in viewport words, with the
        // one-click flip beside it.
        consequence(ui, &boundary_summary::side_caption(state, e.role));
        let other = if e.role == Role::OpenA { Role::OpenB } else { Role::OpenA };
        prop_row(ui, "Machined side", |ui| {
            if ui.small_button("Machine the other side").on_hover_text(format!("switch to {}", other.label())).clicked() {
                acts.push(Act::Role(e.at, other));
            }
            ui.label(if e.role == Role::OpenA { "A" } else { "B" });
        });
    }
    ui.horizontal_wrapped(|ui| {
        if !e.picks.is_empty() {
            let target = boundary_draft::target_of(e.at);
            let on = state.boundary_pick == Some(target);
            if super::pick_button::pick_button(ui, on, "Pick").on_hover_text("route viewport clicks into this boundary").clicked() {
                acts.push(Act::Pick(if on { None } else { Some(target) }));
            }
        }
        if matches!(e.shape, Some(PickShape::Open { .. })) && ui.small_button("Extend to loop").on_hover_text("close the picked edges around their face loop").clicked() {
            acts.push(Act::Extend(e.at));
        }
        if !e.role.is_open() && matches!(e.shape, Some(PickShape::Open { .. })) {
            let block = boundary_draft::role_block(state, e, Role::OpenA);
            let r = ui.add_enabled(block.is_none(), egui::Button::new("Use as open boundary").small());
            if let Some(why) = block {
                r.on_disabled_hover_text(why);
            } else if r.clicked() {
                acts.push(Act::Role(e.at, Role::OpenA));
            }
        }
        if ui.small_button("Remove").clicked() {
            acts.push(Act::Remove(e.at));
        }
    });
    if let Some(issue) = &e.issue {
        ui.colored_label(text_inks(ui, &state.world).warn, issue);
    }
}

fn add_row(ui: &mut Ui, state: &State, acts: &mut Vec<Act>) {
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("+ Pick a boundary").on_hover_text("starts an Include entry - change its role after picking").clicked() {
            acts.push(Act::Pick(Some(BoundaryPick::Include)));
        }
        let has_open = state.op_ui.area.open.is_some();
        let r = ui.add_enabled(!has_open, egui::Button::new("+ Pick an open boundary").small());
        if has_open {
            r.on_disabled_hover_text("an operation has one open boundary - use its Pick button to re-pick it");
        } else if r.on_hover_text("pick the edge chain the operation machines up to").clicked() {
            acts.push(Act::Pick(Some(BoundaryPick::Open)));
        }
        if ui.small_button("+ Stock outline").on_hover_text("include the setup stock's outline").clicked() {
            acts.push(Act::Add(ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Stock }));
        }
        if ui.small_button("+ Part silhouette").on_hover_text("keep the tool out of the part's silhouette").clicked() {
            acts.push(Act::Add(ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Silhouette }));
        }
    });
}

fn collecting_line(ui: &mut Ui, state: &State, acts: &mut Vec<Act>) {
    let Some(t) = state.boundary_pick else { return };
    let what = match t {
        BoundaryPick::Include => "a new boundary".to_owned(),
        BoundaryPick::Exclude => "a new exclude boundary".to_owned(),
        BoundaryPick::Open => "the open boundary".to_owned(),
        BoundaryPick::Extra(i) => format!("boundary {}", i + 1),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Picking into {what} - viewport clicks fill it, not the selection.")).strong());
        if ui.small_button("Done").clicked() {
            acts.push(Act::Pick(None));
        }
    });
}

#[cfg(test)]
mod frame {
    //! The group drawn for real (egui_kittest): the one list with its
    //! summaries, the one-click way out of an open include, the hints.
    use super::*;
    use camcraft_app::geometry::{Point3, cube};
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::{commands::ImportModel, model, tools::catalog};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use crate::state::{CommandKind, OpUi};

    fn state_with_open_include() -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
        state.pending.push_back(CommandKind::AddTool { name: "EM-0400-2FL".into(), tool });
        let params = OpParams::Face { holes: Default::default(),
            blend_turns: false,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(10.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        state.pending.push_back(CommandKind::CreateOperation {
            name: "Face1".into(),
            tool: "EM-0400-2FL".into(),
            src: SrcRef::Stock,
            params,
        });
        state.drain_actions();
        let part = ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
        state.history.execute(&state.world, part);
        let op = model::find_by_name(&state.world, "Face1").and_then(|e| model::operation_of(&state.world, &e)).expect("op");
        state.op_ui = OpUi::from_op("Face1", &op);
        let f = model::find_by_name(&state.world, "part").and_then(|e| model::features_of(&state.world, &e)).expect("features");
        let i = f
            .edges
            .iter()
            .position(|t| {
                let (a, b) = (f.verts[t.a as usize], f.verts[t.b as usize]);
                a[2] == 10.0 && b[2] == 10.0 && a[1] == 20.0 && b[1] == 20.0
            })
            .expect("the north rim edge");
        let edge = camcraft_app::operation::run_pick(&state.world, "part", i as u32).expect("its run");
        state.op_ui.area.extra.push(ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Picks(vec![edge]) });
        state.op_ui.area.extra.push(ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Stock });
        state
    }

    #[test]
    fn the_group_lists_each_boundary_with_its_shape_and_the_way_out() {
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(560.0, 1000.0))
            .build_ui_state(|ui, state: &mut State| boundary_group(ui, state), state_with_open_include());
        harness.run();
        assert!(harness.state().boundary_view.shown, "a set group opens");
        assert!(harness.query_by_label("1 edge, open chain").is_some());
        assert!(harness.query_by_label("stock outline, closed").is_some());
        assert!(harness.query_by_label("Extend to loop").is_some());
        assert!(harness.query_by_label("(i) roles, picking and the fill").is_some(), "the hints ride the (i) hover");
        assert!(harness.query_by_label_contains("Faces the stock top").is_some(), "the summary heads the group");
        harness.get_by_label("Use as open boundary").click();
        harness.run();
        let a = &harness.state().op_ui.area;
        assert_eq!(a.extra.len(), 1, "the open chain left the includes");
        assert!(a.open.is_some(), "and became the open boundary");
        assert!(harness.state().status.contains("Open - machine side A"), "{}", harness.state().status);
    }
}
