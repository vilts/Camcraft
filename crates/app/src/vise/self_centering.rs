//! Built-in 5-axis self-centering vises (#787, part of #339): both jaws
//! close on the vise's centre, so the clamped width is set from the stock
//! and the stock sits centred on the vise. Short, tall and narrow for tool
//! clearance in tilted work; judged by the same fixture laws as any meshed
//! fixture, upright and posed (#339 slice 3).
//!
//! The frame: x across the jaws (centred on the jaw width), y along the
//! screw with the origin midway between the jaw faces (faces at
//! y = +-opening/2, each jaw's body `jaw_body_depth` deep behind its face),
//! z = 0 the floor the part sits on, the body below it down to
//! `-base_height`. An optional dovetail lip is a step `lip_depth` deep at
//! each clamping face, `lip_height` above the jaw top, drawn solid: the
//! dovetail's undercut is filled, so the laws see at least the real steel.
//! An optional grip jaw (#1274) is instead a step cut into each jaw's
//! inside top edge, `grip_depth` down from the jaw top and `grip_width`
//! in from the clamping face: the stock sits on that ledge, not on the
//! floor, and is gripped over the step's depth only. The frame's z = 0
//! stays where the part sits - the ledge - so the floor and body drop
//! by `jaw_height - grip_depth`, and a fit or an edit keeps the stock
//! seated. Dovetail and grip jaw are one choice or the other, never both
//! on one vise.
//!
//! Provenance: our own geometry and parameters; the size table's figures
//! are typical values from public catalog spec sheets (cited in
//! `docs/decisions/787-self-centering-vises.md`), no vendor model copied.
//! The prism and its ear-clipped cap are the textbook method `vise::mesh`
//! uses (Meisters, "Polygons have ears", 1975). The grip step is the
//! stepped (ledge) jaw of general workholding practice; its figures are
//! user parameters, no vendor drawing consulted.

use crate::geometry::{Aabb, IDENTITY_Q, Point3, Triangle, quat_conjugate, quat_rotate_point};
use crate::schema::Location;

use super::{ViseParams, ViseSize, ViseStyle, mesh::triangulate};

/// What a self-centering vise has that a standard one does not: the low
/// end of its clamping range, the optional dovetail lip and the optional
/// grip step (millimetres; a lip of zero height and depth is no lip, a
/// step of zero depth and width no step).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelfCentering {
    pub min_opening: f64,
    pub lip_height: f64,
    pub lip_depth: f64,
    /// How far the grip step's ledge sits below the jaw top (#1274).
    pub grip_depth: f64,
    /// How far the ledge reaches in from the clamping face (#1274).
    pub grip_width: f64,
}

impl SelfCentering {
    /// Is the grip step on?
    #[must_use]
    pub fn has_grip(&self) -> bool {
        self.grip_depth > 0.0
    }

    /// How far above the vise floor the stock's bottom sits: on the grip
    /// step's ledge, or 0 (on the floor) when there is no step.
    #[must_use]
    pub fn seat(&self, jaw_height: f64) -> f64 {
        if self.has_grip() { jaw_height - self.grip_depth } else { 0.0 }
    }
}

/// How far a stock box's centre may sit off the vise's centre along the
/// screw before "Open to stock" refuses (float dust on a fitted pose).
const CENTRE_TOL: f64 = 1e-6;

/// Below this a length in the profile is read as zero (no sliver facets
/// from float dust at a closed vise or a full-length jaw travel).
const SNAP: f64 = 1e-9;

/// A self-centering row, written in millimetres; no lip, no housing.
#[allow(clippy::too_many_arguments)]
const fn sc(
    label: &'static str,
    jaw_width: f64,
    jaw_height: f64,
    jaw_thickness: f64,
    jaw_body_depth: f64,
    min_opening: f64,
    max_opening: f64,
    length: f64,
    width: f64,
    base_height: f64,
) -> ViseSize {
    ViseSize {
        label,
        jaw_width,
        jaw_height,
        jaw_thickness,
        jaw_body_depth,
        max_opening,
        length,
        width,
        base_height,
        housing_length: 0.0,
        housing_height: 0.0,
        style: ViseStyle::SelfCentering(SelfCentering {
            min_opening,
            lip_height: 0.0,
            lip_depth: 0.0,
            grip_depth: 0.0,
            grip_width: 0.0,
        }),
    }
}

