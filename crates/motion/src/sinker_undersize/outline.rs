//! Provenance: general die-sink EDM wall offsets, using the geometry crate's published Clipper2 polygon operations (#1810).
//!
//! Compare electrode material with inward wall offsets. A sharp convex
//! corner offsets further than an axis extent. The stock verdict still
//! judges the round spark gap and any stock left at those corners.

use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;
use crate::sinker_orbit::OrbitKind;
use camcraft_tools::electrode_outline::{bounds, shape_refusal};

use super::{judge, SinkerPass, Undersize, SIZE_SLACK};

fn invalid(why: impl std::fmt::Display) -> GenError {
    GenError::Invalid(format!("sinker cavity outline: {why}"))
}

fn region(points: &[[f64; 2]], name: &str) -> Result<Region, GenError> {
    if let Some(why) = shape_refusal(points) {
        return Err(invalid(why.replace("electrode", name)));
    }
    let ring =
        Loop::new(points.iter().map(|p| Point2::new(p[0], p[1])).collect()).map_err(invalid)?;
    let region = Region::new(vec![ring]).map_err(invalid)?;
    if region.loops().len() != 1 {
        return Err(invalid(format!(
            "the {name} must enclose one nonempty section"
        )));
    }
    Ok(region)
}

/// The cavity outline's structural refusal, before command creation.
#[must_use]
pub fn cavity_outline_refusal(points: &[[f64; 2]]) -> Option<String> {
    region(points, "cavity").err().map(|e| e.to_string())
}

/// Check a structurally validated cavity outline against its stated size.
#[must_use]
pub fn cavity_outline_size_refusal(points: &[[f64; 2]], stated: (f64, f64)) -> Option<String> {
    check_extents(points, stated, "cavity")
        .err()
        .map(|e| e.to_string())
}

fn inset(cavity: &Region, distance: f64) -> Result<Region, GenError> {
    let offset = cavity.offset_sharp(-distance).map_err(invalid)?;
    if offset.loops().len() != 1 {
        return Err(invalid(
            "the wall offset consumes or splits the cavity; this burn cannot be certified",
        ));
    }
    Ok(offset)
}

fn check_extents(points: &[[f64; 2]], stated: (f64, f64), name: &str) -> Result<(), GenError> {
    let Some((lo, hi)) = bounds(points) else {
        return Err(invalid(format!("the {name} has no points")));
    };
    let actual = (hi[0] - lo[0], hi[1] - lo[1]);
    if !((actual.0 - stated.0).abs() <= SIZE_SLACK && (actual.1 - stated.1).abs() <= SIZE_SLACK) {
        return Err(invalid(format!(
            "the {name} spans {:.6} x {:.6}, but its stated size is {} x {}",
            actual.0, actual.1, stated.0, stated.1
        )));
    }
    Ok(())
}

/// Judge a body electrode against the cavity's wall offsets about the burn point.
/// On success, return the stand-off above the finished floor.
///
/// # Errors
/// Refuse malformed outlines, excessive electrode material, an incomplete
/// finish, an unsafe floor gap, or an orbit whose wall reach is not proved.
pub fn judge_outline(
    u: &Undersize,
    cavity: &[[f64; 2]],
    electrode: &[[f64; 2]],
    condition: &str,
) -> Result<f64, GenError> {
    if u.pass == SinkerPass::Single {
        return judge(u, condition);
    }
    let cavity_region = region(cavity, "cavity")?;
    let electrode_region = region(electrode, "electrode")?;
    check_extents(cavity, u.cavity, "cavity")?;
    check_extents(electrode, u.electrode, "electrode")?;

    // These virtual axis widths express wall offsets, not the sharp
    // outline's extents. The existing law keeps the numeric, condition,
    // floor and orbit checks, without judging the diamond's corner recess.
    let wall = Undersize {
        electrode: (
            u.cavity.0 - 2.0 * u.undersize,
            u.cavity.1 - 2.0 * u.undersize,
        ),
        outline: false,
        ..*u
    };
    let stand = judge(&wall, condition)?;
    let radius = u.orbit.map_or(0.0, |o| o.radius);
    if u.orbit
        .is_some_and(|o| o.kind == OrbitKind::Vector && o.radius > 0.0)
    {
        return Err(invalid(
            "a vector orbit does not certify the electrode's reach on angled walls",
        ));
    }
    let allowed = inset(&cavity_region, (u.undersize - SIZE_SLACK).max(0.0))?;
    if !electrode_region
        .difference(&allowed)
        .map_err(invalid)?
        .is_empty()
    {
        return Err(invalid(format!(
            "the electrode is too large for condition {condition:?}; it extends past the cavity's undersize offset"
        )));
    }
    if u.pass == SinkerPass::Finish {
        let required = inset(&cavity_region, u.gap + radius + SIZE_SLACK)?;
        if !required
            .difference(&electrode_region)
            .map_err(invalid)?
            .is_empty()
        {
            return Err(invalid(
                "sinker finish leaves the cavity under size on a wall beyond the size slack",
            ));
        }
    }
    Ok(stand)
}
