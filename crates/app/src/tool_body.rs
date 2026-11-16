//! Tool bodies (#368): a holder or assembly mesh attached to a tool,
//! placed in the tool frame by numbers (the model direction that is the
//! tool axis, the model origin's height above the tip), and the reads
//! every judging law takes from it - the tool with its holder widened to
//! the cylinder enclosing the body, and the body's own band stack for
//! the fixture laws. Specced from the revolved-solid reading of a tool
//! assembly in machine-tool handbooks; the frame change is plain linear
//! algebra. Our own design; no vendor file or source consulted.
//!
//! The laws here:
//! - **No body, no change.** A tool without a [`ToolBody`] reads exactly
//!   its own record, so every output stays byte-identical.
//! - **Never smaller than the body.** Laws that read one holder cylinder
//!   get the cylinder enclosing the body ([`HolderProfile::enclose`]);
//!   the fixture laws, the ones that name a collision, read its bands.
//! - **Refusals by name**, at the command and again at use: a body that
//!   is empty, non-finite, degenerate, flat, off the axis, below the
//!   tip, too large, or reaching into the flutes.
//! - **One body, one frame (#369).** A milling body is placed along the
//!   spindle axis ([`BodyFrame::Spindle`]); a lathe steel's in the
//!   turning tool frame ([`BodyFrame::Lathe`], `crate::tool_body_lathe`).
//!   The milling laws read only the first; a lathe body leaves the tool
//!   record as it is for them and is judged by the revolved-stock law.

use std::sync::Arc;

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use camcraft_tools::holder_profile::{HolderProfile, HolderStack, MAX_BODY_HEIGHT};
use camcraft_tools::spun::SpunProfile;
use camcraft_tools::lathe_holder::{LatheClearance, LathePlacement};
use camcraft_tools::lathe_insert_zone::InsertZone;

use crate::geometry::{Point3, Triangle};
use crate::schema::ToolBody;
use crate::tools::{Tool, ToolGeom, ToolKind};

/// The frame a body is placed in, the placement it landed with, and
/// what the laws read off it.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyFrame {
    /// #368: a milling body - the model direction that became the tool
    /// axis, the model origin's height above the tip, and its revolved
    /// envelope.
    Spindle { axis: [f64; 3], offset: f64, profile: HolderProfile },
    /// #369: a lathe-held body - its placement in the turning tool
    /// frame, its optional insert zone (#396: the insert plate the
    /// judge leaves out), and its clearance bands along the spindle.
    Lathe { place: LathePlacement, zone: Option<InsertZone>, clearance: Arc<LatheClearance> },
}

/// The tool kinds a lathe holds: they take a lathe-frame body only
/// (`Bore` serves both machines and takes either).
pub const LATHE_HELD: [ToolKind; 4] = [ToolKind::Turning, ToolKind::Groove, ToolKind::Thread, ToolKind::Form];

/// The body on a tool entity, if it has one.
#[must_use]
pub fn body_of(world: &World, entity: &Entity) -> Option<ToolBody> {
    entity.entity_view(world).try_get::<&ToolBody>(|b| b.clone())
}

/// Land (`Some`) or clear (`None`) a tool's body - the one writer,
/// called by `SetToolBody` and the project loader.
pub fn set_body(world: &World, entity: &Entity, body: Option<&ToolBody>) {
    let view = entity.entity_view(world);
    match body {
        Some(b) => {
            view.set(b.clone());
        }
        None => {
            view.remove(world.component::<ToolBody>());
        }
    }
}

/// Place a model-frame mesh in the tool frame: `axis` (any non-zero
/// direction, model frame) becomes +Z through the model origin, and the
/// origin stands `offset` above the tip. Refuses a zero or non-finite
/// axis and a non-finite or absurd offset by name.
pub fn place(model: &[Triangle], axis: [f64; 3], offset: f64) -> Result<Vec<Triangle>, String> {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if !(n.is_finite() && n > 1e-12) {
        return Err("the tool axis direction must be a finite, non-zero vector".into());
    }
    if !(offset.is_finite() && offset.abs() <= MAX_BODY_HEIGHT) {
        return Err(format!("the tip offset must be a finite length within {MAX_BODY_HEIGHT} mm"));
    }
    let a = [axis[0] / n, axis[1] / n, axis[2] / n];
    // The perpendicular pair: +Z maps X to X and Y to Y (the identity
    // a vendor file modelled along Z expects); any other axis takes a
    // right-handed pair off the least-aligned helper.
    let helper = if a[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let u = unit(cross(helper, a)).ok_or("the tool axis direction must be a finite, non-zero vector")?;
    let v = cross(a, u);
    let map = |p: Point3| {
        let q = [p.x, p.y, p.z];
        Point3::new(dot(q, u), dot(q, v), dot(q, a) + offset)
    };
    Ok(model.iter().map(|t| Triangle::new(map(t.a), map(t.b), map(t.c))).collect())
}

/// A body built from placed facets: its envelope derived, refused by
/// name when the mesh is no tool body.
pub fn build(source: &str, axis: [f64; 3], offset: f64, placed: Vec<Triangle>) -> Result<ToolBody, String> {
    let tris: Vec<[[f64; 3]; 3]> = placed
        .iter()
        .map(|t| [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]])
        .collect();
    let profile = HolderProfile::envelope(&tris).map_err(|e| e.to_string())?;
    let facets: Arc<[Triangle]> = Arc::from(placed);
    let pieces = vec![crate::tool_body_assembly::BodyPiece {
        source: source.to_owned(),
        facets: Arc::clone(&facets),
        csys: Vec::new(),
    }];
    Ok(ToolBody { source: source.to_owned(), facets, frame: BodyFrame::Spindle { axis, offset, profile }, spun: None, machine_default: false, pieces })
}

