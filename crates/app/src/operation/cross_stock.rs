//! The cross drill stands off the RAW stock (issue #77): a cross drill
//! run before the bar is turned to size meets the setup's stock, not
//! the finished part, so every standoff the radial kernel measures -
//! the hop and clearance radii, the chord law between stations, the
//! feed-entry point above each mouth, the end-face detour - is taken
//! about the larger of the two. A stock wider than the part moves each
//! hole's mouth out to the raw surface along its own axis (the floor
//! stays where it was, so the drill feeds through the uncut skin), and
//! the finished chain is then proven clear of that stock rapid by
//! rapid - refused by name if any one is not, never posted. The
//! verify crate's rapid-in-stock law stands down in `Mode::Radial` (a
//! bore is a tunnel its height field cannot see), so this proof is the
//! only one the radial rapids get.
//!
//! Provenance: traversing only outside the uncut material is the
//! machine-tool handbooks' shop practice; the stock of a lathe job is
//! the bar as clamped (RS-274 / ISO 6983 radial measurement from the
//! spindle centreline). Closest approach of a segment to a line is
//! textbook geometry. Our own design notes; no vendor material
//! consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::drilling::CrossHole;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::units::{self, Unit};

use super::cross_entry::{Line, StockMesh, entry_grown};

/// What the cross drill stands off, measured about world Z through the
/// spindle origin `(ox, oy)` (already the kernel's frame).
pub(super) struct Standoff {
    /// The radius the kernel's traverse law stands off.
    pub r: f64,
    /// The Z top the end-face detour swings above.
    pub z_max: Option<f64>,
    /// The raw stock's radius when it is wider than the part: the
    /// mouths move out to it. `None` keeps the part-only chain.
    pub raw: Option<f64>,
    /// The raw stock's facets in the spindle frame, with the part's
    /// beside them (empty without `raw`): each hole's mouth is where its
    /// own line first meets either (#85) - a part poking out of the
    /// stock (a modelling error) still counts as material on its line.
    pub mesh: StockMesh,
    /// Standoff and slab growth for a reduced-fidelity model (#1053).
    pub grow: f64,
}

fn reach(facets: &[Triangle], ox: f64, oy: f64) -> (f64, Option<f64>) {
    let verts = || facets.iter().flat_map(|t| [t.a, t.b, t.c]).filter(|v| v.is_finite());
    let r = verts().map(|v| (v.x - ox).hypot(v.y - oy)).fold(0.0_f64, f64::max);
    (r, verts().map(|v| v.z).reduce(f64::max))
}

/// Measure the part and, when known, the setup's stock about the
/// spindle. A stock's vertex hull bounds it, so the max vertex radius
/// is a box's circumscribed radius and a cylinder's own. An absent
/// stock (none declared, or the op's source IS the stock) measures the
/// part - today's reading, said so in the doc comment, not guessed.
/// The stock counts only when it reaches past the part, so a stock
/// equal to the part keeps the part-only chain bit for bit.
pub(super) fn standoff(part: &[Triangle], stock: Option<&[Triangle]>, ox: f64, oy: f64, excess: f64) -> Standoff {
    let grow = super::fidelity_law::band_growth(excess);
    let (part_r, part_z) = reach(part, ox, oy);
    let Some((stock_r, stock_z)) = stock.filter(|s| !s.is_empty()).map(|s| reach(s, ox, oy)) else {
        let g = if part.is_empty() { 0.0 } else { grow };
        return Standoff { r: part_r + g, z_max: part_z.map(|z| z + g), raw: None, mesh: StockMesh::default(), grow: g };
    };
    let raw = (stock_r > part_r + 1e-9).then_some(stock_r + grow);
    let z_max = match (part_z, stock_z) {
        (Some(a), Some(b)) if b > a + 1e-9 => Some(b + grow),
        (a, b) => a.or(b).map(|z| z + grow),
    };
    let shift = |p: Point3| Point3::new(p.x - ox, p.y - oy, p.z);
    let frame = |s: &[Triangle]| StockMesh::new(s.iter().map(|t| Triangle::new(shift(t.a), shift(t.b), shift(t.c))).collect());
    let mesh = match (raw, stock) {
        (Some(_), Some(s)) => {
            let mut m = frame(s);
            m.extend(frame(part));
            m
        }
        _ => StockMesh::default(),
    };
    Standoff { r: raw.unwrap_or(part_r + grow), z_max, raw, mesh, grow }
}

