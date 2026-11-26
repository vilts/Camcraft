//! The shared kind editors the family pages call (split out of
//! `pages.rs` with the property grid, user review 2026-11-06): the
//! roughing twins' pane, the 5-axis lead / lag / tilt trio, the shank
//! avoidance switch, the swarf and rotary panels. Rows ride the `layout`
//! form helpers; every value edits the page draft.
//!
//! Provenance: our own port of the M-c / M5.1 page code (each motion law
//! named in its doc comment lives in its own crate with its sources); no
//! vendor material involved.

use super::fields::*;

/// The roughing twins' shared pane (M2.4 A): the pocket's fields minus
/// the Strategy combo - the kind IS the strategy, so the choice that
/// would undo the name is not offered. `tag` scopes the height-editor
/// ids ("adaptive"/"offset"); the engagement door rides along as an
/// Option so the law "a section header renders exactly when its kind
/// has fields" stays one code path for both catalog names.
#[allow(clippy::too_many_arguments)]
pub(super) fn roughing2d_pane(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    op_ctx: Option<&str>,
    u: Unit,
    tab: usize,
    tag: &str,
    stepover: &mut f64,
    stock_to_leave: &mut f64,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    stepdown: &mut f64,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    entry: &mut camcraft_app::machining3d::Entry3D,
    helix_radius: &mut f64,
    optimal_load: Option<&mut f64>,
    mut troch: Option<(&mut camcraft_app::machining2d::AdaptiveMethod, &mut camcraft_app::machining2d::AdaptiveTrochParams)>,
    links: &mut camcraft_app::links::Links,
    arc: (&mut bool, &mut Option<f64>),
    corner_relief: &mut camcraft_app::machining2d::CornerRelief,
    tilt: &mut Option<Tilt>,
    pass_start: Option<&mut camcraft_app::machining2d::PassStart>,
) {
    if tab == SEC_HEIGHTS {
        orientation_editor(ui, u, tilt);
        height_editor(ui, "Clearance Z", world, op_ctx, clearance_z, &format!("{tag}.clearance"));
        height_editor(ui, "Top Z", world, op_ctx, top_z, &format!("{tag}.top"));
        height_editor(ui, "Bottom Z", world, op_ctx, bottom_z, &format!("{tag}.bottom"));
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, op_ctx, retract_z, top_z, clearance_z, &format!("{tag}.retract"));
            prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
        });
    }
    if tab == SEC_PASSES {
        // #1509: how 2D Adaptive clears its region - the choice the
        // Adaptive operations own (maintainer, 2026-11-13).
        let trochoidal = troch.as_ref().is_some_and(|(m, _)| **m == camcraft_app::machining2d::AdaptiveMethod::Trochoidal);
        if let Some((m, _)) = &mut troch {
            adaptive_method_row(ui, m);
        }
        // #1512: the adaptive twin steps by its optimal load and never
        // reads the stepover, so only the offset twin shows the field.
        if optimal_load.is_none() {
            prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
        }
        // #3316: the shared pass-order row; the kinds that own it pass it.
        if let Some(s) = pass_start {
            super::pass_start_row::pass_start_row(ui, tag, s);
            // #3355: the Trochoidal loops have no ring order; said by name.
            if trochoidal && *s != camcraft_app::machining2d::PassStart::Auto {
                consequence(ui, super::pass_start_row::TROCH_PASS_START_REFUSED);
            }
        }
        prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
        arc_turn_rows(ui, u, "Arc turns at corners", arc.0, arc.1);
        if trochoidal && *arc.0 {
            consequence(ui, "arc turns are not available with the Trochoidal method: the operation is refused");
        }
        corner_relief_rows(ui, world, op_ctx, tag, corner_relief);
        if trochoidal && !corner_relief.is_off() {
            consequence(ui, "corner relief is not available with the Trochoidal method: the operation is refused");
        }
        match optimal_load {
            Some(m) => super::load_field::optimal_load_field(ui, m),
            None => {
                hint(ui, "concentric rings from the boundary inward, one stepover apart");
            }
        }
        if let Some((m, t)) = troch
            && *m == camcraft_app::machining2d::AdaptiveMethod::Trochoidal
        {
            adaptive_troch_passes(ui, u, t);
        }
    } else if tab == SEC_LEADS {
        // #3748: the entry advice of the ring twin (2D Offset); the
        // Adaptive twin (an optimal load) has no Arc entry from the air.
        let start = pass_start.as_deref().copied().filter(|_| optimal_load.is_none()).unwrap_or_default();
        lead_page::leads_tab_with_retract(ui, u, tag, Some((entry, helix_radius)), links, retract_z.is_some(), start);
        // #3745: an Inside or Outside pass enters outside the stock edge.
        if let Some(s) = pass_start.as_deref() {
            super::approach_page::approach_distance_group(ui, u, *s, &mut links.approach);
        }
    } else if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        if let Some((m, t)) = troch
            && *m == camcraft_app::machining2d::AdaptiveMethod::Trochoidal
        {
            auto_opt(ui, "Return feed", "Cutting feed", &mut t.return_feed, "the cutting feed", *feed, |ui, v| {
                num_feed(ui, u, v)
            });
        }
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

