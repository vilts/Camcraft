//! Machine definitions and program limits (M2.3 stage C).
//!
//! Provenance: the `.mch` file layout is the machine-definition JSON
//! format reused by user instruction (our reader, our field semantics,
//! nothing copied from any vendor's parser or documentation beyond the
//! observable shape of a machine file); the limit-checking rules are
//! general machine-tool practice - a spindle has a speed range, an
//! axis has a feed ceiling and a stroke, and a tool table has a
//! length and diameter the magazine accepts.
//!
//! The design law: an UNSTATED limit checks nothing. Vendor machine
//! files carry zeros for speeds nobody configured, and a zero rapid
//! speed does not mean the machine cannot rapids - it means the file
//! doesn't say. Every number here is therefore an `Option`, and `0`
//! reads as `None`. A program judged against a sparse machine says
//! exactly what the machine file says, no more - the silent wrong
//! answer in its friendliest form would be a green checkmark earned
//! by inventing a ceiling.
//!
//! Travel is the same law with a sharper edge: standard machine files
//! carry no stroke numbers at all, so the check is live only when our
//! own files add a `"travel": {"min": .., "max": ..}` object to a
//! linear part. Those numbers are read in PROGRAM coordinates through
//! the active WCS - the file cannot know a work offset, and guessing a
//! machine origin would invent the very number the check needs. So a
//! stroke L is stated `-L..L` (the origin may sit anywhere along it);
//! a machine-frame `-500..0` refused every retract (#457).
//!
//! M3.3 stage C widened the readable grammar, all of it optional -
//! files written before stage C parse exactly as they always did:
//! `general.kind` declares what the machine IS (mill / lathe /
//! mill-turn; unstated means the axis inventory decides, stage E's
//! older law); `spindle.power` (kW), `tooling.number_of_tools` (the
//! changer's slots), and `tool_station.coolants` (names, with their
//! M-codes where the file states them) carry the full numbers; and
//! `"type": "rotary"` parts join the axis table with degrees for
//! travel and a `"resolution"` key for positioning steps. The mirror
//! to the post grammar (posting) now speaks coolants too, so a
//! mounted machine's coolant words are its own. M5.3 stage C added
//! the spindle INVENTORY - heads stating a spindle, counted
//! whole-tree - which is what makes a program's part transfer a
//! checkable claim instead of a hope.

//!
//! M4.1 R0 split (docs/M4.1-CRATE-SPLIT.md): the plain types live in
//! [`def`], the `.mch` reader in [`parse`]; this module keeps what
//! touches the world - the singleton glue and the program checker.

// M4.1 R3: the plain types and the `.mch` reader live in camcraft-post
// (crates/post/src/machine/); this module keeps only what touches the
// world - the singleton glue and the program checker.
pub use camcraft_post::machine::*;
// #828: the reader, validator and re-judge with the holder library
// passed in - these shadow the post crate's word-only ones.
mod holders;
pub use holders::{holder_lookup, parse_mch, tooling_refusal, tooling_refusals};

// #287: the Machines pane's catalogue, property sheet and validator.
pub mod library;
// #669: the machine check on a plasma table.
mod plasma;
mod waterjet;
pub mod wire_edm;
mod sheet;
pub mod validate;
// #383: the machine editor - property grid, draft, user folder and save.
pub mod draft;
pub mod fields;
pub mod order;
pub mod store;
// #647: a lathe's slides judged on the posted meridian radius.
mod lathe_stroke;
// #1567: the envelope the viewport draws.
mod work_envelope;
pub use work_envelope::work_envelope;

use flecs_ecs::prelude::World;

use crate::model;
use crate::posting;
use crate::schema::MachineDef;

/// The MOUNTED machine: the active setup's (#343 - its own, else the
/// document's), as (the path it was loaded from, the definition).
/// `("", None)` until a `LoadMachine` command or a project file says
/// otherwise - and a document with no machine checks nothing, which is
/// the unstated-limit law wearing a document-sized hat. The document's
/// stored machine alone is [`document_of`].
#[must_use]
pub fn of(world: &World) -> (String, Option<Machine>) {
    crate::setup_mount::machine_of(world, crate::setup::active(world).as_deref())
}

/// The document's machine as stored - what every setup that owns no
/// machine follows (#343). For `LoadMachine`, the save, and the
/// document's machine section.
#[must_use]
pub fn document_of(world: &World) -> (String, Option<Machine>) {
    crate::setup_mount::document_machine(world)
}

/// The machine a declared setup context mounts (#343).
#[must_use]
pub fn of_setup(world: &World, declared: Option<&str>) -> (String, Option<Machine>) {
    crate::setup_mount::machine_of(world, declared)
}

/// Write the singleton. For the `LoadMachine` command and the project
/// reader only - the same law `units::set` keeps: setup state moves
/// through commands or a document load, never ad hoc.
pub fn set(world: &World, path: &str, machine: Option<&Machine>) {
    world.set(MachineDef {
        path: path.to_owned(),
        machine: machine.cloned(),
    });
}

/// The mounted machine's kind (the active setup's, #343) - the
/// per-frame accessor for ribbon gating and the new-op door (a new op
/// joins the active setup). An op's own laws read [`kind_of_setup`].
#[must_use]
pub fn kind_of(world: &World) -> Kind {
    kind_of_setup(world, crate::setup::active(world).as_deref())
}

pub use crate::setup_mount::kind_of_setup;

