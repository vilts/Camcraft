//! What the centre-cutting audit reads as solid for the kinds that carry
//! no approach (M11.3 owed (5): Face and the 3-D finishes): the setup's
//! stock in the generator's own frame - its XY footprint and its top -
//! or, with no separate stock, the material the op cuts. Read only for
//! a tool that is not centre-cutting, so no centre-cutting path pays.
//! Rotary Offset reads the true revolved bar instead ([`audit_bar`], #16).
//!
//! Provenance: the stock-as-solid reading of the centre-cutting plunge
//! law (`camcraft_motion::plunge_law`); this project's own design notes.

use super::{FOOTPRINT_SAMPLES, GenError, Material, Region, Toolpath, Triangle, slice};

/// The stock read as solid: `(top, footprint)`. A footprint that cannot
/// be projected is `None`, which the law reads as solid everywhere
/// under the top - unseen stock is solid.
pub(super) fn plunge_solid(
    stock_facets: Option<&[Triangle]>,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
) -> (f64, Option<Region>) {
    let local: Vec<Triangle> = match (stock_facets, frame) {
        (Some(s), Some(f)) => f.local_facets(s),
        (Some(s), None) => s.to_vec(),
        (None, _) => material.facets.clone(),
    };
    let top = local
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .filter(|z| z.is_finite())
        .fold(material.top, f64::max);
    let footprint = slice::project(&local, FOOTPRINT_SAMPLES).ok().filter(|r| !r.is_empty());
    (top, footprint)
}

/// The facer's entry for a tool that is not centre-cutting (M11.3 owed
/// (4)): `path` as it stands when its plunges fall through air; else the
/// path `relead` regenerates with the first row's lead-in run on out to
/// air (`camcraft_motion::machining2d::air_lead`, kept off the area
/// law's `forbidden` zones); else - no air within reach, or a zone in
/// the way - the centre-cutting refusal by name, its reason attached.
/// The regenerated path is audited again, so a run-on that still lands
/// on stock can never pass.
pub(super) fn face_air_entry(
    path: Toolpath,
    stock: (Option<&[Triangle]>, &Material, Option<&crate::cl::TiltedFrame>),
    forbidden: &[(&'static str, Region)],
    relead: impl FnOnce(f64) -> Result<Toolpath, GenError>,
    tool_r: f64,
) -> Result<Toolpath, GenError> {
    use camcraft_motion::plunge_law::{audit, Solid};
    const SITE: &str = "the facing entry";
    let (top, fp) = plunge_solid(stock.0, stock.1, stock.2);
    let solid = Solid { top, footprint: fp.as_ref() };
    let blocked: Vec<(&str, &Region)> = forbidden.iter().map(|(l, z)| (*l, z)).collect();
    match crate::machining2d::air_lead(&path, tool_r, &solid, &blocked) {
        Ok(None) => Ok(path),
        Ok(Some(lead)) => {
            let led = relead(lead)?;
            audit(&led, false, tool_r, &solid, SITE)?;
            Ok(led)
        }
        // The plunge law names its spot (#261 part 2) and hands back the
        // audited path as its ghost (part 3): same words, both kept.
        Err(why) => audit(&path, false, tool_r, &solid, SITE)
            .map_err(|e| e.map_msg(|m| format!("{m} ({why})")))
            .map(|()| path),
    }
}

/// The centre-cutting law on a multiaxis kind built on finish params
/// (M11.3 owed: 5-Axis Contour / Parallel / Flow, the swarf and rotary
/// twins): the posed path is judged along the tool's own axis
/// (`plunge_law::posed_axial`), retried with the finish's sloped
/// entries, else refused by name. A centre-cutting tool reads no stock
/// and gets `run`'s path untouched. `ball` says the tool is a ball nose:
/// a posed link's re-entry into the sphere the last cut emptied is then
/// no plunge (`plunge_law::reenters_cut_sphere`, #17).
#[allow(clippy::too_many_arguments)] // the arm's flat read of the law's inputs
pub(super) fn guard_posed<F>(
    kind: &str,
    center_cutting: bool,
    ball: bool,
    stock_facets: Option<&[Triangle]>,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
    fp: &super::Finish3DParams,
    run: F,
) -> Result<super::Toolpath, super::GenError>
where
    F: Fn(&super::Finish3DParams) -> Result<super::Toolpath, super::GenError>,
{
    if center_cutting {
        return run(fp);
    }
    let (top, footprint) = plunge_solid(stock_facets, material, frame);
    let solid = camcraft_motion::plunge_law::Solid { top, footprint: footprint.as_ref() };
    camcraft_motion::finishing3d::center_cut_guard_shaped(kind, false, &solid, ball, fp, run)
}

/// The same law, audit-only, for Rotary Offset (#16): solid is the
/// TRUE bar - the setup stock revolved about the axis the kernel
/// wrapped about (`multiaxis::revolved_bar_stated`, the stated line
/// or the fit `ax` names, read from the generator's own facets so it
/// stands in the path's frame) - not the box reading of top Z over
/// the outline, which also counted the air around the bar. A bar that
/// cannot be read refuses by name: the audit never guesses air.
pub(super) fn audit_bar(
    kind: &str,
    center_cutting: bool,
    facets: &[Triangle],
    stock_facets: Option<&[Triangle]>,
    path: &super::Toolpath,
    tool_r: f64,
    ax: &crate::multiaxis::axis::Axis,
) -> Result<(), super::GenError> {
    if center_cutting {
        return Ok(());
    }
    let bar = crate::multiaxis::revolved_bar_stated(facets, stock_facets, *ax)
        .map_err(|e| GenError::Invalid(format!("{kind}: the bar the centre-cutting audit reads cannot be built: {e}")))?;
    camcraft_motion::plunge_law::audit_bar(path, false, tool_r, &bar, &format!("{kind}'s entry descent"))
}

/// The same law, audit-only, for a posed kind with no sloped entry to
/// retry with (the drive-curve contour): a plunge into solid refuses
/// by name.
pub(super) fn audit_posed(
    kind: &str,
    center_cutting: bool,
    stock_facets: Option<&[Triangle]>,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
    path: &super::Toolpath,
    tool_r: f64,
) -> Result<(), super::GenError> {
    if center_cutting {
        return Ok(());
    }
    let (top, footprint) = plunge_solid(stock_facets, material, frame);
    let solid = camcraft_motion::plunge_law::Solid { top, footprint: footprint.as_ref() };
    camcraft_motion::plunge_law::audit(path, false, tool_r, &solid, &format!("{kind}'s entry descent"))
}
