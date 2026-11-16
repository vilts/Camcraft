//! The standing-steel half of the index-swing clause (#1107): as a 4th
//! axis that turns the work about X indexes, the WORK (stock, parts and
//! the workholding that turns with them) sweeps a cylinder about the A
//! centerline - and that cylinder must clear what stands still under
//! and beside it: the face the rotary is bolted to, and every fixed
//! fixture over the work's X span. A strike refuses the post by name;
//! standing steel the clause cannot read is named in the gate's
//! not-judged line, never skipped in silence.
//!
//! Provenance: our own design (issues #1107, #818). The geometry is
//! plain: the work turning about a level line through the WCS origin
//! keeps every point's distance from that line and its position along
//! it, so it stays within its swept radius of the line over its span
//! along the line; a body is clear when its distance from the line
//! exceeds that radius. For a level unit line `u` that distance is
//! `hypot(s, z)` with `s` the offset along `u x Z` and `z` the height
//! over the origin, so a body's box in `(s, z)` bounds it from below. Two
//! rotaries turning the work move each other's lines, which all pass
//! through the WCS origin: the work then stays within its swept radius
//! of the origin itself. The microARC 4's 3.15 in centre height is
//! Tormach's 4th-axis comparison sheet 02/24 - no vendor implementation
//! inspected.
//!
//! The laws:
//! - **Who is judged.** The ops the tool half judges ([`verdict`]): a
//!   rotary that turns the work about a level line - A about X, B about
//!   Y, or a stated level vector off both. An unlined or leaning
//!   rotary is the tool half's refusal already; a vertical one keeps
//!   every height.
//! - **The face the rotary stands on.** The machine file's
//!   `axis_to_table` - the rotary's centre height over its own mounting
//!   face. Whatever stands under the rotary (the table, a fixture plate,
//!   a subplate) only lifts the line with it, so the nearest standing
//!   face under the rotary is exactly this far down. Unstated, or more
//!   than one rotary turning the work (the face under them moves with
//!   the turns): named not judged.
//! - **Fixed fixtures, one line.** Every fixed fixture of the setup whose
//!   span along the line overlaps the work's: its `(s, z)` box's
//!   distance from the line. Steel wholly beside the work along the line
//!   is clear. A box that reaches across the line (a tailstock, the
//!   rotary's own body) or a fixture with no facets cannot be judged by
//!   a radius and is named not judged.
//! - **Fixed fixtures, stacked rotaries.** The fixture box's distance
//!   from the WCS origin against the work's swept radius about it; a box
//!   holding the origin is named not judged.
//! - **The margin.** [`MARGIN_MM`], the same as the tool's.

use super::index_swing::{MARGIN_MM, Swept, Verdict, index_angle, swept, verdict};
use crate::{fixture, model};
use flecs_ecs::prelude::World;

/// The prefix of the clause's not-judged line, said once.
pub const SWING_NOT_JUDGED: &str = "index swing did not judge the work against the standing steel";

/// Overlap slack, mm: steel touching the work's end face is beside it.
const TOUCH_MM: f64 = 1e-6;

/// The clause's output: blocking problems (one per strike) and the
/// names of what it could not judge (deduplicated, program order).
#[derive(Debug, Default)]
pub(super) struct Standing {
    pub problems: Vec<String>,
    pub not_judged: Vec<String>,
}

/// The distance from the origin to the box spanned by `ranges`, or None
/// when the box holds the origin (it reaches across the line).
fn box_distance(ranges: &[(f64, f64)]) -> Option<f64> {
    let gap = |(lo, hi): (f64, f64)| {
        if lo > 0.0 {
            lo
        } else if hi < 0.0 {
            -hi
        } else {
            0.0
        }
    };
    let gaps: Vec<f64> = ranges.iter().map(|r| gap(*r)).collect();
    gaps.iter().any(|g| *g > 0.0).then(|| gaps.iter().fold(0.0_f64, |d, g| d.hypot(*g)))
}

/// The ranges of `f` over `pts`.
fn ranges<const N: usize>(pts: &[[f64; 3]], f: impl Fn([f64; 3]) -> [f64; N]) -> [(f64, f64); N] {
    let mut out = [(f64::INFINITY, f64::NEG_INFINITY); N];
    for p in pts {
        for (r, v) in out.iter_mut().zip(f(*p)) {
            *r = (r.0.min(v), r.1.max(v));
        }
    }
    out
}