/// The self-centering sizes, ascending jaw width. Columns: jaw width, jaw
/// height above the floor, jaw plate thickness, jaw body depth, clamping
/// range (min, max), overall length, overall width, body height (table to
/// floor). Typical values; see the decision file for the sources.
pub const SIZES: &[ViseSize] = &[
    sc("52 mm self-centering", 52.0, 10.0, 8.0, 15.0, 3.5, 38.5, 75.0, 52.0, 28.0),
    sc("75 mm self-centering", 75.0, 12.0, 12.0, 25.0, 0.0, 40.0, 100.0, 75.0, 44.0),
    sc("80 mm self-centering", 80.0, 15.0, 12.0, 25.0, 0.0, 127.0, 182.0, 80.0, 50.0),
    sc("125 mm self-centering", 125.0, 20.0, 15.0, 25.0, 0.0, 208.0, 260.0, 125.0, 69.0),
];

/// The self-centering vise law; refusals name the dimension.
pub fn check(p: &ViseParams, sc: &SelfCentering) -> Result<(), String> {
    for (what, v) in [
        ("jaw width", p.jaw_width),
        ("jaw height", p.jaw_height),
        ("jaw thickness", p.jaw_thickness),
        ("jaw body depth", p.jaw_body_depth),
        ("max opening", p.max_opening),
        ("overall length", p.length),
        ("overall width", p.width),
        ("base height", p.base_height),
    ] {
        if !v.is_finite() {
            return Err(format!("vise {what} must be a finite length (got {v})"));
        }
        if v <= 0.0 {
            return Err(format!("vise {what} must be more than zero (got {v} mm)"));
        }
    }
    for (what, v) in [
        ("jaw opening", p.opening),
        ("min opening", sc.min_opening),
        ("lip height", sc.lip_height),
        ("lip depth", sc.lip_depth),
        ("grip step depth", sc.grip_depth),
        ("grip step width", sc.grip_width),
    ] {
        if !v.is_finite() {
            return Err(format!("vise {what} must be a finite length (got {v})"));
        }
        if v < 0.0 {
            return Err(format!("vise {what} must not be negative (got {v} mm)"));
        }
    }
    if p.housing_length != 0.0 || p.housing_height != 0.0 {
        return Err(format!(
            "a self-centering vise has no screw housing: housing length and height must be 0 (got {} and {} mm)",
            p.housing_length, p.housing_height
        ));
    }
    if sc.min_opening >= p.max_opening {
        return Err(format!(
            "vise min opening {:.3} mm must be less than its max opening {:.3} mm",
            sc.min_opening, p.max_opening
        ));
    }
    if p.opening < sc.min_opening || p.opening > p.max_opening {
        return Err(format!(
            "vise jaw opening {:.3} mm is outside its clamping range {:.3}-{:.3} mm",
            p.opening, sc.min_opening, p.max_opening
        ));
    }
    if p.width < p.jaw_width {
        return Err(format!(
            "vise overall width {:.3} mm is narrower than its jaw width {:.3} mm",
            p.width, p.jaw_width
        ));
    }
    if p.jaw_body_depth < p.jaw_thickness {
        return Err(format!(
            "vise jaw body depth {:.3} mm is less than its jaw thickness {:.3} mm",
            p.jaw_body_depth, p.jaw_thickness
        ));
    }
    if (sc.lip_height > 0.0) != (sc.lip_depth > 0.0) {
        return Err(format!(
            "vise dovetail lip needs both a height and a depth, or neither (got lip height {} and lip depth {} mm)",
            sc.lip_height, sc.lip_depth
        ));
    }
    if sc.lip_depth > p.jaw_body_depth {
        return Err(format!(
            "vise lip depth {:.3} mm is more than its jaw body depth {:.3} mm",
            sc.lip_depth, p.jaw_body_depth
        ));
    }
    check_grip(p, sc)?;
    let carry = p.max_opening + 2.0 * p.jaw_body_depth;
    if p.length < carry {
        return Err(format!(
            "vise overall length {:.3} mm must be at least the max opening and two jaw bodies ({carry:.3} mm) - the jaws ride on the body",
            p.length
        ));
    }
    Ok(())
}

