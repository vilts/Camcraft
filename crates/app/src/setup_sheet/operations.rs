//! Setup-sheet operation table: the program-ordered rows with their
//! resolved depths, and the drill-cycle lines under each Drill row (M3.2
//! stage D). Provenance: our own text; machine-shop setup-sheet
//! convention, behavior only - see the module doc in `setup_sheet/mod.rs`.

use flecs_ecs::prelude::World;

use super::heights::height_fields;
use super::mm;
use crate::cl::CycleKind;
use crate::drilling::{self, CycleChoice};
use crate::model;
use crate::operation::{self, OpParams, SrcRef};
use crate::setup;

pub(super) fn render_operations(world: &World, target: Option<&String>, out: &mut String) {
    out.push_str("OPERATIONS (program order)\n");
    out.push_str(&format!(
        "  {:>2}  {:<12} {:<10} {:<14} {:<12} {:>8} {:>8} {:>8} {:>8}\n",
        "#", "NAME", "TYPE", "TOOL", "SOURCE", "CLR", "RET", "TOP", "BOT"
    ));
    let mut count = 0usize;
    for (name, entity) in setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &entity) else {
            continue;
        };
        // Membership by the resolution law, so implicit members (declared
        // `None`) ride in exactly where the program puts them.
        let in_this = match (setup::effective(world, op.setup.as_deref()), target) {
            (Some(s), Some(t)) => s == *t,
            (None, None) => true,
            _ => false,
        };
        if !in_this {
            continue;
        }
        count += 1;
        let src = src_label(&op.src);
        let cols = height_fields(&op.params);
        let mut cells: [String; 4] = std::array::from_fn(|_| String::new());
        let mut notes: Vec<String> = Vec::new();
        for (i, (col, field, expr)) in cols.iter().enumerate() {
            match expr {
                Some(h) => match operation::resolve_height(world, op.setup.as_deref(), Some(&op.src), *h) {
                    Some(z) => cells[i] = mm(z),
                    None => {
                        cells[i] = "-".to_string();
                        let why = operation::missing_source(
                            world,
                            op.setup.as_deref(),
                            Some(&op.src),
                            h.reference,
                        )
                        .unwrap_or("the height does not resolve");
                        notes.push(format!(
                            "      ! {col} ({field}): From {} {} - {why}",
                            h.reference,
                            signed(h.offset)
                        ));
                    }
                },
                None => cells[i] = "-".to_string(),
            }
        }
        out.push_str(&format!(
            "  {count:>2}  {:<12} {:<10} {:<14} {:<12} {:>8} {:>8} {:>8} {:>8}\n",
            model::operation_label(world, &name, &entity),
            op.params.kind(),
            tool_cell(&op.params, &op.tool),
            src,
            cells[0],
            cells[1],
            cells[2],
            cells[3],
        ));
        for n in notes {
            out.push_str(&n);
            out.push('\n');
        }
        render_wire(world, op.setup.as_deref(), &op.params, out);
        render_cycles(world, op.setup.as_deref(), &op.src, &op.tool, &op.params, out);
    }
    if count == 0 {
        out.push_str("  (no operations in this setup)\n");
    }
    out.push('\n');
}