/// 2D Adaptive's trochoidal-only Passes fields (#1588): hidden under Spiral.
fn adaptive_troch_passes(ui: &mut Ui, u: Unit, t: &mut camcraft_app::machining2d::AdaptiveTrochParams) {
    use camcraft_app::machining2d::{DEFAULT_MICROLIFT, TrochoidReturn};
    prop_row(ui, "Max engagement", |ui| num_unit(ui, &mut t.max_engagement, "deg"));
    hint(ui, "most cutter contact allowed on a loop; the forward step tightens to hold it");
    auto_opt(ui, "Loop width", "All the room", &mut t.loop_width, "medial room", 2.0, |ui, v| num(ui, u, v));
    hint(ui, "band the loop centres sweep along the medial line");
    let before = match t.ret {
        None => 0,
        Some(TrochoidReturn::Microlift { .. }) => 1,
        Some(TrochoidReturn::Retract) => 2,
    };
    let mut idx = before;
    prop_row(ui, "Return", |ui| combo(ui, "a_troch_return", &mut idx, &["Auto", "Microlift", "Retract"]));
    if idx != before {
        t.ret = match idx {
            1 => Some(TrochoidReturn::Microlift { height: DEFAULT_MICROLIFT }),
            2 => Some(TrochoidReturn::Retract),
            _ => None,
        };
    }
    match &mut t.ret {
        None => hint(ui, "0.3 mm microlift when clearance is at least 0.3 mm above top, else retract"),
        Some(TrochoidReturn::Microlift { height }) => prop_row(ui, "Microlift height", |ui| num(ui, u, height)),
        Some(TrochoidReturn::Retract) => hint(ui, "rapid up to the retract plane between loops"),
    }
}

/// The corner-arc option and its radius (#1733): the switch, then -
/// while it is on - the radius, `Tool radius` (the `None` every earlier
/// file holds) or a set value the generator bounds by the tool radius.
pub(super) fn arc_turn_rows(ui: &mut Ui, u: Unit, label: &str, on: &mut bool, radius: &mut Option<f64>) {
    arc_turn_rows_auto(ui, u, (label, "Arc radius = tool radius"), on, radius);
}

/// [`arc_turn_rows`] with the name of what no radius means (#1759: a
/// Face turns on half the row spacing, not on the tool radius).
pub(super) fn arc_turn_rows_auto(ui: &mut Ui, u: Unit, (label, auto_label): (&str, &str), on: &mut bool, radius: &mut Option<f64>) {
    check_row(ui, label, on);
    if !*on {
        return;
    }
    let mut auto = radius.is_none();
    check_row(ui, auto_label, &mut auto);
    match (auto, radius.as_mut()) {
        (true, _) => *radius = None,
        (false, Some(r)) => prop_row(ui, "Arc radius", |ui| num(ui, u, r)),
        // Leaving Auto seeds a small radius; the generator refuses one
        // past the tool radius by name.
        (false, None) => *radius = Some(ARC_RADIUS_SEED),
    }
}

