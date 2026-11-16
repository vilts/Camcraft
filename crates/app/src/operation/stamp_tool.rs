//! The tool's share of an operation's generation stamp: the library
//! tool's name, kind and every number its kind reads, and its modelled
//! body's envelope. Split out of `stamp` along the tool seam (#532),
//! unchanged - the bytes written are the ones `current_stamp` always
//! wrote, so no stored stamp moves.
//!
//! Provenance: our own staleness design (see `stamp`).

use std::hash::Hasher;

use flecs_ecs::prelude::World;

use crate::model;

use super::OpParams;

/// Write the tool named `tool_name` into `h`. `None` when the tool is
/// gone - an unresolvable digest is never equal to a stored one.
pub(super) fn write(world: &World, tool_name: &str, params: &OpParams, h: &mut std::hash::DefaultHasher) -> Option<()> {
    let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
    h.write(tool_name.as_bytes());
    h.write_u8(0xff);
    let tool_entity = model::find_by_name(world, tool_name)?;
    let tool = model::tool_of(world, &tool_entity)?;
    h.write_u8(match tool.kind {
        crate::tools::ToolKind::EndMill => 0,
        crate::tools::ToolKind::BallNose => 1,
        crate::tools::ToolKind::BullNose => 2,
        crate::tools::ToolKind::Drill => 3,
        crate::tools::ToolKind::Tap => 4,
        crate::tools::ToolKind::Bore => 5,
        crate::tools::ToolKind::Turning => 6,
        crate::tools::ToolKind::Groove => 7,
        crate::tools::ToolKind::Thread => 8,
        crate::tools::ToolKind::Form => 9,
        crate::tools::ToolKind::Taper => 10,
        crate::tools::ToolKind::Probe => 11,
        crate::tools::ToolKind::Torch => 12,
        crate::tools::ToolKind::Electrode => 13,
        crate::tools::ToolKind::ThreadMill => 14,
        crate::tools::ToolKind::ThreadMillSingle => 15,
        crate::tools::ToolKind::Waterjet => 16,
    });
    for v in [
        tool.diameter,
        tool.corner_r,
        // The nose circle rides the digest from its first build
        // (M5.2 A): it is the envelope ball, so an edit re-marks the
        // verdict Stale through here, exactly like the diameter does.
        tool.nose_radius,
        tool.flute_len,
        tool.shank_d,
        tool.shank_len,
        tool.holder_d,
        tool.holder_len,
    ] {
        w(h, v);
    }
    // Kind-scoped numbers: a tool digests only what its kind reads. A
    // tip_angle parked on a ball nose is inert data, and inert data must
    // never make an operation look stale - the same law the profile and
    // the cycles draw by. It also keeps every pre-M1.6 stamp value
    // unchanged: no one-time churn for the milling kinds.
    // The blade's width rides the digest from its first build
    // (M5.2 C): the kerf's width IS the tool, so a re-ground blade
    // is a different cut - marked Stale through here, never through
    // the params stamp, which never reads it.
    match tool.kind {
        crate::tools::ToolKind::Drill => w(h, tool.tip_angle),
        crate::tools::ToolKind::Tap => w(h, tool.pitch),
        crate::tools::ToolKind::Groove => w(h, tool.width),
        // The electrode's Y size is its face (#541); a body electrode's
        // outline is its section (#1152) - bytes only when it states
        // one, so no rectangular electrode's digest moves.
        crate::tools::ToolKind::Electrode => {
            w(h, tool.width);
            for p in &tool.outline {
                w(h, p[0]);
                w(h, p[1]);
            }
        }
        // The threading insert digests BOTH its profile numbers
        // (M5.2 D): the crest width and the flanks' angle are the
        // wave's footprint - a recut V is a different thread even
        // when the operation's pitch and depth never moved.
        crate::tools::ToolKind::Thread => {
            w(h, tool.width);
            w(h, tool.tip_angle);
        }
        // The gauge digests its band width - the kerf's span, the
        // blade's law - and its crown needs no line here: the arc
        // radius rides the global nose_radius row above, read at
        // the gate that makes it mean anything (M5.2 E2).
        crate::tools::ToolKind::Form => w(h, tool.width),
        // The chamfer mill digests its cone angle from its first
        // build (M2.4 B): the congruence door reads it against the
        // face's angle, so a re-cut cone is a different chamfer even
        // when width, depth and the wall itself never moved.
        crate::tools::ToolKind::Taper => w(h, tool.tip_angle),
        // The thread mills digest their form (#894): the flank angle
        // and the pitch range decide which threads the tool may cut
        // and how many rows ride a revolution (the pitch itself rides
        // the Thread clause below).
        crate::tools::ToolKind::ThreadMill | crate::tools::ToolKind::ThreadMillSingle => {
            w(h, tool.tip_angle);
            w(h, tool.pitch_min);
        }
        _ => {}
    }
    // A Thread OP times its helix against the TOOL's pitch (M2.4 C, the
    // tap cycle's reading), whatever kind carries it - so the clause
    // belongs to the operation, not the tool kind: the turning thread
    // insert's pitch stays inert there (that ride reads pitch from
    // params), and no pre-M2.4-C digest changes, because only the new
    // operation kinds reach past this line.
    if matches!(params, OpParams::Thread { .. }) {
        w(h, tool.pitch);
    }
    // M11.3: a tool that may not plunge into solid changes the entries;
    // a byte only then, so no centre-cutting op's digest moves.
    if !tool.center_cutting {
        h.write_u8(0xCC);
    }
    // #368: a modelled body's envelope - bytes only when there is one,
    // so no digest without a body moves.
    crate::tool_body::digest(world, &tool_entity, h);
    // #852: a drawn neck the stack laws read - bytes only with one.
    crate::tool_cutter::digest(world, &tool_entity, &tool, h);
    Some(())
}