/// The hole line through `p` plunging along the XY axis-aligned `dir`
/// (the kernel refuses anything else), outward against `dir`.
fn hole_line(p: Point3, dir: Point3) -> Line {
    if dir.x.abs() >= dir.y.abs() {
        Line { a: 0, out: -dir.x.signum(), t: p.y, z: p.z }
    } else {
        Line { a: 1, out: -dir.y.signum(), t: p.x, z: p.z }
    }
}

/// The tool body's half-width the proof stands off: the drill's own
/// rim, or its shank's when the shank is the wider (a 3 mm drill on a
/// 6 mm shank lies across the end face on the swing at r3, not r1.5).
fn body_rim(tool: &Tool) -> f64 {
    let shank = if tool.shank_d.is_finite() { tool.shank_d } else { 0.0 };
    0.5 * tool.diameter.max(shank)
}

/// The clearance and hop standoffs handed to the kernel, raised so a
/// shank wider than the drill clears what it sweeps: the kernel's own
/// margin is the drill's `d / 2 + 0.1`, and its end-face swing, hop
/// and end planes all take `.max(margin)` of these - so lifting them
/// to the shank's `shank_d / 2 + 0.1` moves nothing for a shank no
/// wider than the drill (bit for bit) and clears the shank otherwise.
pub(super) fn body_clear(clearance_d: f64, hop_d: Option<f64>, tool: &Tool) -> (f64, Option<f64>) {
    if !(tool.shank_d.is_finite() && tool.shank_d > tool.diameter) {
        return (clearance_d, hop_d);
    }
    let floor = 0.5 * tool.shank_d + 0.1;
    (clearance_d.max(floor), hop_d.map(|h| h.max(floor)))
}

/// Each hole's mouth moved out along its own axis to where the drill
/// of diameter `d` first meets the raw stock (the floor unchanged:
/// `floor_d` deepens by the shift). Issue #85: that point is read off
/// the stock mesh along the hole's own line (`cross_entry::entry`: the
/// drill's whole body, so an off-centre hole's edge nearest the axis
/// is what meets the surface first, #77), so a box enters at its face,
/// not its corner radius `raw`; a round bar, or a line whose body
/// meets no facet, keeps the circumscribed reading at the edge -
/// transverse offset `|t| - d/2` - as before. A hole whose drill
/// misses the stock radius, or whose mouth already stands outside it,
/// is untouched; a
/// hole plunging away from the spindle is refused by name (moving its
/// mouth "out" would move it in). `holes` are in the spindle frame.
///
/// Beside each hole, the reach the wider body adds (issue #84): a shank
/// wider than the drill has its near edge at `|t| - shank_d/2` and so
/// meets the raw surface further out than the drill's mouth - by that
/// much more of the shank's length is in stock at the floor. The mouth
/// (where cutting starts) stays the drill's; the reach law adds this.
pub(super) fn to_raw_mouths(
    holes: &[CrossHole],
    st: &Standoff,
    raw: f64,
    tool: &Tool,
    unit: Unit,
) -> Result<(Vec<CrossHole>, Vec<f64>), GenError> {
    let (d, rim) = (tool.diameter, body_rim(tool));
    let mouths: Vec<CrossHole> = holes
        .iter()
        .map(|h| {
            // Axis-aligned XY plunges (the kernel refuses anything
            // else): the plunge coordinate and its direction.
            let (c, dc) = if h.dir.x.abs() >= h.dir.y.abs() { (h.origin.x, h.dir.x) } else { (h.origin.y, h.dir.y) };
            if c * dc > 0.0 {
                return Err(GenError::invalid_at(
                    format!(
                        "cross hole at ({}, {}, {}) plunges away from the spindle - a cross \
                         drill feeds toward the bar's axis; flip the hole's direction",
                        units::len(h.origin.x, unit, 3),
                        units::len(h.origin.y, unit, 3),
                        units::len(h.origin.z, unit, 3)
                    ),
                    h.origin,
                ));
            }
            let line = hole_line(h.origin, h.dir);
            let shift = entry_grown(&st.mesh, raw, line, 0.5 * d, st.grow).unwrap_or(0.0) - c.abs();
            if !(shift.is_finite() && shift > 0.0) {
                return Ok(*h);
            }
            Ok(CrossHole {
                origin: Point3::new(
                    h.origin.x - h.dir.x * shift,
                    h.origin.y - h.dir.y * shift,
                    h.origin.z - h.dir.z * shift,
                ),
                floor_d: h.floor_d - shift,
                ..*h
            })
        })
        .collect::<Result<_, _>>()?;
    // The mouth's plunge coordinate is where the drill starts; the
    // body's raw meeting point beyond it is the reach the body adds.
    let extra = mouths
        .iter()
        .map(|m| {
            let line = hole_line(m.origin, m.dir);
            let added = entry_grown(&st.mesh, raw, line, rim, st.grow).map_or(0.0, |s| s - line.u(m.origin).abs());
            if added.is_finite() { added.max(0.0) } else { 0.0 }
        })
        .collect();
    Ok((mouths, extra))
}