/// The radius `arc_turn_rows` seeds when the user leaves `Tool radius`,
/// mm: small enough for the common end mills.
const ARC_RADIUS_SEED: f64 = 0.5;

const RELIEF_KINDS: [&str; 3] = ["None", "Dogbone", "T-bone"];
const TBONE_DIRS: [&str; 4] = ["Long edge", "Short edge", "Horizontal", "Vertical"];

/// Corner relief controls (#2400) shared by Pocket, 2D Adaptive, and 2D Offset.
/// A router option (#2410): drawn when the operation's machine offers it,
/// and always when the operation already states a relief (never hidden).
pub(super) fn corner_relief_rows(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    op_ctx: Option<&str>,
    tag: &str,
    relief: &mut camcraft_app::machining2d::CornerRelief,
) {
    use camcraft_app::machining2d::{CornerReliefKind, TBoneDirection};
    use camcraft_app::router_offer::{offers, RouterOption};
    if relief.is_off() && !offers(world, op_ctx, RouterOption::CornerRelief) {
        return;
    }
    let before = match relief.kind {
        CornerReliefKind::None => 0,
        CornerReliefKind::Dogbone => 1,
        CornerReliefKind::TBone => 2,
    };
    let mut idx = before;
    let id_kind = format!("{tag}_corner_relief");
    prop_row(ui, "Corner relief", |ui| combo(ui, &id_kind, &mut idx, &RELIEF_KINDS));
    if idx != before {
        relief.kind = match idx {
            1 => CornerReliefKind::Dogbone,
            2 => CornerReliefKind::TBone,
            _ => CornerReliefKind::None,
        };
    }
    if relief.kind == CornerReliefKind::TBone {
        let d_before = match relief.tbone_direction {
            TBoneDirection::LongEdge => 0,
            TBoneDirection::ShortEdge => 1,
            TBoneDirection::Horizontal => 2,
            TBoneDirection::Vertical => 3,
        };
        let mut d_idx = d_before;
        let id_dir = format!("{tag}_tbone_dir");
        prop_row(ui, "T-bone direction", |ui| combo(ui, &id_dir, &mut d_idx, &TBONE_DIRS));
        if d_idx != d_before {
            relief.tbone_direction = match d_idx {
                1 => TBoneDirection::ShortEdge,
                2 => TBoneDirection::Horizontal,
                3 => TBoneDirection::Vertical,
                _ => TBoneDirection::LongEdge,
            };
        }
    }
    if !relief.overrides.is_empty() {
        hint(ui, &format!("{} per-corner overrides active", relief.overrides.len()));
    }
}

/// The names of 2D Adaptive's Method combo, in the combo's order
/// (`AdaptiveMethod::ALL`).
pub(crate) fn adaptive_method_names() -> Vec<&'static str> {
    camcraft_app::machining2d::AdaptiveMethod::ALL.iter().map(|m| m.label()).collect()
}

/// What the two methods do: the Method row's hover text.
const METHOD_HINT: &str = "Spiral: a spiral out of the entry bore, then passes that hold the optimal load. Trochoidal: loops along the region's medial line, kept inside the region; the optimal load is the most the loops advance a turn";

/// 2D Adaptive's Method row (#1509): Spiral (the path it always cut)
/// or Trochoidal; what each does is the row's hover text.
fn adaptive_method_row(ui: &mut Ui, method: &mut camcraft_app::machining2d::AdaptiveMethod) {
    use camcraft_app::machining2d::AdaptiveMethod;
    let names = adaptive_method_names();
    let before = AdaptiveMethod::ALL.iter().position(|m| m == method).unwrap_or(0);
    let mut idx = before;
    prop_row(ui, "Method", |ui| combo(ui, "a_method", &mut idx, &names));
    if idx != before
        && let Some(m) = AdaptiveMethod::ALL.get(idx)
    {
        *method = *m;
    }
    hint(ui, METHOD_HINT);
}

