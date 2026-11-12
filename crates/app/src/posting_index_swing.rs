//! The index-swing clause of the program gate (#804, #818): before a
//! rotary that turns the work about a horizontal line indexes, the tool
//! is retracted to the machine's safe Z - and the tool tip there must
//! clear the radius the stock, the parts and the workholding that turns
//! with them sweep about that line. Too short a clearance refuses the
//! post by name.
//!
//! Provenance: our own design (issues #804, #813, #818). The geometry is
//! plain: every point of a vertical tool at height h above a horizontal
//! line through the WCS origin is at least h from it (its distance is
//! the hypotenuse of h and the level offset), so the tip is the tool's
//! nearest point to the line; the swept radius is the farthest stated
//! vertex from that line, `|(p - o) x u|`. Machine numbers are the
//! `.mch` file's (Tormach 770MX spec page, Tormach 4th-axis comparison
//! sheet 02/24); rotary lettering is RS-274 / ISO 6983 - no vendor
//! implementation inspected.
//!
//! The laws:
//! - **Who is judged.** Every posted op whose machine states a rotary
//!   that turns the work ([`verdict`]): a G30 or a section start can
//!   index before any op, so every op's retract is an index pose. The
//!   line is the stated `axis` vector, else the RS-274 letter's (A about
//!   X, B about Y, C about Z, #813). A level line (a table A, a table B,
//!   an A stated off X) is judged; a vertical one (a C) keeps every
//!   height and is safe; a leaning, zero or non-finite vector, or no
//!   vector and no RS-274 letter, refuses by name - never judged as if
//!   level. Machines without one are untouched.
//! - **Where the tip is.** The WCS origin sits on the rotary's line
//!   (the indexed post's setup law), so at the safe Z the tip is at
//!   program Z `nose_to_axis - tool length` (cutter, shank and holder:
//!   the same protrusion the magazine law measures). An unstated or
//!   non-finite `nose_to_axis` refuses: the gate cannot show the turn
//!   clears.
//! - **What sweeps.** The farthest vertex, from the line through the
//!   WCS origin, of the setup's stock, its parts and its tagged fixtures
//!   that turn with the work (a chuck); fixed steel does not turn. Along
//!   the whole line: the tool's position at the index is the G30
//!   position, which the program does not know. With two rotaries
//!   turning the work (a trunnion's A and C), each moves the other's
//!   line, so the sweep is taken from the WCS origin itself - the only
//!   distance every turn about a line through it keeps.
//! - **The margin.** [`MARGIN_MM`] between the tip and the swept radius.

use crate::{fixture, model, setup, setup_align};
use flecs_ecs::prelude::World;

/// The clearance the tip must keep over the swept radius, mm (5 mm,
/// about 0.2 in): covers the nominal spec numbers behind `nose_to_axis`,
/// a tool-length measurement and the chuck's centring.
pub const MARGIN_MM: f64 = 5.0;

/// How far a stated axis vector may lean, as a fraction of its length,
/// and still read as horizontal (or vertical). 1e-6 rad is 1 um of
/// height over a metre of travel - far inside [`MARGIN_MM`] - while a
/// visibly leaning line is refused, never judged as if level.
const LEVEL_TOL: f64 = 1e-6;

/// Which line a rotary turns about, as far as the swing clause can
/// tell (#813, #818). Every line passes through the WCS origin (the
/// indexed post's setup law).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RotaryLine {
    /// A horizontal line along this unit vector (z = 0): a stated level
    /// vector, or an unstated one on a rotary named A (RS-274 / ISO
    /// 6983: about X) or B (about Y). Its index can swing the work up
    /// into the tool - judged.
    Horizontal([f64; 3]),
    /// A vertical line: a stated vector along Z, or an unstated one on a
    /// rotary named C. Turning keeps every point's height - safe.
    Vertical,
    /// A stated vector that leans - neither level nor vertical - or one
    /// that is zero or not finite. Not judged: refused by name.
    Leaning,
    /// No vector and no RS-274 rotary letter: the line is unknown.
    Unknown,
}

/// The line rotary `a` turns about (#813, #818): its stated `axis`
/// vector, else the RS-274 letter's default line, else
/// [`RotaryLine::Unknown`].
#[must_use]
pub fn rotary_line(a: &crate::machine::AxisLimits) -> RotaryLine {
    let Some(d) = a.axis else {
        return match a.name.trim().to_ascii_uppercase().as_str() {
            "A" => RotaryLine::Horizontal([1.0, 0.0, 0.0]),
            "B" => RotaryLine::Horizontal([0.0, 1.0, 0.0]),
            "C" => RotaryLine::Vertical,
            _ => RotaryLine::Unknown,
        };
    };
    let len = d[0].hypot(d[1]).hypot(d[2]);
    if !(len.is_finite() && len > 0.0 && d.iter().all(|c| c.is_finite())) {
        return RotaryLine::Leaning;
    }
    let level = d[0].hypot(d[1]);
    if d[2].abs() <= LEVEL_TOL * len && level > 0.0 {
        // Exactly level: the tip's distance to it is at least its height.
        RotaryLine::Horizontal([d[0] / level, d[1] / level, 0.0])
    } else if level <= LEVEL_TOL * len {
        RotaryLine::Vertical
    } else {
        RotaryLine::Leaning
    }
}

