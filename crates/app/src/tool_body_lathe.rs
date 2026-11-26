//! Lathe-held tool bodies (#369): a holder or boring-bar mesh placed in
//! the turning tool frame by numbers (the model directions that are
//! radial and along the spindle, the model point that is the tip), the
//! clearance bands the revolved-stock law reads off it, and which tools
//! may carry one. Specced from the handbook turning-tool convention (X
//! radial, Z along the spindle); the frame change is plain linear
//! algebra. Our own design; no vendor file or source consulted.
//!
//! The laws here:
//! - **Lathe steel only.** A turning insert, grooving blade, threading
//!   insert, form gauge or boring bar may carry a lathe body; any other
//!   kind is refused by name, at the command, at `ChangeTool` and on
//!   load (`tool_body::fits`).
//! - **Refusals by name** for a placement that is no frame (a zero or
//!   non-finite direction, directions not square, a wild tip) and a mesh
//!   that is no body (empty, non-finite, degenerate, flat along the
//!   spindle, too large, its tip far from it) -
//!   `camcraft_tools::lathe_holder`.
//! - **An insert zone holds one insert, never the holder** (#396): a box
//!   about the tip the judge leaves out, reaching no farther along any
//!   axis than the tool's own record allows an insert to - its body size
//!   (`diameter`), a blade's half width along the spindle, and 25.4 mm
//!   (the largest ISO turning insert) at most - and never the whole
//!   body. Refused by name at the command, at `ChangeTool` and on load.

use std::sync::Arc;

use flecs_ecs::prelude::{Entity, World};

use camcraft_tools::lathe_holder::{LatheClearance, LathePlacement};
use camcraft_tools::lathe_insert_zone::{InsertZone, MAX_INSERT_REACH};

use crate::geometry::{Point3, Triangle};
use crate::schema::ToolBody;
use crate::tool_body::{body_of, BodyFrame, LATHE_HELD};
use crate::tools::{Tool, ToolKind};

/// The informational note a turning op carries when its tool has no
/// lathe body: only the cutting footprint was judged.
pub const HOLDER_NOTE: &str = "holder not judged - the turning tool carries no modelled body, so only its cutting footprint was judged against the stock";

/// Place model-frame facets in the turning tool frame.
pub fn place(model: &[Triangle], place: &LathePlacement) -> Result<Vec<Triangle>, String> {
    let tris: Vec<[[f64; 3]; 3]> = model.iter().map(tri).collect();
    let placed = place.place(&tris).map_err(|e| e.to_string())?;
    Ok(placed.iter().map(|t| Triangle::new(pt(t[0]), pt(t[1]), pt(t[2]))).collect())
}

/// A lathe body built from facets already placed in the turning tool
/// frame: its clearance bands derived (less its insert zone, #396),
/// refused by name when the mesh is no lathe body or the zone no zone.
pub fn build(source: &str, place: LathePlacement, zone: Option<InsertZone>, placed: Vec<Triangle>) -> Result<ToolBody, String> {
    place.basis().map_err(|e| e.to_string())?;
    if let Some(z) = &zone {
        z.check([MAX_INSERT_REACH; 3]).map_err(|e| e.to_string())?;
    }
    let tris: Vec<[[f64; 3]; 3]> = placed.iter().map(tri).collect();
    let clearance = LatheClearance::of_zoned(&tris, zone.as_ref()).map_err(|e| e.to_string())?;
    let facets: Arc<[Triangle]> = Arc::from(placed);
    let pieces = vec![crate::tool_body_assembly::BodyPiece {
        source: source.to_owned(),
        facets: Arc::clone(&facets),
        csys: Vec::new(),
    }];
    Ok(ToolBody { source: source.to_owned(), facets, frame: BodyFrame::Lathe { place, zone, clearance: Arc::new(clearance) }, spun: None, machine_default: false, library: None, pieces })
}

/// Can `tool` carry a lathe body (with insert zone `zone`)? Only a
/// lathe steel can, and only with a zone one of its inserts could fill.
pub fn fits(tool: &Tool, zone: Option<&InsertZone>) -> Result<(), String> {
    if !(LATHE_HELD.contains(&tool.kind) || tool.kind == ToolKind::Bore) {
        return Err(format!("a {} is no lathe steel - a lathe-frame body needs a turning, grooving, threading or form insert or a boring bar", tool.kind));
    }
    zone.map_or(Ok(()), |z| z.check(zone_cap(tool)).map_err(|e| e.to_string()))
}

/// How far from the tip, per axis (x radial, y normal, z along the
/// spindle), an insert of `tool` can reach: its body size, which no
/// insert clamped on it outgrows, and along the spindle a grooving or
/// form blade's half width (the tip is the band centre and the blade is
/// the band). A thread insert's laydown plate is wider than its V, so
/// it takes the body size. `InsertZone::check` holds each to 25.4 mm.
fn zone_cap(tool: &Tool) -> [f64; 3] {
    let body = if tool.diameter.is_finite() { tool.diameter.max(0.0) } else { 0.0 };
    let z = match tool.kind {
        ToolKind::Groove | ToolKind::Form => body.min(tool.width.max(0.0) * 0.5),
        _ => body,
    };
    [body, body, z]
}

/// The clearance bands of the tool entity's lathe body, if it has one.
#[must_use]
pub fn clearance_of(world: &World, entity: &Entity) -> Option<Arc<LatheClearance>> {
    match body_of(world, entity)?.frame {
        BodyFrame::Lathe { clearance, .. } => Some(clearance),
        BodyFrame::Spindle { .. } => None,
    }
}

fn tri(t: &Triangle) -> [[f64; 3]; 3] {
    [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]]
}

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}