/// Check the WHOLE program - the same op walk the sim and the post
/// share, so the gate that stage D wires sees one machine judgment
/// over exactly what would post. Never re-plans: cached moves only.
pub fn check_document(world: &World, machine: &Machine) -> Result<MachineCheck, String> {
    check_ops(world, machine, &posting::program_ops(world)?)
}

/// [`check_document`] over a slice of the program (#388): the ops one
/// machine runs, when setups mount different machines.
pub fn check_ops(world: &World, machine: &Machine, program: &[(String, flecs_ecs::prelude::Entity)]) -> Result<MachineCheck, String> {
    if kind(Some(machine)) == Kind::Waterjet {
        return waterjet::check_waterjet_ops(world, machine, program);
    }
    if kind(Some(machine)) == Kind::Plasma {
        return plasma::check_plasma_ops(world, machine, program);
    }
    if kind(Some(machine)) == Kind::WireEdm {
        return wire_edm::check_wire_ops(world, machine, program);
    }
    let ops = program.to_vec();
    let mut out = MachineCheck::default();

    // The changer (M3.3 C): a program whose ops draw on more distinct
    // tools than the drum holds physically cannot run, however each
    // tool passes on its own. Counted by name, the same identity the
    // ops and the library agree on.
    let names: Vec<String> = ops.iter()
        .filter_map(|(_, e)| model::operation_of(world, e).map(|op| op.tool.clone()))
        .collect();
    out.check_tool_count(machine, &names.iter().map(String::as_str).collect::<Vec<_>>());

    for (name, e) in &ops {
        let op = model::operation_of(world, e).ok_or_else(|| format!("{name}: lost its operation"))?;
        let tool_entity = model::find_by_name(world, &op.tool)
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let tool = model::tool_of(world, &tool_entity)
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        if let Some(number) = crate::tool_numbers::number_of(world, &op.tool)
            && let Err(why) = crate::tool_reservations::on_machine(machine, &op.tool, tool.kind, number)
        {
            say(&mut out, format!("{name}: {why}"));
        }
        // #368: a modelled body is the holder the magazine measures.
        let tool = crate::tool_body::judged(world, &tool_entity, tool)
            .map_err(|e| format!("{name}: tool {}: {e}", op.tool))?
            .0;
        let cache = model::toolpath_of(world, e).ok_or_else(|| format!("{name}: lost its toolpath"))?;
        // Travel numbers in the file are PROGRAM coordinates; the
        // cached moves are world millimeters. This is the post's own
        // origin law (one copy, in posting), so a G54 slide shifts
        // what passes exactly as it shifts the G-code.
        let (_, origin) = posting::wcs_context(world, op.setup.as_deref());
        let flip = crate::setup_wcs_flip::of_setup(world, op.setup.as_deref());

        // Spindle: the speed the program commands for this op - the
        // post's own answer (`posting::posted_rpm`), so a touch probe,
        // which is never spun, is not judged against the spindle range
        // (#565); every cutter is, by its own rpm.
        let rpm = posting::posted_rpm(&tool);
        out.check_spindle(machine, Some(SourceLocation::Operation(name)), &op.tool, rpm);
        // Spindle power (M13.2): the estimate the cutting data makes
        // of the op's own engagement against the stated motor. Every
        // input must be stated - machine power, document material,
        // tool flutes, an engagement the op's numbers fix - or no
        // claim is made (the unstated-limit law, `crate::cutting`).
        if let Some(limit) = machine.spindle_power
            && let Some(mat) = crate::cutting::material_of(world)
            && let Some(p) = crate::cutting::power_of(&op.params, &tool, mat)
            && p.kw > limit
        {
            say(
                &mut out,
                format!(
                    "{name}: spindle power estimated {:.2} kW ({:.1} cm3/min of {} at kc {:.0} N/mm2), above the machine's {limit:.2} kW",
                    p.kw,
                    p.mrr / 1000.0,
                    mat.label(),
                    p.kc
                ),
            );
        }
        // The tool table: long or fat cutters the magazine refuses.
        // "Tool length" is the whole protrusion from the spindle nose
        // to the tip - what the magazine's clearance measures: cutter,
        // shank, and the holder the setup assumes above them.
        out.check_tool_size(
            machine, Some(SourceLocation::Operation(name)), &op.tool,
            tool.flute_len + tool.shank_len + tool.holder_len, tool.diameter,
        );
        // The handoff step (M5.3 C): a parting op that asks for its
        // part to move to a second spindle wants steel the machine
        // may not have. Stated spindles check, unstated check
        // nothing - the same floor the tilt refusal stands on, and
        // the number a `PART TRANSFER 1 2` comment would have the
        // operator reach for a hand that isn't there.
        if let crate::operation::OpParams::Part { bar_transfer: true, .. } = &op.params
            && let Some(n) = machine.spindles
            && n < 2
        {
            say(
                &mut out,
                format!(
                    "{name}: parting hands the part to a second spindle; {} states {n} spindle(s)",
                    machine.model
                ),
            );
        }

        // #786: a tap cycle on a machine that says it cannot rigid-tap.
        tap_gate(&mut out, machine, name, &cache.path);
        // #1445: a fine bore on a machine that says it cannot stop oriented.
        fine_bore_gate(&mut out, machine, name, &cache.path);

        // #647: on a lathe the slides are judged on the POSTED path's
        // meridian radius (`lathe_stroke`), not on the cache's world X.
        let lathe_judged = lathe_stroke::judge(&mut out, name, &op.params, &op.tool, &tool, &cache.path, machine, origin);

        out.check_path(
            machine, Some(SourceLocation::Operation(name)), &cache.path,
            origin, flip, !lathe_judged,
        );
    }
    Ok(out)
}