/// The grip step's own law (#1274), refusals by name: both figures or
/// neither, not beside a dovetail lip, the ledge inside the jaw, and the
/// two ledges apart at the opening (else the jaws' feet would meet under
/// the stock - no vise closes that far).
fn check_grip(p: &ViseParams, sc: &SelfCentering) -> Result<(), String> {
    if (sc.grip_depth > 0.0) != (sc.grip_width > 0.0) {
        return Err(format!(
            "vise grip step needs both a depth and a width, or neither (got grip step depth {} and grip step width {} mm)",
            sc.grip_depth, sc.grip_width
        ));
    }
    if !sc.has_grip() {
        return Ok(());
    }
    if sc.lip_height > 0.0 {
        return Err(format!(
            "a self-centering jaw is a dovetail or a grip jaw, not both: set the dovetail lip or the grip step to 0 (got lip height {} and grip step depth {} mm)",
            sc.lip_height, sc.grip_depth
        ));
    }
    if sc.grip_depth >= p.jaw_height {
        return Err(format!(
            "vise grip step depth {:.3} mm must be less than its jaw height {:.3} mm",
            sc.grip_depth, p.jaw_height
        ));
    }
    if sc.grip_width >= p.jaw_thickness {
        return Err(format!(
            "vise grip step width {:.3} mm must be less than its jaw thickness {:.3} mm",
            sc.grip_width, p.jaw_thickness
        ));
    }
    if p.opening <= 2.0 * sc.grip_width {
        return Err(format!(
            "vise jaw opening {:.3} mm is not more than its two grip step widths ({:.3} mm): the ledges would meet under the stock",
            p.opening,
            2.0 * sc.grip_width
        ));
    }
    Ok(())
}

/// The side profile in the vise frame's (y, z), counter-clockwise seen
/// from +X, with repeated and collinear corners dropped (a closed vise's
/// jaws merge into one block; a jaw at the body's end runs flush with it).
/// `p` is assumed checked.
#[must_use]
pub fn profile(p: &ViseParams, sc: &SelfCentering) -> Vec<(f64, f64)> {
    let half_len = 0.5 * p.length;
    let face = if p.opening * 0.5 < SNAP { 0.0 } else { 0.5 * p.opening };
    let back = if half_len - (face + p.jaw_body_depth) < SNAP { half_len } else { face + p.jaw_body_depth };
    let jh = p.jaw_height;
    // The +y jaw, walked from its back foot up, over and down its face.
    let mut jaw = vec![(back, 0.0), (back, jh)];
    let foot = if sc.has_grip() {
        // Down the face to the ledge, in along it, down to the floor.
        let ledge = jh - sc.grip_depth;
        let foot = (face - sc.grip_width).max(0.0);
        jaw.extend([(face, jh), (face, ledge), (foot, ledge)]);
        foot
    } else if sc.lip_height > 0.0 {
        let lip = (face + sc.lip_depth).min(back);
        let top = jh + sc.lip_height;
        jaw.extend([(lip, jh), (lip, top), (face, top)]);
        face
    } else {
        jaw.push((face, jh));
        face
    };
    jaw.push((foot, 0.0));
    let mut poly = vec![(half_len, -p.base_height), (half_len, 0.0)];
    poly.extend(jaw.iter().copied());
    // The -y jaw: the mirror, walked the other way.
    poly.extend(jaw.iter().rev().map(|&(y, z)| (-y, z)));
    poly.extend([(-half_len, 0.0), (-half_len, -p.base_height)]);
    // Drawn from the floor up; the frame's z = 0 is the seat (#1274).
    let seat = sc.seat(jh);
    if seat > 0.0 {
        for c in &mut poly {
            c.1 -= seat;
        }
    }
    simplify(poly)
}

/// Drop repeated corners and corners on a straight run, until none is left.
fn simplify(mut poly: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    loop {
        let n = poly.len();
        if n < 3 {
            return poly;
        }
        let drop = (0..n).find(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            a == b || (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0) == 0.0
        });
        match drop {
            Some(i) => {
                poly.remove(i);
            }
            None => return poly,
        }
    }
}