/// The hole line a rapid along the pose `axis` (tip toward shank, so
/// outward) through `p` rides, when that axis is XY axis-aligned - the
/// only radial pose the kernel emits. `None` for anything else.
fn pose_line(p: Point3, axis: [f64; 3]) -> Option<Line> {
    let [x, y, z] = axis;
    let aligned = |a: f64, b: f64| a.abs() > 1e-9 && b.abs() <= 1e-9 && z.abs() <= 1e-9;
    if aligned(x, y) {
        Some(Line { a: 0, out: x.signum(), t: p.y, z: p.z })
    } else if aligned(y, x) {
        Some(Line { a: 1, out: y.signum(), t: p.x, z: p.z })
    } else {
        None
    }
}

fn approach(a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll <= 0.0 { 0.0 } else { (-(a.x * dx + a.y * dy) / ll).clamp(0.0, 1.0) };
    (a.x + t * dx).hypot(a.y + t * dy)
}

/// On the line through `p` along unit `dir`, within `tol`?
fn on_line(q: Point3, p: Point3, dir: [f64; 3]) -> bool {
    let w = [q.x - p.x, q.y - p.y, q.z - p.z];
    let s = w[0] * dir[0] + w[1] * dir[1] + w[2] * dir[2];
    let perp = [w[0] - s * dir[0], w[1] - s * dir[1], w[2] - s * dir[2]];
    perp.iter().map(|c| c * c).sum::<f64>() <= 1e-12
}

/// Closest XY approach to the spindle of a tool body of half-width
/// `w` swept from `a` to `b` along its own `axis`: the rectangle its
/// cross-section (`w` either side of the axis, across it in XY) sweeps.
/// An axis with no XY part sweeps a disk of radius `w`.
fn slab_approach(a: Point3, b: Point3, axis: [f64; 3], w: f64) -> f64 {
    let l = axis[0].hypot(axis[1]);
    if l.is_nan() || l <= 1e-9 {
        return (approach(a, b) - w).max(0.0);
    }
    let (ux, uy) = (axis[0] / l, axis[1] / l);
    let (nx, ny) = (-uy, ux);
    let (s0, s1) = (-(a.x * ux + a.y * uy), (b.x - a.x) * ux + (b.y - a.y) * uy);
    let across = -(a.x * nx + a.y * ny);
    if across.abs() <= w && s0 >= s1.min(0.0) && s0 <= s1.max(0.0) {
        return 0.0;
    }
    let off = |p: Point3, k: f64| Point3::new(p.x + k * nx, p.y + k * ny, p.z);
    [(off(a, w), off(b, w)), (off(a, -w), off(b, -w)), (off(a, w), off(a, -w)), (off(b, w), off(b, -w))]
        .iter()
        .map(|&(p, q)| approach(p, q))
        .fold(f64::INFINITY, f64::min)
}