/// The axis trio both 5-axis kinds carry (M5.1 stage C). The law lives
/// in `camcraft_app::multiaxis::tilt`: lead leans along the feed, lag
/// leans across it, and the max tilt is the cone half-angle the
/// spindle may open from the surface normal - past it the commanded
/// direction folds onto the cone's edge. All three are degrees, so
/// they ride `num_plain`: the unit is a lens on lengths and feeds,
/// never on angles.
pub(super) fn axis_editor(ui: &mut Ui, lead_deg: &mut f64, lag_deg: &mut f64, max_tilt_deg: &mut f64) {
    prop_row(ui, "Tool lead angle", |ui| num_unit(ui, lead_deg, "°"));
    hint(ui, "lean with the feed, off the surface normal - tips the shank away from the wall ahead of the cut");
    prop_row(ui, "Tool lag angle", |ui| num_unit(ui, lag_deg, "°"));
    hint(ui, "lean across the feed (side tilt) - clears the wall the tool has just cut");
    prop_row(ui, "Max tilt", |ui| num_unit(ui, max_tilt_deg, "°"));
    hint(ui, "the cone the spindle may open from the normal; where the surface demands more, the lean folds onto the cone's edge");
}

/// Stage F's one switch (M5.1): where the posed axis would drive the
/// shank into the part, the spindle tilts to the nearest clear angle
/// inside the max-tilt cone - the shortest travel from the pose it
/// held, so neighboring cuts do not swing the head back and forth.
/// The ball's envelope is rotation-invariant, so this can only ever
/// change which way the spindle stands, never where the cut lands;
/// and where no angle inside the cone clears, the operation refuses
/// by name rather than post a path the verifier would warn on.
pub(super) fn shank_avoid_editor(ui: &mut Ui, shank_avoid: &mut bool) {
    check_row(ui, "Avoid shank collisions", shank_avoid);
    hint(ui, "tilt the spindle off a leaning pose that would rub the shank; positions never move, and a wall no tilt clears refuses the job");
}

/// The swarf family's panel (M5.1 stage D): the raster's sections
/// worded for the ruling, in one body both kinds share. `passes` is
/// Some only for the ladder - the or-pattern cannot name a field one
/// variant lacks, so the arms hand their extra word in as data.
#[allow(clippy::too_many_arguments)] // flat panel args, same shape as `cell`
pub(super) fn swarf_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    u: Unit,
    tab: usize,
    tolerance: &mut f64,
    stepover: &mut Option<f64>,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    lead_deg: &mut f64,
    lag_deg: &mut f64,
    max_tilt_deg: &mut f64,
    passes: Option<&mut u32>,
) {
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, setup, clearance_z, "swarf.clearance");
        height_editor(ui, "Top Z", world, setup, top_z, "swarf.top");
        height_editor(ui, "Bottom Z", world, setup, bottom_z, "swarf.bottom");
        hint(ui, "Top is the first line's plane, Bottom the last one's");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, setup, retract_z, top_z, clearance_z, "swarf.retract");
        });
    }
    if tab == SEC_PASSES {
        auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
            num(ui, u, s)
        });
        hint(ui, "Auto: from tolerance and tool by the scallop law, across the ruling. Stated: spacing ACROSS the ruling; along it the lines never part - the surface's own direction spaces them");
        // The ladder's rung count, the rung-count law: a plain number
        // clamped into the generator's named range at the draft, so
        // no panel edit ever holds a 0 or a 9. Named "Pass count" - a
        // field reading exactly "Passes" would echo its own tab's label
        // (M9.1 B named this by pinning headers; the tab bar keeps it).
        if let Some(passes) = passes {
            let mut n = f64::from(*passes);
            prop_row(ui, "Pass count", |ui| num_plain(ui, &mut n));
            *passes = n.clamp(1.0, 4.0).round() as u32;
            hint(ui, "coarse-to-fine runs: each rough pass relaxes the tolerance fourfold (and doubles the spacing), the last cuts to the value set below");
        }
        axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
        hint(ui, "the finish budget: lines are spaced from it, so the cusp along the flank stays within it");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

