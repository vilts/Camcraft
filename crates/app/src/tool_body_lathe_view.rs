//! Lathe tool-body editing reads (#361): what the tool editor and the
//! MCP tools need to place a lathe-held body without committing
//! anything - which frame a tool kind's body is placed in, the
//! placement preview (the same place-and-build `SetToolBody::lathe`
//! runs), the placement a landed body re-edits with, an insert zone
//! from its reaches, and the body's extent in the turning tool frame.
//! Specced from the handbook turning-tool convention (X radial, Z along
//! the spindle) `camcraft_tools::lathe_holder` states; plain linear
//! algebra. Our own design; no vendor file or source consulted.
//!
//! Nothing here writes the world: the one writer stays `SetToolBody`
//! (AGENTS.md: everything is a command).

use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

use crate::geometry::Triangle;
use crate::schema::ToolBody;
use crate::tool_body::LATHE_HELD;
use crate::tool_body_lathe;
use crate::tools::ToolKind;

/// How a landed lathe body re-edits: its facets already stand in the
/// turning tool frame (+X radial, +Z along the spindle, tip at the
/// origin), so this placement re-applies to the same facets.
pub const AS_PLACED: LathePlacement = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };

/// Which frame a tool kind's body is placed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameChoice {
    /// A spun tool: along the spindle axis only (#368).
    Spindle,
    /// A lathe steel: the turning tool frame only (#369).
    Lathe,
    /// A boring bar serves a mill's spindle and a lathe's turret.
    Either,
}

/// The frame(s) a body on a tool of `kind` may be placed in - the same
/// split `tool_body::fits` and `tool_body_lathe::fits` refuse by.
#[must_use]
pub fn frame_choice(kind: ToolKind) -> FrameChoice {
    if LATHE_HELD.contains(&kind) {
        FrameChoice::Lathe
    } else if kind == ToolKind::Bore {
        FrameChoice::Either
    } else {
        FrameChoice::Spindle
    }
}

/// The placement as `SetToolBody::lathe` would judge it, without
/// landing it: the body placed in the turning tool frame with its
/// clearance bands, or the refusal by name. (The tool-fit law - the
/// zone against the tool's own insert size - is the command's.)
pub fn preview(model: &[Triangle], place: &LathePlacement, zone: Option<InsertZone>) -> Result<ToolBody, String> {
    let placed = tool_body_lathe::place(model, place)?;
    tool_body_lathe::build("preview", *place, zone, placed)
}

/// An insert zone from how far it reaches from the tip, per axis (x
/// radial, y normal, z along the spindle): `toward` on the negative
/// side, `away` on the positive, both stated as lengths. A negative or
/// non-finite reach is passed on as it is, so the zone's own law
/// (`InsertZone::check`) names it.
#[must_use]
pub fn zone_of_reaches(toward: [f64; 3], away: [f64; 3]) -> InsertZone {
    InsertZone { lo: toward.map(|v| -v), hi: away }
}

/// The reaches [`zone_of_reaches`] takes, read back off a zone.
#[must_use]
pub fn reaches_of_zone(zone: &InsertZone) -> ([f64; 3], [f64; 3]) {
    (zone.lo.map(|v| -v), zone.hi)
}

/// The least and greatest coordinates of a body's facets in its own
/// (tool) frame, per axis; `None` for a body with no finite facet.
#[must_use]
pub fn extent(body: &ToolBody) -> Option<([f64; 3], [f64; 3])> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for t in body.facets.iter() {
        for p in [t.a, t.b, t.c] {
            for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                if v.is_finite() {
                    lo[k] = lo[k].min(v);
                    hi[k] = hi[k].max(v);
                }
            }
        }
    }
    (0..3).all(|k| lo[k] <= hi[k]).then_some((lo, hi))
}