/// Every rapid of the kernel's chain (spindle frame) proven clear of
/// the stock it stood off: a retract up the line its own cut just
/// drilled is the kerf; a swing above the stock's Z top by the tool
/// body's rim (the wider of drill and shank, `body_rim`) is the end-
/// face detour (checked first: the swing between opposite stations
/// runs along the new pose's own axis); a rapid along its pose axis
/// (the approach to a feed-entry point) keeps the drill's edge - and,
/// behind the flutes, a wider shank's - outside the raw stock when
/// there is one; every other rapid keeps the body's rim outside
/// `st.r`. Anything else is a rapid through material, refused by name.
pub(super) fn rapids_clear(t: &Toolpath, st: &Standoff, tool: &Tool, unit: Unit) -> Result<(), GenError> {
    let tol = 1e-6;
    let (d, rim) = (tool.diameter, body_rim(tool));
    let mut kerf: Option<(Point3, [f64; 3])> = None;
    for (i, m) in t.moves.iter().enumerate() {
        let axis = m.axis.unwrap_or([0.0, 0.0, 1.0]);
        if m.kind != MoveKind::Rapid {
            kerf = Some((m.to, axis));
            continue;
        }
        if kerf.is_some_and(|(p, a)| on_line(m.from, p, a) && on_line(m.to, p, a)) {
            continue;
        }
        kerf = None;
        if st.z_max.is_some_and(|z| m.from.z.min(m.to.z) >= z + rim - tol) {
            continue;
        }
        let (got, need, what) = if on_line(m.to, m.from, axis) {
            let Some(raw) = st.raw else { continue };
            if let Some(line) = pose_line(m.from, axis) {
                // Issue #85: along a hole's own line the body must stand
                // outside where it first meets the stock on that line -
                // the face of a box, not its corner radius.
                let tip = line.u(m.from).min(line.u(m.to));
                let mut short = entry_grown(&st.mesh, raw, line, 0.5 * d, st.grow).map(|e| (tip, e));
                if tool.shank_d.is_finite() && tool.shank_d > d && tool.flute_len.is_finite() {
                    let fl = tool.flute_len.max(0.0);
                    let s = entry_grown(&st.mesh, raw, line, 0.5 * tool.shank_d, st.grow).map(|e| (tip + fl, e));
                    short = match (short, s) {
                        (Some(a), Some(b)) => Some(if b.1 - b.0 > a.1 - a.0 { b } else { a }),
                        (a, b) => a.or(b),
                    };
                }
                let Some((got, need)) = short else { continue };
                if got.is_nan() || got < need - tol {
                    return Err(GenError::Invalid(format!(
                        "cross-drill rapid {i} stops {} {} out along its hole's line, inside where \
                         the tool body meets the stock ({}, {}) - a rapid through material; raise \
                         the clearance or retract height, or turn the bar to size first",
                        units::len(got, unit, 3),
                        unit.suffix(),
                        units::len(need, unit, 3),
                        unit.suffix()
                    )));
                }
                continue;
            }
            let mut got = slab_approach(m.from, m.to, axis, 0.5 * d);
            if tool.shank_d.is_finite() && tool.shank_d > d && tool.flute_len.is_finite() {
                let fl = tool.flute_len.max(0.0);
                let back = |p: Point3| Point3::new(p.x + fl * axis[0], p.y + fl * axis[1], p.z + fl * axis[2]);
                got = got.min(slab_approach(back(m.from), back(m.to), axis, 0.5 * tool.shank_d));
            }
            (got, raw, "radius at the tool's edge")
        } else {
            (approach(m.from, m.to), st.r + rim, "radius plus the tool's rim")
        };
        if got.is_nan() || got < need - tol {
            return Err(GenError::Invalid(format!(
                "cross-drill rapid {i} passes {} {} from the spindle, inside the stock's \
                 {what} ({}, {}) - a rapid through material; raise the clearance or \
                 retract height, or turn the bar to size first",
                units::len(got, unit, 3),
                unit.suffix(),
                units::len(need, unit, 3),
                unit.suffix()
            )));
        }
    }
    Ok(())
}
