//! The one region law (M11.4 A): what an operation's selection encloses,
//! whatever was picked - a face, its boundary edge loop, the wall patch -
//! followed by the single area-controls seam every kind's region passes
//! through. Provenance: our own design notes on 2.5D pocketing (a
//! boundary plus the islands that stand inside it, the textbook pocket
//! definition of any machining handbook); no vendor material consulted.
//!
//! The pocket family on a PART cuts inside the enclosed area and around
//! the part material standing in it: the enclosed area is the union of
//! the picked rings' interiors (a face's hole loops are not trusted as
//! the island list - its edge loop has none), and the islands are read
//! from the part itself at every plane the ladder cuts: ALL the part
//! material standing above the plane - its section plus the shadow of
//! what hangs over it (`slice::material_overhead`'s law), so a plane
//! under a closed floor or an overhang never reads as open air. A floor face, its rim
//! chain, the rim at the top of the wall and the wall patch therefore
//! resolve to one region, byte for byte, and the strategy the user chose
//! runs on it - nothing here looks at the region's shape.

use super::*;
use crate::units::{self, Unit};

/// The seam itself (M11.4 A / M11.5): every region an operation cuts -
/// selection-built or source-built, Face included, and the Contour
/// selection arm's rings - passes through here last. `None` is the
/// identity controls (nothing resolved, nothing changed: every golden
/// stays what it was); `Some` runs the `boundary` law.
pub(super) fn apply_area_controls(region: Region, ctx: Option<&AreaCtx>) -> Result<Region, GenError> {
    Ok(apply_area_full(region, ctx)?.region)
}

/// The seam's full answer: the region plus Face's band constraints.
pub(super) fn apply_area_full(region: Region, ctx: Option<&AreaCtx>) -> Result<AreaApplied, GenError> {
    match ctx {
        None => Ok(AreaApplied { region, forbidden: Vec::new(), turn: 0.0 }),
        Some(c) => boundary::resolve(region, c),
    }
}

/// The part's facets and the ladder planes, top down.
pub(super) type PocketLadder = (Vec<Triangle>, Vec<f64>);

/// The part facets and ladder planes the pocket region law reads, or
/// `None` where it does not apply: not a pocket-family kind, or picks
/// that touch the setup's stock (stock is the material to remove, not a
/// part to keep - the depth verify's `part_solid` rule).
pub(super) fn pocket_ladder(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<Option<PocketLadder>, GenError> {
    let (top, bottom, stepdown) = match params {
        OpParams::Pocket { top_z, bottom_z, stepdown, .. }
        | OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, .. } => (*top_z, *bottom_z, *stepdown),
        _ => return Ok(None),
    };
    let stock_name = crate::setup::stock_entity_name(world, setup);
    if items.is_empty() || items.iter().any(|g| g.entity == stock_name) {
        return Ok(None);
    }
    let sel = SrcRef::Selection(items.to_vec());
    let material = material_of(world, setup, &sel)?;
    if material.facets.is_empty() {
        return Ok(None);
    }
    // Unresolvable heights are the generator's refusal of record (named
    // there, in its precedence); the region law simply stands down.
    let (Some(t), Some(b)) = (resolve_height(world, setup, Some(&sel), top), resolve_height(world, setup, Some(&sel), bottom))
    else {
        return Ok(None);
    };
    Ok(Some((material.facets, ladder_planes(t, b, stepdown))))
}

/// Below this (mm^2) the part material inside the enclosed area is
/// tessellation noise along a shared wall, not an island.
const ISLAND_AREA_MIN: f64 = 1e-4;

/// The pocket family's selection region on a part: the enclosed area of
/// `loops` minus the part material standing in it at each ladder plane.
/// Where no material stands inside, the answer is exactly today's
/// `Region::new(loops)` (byte-stable for every existing pick). A plane
/// where the part fills the whole area is a named refusal - the pocket's
/// Bottom sits below the selection's floor - never a cut into the part.
pub(super) fn pocket_region_on_part(
    loops: Vec<Loop>,
    part: &[Triangle],
    planes: &[f64],
    unit: Unit,
) -> Result<Region, GenError> {
    let mut enclosed = Region::empty();
    for l in &loops {
        enclosed = enclosed.union(&Region::new(vec![l.clone()])?)?;
    }
    if enclosed.is_empty() {
        return Err(GenError::Empty("nothing in the selection bounds an area".into()));
    }
    let full = |z: f64| {
        format!(
            "the part fills the whole selection at Z {} - Bottom Z sits below the selected pocket's floor; raise Bottom Z to the floor",
            units::len(z, unit, 3)
        )
    };
    match minus_standing(&enclosed, part, planes, full)? {
        Some(r) => Ok(r),
        None => Ok(Region::new(loops)?),
    }
}

