//! The stated-axis law of #2772 (parts 1 and 2): a rotary kind's
//! axis taken from a STATEMENT - the machine's stated rotary line
//! anchored through the work offset's origin, or the operation's own
//! stated direction - never a free fit alone. The fit (`axis.rs`)
//! reads the axis off the model's normals; a helical rotor or a
//! carving shares no such axis, while a machine's rotary line is a
//! fact about the steel, not a reading of the mesh. But a stated
//! line is JUDGED, never trusted: the model's normal lines must meet
//! it the way a surface of revolution's do - the same concurrency
//! share, coarseness gate and bearing law `axis::judge` runs on a
//! fitted line - because the radial kinds plan contact for a turned
//! surface. A line the faces disagree with would pose the radial
//! where the work is not, and on helical lobes that pose gouges (the
//! decision doc `2772-rotary-4axis-refusal` records why a looser fit
//! alone was dropped). So a disagreement refuses by name: with the
//! part's own fitted axis in the message when the model is a revolve
//! merely placed off the stated line (align the part, or move the
//! work offset), and with the freeform's 4-axis wording when no axis
//! fits at all - the radial drop, a later slice of the same issue, is
//! the answer for that work. A stated line never silently falls back
//! to a free fit: the statement is the machine's, and the refusal is
//! the honest output until a kernel can hold it.
//!
//! Provenance: the judging law is `axis.rs`'s own (its published
//! sources); a stated rotary line through the work offset's origin is
//! the indexed-post setup law of machine-tool practice. Our own
//! design notes; nothing of any vendor's consulted.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::axis::{self, data_of, judge, signed, Axis, Datum};

/// One stated line as the caller names it (the machine's rotary
/// letter, or "the operation's stated axis") - the refusal names what
/// the model's faces disagreed with.
pub type Stated = (String, Axis);

/// A stated line, canonical: the direction unit and determinately
/// signed (`axis::signed`'s law, so a stated line and the line the
/// fit reads agree bit for bit), the origin pinned to the axis's
/// canonical foot `o . u = 0` through `anchor` (the work offset's
/// world position - the fit's own pin, so a part truly on the line
/// keeps its path). `None` when `dir` is zero or not finite; the
/// caller refuses that by name, never reads it as unstated.
#[must_use]
pub fn canonical(dir: [f64; 3], anchor: Point3) -> Option<Axis> {
    let u = signed(Point3::new(dir[0], dir[1], dir[2]))?;
    let t = anchor.dot(u);
    Some(Axis {
        origin: Point3::new(anchor.x - u.x * t, anchor.y - u.y * t, anchor.z - u.z * t),
        dir: u,
    })
}

/// Whether `facets` read as a surface of revolution about `line`, and
/// the canonical line they read about: the fit's own gate (`judge`)
/// with none of its candidate search. `None` when they do not.
#[must_use]
pub fn holds(facets: &[Triangle], line: Axis) -> Option<Axis> {
    let (data, a_total) = data_of(facets);
    holds_data(&data, a_total, line)
}

/// The first of `lines` the model's faces hold as their axis of
/// revolution, in the caller's order (a trunnion's work-turning pair
/// read in chain order); `None` when none does. The silent form of
/// [`judged_any`] for a consumer that only wants the axis (the verify
/// pass: the generator has refused the op by name already).
#[must_use]
pub fn holds_any(facets: &[Triangle], lines: &[Axis]) -> Option<Axis> {
    let (data, a_total) = data_of(facets);
    lines.iter().find_map(|l| holds_data(&data, a_total, *l))
}

/// [`holds_any`] that refuses by name when no stated line holds: the
/// disagreement refusal of #2772, worded by what the free fit says
/// about the model (its own axis when one fits, the freeform's 4-axis
/// wording when none does - the fit is ASKED here for the message
/// only, never adopted as the axis).
pub fn judged_any(facets: &[Triangle], lines: &[Stated]) -> Result<Axis, GenError> {
    let (data, a_total) = data_of(facets);
    if data.is_empty() || a_total <= 0.0 {
        return Err(GenError::Invalid(
            "rotary has no surface to read a rotation axis from".into(),
        ));
    }
    lines
        .iter()
        .find_map(|(_, l)| holds_data(&data, a_total, *l))
        .ok_or_else(|| refused(facets, lines))
}

