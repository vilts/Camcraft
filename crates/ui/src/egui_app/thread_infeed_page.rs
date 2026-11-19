//! The Threading page's infeed plan rows (issue #416): how the passes
//! enter (radial, flank, alternating), the depth schedule, the minimum
//! depth, finish allowance and spring passes, and the repeat of each
//! roughing depth (issue #418), and the in-pass chip-break dips (issue
//! #419). Every value edits the
//! page draft; Apply rides EditOperationParams, whose threading door
//! refuses a plan the insert cannot cut, by name.
//!
//! Provenance: our own UI over the operation parameter model; the
//! infeed vocabulary from public tooling-maker threading guides
//! (behaviour only). No vendor material or text.

use camcraft_app::turning::{DepthSchedule, InfeedMethod, ThreadInfeed};

use super::fields::*;

const METHODS: [&str; 3] = ["Radial", "Flank", "Alternating flank"];
const SCHEDULES: [&str; 2] = ["Constant depth", "Constant area"];

pub(super) fn rows(ui: &mut Ui, u: Unit, plan: &mut ThreadInfeed) {
    let mut m = match plan.method {
        InfeedMethod::Radial => 0,
        InfeedMethod::Flank => 1,
        InfeedMethod::Alternating => 2,
    };
    prop_row(ui, "Infeed method", |ui| combo(ui, "threading.infeed_method", &mut m, &METHODS));
    plan.method = match m {
        1 => InfeedMethod::Flank,
        2 => InfeedMethod::Alternating,
        _ => InfeedMethod::Radial,
    };
    hint(ui, "radial plunges square to the axis; flank steps each deeper pass along one flank so one edge cuts; alternating flips the flank every pass");
    if plan.method != InfeedMethod::Radial {
        prop_row(ui, "Flank angle", |ui| num_unit(ui, &mut plan.flank_angle, "°"));
        hint(ui, "compound angle off the radial, at most half the insert's included angle - a little under it (29.5 for 60°) lets the trailing edge skim");
    }
    let mut s = match plan.schedule {
        DepthSchedule::ConstantDepth => 0,
        DepthSchedule::ConstantArea => 1,
    };
    prop_row(ui, "Depth schedule", |ui| combo(ui, "threading.schedule", &mut s, &SCHEDULES));
    plan.schedule = if s == 1 { DepthSchedule::ConstantArea } else { DepthSchedule::ConstantDepth };
    hint(ui, "constant depth: every pass one infeed deeper; constant area: the infeed is the first pass, pass n reaches it times √n");
    if plan.schedule == DepthSchedule::ConstantArea {
        prop_row(ui, "Minimum depth", |ui| num(ui, u, &mut plan.min_depth));
        hint(ui, "no constant-area pass cuts shallower than this");
    }
    prop_row(ui, "Finish allowance", |ui| num(ui, u, &mut plan.finish_allowance));
    hint(ui, "radial stock the finishing pass takes at the root; 0 lets the last roughing pass reach the root");
    let h = ui.spacing().interact_size.y;
    prop_row(ui, "Spring passes", |ui| {
        ui.add_sized([CTRL_W, h], DragValue::new(&mut plan.spring_passes).range(0..=camcraft_app::turning::infeed::MAX_SPRING_PASSES));
    });
    hint(ui, "extra passes at full depth after the finish - tooling makers advise 0 unless the thread needs them");
    check_row(ui, "Repeat each depth", &mut plan.repeat_each_depth);
    hint(ui, "cut every roughing depth twice, the second cut on the same line, before going deeper - G32 passes only (a G76 cycle refuses it); tooling makers recommend against routine spring passes, so leave it off unless the flanks come out rough");
    prop_row(ui, "Chip-break every", |ui| num(ui, u, &mut plan.dips.interval));
    hint(ui, "0 for none; otherwise each roughing pass lifts out and back this often along the thread so the chip breaks - at least two leads, never in the first or last lead, never on the finishing or spring passes; G32 passes on a continuous-threading post only (a G76 cycle and other posts refuse it)");
    if plan.dips.is_on() {
        prop_row(ui, "Dip height", |ui| num(ui, u, &mut plan.dips.height));
        hint(ui, "how far the insert lifts, radially - at most the shallowest roughing pass's depth of cut");
        prop_row(ui, "Dip length", |ui| num(ui, u, &mut plan.dips.length));
        hint(ui, "axial length of one dip, ramps included; 0 is the shortest dip the ramps allow (each ramp stays under 45 degrees)");
    }
}