/// What a drawn (spun) holder body is called where a file name would be.
pub const SPUN_SOURCE: &str = "drawn profile";

/// A body drawn as a spun profile (#93), already in the tool frame (a
/// spindle-frame body, axis +Z, origin on the tip): its facets the
/// revolve of the conservative outline (for display and for any reader
/// that only has facets), its envelope the profile's exact bands. The
/// validation is the profile's own (`SpunProfile::new`).
#[must_use]
pub fn build_spun(spun: SpunProfile) -> ToolBody {
    let facets = crate::tool_body_view::revolve(&spun.outline());
    ToolBody {
        source: SPUN_SOURCE.to_owned(),
        facets: Arc::from(facets),
        frame: BodyFrame::Spindle { axis: [0.0, 0.0, 1.0], offset: 0.0, profile: spun.holder_profile() },
        spun: Some(spun),
        machine_default: false,
        pieces: Vec::new(),
    }
}

/// Can `tool` carry `body`? A lathe-held insert takes a lathe-frame
/// body (#369) and a lathe-frame body needs a lathe steel; a spindle
/// body reaching into the flutes has no cylinder stack that could
/// stand it.
pub fn fits(tool: &Tool, body: &ToolBody) -> Result<(), String> {
    let profile = match &body.frame {
        BodyFrame::Lathe { zone, .. } => return crate::tool_body_lathe::fits(tool, zone.as_ref()),
        BodyFrame::Spindle { profile, .. } => profile,
    };
    if LATHE_HELD.contains(&tool.kind) {
        return Err(format!(
            "a {} is lathe-held - lathe holders are placed in the turning tool frame (radial and spindle directions, tip point), not along a spindle axis",
            tool.kind
        ));
    }
    if profile.enclose(tool).is_none() {
        return Err(format!(
            "the tool body reaches into the flutes (its foot is {:.3} mm above the tip, the flutes run {:.3} mm)",
            profile.bottom(),
            tool.flute_len
        ));
    }
    Ok(())
}

/// The tool as every judging law should read it, and the body behind
/// it: unchanged with no body; with one, the holder widened to the
/// cylinder enclosing the body - a machine-default body (#828) first
/// floored at the tool's own holder cylinder, an explicit one as drawn. Refuses by name when the body no longer
/// fits the tool (a later edit shortened the shank under it). A lathe
/// body (#369) leaves the record as it is: the revolved-stock law
/// reads it (`tool_body_lathe::clearance_of`), no cylinder law does.
pub fn judged(world: &World, entity: &Entity, tool: Tool) -> Result<(Tool, Option<HolderProfile>), String> {
    let Some(body) = body_of(world, entity) else {
        return Ok((tool, None));
    };
    fits(&tool, &body)?;
    match body.frame {
        BodyFrame::Lathe { .. } => Ok((tool, None)),
        BodyFrame::Spindle { profile, .. } => {
            // #828: an assumed (machine-default) holder never judges
            // looser than the stand-in cylinder the bare tool gets.
            let profile = if body.machine_default { profile.floored(&tool.geom()) } else { profile };
            let enclosed = profile.enclose(&tool).ok_or("the tool body reaches into the flutes")?;
            Ok((enclosed, Some(profile)))
        }
    }
}

/// What the fixture laws sweep above the shank of `geom`: the body's
/// bands, or the holder cylinder when there is no body.
#[must_use]
pub fn stack(geom: &ToolGeom, body: Option<&HolderProfile>) -> HolderStack {
    body.map_or_else(|| HolderStack::cylinder(geom), |p| p.stack(geom))
}

/// The body's term in an operation's stamp: nothing without one (no
/// digest moves), the envelope's bands with one - the only numbers a
/// law reads off it.
pub fn digest(world: &World, entity: &Entity, h: &mut impl std::hash::Hasher) {
    let body = body_of(world, entity);
    // #828: an assumed holder is judged floored - its own stamp term.
    if body.as_ref().is_some_and(|b| b.machine_default) {
        h.write(b"machine-default");
    }
    match body.map(|b| b.frame) {
        None => {}
        Some(BodyFrame::Spindle { profile, .. }) => {
            h.write(b"tool-body");
            for band in profile.bands() {
                for v in [band.above, band.len, band.r] {
                    h.write(&v.to_bits().to_le_bytes());
                }
            }
        }
        // #369: the clearance bands, the only numbers its law reads.
        Some(BodyFrame::Lathe { clearance, .. }) => {
            h.write(b"lathe-body");
            for v in clearance.words() {
                h.write(&v.to_bits().to_le_bytes());
            }
        }
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