/// The closed, outward shell in the vise frame: the profile extruded over
/// x in [-width/2, width/2] (jaws drawn as wide as the body - never less
/// steel than the real vise). `p` is assumed checked.
pub fn solid(p: &ViseParams, sc: &SelfCentering) -> Result<Vec<Triangle>, String> {
    let mut poly = profile(p, sc);
    let n = poly.len();
    let area2: f64 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    if area2 < 0.0 {
        poly.reverse();
    }
    let caps = triangulate(&poly)?;
    let h = 0.5 * p.width;
    let at = |x: f64, k: usize| Point3::new(x, poly[k].0, poly[k].1);
    let mut tris = Vec::with_capacity(2 * caps.len() + 2 * n);
    for [a, b, c] in &caps {
        tris.push(Triangle::new(at(h, *a), at(h, *b), at(h, *c)));
        tris.push(Triangle::new(at(-h, *a), at(-h, *c), at(-h, *b)));
    }
    for i in 0..n {
        let j = (i + 1) % n;
        tris.push(Triangle::new(at(-h, i), at(-h, j), at(h, j)));
        tris.push(Triangle::new(at(-h, i), at(h, j), at(h, i)));
    }
    Ok(tris)
}

/// Refuse a clamped width outside the clamping range, by name.
fn in_range(p: &ViseParams, sc: &SelfCentering, across: f64) -> Result<(), String> {
    if !across.is_finite() || across <= 0.0 {
        return Err("the stock has no width to clamp".into());
    }
    let range = format!("the {} vise clamps {:.3}-{:.3} mm", p.base_name(), sc.min_opening, p.max_opening);
    if across > p.max_opening {
        return Err(format!("the stock is {across:.3} mm across the jaws, wider than its clamping range: {range}"));
    }
    if across < sc.min_opening {
        return Err(format!("the stock is {across:.3} mm across the jaws, narrower than its clamping range: {range}"));
    }
    Ok(())
}

/// Fit at identity rotation to stock box `stock`: the vise centred on the
/// stock in x and y, the seat under it (the floor, or a grip step's
/// ledge with the floor that much lower), the jaws closed to its y extent.
pub fn fit_to_stock(p: &ViseParams, sc: &SelfCentering, stock: &Aabb) -> Result<(ViseParams, Location), String> {
    let across = stock.max.y - stock.min.y;
    in_range(p, sc, across)?;
    let fitted = p.with_opening(across);
    fitted.check()?;
    let pos = Point3::new(0.5 * (stock.min.x + stock.max.x), 0.5 * (stock.min.y + stock.max.y), stock.min.z);
    Ok((fitted, Location { pos, rot: IDENTITY_Q }))
}

/// The opening that closes the jaws on stock box `stock` for the vise at
/// `pose`, measured along its screw. Both jaws close on the centre, so a
/// stock off that centre is refused by name - drawing the jaws at its far
/// side would leave the near jaw's steel short of where it really stops.
pub fn opening_from_stock(p: &ViseParams, sc: &SelfCentering, pose: &Location, stock: &Aabb) -> Result<f64, String> {
    let inv = quat_conjugate(pose.rot);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for k in 0..8 {
        let c = Point3::new(
            if k & 1 == 0 { stock.min.x } else { stock.max.x },
            if k & 2 == 0 { stock.min.y } else { stock.max.y },
            if k & 4 == 0 { stock.min.z } else { stock.max.z },
        );
        let local = quat_rotate_point(inv, Point3::new(c.x - pose.pos.x, c.y - pose.pos.y, c.z - pose.pos.z));
        lo = lo.min(local.y);
        hi = hi.max(local.y);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return Err("the stock box is not finite".into());
    }
    let off = 0.5 * (lo + hi);
    if off.abs() > CENTRE_TOL {
        return Err(format!(
            "the stock's centre is {off:.3} mm off the self-centering vise's centre along its jaws - the jaws clamp it centred; place the vise on the stock (Fit) first"
        ));
    }
    let across = hi - lo;
    in_range(p, sc, across)?;
    Ok(across)
}