/// The rotary twins' panel (M5.1 stage E): the swarf body worded for
/// the turn. `stepover` is Some only for the parallel kind - the
/// contour's stations drop by the scallop law alone, and an or-pattern
/// cannot name a field one variant lacks, so the arms hand their
/// spacing in as data.
#[allow(clippy::too_many_arguments)] // flat panel args, same shape as `swarf_editor`
pub(super) fn rotary_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    u: Unit,
    tab: usize,
    tolerance: &mut f64,
    stepover: Option<&mut Option<f64>>,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    lead_deg: &mut f64,
    lag_deg: &mut f64,
    max_tilt_deg: &mut f64,
    stated_axis: &mut Option<[f64; 3]>,
) {
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, setup, clearance_z, "rotary.clearance");
        height_editor(ui, "Top Z", world, setup, top_z, "rotary.top");
        height_editor(ui, "Bottom Z", world, setup, bottom_z, "rotary.bottom");
        hint(ui, "the world planes map onto stations along the fitted axis - Top where the run starts, Bottom where it ends");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, setup, retract_z, top_z, clearance_z, "rotary.retract");
        });
    }
    if tab == SEC_PASSES {
        // #2772: the op's own stated rotation axis, above the pose
        // trio it stands in for.
        rotary_stated_axis_editor(ui, stated_axis);
        if let Some(stepover) = stepover {
            auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
                num(ui, u, s)
            });
            hint(ui, "Auto: from tolerance and tool by the scallop law, around the turn. Stated: spacing AROUND the turn; along the axis the lines never part - the cylinder's own direction spaces them");
        }
        rotary_axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
        hint(ui, "the finish budget: stations and lines follow from it, so the cusp under the ball stays within it");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

/// The axis trio, worded for the rotary twins: the leans pose off the
/// RADIAL, not the surface normal - the wall of a turn answers for the
/// whole ring at once, and the law lives in
/// `camcraft_motion::multiaxis::rotary` (M5.1 stage E).
pub(super) fn rotary_axis_editor(ui: &mut Ui, lead_deg: &mut f64, lag_deg: &mut f64, max_tilt_deg: &mut f64) {
    prop_row(ui, "Tool lead angle", |ui| num_unit(ui, lead_deg, "°"));
    hint(ui, "lean with the feed, off the radial from the fitted axis - tips the shank away from the wall ahead of the cut");
    prop_row(ui, "Tool lag angle", |ui| num_unit(ui, lag_deg, "°"));
    hint(ui, "lean across the feed (side tilt), off the radial - clears the wall the tool has just cut");
    prop_row(ui, "Max tilt", |ui| num_unit(ui, max_tilt_deg, "°"));
    hint(ui, "the cone the spindle may open from the radial; where the stance demands more, the lean folds onto the cone's edge");
}

/// The stated rotation axis (#2772): a group armed by a checkbox and
/// three direction numbers - a RATIO, never unit-scaled (the machine
/// file's own law for its `axis` vector), through the setup's work
/// offset origin. It writes the draft, so the existing
/// EditOperationParams commit path makes the edit undoable. Absent
/// follows the machine's stated rotary line; only when nothing is
/// stated does the model's fit run - the resolver's own order
/// (`rotary_stated::lines_of`). The panel never judges; the
/// resolver's disagreement refusal owns the door.
pub(super) fn rotary_stated_axis_editor(ui: &mut Ui, v: &mut Option<[f64; 3]>) {
    let mut armed = v.is_some();
    let g = group(ui, "Stated rotation axis", Some(&mut armed), |ui| {
        if let Some(a) = v.as_mut() {
            prop_row(ui, "Direction X", |ui| num_unit(ui, &mut a[0], ""));
            prop_row(ui, "Direction Y", |ui| num_unit(ui, &mut a[1], ""));
            prop_row(ui, "Direction Z", |ui| num_unit(ui, &mut a[2], ""));
        }
    });
    if g.toggled {
        *v = armed.then_some([1.0, 0.0, 0.0]);
    }
    hint(ui, "a direction through the work offset origin; absent: the machine's stated rotary line, else fitted from the model - a line the model's faces disagree with refuses by name");
}
