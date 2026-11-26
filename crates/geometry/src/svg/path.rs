//! SVG path data (`d`) to absolute drawing commands.
//! Provenance: the W3C SVG 1.1 path-data grammar (M L H V C S Q T A Z, absolute and relative,
//! implicit repeats, the S/T reflection rule and the Z current-point rule); original
//! implementation.

use super::number::{Nums, finite};
use super::{SvgError, invalid};
use crate::geo2d::Point2;

/// One drawing command, absolute, in user units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Cmd {
    Move(Point2),
    Line(Point2),
    Cubic(Point2, Point2, Point2),
    Quad(Point2, Point2),
    /// The endpoint form of an elliptical arc; `phi` in degrees.
    Arc { rx: f64, ry: f64, phi: f64, large: bool, sweep: bool, to: Point2 },
    Close,
}

fn add(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x + b.x, a.y + b.y)
}

/// The reflection of `c` about `p`.
fn reflect(c: Point2, p: Point2) -> Point2 {
    Point2::new(2.0 * p.x - c.x, 2.0 * p.y - c.y)
}

fn bad(at: usize, why: &str) -> SvgError {
    invalid(format!("malformed path data at byte {at}: {why}"))
}

/// One number of command `op`, or a named refusal.
fn num(n: &mut Nums<'_>, op: u8, what: &str) -> Result<f64, SvgError> {
    n.sep();
    let at = n.at;
    let v = n.number().ok_or_else(|| bad(at, &format!("{what} is missing after '{}'", op as char)))?;
    finite(v, "a path value")
}

/// One point of command `op`, made absolute from `base`.
fn pt(n: &mut Nums<'_>, op: u8, base: Point2) -> Result<Point2, SvgError> {
    let x = num(n, op, "x")?;
    let y = num(n, op, "y")?;
    Ok(add(base, Point2::new(x, y)))
}

/// Parse `d`, refusing a malformed path by name. `max_cmds` bounds the command count.
pub(super) fn parse(d: &str, max_cmds: usize) -> Result<Vec<Cmd>, SvgError> {
    let mut nums = Nums::new(d);
    let mut out: Vec<Cmd> = Vec::new();
    let mut cur = Point2::default();
    let mut start = Point2::default();
    // The second control point of the last C/S, or the control point of the last Q/T.
    let mut last_cubic: Option<Point2> = None;
    let mut last_quad: Option<Point2> = None;
    let mut cmd: Option<u8> = None;
    loop {
        if nums.done() {
            break;
        }
        let letter = nums.letter();
        let op = match (letter, cmd) {
            (Some(l), _) => {
                nums.bump();
                l
            }
            // A number repeats the last command; after a moveto it is a lineto.
            (None, Some(b'M')) => b'L',
            (None, Some(b'm')) => b'l',
            (None, Some(c)) if !matches!(c, b'Z' | b'z') => c,
            _ => return Err(bad(nums.at, "a command letter is expected")),
        };
        if out.is_empty() && !matches!(op, b'M' | b'm') {
            return Err(bad(nums.at, "path data must start with a moveto (M or m)"));
        }
        if out.len() >= max_cmds {
            return Err(invalid(format!("path has more than {max_cmds} commands: larger than the import budget")));
        }
        let rel = op.is_ascii_lowercase();
        let base = if rel { cur } else { Point2::default() };
        let n = &mut nums;
        let (next, keep_cubic, keep_quad) = match op.to_ascii_uppercase() {
            b'M' => {
                let p = pt(n, op, base)?;
                start = p;
                (Cmd::Move(p), None, None)
            }
            b'L' => (Cmd::Line(pt(n, op, base)?), None, None),
            b'H' => {
                let x = num(n, op, "x")?;
                (Cmd::Line(Point2::new(if rel { cur.x + x } else { x }, cur.y)), None, None)
            }
            b'V' => {
                let y = num(n, op, "y")?;
                (Cmd::Line(Point2::new(cur.x, if rel { cur.y + y } else { y })), None, None)
            }
            b'C' => {
                let (c1, c2, p) = (pt(n, op, base)?, pt(n, op, base)?, pt(n, op, base)?);
                (Cmd::Cubic(c1, c2, p), Some(c2), None)
            }
            b'S' => {
                let c1 = last_cubic.map_or(cur, |c| reflect(c, cur));
                let (c2, p) = (pt(n, op, base)?, pt(n, op, base)?);
                (Cmd::Cubic(c1, c2, p), Some(c2), None)
            }
            b'Q' => {
                let (c, p) = (pt(n, op, base)?, pt(n, op, base)?);
                (Cmd::Quad(c, p), None, Some(c))
            }
            b'T' => {
                let c = last_quad.map_or(cur, |q| reflect(q, cur));
                let p = pt(n, op, base)?;
                (Cmd::Quad(c, p), None, Some(c))
            }
            b'A' => {
                let rx = num(n, op, "rx")?;
                let ry = num(n, op, "ry")?;
                let phi = num(n, op, "the x-axis rotation")?;
                n.sep();
                let at = n.at;
                let large = n.flag().ok_or_else(|| bad(at, "the large-arc flag must be 0 or 1"))?;
                n.sep();
                let at = n.at;
                let sweep = n.flag().ok_or_else(|| bad(at, "the sweep flag must be 0 or 1"))?;
                let to = pt(n, op, base)?;
                (Cmd::Arc { rx, ry, phi, large, sweep, to }, None, None)
            }
            b'Z' => (Cmd::Close, None, None),
            _ => return Err(bad(n.at.saturating_sub(1), &format!("'{}' is not a path command", op as char))),
        };
        cur = match next {
            Cmd::Move(p) | Cmd::Line(p) | Cmd::Cubic(_, _, p) | Cmd::Quad(_, p) | Cmd::Arc { to: p, .. } => p,
            Cmd::Close => start,
        };
        last_cubic = keep_cubic;
        last_quad = keep_quad;
        cmd = Some(op);
        out.push(next);
        nums.sep();
    }
    Ok(out)
}
