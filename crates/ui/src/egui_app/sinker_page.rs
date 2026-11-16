//! The operation page: the Sinker Burn's fields (#541) - the burn point
//! and the machine condition on Geometry, the approach plane, work top
//! and cavity floor datum on Heights, the datum and depth on Passes, the
//! servo feed on Feeds, the orbit (#544), the layer step and the
//! electrode wear compensation (#546) on Passes - or, for the Side
//! Burn (#545), its wall, amount and undersize there in place of the pass
//! and orbit, with the same layer and wear rows (#1340). Every value edits the page draft; Apply rides
//! the command history (EditOperationParams, undoable). The full sinker
//! setup and op pages are #548's.
//!
//! Provenance: our own UI over our own sinker parameters (general
//! die-sink EDM practice); no vendor text.

use super::fields::*;
use camcraft_app::operation::{
    SinkerBurnOp, SinkerDatum, SinkerOrbit, SinkerOrbitKind, SinkerPass, SinkerSideBurnOp, SinkerSideDirection,
};

const PASSES: [&str; 3] = ["Single electrode", "Rough (undersized)", "Finish (to size)"];
const DATUMS: [&str; 3] = ["Work top (depth down)", "Cavity floor (stand-off up)", "Electrode tip (down from approach)"];
const ORBITS: [&str; 4] = ["None (plain plunge)", "Circular", "Vector", "Planetary"];
const SIDES: [&str; 4] = ["+X wall", "-X wall", "+Y wall", "-Y wall"];

/// The side burn's rows (#545): the wall it burns, the amount and the
/// electrode's undersize per side.
fn side_rows(ui: &mut Ui, u: Unit, direction: &mut SinkerSideDirection, amount: &mut f64, undersize: &mut f64) {
    let mut si = SinkerSideDirection::ALL.iter().position(|d| d == direction).unwrap_or(0);
    prop_row(ui, "Burn wall", |ui| combo(ui, "sinker.side", &mut si, &SIDES));
    *direction = SinkerSideDirection::ALL.get(si).copied().unwrap_or_default();
    prop_row(ui, "Wall moves by", |ui| num(ui, u, amount));
    prop_row(ui, "Electrode undersize", |ui| num(ui, u, undersize));
    hint(ui, "per side; the move may not pass the undersize less the condition's spark gap - a move into the part is refused");
}

/// The pass rows (#543): single electrode, or the rough or finish of an
/// undersized pair with the finished cavity it is judged against. A
/// plain Sinker Burn's only - the side burn sizes its own electrode.
fn pass_rows(ui: &mut Ui, u: Unit, pass: &mut SinkerPass, cavity_x: &mut f64, cavity_y: &mut f64) {
    let mut pi = match pass {
        SinkerPass::Single => 0,
        SinkerPass::Rough => 1,
        SinkerPass::Finish => 2,
    };
    prop_row(ui, "Electrode pass", |ui| combo(ui, "sinker.pass", &mut pi, &PASSES));
    *pass = match pi {
        1 => SinkerPass::Rough,
        2 => SinkerPass::Finish,
        _ => SinkerPass::Single,
    };
    if *pass != SinkerPass::Single {
        prop_row(ui, "Finished cavity X", |ui| num(ui, u, cavity_x));
        prop_row(ui, "Finished cavity Y", |ui| num(ui, u, cavity_y));
        hint(ui, "the condition's undersize sizes the electrode; a finish must burn exactly this size, or it is refused - burn a rough or finish without an orbit");
    }
}