/// One line per hole of a Drill operation: the cycle kind, the XY, and
/// the bottom that hole machines to (the params override when set, the
/// hole's own floor otherwise - the same rule `drilling` cuts by).
/// Silent for every other operation kind - the Drill arm lives in the
/// params, not at the call site.
fn render_cycles(
    world: &World,
    op_setup: Option<&str>,
    src: &operation::SrcRef,
    tool_name: &str,
    params: &OpParams,
    out: &mut String,
) {
    let OpParams::Drill {
        top_z,
        bottom_z,
        cycle,
        holes,
        ..
    } = params
    else {
        return;
    };
    if holes.is_empty() {
        out.push_str("      (no holes)\n");
        return;
    }
    let r_z = operation::resolve_height(world, op_setup, Some(src), *top_z);
    let override_z =
        (*bottom_z).and_then(|b| operation::resolve_height(world, op_setup, Some(src), b));
    let tool = model::find_by_name(world, tool_name).and_then(|e| model::tool_of(world, &e));
    for (i, h) in holes.iter().enumerate() {
        let bottom = override_z.unwrap_or(h.floor_z);
        let kind = match cycle {
            CycleChoice::Auto => match (&tool, r_z) {
                (Some(t), Some(r)) if (r - bottom).is_finite() && t.diameter.is_finite() => {
                    kind_label(drilling::auto_kind(t.kind, r - bottom, t.diameter))
                }
                _ => "auto".to_string(),
            },
            other => choice_label(*other).to_lowercase(),
        };
        let thru = if h.through { "  thru" } else { "" };
        out.push_str(&format!(
            "      hole {:>2}  {:<10} X {}  Y {}  bottom {}{thru}\n",
            i + 1,
            kind,
            mm(h.xy.x),
            mm(h.xy.y),
            mm(bottom),
        ));
    }
}

/// The TOOL cell (#1110): the library tool's name, or for a kind that
/// mounts none (the wire family) the EDM condition it cuts with.
fn tool_cell(params: &OpParams, tool: &str) -> String {
    if params.kind().info().mounts_tool() {
        return tool.to_owned();
    }
    match params {
        OpParams::WireContour(w) => format!("cond {}", w.condition),
        _ => "(no tool)".to_owned(),
    }
}

/// The line under a wire cut's row (#1110): the condition, wire
/// diameter and spark gap its setup's machine states for it - read
/// through the same door generation reads (`wire_op::condition_of`), so
/// the sheet never states a number the cut was not made with. A
/// condition the machine no longer states is said by name. Silent for
/// every kind that mounts a library tool.
fn render_wire(world: &World, op_setup: Option<&str>, params: &OpParams, out: &mut String) {
    let OpParams::WireContour(w) = params else {
        return;
    };
    match operation::wire_op::condition_of(world, op_setup, w) {
        Ok(c) => out.push_str(&format!(
            "      CONDITION {}  WIRE D {}  GAP {}\n",
            c.id,
            mm(c.wire_diameter),
            mm(c.gap)
        )),
        Err(why) => out.push_str(&format!("      ! wire condition: {why}\n")),
    }
}

// ------------------------------------------------------------------- helpers

/// The operation's geometry source, labeled.
fn src_label(src: &SrcRef) -> String {
    match src {
        SrcRef::Stock => "stock".to_string(),
        SrcRef::Entity(name) => format!("model '{name}'"),
        SrcRef::Selection(items) => format!("selection x{}", items.len()),
    }
}

/// A height offset with its sign printed (the reference Display words the
/// source; the signed number words the push).
fn signed(v: f64) -> String {
    if v >= 0.0 {
        format!("+{}", mm(v))
    } else {
        mm(v)
    }
}

fn kind_label(k: CycleKind) -> String {
    let s = match k {
        CycleKind::Spot => "spot",
        CycleKind::Drill => "drill",
        CycleKind::Peck => "peck",
        CycleKind::Deep => "deep-peck",
        CycleKind::Tap => "tap",
        CycleKind::FloatingTap => "tap (floating holder)",
        CycleKind::Bore => "bore",
        CycleKind::BackBore => "back-bore",
        CycleKind::Ream => "ream",
    };
    s.to_string()
}

fn choice_label(c: CycleChoice) -> &'static str {
    match c {
        CycleChoice::Auto => "Auto",
        CycleChoice::Spot => "Spot",
        CycleChoice::Drill => "Drill",
        CycleChoice::Peck => "Peck",
        CycleChoice::Tap => "Tap",
        CycleChoice::FloatingTap => "Tap (floating holder)",
        CycleChoice::Bore => "Bore",
        CycleChoice::BackBore => "BackBore",
        CycleChoice::Ream => "Ream",
    }
}