/// `enclosed` minus ALL the part material standing in it at each of
/// `planes`; `None` when nothing stands inside (the caller keeps its own
/// byte-stable reading). A plane where the part fills the whole area is
/// the named refusal `full` spells - never a cut into the part.
fn minus_standing(
    enclosed: &Region,
    part: &[Triangle],
    planes: &[f64],
    full: impl Fn(f64) -> String,
) -> Result<Option<Region>, GenError> {
    let mut standing = Region::empty();
    for &z in planes {
        let inside = standing_inside(enclosed, part, z)?;
        if inside.area().abs() < ISLAND_AREA_MIN {
            continue;
        }
        if enclosed.difference(&inside)?.area().abs() < ISLAND_AREA_MIN {
            return Err(GenError::Invalid(full(z)));
        }
        standing = standing.union(&inside)?;
    }
    if standing.area().abs() < ISLAND_AREA_MIN {
        return Ok(None);
    }
    Ok(Some(enclosed.difference(&standing)?))
}

/// The pocket family on a WHOLE PART source (issue #12): the part is
/// material to keep, so the area to clear is the setup stock's outline
/// minus every part section standing at the ladder planes - the stock
/// AROUND the part, the same island law the picked pockets ride. `None`
/// where the reading does not apply (not a pocket-family kind, a Stock
/// or Selection source, the source IS the stock, no stock set up: there
/// the model stays the volume to remove, as documented) or where a
/// height is unresolvable (the generator's refusal of record). Under a
/// 3+2 index the answer is in the frame's local coordinates, exactly
/// where `region_core` would have built it.
pub(super) fn whole_model_pocket(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Option<Result<Region, GenError>> {
    let (top, bottom, stepdown) = match params {
        OpParams::Pocket { top_z, bottom_z, stepdown, .. }
        | OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, .. } => (*top_z, *bottom_z, *stepdown),
        _ => return None,
    };
    let SrcRef::Entity(name) = src else { return None };
    let stock_name = crate::setup::stock_entity_name(world, setup);
    if *name == stock_name {
        return None;
    }
    let stock = model::find_by_name(world, &stock_name).and_then(|e| model::world_facets(world, &e))?;
    if stock.is_empty() {
        return None;
    }
    let t = resolve_height(world, setup, Some(src), top)?;
    let b = resolve_height(world, setup, Some(src), bottom)?;
    let part = match material_of(world, setup, src) {
        Ok(m) => m.facets,
        Err(e) => return Some(Err(e)),
    };
    let oz = frame.map_or(0.0, |f| f.origin.z);
    let (part, stock) = match frame {
        Some(f) => (f.local_facets(&part), f.local_facets(&stock)),
        None => (part, stock),
    };
    Some(around_part(&part, &stock, &ladder_planes(t - oz, b - oz, stepdown), crate::units::of(world)))
}