/// What the clause makes of a machine's work-turning rotaries.
enum Verdict {
    /// The rotaries whose index can swing the work up: the first one's
    /// letter and unit line, the least stated `nose_to_axis` (or the
    /// first rotary whose one is unstated or garbled), and whether another
    /// rotary also turns the work (then lines move: the sweep is taken
    /// about the WCS origin, which every line passes through).
    Judged { letter: String, line: [f64; 3], nose: Result<f64, Nose>, stacked: bool },
    /// A rotary turning the work about a line nobody stated (#813).
    Unlined(String),
    /// A rotary turning the work about a leaning or garbled line (#818).
    Leaning(String),
}

/// Why a judged rotary's `nose_to_axis` cannot be used: its letter.
enum Nose {
    Unstated(String),
    NotFinite(String),
}

/// The machine's work-turning rotaries, judged (#818): an unknown or a
/// leaning line refuses first, then every horizontal one is judged;
/// vertical ones alone are safe. None when nothing needs judging.
fn verdict(m: &crate::machine::Machine) -> Option<Verdict> {
    let turning: Vec<_> = m.axes.iter().filter(|a| a.rotary && a.turns_part == Some(true)).collect();
    let lines: Vec<_> = turning.iter().map(|a| (*a, rotary_line(a))).collect();
    if let Some((a, _)) = lines.iter().find(|(_, l)| *l == RotaryLine::Unknown) {
        return Some(Verdict::Unlined(a.name.clone()));
    }
    if let Some((a, _)) = lines.iter().find(|(_, l)| *l == RotaryLine::Leaning) {
        return Some(Verdict::Leaning(a.name.clone()));
    }
    let level: Vec<_> = lines
        .iter()
        .filter_map(|(a, l)| match l {
            RotaryLine::Horizontal(u) => Some((*a, *u)),
            _ => None,
        })
        .collect();
    let (first, line) = *level.first()?;
    let mut nose = Ok(f64::INFINITY);
    for (a, _) in &level {
        nose = match (nose, a.nose_to_axis) {
            (Ok(n), Some(d)) if d.is_finite() => Ok(n.min(d)),
            (Ok(_), Some(_)) => Err(Nose::NotFinite(a.name.clone())),
            (Ok(_), None) => Err(Nose::Unstated(a.name.clone())),
            (e, _) => e,
        };
    }
    Some(Verdict::Judged { letter: first.name.clone(), line, nose, stacked: turning.len() > 1 })
}

/// The angle an op indexes to about `line` (horizontal, unit): the
/// first leaning move's tool axis read as the turn that brings +Z onto
/// it, `atan2(t . (line x Z), t . Z)` - for X the post's own
/// `a = atan2(-y, z)`, for Y `b = atan2(x, z)` - else 0.
fn index_angle(path: &crate::cl::Toolpath, line: [f64; 3]) -> f64 {
    let side = [line[1], -line[0], 0.0]; // line x Z
    path.moves
        .iter()
        .find_map(|mv| mv.axis.filter(|a| !crate::cl::is_vertical(a)))
        .map_or(0.0, |t| (t[0] * side[0] + t[1] * side[1]).atan2(t[2]).to_degrees())
}

/// The distance of `p - origin` from the line through `origin` along
/// unit `line`, `|(p - o) x u|`; with `line` None, from `origin` itself.
fn distance(p: [f64; 3], origin: [f64; 3], line: Option<[f64; 3]>) -> f64 {
    let v = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
    match line {
        None => v[0].hypot(v[1]).hypot(v[2]),
        Some(u) => {
            let c = [v[1] * u[2] - v[2] * u[1], v[2] * u[0] - v[0] * u[2], v[0] * u[1] - v[1] * u[0]];
            c[0].hypot(c[1]).hypot(c[2])
        }
    }
}

