//! The operation page: the 3D milling kinds' fields: 3D Pocket and the finish patterns - one module per kind family, split out
//! of `pages.rs` (the 400-line law) when the property grid landed (user
//! review 2026-11-06). Each arm draws its kind's fields for the open tab
//! through the `layout` form helpers; every value edits the page draft,
//! and Apply rides the command history.
//!
//! Provenance: our own UI over the operation parameter model (each
//! kind's own module names its sources); no vendor material or text.

use super::fields::*;

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let _ = (u, tab, op_ctx);
    #[allow(unused_variables)]
    let (side_note, auto_note, cycle_note, engrave) = (cx.side_note, cx.auto_note, cx.cycle_note, cx.engrave);
    match params {
        OpParams::Pocket3D {
            links,
            stepover,
            stock_to_leave,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            rest_rough,
            optimal_load,
            feed_opt,
            shell,
        } => {
            if tab == SEC_LEADS {
                lead_page::leads_tab(ui, u, "p3", Some((entry, helix_radius)), links);
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "pocket3d.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "pocket3d.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "pocket3d.bottom",
                );
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "pocket3d.retract");
                    prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                });
            }
            if tab == SEC_PASSES {
                if !strategy.steps_by_load() {
                    prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
                }
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                    // M11.4: the HSM plan per level, or the rings at
                    // the load step every earlier Adaptive posted.
                    PocketStrategy::OffsetRings => 4,
                };
                let names = strategy3d_names();
                let before = st_idx;
                prop_row(ui, "Strategy", |ui| combo(ui, "p3_strat", &mut st_idx, names));
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        4 => PocketStrategy::OffsetRings,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if strategy.steps_by_load() {
                    super::load_field::optimal_load_field(ui, optimal_load);
                }
                if matches!(strategy, PocketStrategy::Adaptive) {
                    hint(ui, "HSM adaptive clearing on every level: a spiral at the optimal load, lifted returns, arcs; a level the planner refuses (an island or channel past its budget) cuts Offset rings and says so");
                }
                hint(ui, "levels come from the mesh: each shelf cuts around the material standing above it");
                check_row(ui, "Offset shells", shell);
                hint(ui, "rings ride the model's own normals between levels - the staircase becomes a waterline wherever the wall turns steep");
                check_row(ui, "Rest roughing", rest_rough);
                hint(ui, "a closing ladder, half the step, cuts what the levels leave standing");
                check_row(ui, "Feed optimization", feed_opt);
                hint(ui, "heavier bites slow toward the programmed feed (never faster); the sim measures each move");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        OpParams::Finish3D {
            pattern,
            stepover,
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            ramp,
            min_step,
            ramp_angle,
            entry_ramp,
            steep_stepover,
            pencil_reference,
            fill_openings,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "finish3d.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "finish3d.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "finish3d.bottom",
                );
                hint(ui, "Top is the first ring's plane, Bottom the last level's");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "finish3d.retract");
                });
            }
            if tab == SEC_PASSES {
                let mut pat_idx = match pattern {
                    camcraft_app::finishing3d::FinishPattern::Waterline => 0,
                    camcraft_app::finishing3d::FinishPattern::Parallel => 1,
                    camcraft_app::finishing3d::FinishPattern::ParallelMulti => 2,
                    camcraft_app::finishing3d::FinishPattern::Scallop => 3,
                    camcraft_app::finishing3d::FinishPattern::Radial => 4,
                    camcraft_app::finishing3d::FinishPattern::Spiral => 5,
                    camcraft_app::finishing3d::FinishPattern::Contour => 6,
                    camcraft_app::finishing3d::FinishPattern::Project => 7,
                    camcraft_app::finishing3d::FinishPattern::SteepShallow => 8,
                    camcraft_app::finishing3d::FinishPattern::Flat => 9,
                    camcraft_app::finishing3d::FinishPattern::Pencil => 10,
                    camcraft_app::finishing3d::FinishPattern::Corner => 11,
                    camcraft_app::finishing3d::FinishPattern::Ramp => 12,
                    camcraft_app::finishing3d::FinishPattern::MorphSpiral => 13,
                    camcraft_app::finishing3d::FinishPattern::Geodesic => 14,
                    camcraft_app::finishing3d::FinishPattern::Blend => 15,
                    camcraft_app::finishing3d::FinishPattern::Morph => 16,
                    camcraft_app::finishing3d::FinishPattern::Flow => 17,
                };
                let before = pat_idx;
                prop_row(ui, "Pattern", |ui| combo(ui, "f3_pattern", &mut pat_idx, &FINISH_PATTERNS));
                if pat_idx != before {
                    *pattern = match pat_idx {
                        1 => camcraft_app::finishing3d::FinishPattern::Parallel,
                        2 => camcraft_app::finishing3d::FinishPattern::ParallelMulti,
                        3 => camcraft_app::finishing3d::FinishPattern::Scallop,
                        4 => camcraft_app::finishing3d::FinishPattern::Radial,
                        5 => camcraft_app::finishing3d::FinishPattern::Spiral,
                        6 => camcraft_app::finishing3d::FinishPattern::Contour,
                        7 => camcraft_app::finishing3d::FinishPattern::Project,
                        8 => camcraft_app::finishing3d::FinishPattern::SteepShallow,
                        9 => camcraft_app::finishing3d::FinishPattern::Flat,
                        10 => camcraft_app::finishing3d::FinishPattern::Pencil,
                        11 => camcraft_app::finishing3d::FinishPattern::Corner,
                        12 => camcraft_app::finishing3d::FinishPattern::Ramp,
                        13 => camcraft_app::finishing3d::FinishPattern::MorphSpiral,
                        14 => camcraft_app::finishing3d::FinishPattern::Geodesic,
                        15 => camcraft_app::finishing3d::FinishPattern::Blend,
                        16 => camcraft_app::finishing3d::FinishPattern::Morph,
                        17 => camcraft_app::finishing3d::FinishPattern::Flow,
                        _ => camcraft_app::finishing3d::FinishPattern::Waterline,
                    };
                }
                hint(ui, "Waterline: tangent rings only. The others march the flat ground first - Parallel along the axes (Multi both, Scallop the diagonals), Radial as chords through the centre, Spiral as one in-winding curve, Contour as concentric loops of the model's shadow, Project as its lifted silhouette, and every closed edge the part itself carries - a trim's rim, a hole's edge - XY kept and Z grounded, cut exactly where the model's facets end rather than where their shadow falls (a closed solid has no open edge, so its path is what it always was). Then the rings take the flanks. Steep and shallow draws that split as a border: the rings keep their hands off shallow ground, and the flanks answer to their own spacing. Flat rides the floors only, one pass per shrink. Pencil and Corner read the bends instead of the slope: each rides the line of centres tangent to both faces of every concave corner, Corner with the op's own ball and Pencil with a fatter reference ball, cleaning only what that pen leaves standing. Ramp is Parallel with the sloped entries always on. The morphed spiral keeps one in-winding curve but spaces its turns by the ground's own cosine under them. Geodesic rings start at the model's open edge and march outward by distance ALONG the surface, so one loop can run floor, wall and rim together; a closed shell with no open edge is refused by name. Blend rides the slope from both directions: the parallel lines keep the flat ground, and the rings keep every run touching ground past thirty degrees, so the transition band is cut twice and the border's staircase is shaved away. Morph joins the ladder's levels: the rings ride as Waterline places them, and between each two successive rings that pair as one feature a middle guide is laid halfway between the loops and grounded on the surface - half the spacing along the wall, a quarter of the cusp. Flow traces the surface's own grain: each line starts where the ground is widest and runs along the direction that curves least under it, the field the Flow5D operation reads - so on a blade the passes lie WITH it; on ground that claims no direction the lines degrade to the straight raster and still cover, and the rings take the flanks as always.");
                if !matches!(pattern, camcraft_app::finishing3d::FinishPattern::Waterline) {
                    // The scallop law on the reference R3 ball -
                    // sqrt(8 r tol) - seeds the stated spacing.
                    let seed = (24.0 * tolerance.max(1e-9)).sqrt();
                    auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", seed, |ui, s| num(ui, u, s));
                    hint(ui, "Auto: from tolerance and tool by the scallop law, times the slope's cosine. Stated: the flat-ground spacing; on a slope it follows the terrain's cosine");
                    // M2.5 C honesty: Geodesic carries no chord march
                    // to floor, and rides neither chain-ramp nor
                    // entry-ramp law - every loop enters by its own
                    // certified plunge. Ramp IS the sloped laws,
                    // always on, so the checkboxes would only lie.
                    let geodesic_pat = matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::Geodesic
                    );
                    let ramp_pat = matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::Ramp
                    );
                    if !geodesic_pat {
                        // A tenth of the march's own coarse step - the
                        // scallop law's 2·√(r·tol) on the reference R3
                        // ball: the refinement stays alive, it just
                        // stops here.
                        let seed = 0.1 * (12.0 * tolerance.max(1e-9)).sqrt();
                        auto_opt(ui, "Minimum step", "Auto", min_step, "march's own", seed, |ui, s| num(ui, u, s));
                        hint(ui, "Auto: the march's own resolution floor. Stated: no cut segment shorter than this; run edges still land where the material says");
                    }
                    if ramp_pat {
                        hint(ui, "the diagonal chains between lines and the ramped ring entries are this pattern's whole story - always on, whatever the boxes below would say");
                    } else if !geodesic_pat {
                        check_row(ui, "Ramp between lines", ramp);
                        hint(ui, "one diagonal feed chains each line's end to the next where the ground allows; where it doesn't, the hop stands");
                        if *ramp {
                            prop_row(ui, "Ramp angle", |ui| num_plain(ui, ramp_angle));
                            hint(ui, "the steepest descent a chain may carry; past it a link is a plunge in a diagonal coat, and the hop holds the line");
                        }
                        check_row(ui, "Ramp ring entries", entry_ramp);
                        hint(ui, "on flanks steeper than the split, a ring's first contact slides in along its contact normal instead of dropping; where the normal cannot certify the way in, the plunge stands and the path says why");
                    } else {
                        hint(ui, "geodesic rings ride the surface, not the ladder: each run enters by its own certified plunge, and the sloped-entry laws keep their hands off");
                    }
                    if matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::SteepShallow
                    ) {
                        // The scallop law's own figure on the reference
                        // R3 ball - 2 x sqrt(r tol) - as a starting
                        // value. The envelope verdict, not the number,
                        // is what keeps the promise.
                        let seed = (12.0 * tolerance.max(1e-9)).sqrt();
                        auto_opt(ui, "Steep stepover", "Auto", steep_stepover, "scallop law", seed, |ui, s| num(ui, u, s));
                        hint(ui, "Auto: from tolerance and tool by the scallop law, 2 x sqrt(r x tolerance). Stated: the flank's own level spacing - the ladder obeys; the cusp it leaves is still measured, never assumed");
                    }
                    if matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::Pencil
                    ) {
                        // Four times the reference R3 ball's own diameter
                        // - a pen a band can hide under is no reference.
                        auto_opt(ui, "Reference ball", "None", pencil_reference, "every corner", 24.0, |ui, d| num(ui, u, d));
                        hint(ui, "None: every concave corner whose locus rests on the solid is cleaned. Stated: the pen whose leftovers this op cleans - it must be fatter than the op's ball, and the path names it if it is not");
                    }
                }
                // The land model (#1196): Off is the model itself.
                auto_opt(ui, "Fill openings", "Off", fill_openings, "model surface", 5.0, |ui, d| num(ui, u, d));
                hint(ui, "Off: the path follows the model, holes and all. Stated: closed holes and pockets a disk this wide cannot enter are lidded at their rim and flown over - the walls stay uncut; the gouge check still uses the real model");
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the finish budget: levels drop by 2 x sqrt(tool radius x tolerance), so the cusp under the ball stays within it");
                hint(ui, "stepdown follows from tolerance and tool - it is not a second knob");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        _ => {}
    }
}