/// The pocket family on the STOCK as its source (#1554): the stock is the
/// material to remove, but the setup's parts standing in it are not. So
/// the area to clear is the stock's outline minus every part section
/// standing at the ladder planes - the same island law as a whole-part
/// source (#12). `None` where the reading does not apply (not a
/// pocket-family kind, not a Stock source, no stock facets, no part in
/// the setup: there the whole stock stays the volume to remove) or where
/// a height is unresolvable (the generator's refusal of record).
pub(super) fn stock_pocket(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Option<Result<Region, GenError>> {
    let (top, bottom, stepdown) = match params {
        OpParams::Pocket { top_z, bottom_z, stepdown, .. }
        | OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, .. } => (*top_z, *bottom_z, *stepdown),
        _ => return None,
    };
    if !matches!(src, SrcRef::Stock) {
        return None;
    }
    let parts = crate::setup_align::parts_facets_in(world, setup);
    if parts.is_empty() {
        return None;
    }
    let stock_name = crate::setup::stock_entity_name(world, setup);
    let stock = model::find_by_name(world, &stock_name).and_then(|e| model::world_facets(world, &e))?;
    if stock.is_empty() {
        return None;
    }
    let t = resolve_height(world, setup, Some(src), top)?;
    let b = resolve_height(world, setup, Some(src), bottom)?;
    let oz = frame.map_or(0.0, |f| f.origin.z);
    let (parts, stock) = match frame {
        Some(f) => (f.local_facets(&parts), f.local_facets(&stock)),
        None => (parts, stock),
    };
    let unit = crate::units::of(world);
    let full = |z: f64| {
        format!(
            "a pocket on the stock clears the stock around the setup's parts, and the parts fill the whole \
             stock outline at Z {} - pick the area to clear, or use the model as the source",
            units::len(z, unit, 3)
        )
    };
    let planes = ladder_planes(t - oz, b - oz, stepdown);
    let enclosed = match slice::project(&stock, FOOTPRINT_SAMPLES).map_err(from_slice) {
        Ok(r) if !r.is_empty() => r,
        Ok(_) => return None,
        Err(e) => return Some(Err(e)),
    };
    // No part stands in the ladder: the region the source builds itself
    // stays the answer, byte for byte (every path that cut no part).
    minus_standing(&enclosed, &parts, &planes, full).transpose()
}

/// [`whole_model_pocket`]'s pure law: the stock's silhouette minus the
/// part standing at `planes`. A part that covers the whole stock at a
/// plane leaves nothing to clear around it - refused by name.
pub(super) fn around_part(part: &[Triangle], stock: &[Triangle], planes: &[f64], unit: Unit) -> Result<Region, GenError> {
    let enclosed = slice::project(stock, FOOTPRINT_SAMPLES).map_err(from_slice)?;
    if enclosed.is_empty() {
        return Err(GenError::Empty("the stock casts no outline to clear around the part".into()));
    }
    let full = |z: f64| {
        format!(
            "a whole-model pocket clears the stock around the part, and the part fills the whole stock \
             outline at Z {} - pick the pocket's faces instead",
            units::len(z, unit, 3)
        )
    };
    Ok(minus_standing(&enclosed, part, planes, full)?.unwrap_or(enclosed))
}

/// The part material standing above `z` inside `enclosed`: the plane's
/// section, plus - only where it adds area - the shadow of what hangs
/// over the plane (a closed floor's underside, an overhang). Read in two
/// steps so a part solid down to the plane keeps its region byte for
/// byte; the shadow is united only when it reaches past the section by
/// more than tessellation noise.
fn standing_inside(enclosed: &Region, part: &[Triangle], z: f64) -> Result<Region, GenError> {
    let at = slice::material_above(part, z).map_err(from_slice)?;
    let inside = enclosed.intersect(&at)?;
    let hang = slice::shadow_above(part, z).map_err(from_slice)?;
    if hang.is_empty() {
        return Ok(inside);
    }
    let hang = enclosed.intersect(&hang)?;
    if hang.difference(&inside)?.area().abs() <= SHADOW_AREA_MIN {
        return Ok(inside);
    }
    Ok(inside.union(&hang)?)
}

/// A hanging shadow adding less than this (mm^2) past the section changes
/// nothing a cutter can reach (the 3-D ladder's own gate).
const SHADOW_AREA_MIN: f64 = 1e-6;

/// The planes a pocket ladder cuts at, top down, the last one `bottom`
/// exactly - the `machining2d` level law, restated here because the
/// region is built before the generator runs (and the motion crate keeps
/// its helper private). Degenerate spans and steps collapse to the one
/// bottom plane; the generator refuses them by name later.
pub(super) fn ladder_planes(top: f64, bottom: f64, stepdown: f64) -> Vec<f64> {
    let span = top - bottom;
    let walkable = span.is_finite() && span > 0.0 && stepdown > 0.0;
    if !walkable {
        return vec![bottom];
    }
    let tol = span * 1e-9;
    let mut zs = Vec::new();
    let mut z = top;
    while z > bottom + tol && zs.len() < 100_000 {
        zs.push(z);
        z -= stepdown;
    }
    zs.push(bottom);
    zs
}

#[cfg(test)]
mod law {
    use super::*;