/// One line's judgement over data already read.
fn holds_data(data: &[Datum], a_total: f64, line: Axis) -> Option<Axis> {
    if data.is_empty() || a_total <= 0.0 {
        return None;
    }
    let u = signed(line.dir)?;
    let t = line.origin.dot(u);
    let ax = Axis {
        origin: Point3::new(line.origin.x - u.x * t, line.origin.y - u.y * t, line.origin.z - u.z * t),
        dir: u,
    };
    // The sagitta budget's scale: the area-weighted mean radius about
    // the line, over the facets that answer it (the fit's own `line`
    // accumulation, about the stated line instead).
    let (mut area_ans, mut r_wsum) = (0.0, 0.0);
    for d in data {
        let k = d.n.cross(u);
        if k.dot(k) <= 1e-8 {
            continue;
        }
        // The radius is the facet's distance FROM THE STATED LINE, not
        // from the world origin: a common model or WCS translation
        // moves the part and its line together, and a radius read off
        // the origin would scale the sagitta budget with the
        // translation, letting a wrong line pass far from home (the
        // review of PR #3674 - a 1 mm error defeated by +10000 mm).
        // The radius is the facet's distance FROM THE STATED LINE, not
        // from the world origin: a common model or WCS translation
        // moves the part and its line together, and a radius read off
        // the origin would scale the sagitta budget with the
        // translation, letting a wrong line pass far from home (the
        // review of PR #3674 - a 1 mm error defeated by +10000 mm).
        let rel = Point3::new(d.p.x - ax.origin.x, d.p.y - ax.origin.y, d.p.z - ax.origin.z);
        let s = rel.dot(u);
        let cp = Point3::new(rel.x - u.x * s, rel.y - u.y * s, rel.z - u.z * s);
        let rr = cp.dot(cp).sqrt();
        if rr.is_finite() {
            r_wsum += rr * d.a;
        }
        area_ans += d.a;
    }
    if area_ans <= 0.0 || r_wsum <= 0.0 {
        return None;
    }
    judge(data, ax, r_wsum / area_ans, a_total).map(|_| ax)
}

/// The refusal when the model's faces disagree with every stated
/// line: named, and worded by the free fit's own reading of the
/// model - never a silent fall back to that fit.
fn refused(facets: &[Triangle], lines: &[Stated]) -> GenError {
    let names: Vec<String> = lines.iter().map(|(n, _)| n.clone()).collect();
    let named = if names.len() == 1 {
        format!("the stated rotary line ({})", names[0])
    } else {
        format!("the stated rotary lines ({})", names.join(", "))
    };
    match axis::fit_report(facets) {
        // A revolve placed off the stated line: say where the part
        // turns, and the two acts that fix it.
        Ok(f) => GenError::Invalid(format!(
            "the model's faces disagree with {named} - they turn about their own axis through \
             ({:.3}, {:.3}, {:.3}) along ({:.3}, {:.3}, {:.3}). Align the part on the stated \
             line, or move the work offset onto the part's axis",
            f.axis.origin.x,
            f.axis.origin.y,
            f.axis.origin.z,
            f.axis.dir.x,
            f.axis.dir.y,
            f.axis.dir.z
        )),
        // No axis of revolution at all: the freeform's wording, with
        // the 4-axis case named (the radial drop is the coming slice).
        Err(_) => GenError::Invalid(format!(
            "the model's faces disagree with {named}, and they share no rotation axis - the \
             model is not a shape of revolution, and the rotary kinds plan contact for a turned \
             surface. 4-axis work on a part clamped on A that is not turned (helical lobes, a \
             carving) is not supported yet (#2772); on a 5-axis machine, finish a freeform with \
             the 5-Axis kinds"
        )),
    }
}

#[cfg(test)]
#[path = "axis_stated_tests.rs"]
mod tests;