/// One fixed fixture against the swept work: Ok(None) clear or beside
/// the work, Ok(Some(d)) its distance from the line (or, `line` None,
/// from the origin) when the work reaches it, Err(why) when it cannot
/// be judged.
fn fixed_strike(
    world: &World,
    e: &flecs_ecs::prelude::Entity,
    s: &Swept,
    origin: [f64; 3],
    line: Option<([f64; 3], &str)>,
) -> Result<Option<f64>, String> {
    let tris = model::world_facets(world, e).unwrap_or_default();
    if tris.is_empty() {
        return Err("it has no facets".into());
    }
    let mut pts = Vec::with_capacity(tris.len() * 3);
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        let v = [p.x - origin[0], p.y - origin[1], p.z - origin[2]];
        if !v.iter().all(|c| c.is_finite()) {
            return Err("its facets are not finite".into());
        }
        pts.push(v);
    }
    let d = match line {
        Some((u, letter)) => {
            let Some((w0, w1)) = s.span else {
                return Ok(None);
            };
            let side = [u[1], -u[0]]; // u x Z
            let [t, sd, z] = ranges(&pts, |v| [v[0] * u[0] + v[1] * u[1], v[0] * side[0] + v[1] * side[1], v[2]]);
            if t.1 <= w0 + TOUCH_MM || t.0 >= w1 - TOUCH_MM {
                return Ok(None); // beside the work along the line: the swing never reaches it
            }
            box_distance(&[sd, z])
                .ok_or_else(|| format!("it reaches across the {letter} centerline over the work's length"))?
        }
        None => box_distance(&ranges(&pts, |v| v))
            .ok_or_else(|| "it holds the WCS origin the stacked rotaries turn the work about".to_string())?,
    };
    Ok((s.r + MARGIN_MM > d).then_some(d))
}

/// Name `line` as not judged, once.
fn note(out: &mut Standing, line: String) {
    if !out.not_judged.contains(&line) {
        out.not_judged.push(line);
    }
}

/// The clause, in program order.
pub(super) fn standing_problems(world: &World) -> Standing {
    let mut out = Standing::default();
    let Ok(ops) = super::program_ops(world) else {
        return out; // the post refuses the program by name first
    };
    for (name, e) in ops {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        let Some(m) = crate::machine::of_setup(world, op.setup.as_deref()).1 else {
            continue;
        };
        let Some(Verdict::Judged { letter, line, stacked, .. }) = verdict(&m) else {
            continue;
        };
        let a = model::toolpath_of(world, &e).map_or(0.0, |c| index_angle(&c.path, line));
        let (_, origin) = super::wcs_context(world, op.setup.as_deref());
        let s = swept(world, op.setup.as_deref(), origin, (!stacked).then_some(line));
        let about = if stacked {
            "the WCS origin (more than one rotary turns the work)".to_string()
        } else {
            format!("the {letter} centerline")
        };
        let face = m.axes.iter().find(|x| x.name == letter).and_then(|x| x.axis_to_table);
        match face {
            _ if stacked => note(
                &mut out,
                format!(
                    "{} turns the work on more than one rotary: the face under them moves with the turns, so the work is not judged against the face the rotary stands on",
                    m.model
                ),
            ),
            None => note(
                &mut out,
                format!(
                    "{} states no axis_to_table for {letter}: the work is not judged against the face the rotary stands on",
                    m.model
                ),
            ),
            Some(h) if !(h.is_finite() && h - s.r >= MARGIN_MM) => out.problems.push(format!(
                "{name}: the {letter}{a:.3} index swings the work into the face the rotary stands on - the stock, \
                 part and turning fixtures sweep {:.3} mm about the {letter} centerline, which stands {h:.3} mm \
                 above that face (table, plate or subplate), a clearance of {:.3} mm against the {MARGIN_MM:.1} mm \
                 needed",
                s.r,
                h - s.r
            )),
            Some(_) => {}
        }
        for (fname, fe) in fixture::tagged(world, op.setup.as_deref()) {
            if !fixture::def_of(world, &fe).is_some_and(|d| d.fixed) {
                continue; // it turns with the work: part of the sweep
            }
            match fixed_strike(world, &fe, &s, origin, (!stacked).then_some((line, letter.as_str()))) {
                Ok(None) => {}
                Ok(Some(d)) => out.problems.push(format!(
                    "{name}: the {letter}{a:.3} index swings the work into standing fixture {fname} - the work \
                     sweeps {:.3} mm about {about} and {fname} stands {d:.3} mm from it, a clearance of {:.3} mm \
                     against the {MARGIN_MM:.1} mm needed",
                    s.r,
                    d - s.r
                )),
                Err(why) => note(&mut out, format!("standing fixture {fname}: {why}")),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::box_distance;

    #[test]
    fn a_box_across_the_line_has_no_distance_and_one_under_it_has_its_gap() {
        assert_eq!(box_distance(&[(-5.0, 5.0), (-5.0, 5.0)]), None);
        assert_eq!(box_distance(&[(-5.0, 5.0), (-90.0, -80.0)]), Some(80.0));
        let d = box_distance(&[(30.0, 50.0), (-90.0, -40.0)]).unwrap_or(f64::NAN);
        assert!((d - 50.0).abs() < 1e-12, "{d}");
        // Three ranges: the distance from the origin itself.
        let d = box_distance(&[(3.0, 9.0), (-1.0, 1.0), (-8.0, -4.0)]).unwrap_or(f64::NAN);
        assert!((d - 5.0).abs() < 1e-12, "{d}");
        assert_eq!(box_distance(&[(-1.0, 1.0); 3]), None);
    }
}