    #[test]
    fn a_ring_and_its_mirror_share_one_canonical_rim() {
        // The dedupe key a face loop and its reversed edge chain meet on.
        let ring = [(0, 0), (5, 0), (7, 3), (4, 8), (-1, 4)];
        let mut rev = ring.to_vec();
        rev.reverse();
        rev.rotate_left(2);
        assert_eq!(canon_ring(&ring), canon_ring(&rev));
        let other = [(0, 0), (5, 0), (4, 8), (7, 3), (-1, 4)];
        assert_ne!(canon_ring(&ring), canon_ring(&other), "a different ring is not a mirror");
    }

    #[test]
    fn the_area_controls_seam_is_the_identity_today() {
        let r = Region::new(vec![
            Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), Point2::new(4.0, 3.0)])
                .expect("loop"),
        ])
        .expect("region");
        let out = apply_area_controls(r.clone(), None).expect("identity");
        assert_eq!(out.loops(), r.loops());
    }

    fn boxf(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
        let v = |i: usize| {
            crate::geometry::Point3::new(
                [min[0], max[0]][i & 1],
                [min[1], max[1]][(i >> 1) & 1],
                [min[2], max[2]][(i >> 2) & 1],
            )
        };
        let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
        [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
            .into_iter()
            .flatten()
            .collect()
    }

    #[test]
    fn an_undercut_lip_no_plane_sections_is_an_island_of_every_plane_under_it() {
        // A [-10, 10]^2 pocket in a frame to Z 20, an undercut lip
        // x in [6, 10.5] at Z [17, 19] jutting in from the +x wall. The
        // planes 20, 15.4, ..., 0 never section the lip: pre-fix the
        // region was the whole square and a cutter at 15.4 stood under
        // the lip, shank through it. The shadow makes it an island.
        let mut part = boxf([-20.0, -20.0, 0.0], [-10.0, 20.0, 20.0]);
        part.extend(boxf([10.0, -20.0, 0.0], [20.0, 20.0, 20.0]));
        part.extend(boxf([-10.5, -20.0, 0.0], [10.5, -10.0, 20.0]));
        part.extend(boxf([-10.5, 10.0, 0.0], [10.5, 20.0, 20.0]));
        part.extend(boxf([6.0, -10.5, 17.0], [10.5, 10.5, 19.0]));
        let ring = Loop::new(vec![
            Point2::new(-10.0, -10.0),
            Point2::new(10.0, -10.0),
            Point2::new(10.0, 10.0),
            Point2::new(-10.0, 10.0),
        ])
        .expect("ring");
        let planes = ladder_planes(20.0, 0.0, 4.6);
        assert!(planes.iter().all(|&z| !(17.0..19.0).contains(&z)), "{planes:?}");
        let r = pocket_region_on_part(vec![ring.clone()], &part, &planes, Unit::Millimeters).expect("region");
        assert!(!r.contains(Point2::new(8.0, 0.0)), "under the lip is part material");
        assert!(r.contains(Point2::new(0.0, 0.0)) && r.contains(Point2::new(5.5, 0.0)), "the open pocket stays");
        assert!((r.area() - 16.0 * 20.0).abs() < 1e-3, "the square less the lip: {}", r.area());
        // No lip, no shadow inside: today's region, byte for byte.
        part.truncate(part.len() - 12);
        let plain = pocket_region_on_part(vec![ring.clone()], &part, &planes, Unit::Millimeters).expect("region");
        assert_eq!(plain.loops(), Region::new(vec![ring]).expect("region").loops());
    }

    #[test]
    fn ladder_planes_match_the_level_law_and_never_hang() {
        assert_eq!(ladder_planes(20.0, 0.0, 5.0), vec![20.0, 15.0, 10.0, 5.0, 0.0]);
        assert_eq!(ladder_planes(20.0, 2.0, 5.0), vec![20.0, 15.0, 10.0, 5.0, 2.0]);
        assert_eq!(ladder_planes(1.0, 1.0, 5.0), vec![1.0]);
        assert_eq!(ladder_planes(10.0, 0.0, 0.0), vec![0.0]);
        assert_eq!(ladder_planes(f64::NAN, 0.0, 1.0), vec![0.0]);
        assert!(ladder_planes(1e9, 0.0, 1e-9).len() <= 100_001);
    }
}