/// The farthest point of the setup's turning bodies from `line` (or,
/// None, from `origin`), mm (0 with nothing stated).
fn swept_radius(world: &World, declared: Option<&str>, origin: [f64; 3], line: Option<[f64; 3]>) -> f64 {
    let mut bodies: Vec<flecs_ecs::prelude::Entity> = Vec::new();
    bodies.extend(setup::stock_entity(world, declared));
    bodies.extend(
        model::model_entities(world)
            .into_iter()
            .filter(|(_, e)| setup_align::is_part(world, e))
            .map(|(_, e)| e),
    );
    bodies.extend(
        fixture::tagged(world, declared)
            .into_iter()
            .filter(|(_, e)| !fixture::def_of(world, e).is_some_and(|d| d.fixed))
            .map(|(_, e)| e),
    );
    let mut r = 0.0_f64;
    for e in bodies {
        for t in setup_align::facets_in(world, &e, declared).unwrap_or_default() {
            for p in [t.a, t.b, t.c] {
                // A vertex nobody can place sweeps without bound: refused.
                let d = distance([p.x, p.y, p.z], origin, line);
                r = if d.is_finite() { r.max(d) } else { f64::INFINITY };
            }
        }
    }
    r
}

/// The clause: one problem per op whose index pose cannot be shown to
/// clear, in program order.
pub(super) fn index_swing_problems(world: &World) -> Vec<String> {
    let Ok(ops) = super::program_ops(world) else {
        return Vec::new(); // the post refuses the program by name first
    };
    let mut out = Vec::new();
    for (name, e) in ops {
        let Some(op) = model::operation_of(world, &e) else { continue };
        let Some(m) = crate::machine::of_setup(world, op.setup.as_deref()).1 else { continue };
        let (letter, line, nose, stacked) = match verdict(&m) {
            None => continue,
            Some(Verdict::Unlined(r)) => {
                out.push(format!(
                    "{name}: {} turns the work on rotary {r} but states no axis vector for it, and {r} is no \
                     RS-274 rotary letter - whether its index swings the work under the tool cannot be judged \
                     (state its \"axis\" in the machine file)",
                    m.model
                ));
                continue;
            }
            Some(Verdict::Leaning(r)) => {
                out.push(format!(
                    "{name}: {} turns the work on rotary {r} about a line that is neither level nor vertical \
                     (or no direction at all) - its index swing is not judged, so the post is refused (state a \
                     level or vertical \"axis\" for {r} in the machine file)",
                    m.model
                ));
                continue;
            }
            Some(Verdict::Judged { letter, line, nose, stacked }) => (letter, line, nose, stacked),
        };
        let a = model::toolpath_of(world, &e).map_or(0.0, |c| index_angle(&c.path, line));
        let finite = |n: f64| if n.is_finite() { Ok(n) } else { Err(Nose::NotFinite(letter.clone())) };
        let nose = match nose.and_then(finite) {
            Ok(n) => n,
            Err(Nose::NotFinite(r)) => {
                out.push(format!(
                    "{name}: {} turns the work on {r} with a nose_to_axis that is no finite number - the \
                     tool's clearance over the part swept by the {letter}{a:.3} index cannot be judged",
                    m.model
                ));
                continue;
            }
            Err(Nose::Unstated(r)) => {
                out.push(format!(
                    "{name}: {} turns the work on {r} but states no nose_to_axis - the tool's clearance \
                     over the part swept by the {letter}{a:.3} index cannot be judged",
                    m.model
                ));
                continue;
            }
        };
        // A lost tool is the machine clause's refusal, by name, already.
        let Some(tool_e) = model::find_by_name(world, &op.tool) else { continue };
        let Some(tool) = model::tool_of(world, &tool_e) else { continue };
        let tool = match crate::tool_body::judged(world, &tool_e, tool) {
            Ok((t, _)) => t,
            Err(why) => {
                out.push(format!("{name}: tool {}: {why} - the {letter} index clearance cannot be judged", op.tool));
                continue;
            }
        };
        let len = tool.flute_len + tool.shank_len + tool.holder_len;
        let (_, origin) = super::wcs_context(world, op.setup.as_deref());
        // Two rotaries turning the work move each other's lines: only the
        // distance from the WCS origin, on every line, survives both turns.
        let swept = swept_radius(world, op.setup.as_deref(), origin, (!stacked).then_some(line));
        let tip = nose - len;
        let clearance = tip - swept;
        if !(clearance.is_finite() && clearance >= MARGIN_MM) {
            out.push(format!(
                "{name}: the {letter}{a:.3} index turns the work under the tool - at the safe Z the tip of \
                 {} ({len:.3} mm long) is {tip:.3} mm above the {letter} centerline, the stock, part and \
                 turning fixtures sweep {swept:.3} mm{}, a clearance of {clearance:.3} mm against the \
                 {MARGIN_MM:.1} mm needed",
                op.tool,
                if stacked { " (from the WCS origin: more than one rotary turns the work)" } else { "" }
            ));
        }
    }
    out
}