/// The orbit rows (#544): the pattern, its radius and its count. A
/// pattern picked fresh starts at a 0.1 mm radius, 4 vectors, 1 turn.
fn orbit_rows(ui: &mut Ui, u: Unit, orbit: &mut Option<SinkerOrbit>) {
    let mut oi = match orbit.map(|o| o.kind) {
        None => 0,
        Some(SinkerOrbitKind::Circular) => 1,
        Some(SinkerOrbitKind::Vector) => 2,
        Some(SinkerOrbitKind::Planetary) => 3,
    };
    prop_row(ui, "Orbit", |ui| combo(ui, "sinker.orbit", &mut oi, &ORBITS));
    let kind = match oi {
        1 => SinkerOrbitKind::Circular,
        2 => SinkerOrbitKind::Vector,
        3 => SinkerOrbitKind::Planetary,
        _ => {
            *orbit = None;
            return;
        }
    };
    let o = orbit.get_or_insert(SinkerOrbit { kind, radius: 0.1, vectors: 4, turns: 1 });
    o.kind = kind;
    prop_row(ui, "Orbit radius", |ui| num(ui, u, &mut o.radius));
    if kind == SinkerOrbitKind::Vector {
        prop_row(ui, "Vectors", |ui| ui.add(DragValue::new(&mut o.vectors).range(1..=camcraft_app::sinker_orbit::MAX_VECTORS)));
    }
    if kind == SinkerOrbitKind::Planetary {
        prop_row(ui, "Turns", |ui| ui.add(DragValue::new(&mut o.turns).range(1..=camcraft_app::sinker_orbit::MAX_TURNS)));
    }
    hint(ui, "the machine must list the pattern and state a maximum orbit radius - otherwise the burn is refused");
}

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let (p, side) = match params {
        OpParams::SinkerBurn(p) => (p, None),
        // #545: the side burn's plunge reads the burn's rows.
        OpParams::SinkerSideBurn(SinkerSideBurnOp { burn, direction, amount, undersize }) => (burn, Some((direction, amount, undersize))),
        _ => return,
    };
    // The wear line reads the draft before its fields are borrowed (#546);
    // a side burn reads its measured wear only (#1340).
    let is_side = side.is_some();
    let wear = if is_side {
        camcraft_app::operation::sinker_wear_op::side_wear_status(p)
    } else {
        camcraft_app::operation::sinker_wear_op::wear_status(world, op_ctx.as_deref(), p)
    };
    let SinkerBurnOp { clearance_z, top_z, floor_z, datum, depth, x, y, feed, condition, pass, cavity_x, cavity_y, orbit, layer, wear_lost, wear_over } = p;
    if tab == SEC_GEO {
        prop_row(ui, "Burn at X", |ui| num(ui, u, x));
        prop_row(ui, "Burn at Y", |ui| num(ui, u, y));
        prop_row(ui, "EDM condition", |ui| ui.add(egui::TextEdit::singleline(condition).desired_width(90.0)));
        hint(ui, "the machine's own condition id - its spark gap places the electrode tip above the floor");
        condition_readout(ui, u, world, op_ctx.as_deref(), condition);
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Approach plane", world, op_ctx.as_deref(), clearance_z, "sinker.clearance");
        height_editor(ui, "Work top", world, op_ctx.as_deref(), top_z, "sinker.top");
        height_editor(ui, "Cavity floor datum", world, op_ctx.as_deref(), floor_z, "sinker.floor");
        hint(ui, "no burn passes the cavity floor datum or the model's floor - such a depth is refused");
    }
    if tab == SEC_PASSES {
        let mut di = match datum {
            SinkerDatum::WorkTop => 0,
            SinkerDatum::CavityFloor => 1,
            SinkerDatum::ElectrodeTip => 2,
        };
        prop_row(ui, "Depth from", |ui| combo(ui, "sinker.datum", &mut di, &DATUMS));
        *datum = match di {
            1 => SinkerDatum::CavityFloor,
            2 => SinkerDatum::ElectrodeTip,
            _ => SinkerDatum::WorkTop,
        };
        prop_row(ui, "Depth", |ui| num(ui, u, depth));
        match side {
            Some((direction, amount, undersize)) => side_rows(ui, u, direction, amount, undersize),
            None => {
                pass_rows(ui, u, pass, cavity_x, cavity_y);
                orbit_rows(ui, u, orbit);
            }
        }
        auto_opt(ui, "Layer step", "One layer", layer, "the whole depth at once", 1.0, |ui, v| num(ui, u, v));
        auto_opt(ui, "Measured wear", "None", wear_lost, "not measured", 0.05, |ui, v| num(ui, u, v));
        auto_opt(ui, "Measured over", "None", wear_over, "not measured", 5.0, |ui, v| num(ui, u, v));
        hint(ui, &wear);
        hint(ui, if is_side {
            "the electrode wear measured between burns (length lost over depth burned) deepens the plunge; the lateral move runs at the last compensated tip; with none the side burn is not compensated"
        } else {
            "the electrode wear measured between burns (length lost over depth burned) wins over the condition's stated rate; with neither the burn is not compensated"
        });
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Servo feed", |ui| num_feed(ui, u, feed));
    }
}

/// What the mounted machine states for the typed condition (#548):
/// its spark gap and electrode undersize, or "not stated" - the page
/// shows what the generator will read, so a refusal is no surprise.
fn condition_readout(ui: &mut Ui, u: Unit, world: &World, setup: Option<&str>, id: &str) {
    let m = camcraft_app::machine::of_setup(world, setup).1;
    let cond = m.as_ref().and_then(|m| m.edm.as_ref()).and_then(|e| e.conditions.iter().find(|c| c.id == id));
    let word = |v: Option<f64>| v.map_or_else(|| "not stated".to_owned(), |x| format!("{} {}", units::len(x, u, 3), u.suffix()));
    prop_row(ui, "Condition gap", |ui| ui.label(word(cond.and_then(|c| c.gap))));
    prop_row(ui, "Condition undersize", |ui| ui.label(word(cond.and_then(|c| c.undersize))));
    if cond.is_none() {
        hint(ui, "the mounted machine states no condition with this id - the op refuses until it does");
    }
}
