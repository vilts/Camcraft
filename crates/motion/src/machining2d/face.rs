//! Serpentine facing (split from `machining2d.rs` by M5.0.1); M11.1 A
//! adds the optional G2/G3 row-end reversal blends - provenance in
//! `blend.rs` (RS-274 / ISO 6983 circular interpolation, path filleting).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::path_picks::PathPicks;

use super::blend::{Turn, plan_reversals};
use super::face_holes::{self, FaceHoles};
use super::{face_keep, face_raster};
pub use super::face_keep::FaceKeep;
use super::levels::cut_levels_under;
// The enum moved to its own module (#261 part 2); every
// `super::face::GenError` path keeps working through this re-export.
pub use super::gen_error::GenError;

/// Facing: clear the top plane of a footprint in straight XY passes.
///
/// The path lives in world coordinates (regions arrive in the frame the
/// stock is drawn in). Rows band the footprint's Y extent at `stepover`,
/// centered a tool radius in from each edge so the full width is covered;
/// X runs overrun the bbox by a radius for clean entry and exit. Passes
/// snake back and forth at each cut depth - in the single-level default
/// rapids appear only at the two ends, matching how facers actually run;
/// staged facing adds the level-to-level lift through clearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceParams {
    pub tool_diameter: f64,
    /// Pass spacing in Y; the overlap 2r - stepover is what leaves the
    /// scallop height, which facing simply accepts.
    pub stepover: f64,
    /// Rapid plane above the material.
    pub clearance_z: f64,
    /// The finished surface height.
    pub cut_z: f64,
    /// First cut level for staged facing. `None` - the default, and what
    /// every pre-stepdown file carries - faces in one level at `cut_z`.
    pub top_z: Option<f64>,
    /// Level spacing while staging; must be positive when `top_z` is
    /// set, ignored otherwise.
    pub stepdown: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling: the first pass runs -X to +X. Conventional starts
    /// the other way.
    pub climb: bool,
    /// Turning-point blends (M11.1 A): each row-end reversal rides one
    /// tangent semicircle (a single G2/G3 move) where the legality law
    /// in `blend.rs` proves it; refused turns stay sharp and are named
    /// in the path's notes. `false` - every pre-M11.1 file - is the
    /// sharp serpentine, unchanged to the byte.
    pub blend_turns: bool,
    /// The turn's arc radius (#1759), read only with `blend_turns`:
    /// `None` is the semicircle of half the row spacing every earlier
    /// file turns on. A smaller radius turns on two quarter arcs joined
    /// by a straight step (`blend.rs`); a radius is held to
    /// `corner_arc_radius`'s door.
    pub blend_radius: Option<f64>,
    /// How a row meets a hole in the region (M13.11): lifted over at
    /// the clearance plane (the default), cut across when the hole is
    /// air, or ignored - the pre-M13.11 band serpentine. A region with
    /// no holes faces the same under every law.
    pub holes: FaceHoles,
    /// The stock top in the facer's frame (#2004), read only when
    /// staging: stock over `top_z` keeps a level at `top_z`, so the first
    /// bite is never deeper than one stepdown. `None` is Top Z as the
    /// stock top (no level there). A runtime value: never saved.
    pub stock_top: Option<f64>,
}

impl FaceParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut vals = vec![
            self.tool_diameter,
            self.stepover,
            self.clearance_z,
            self.cut_z,
            self.feed,
            self.plunge_feed,
        ];
        vals.extend([self.top_z, self.stepdown, self.stock_top].into_iter().flatten());
        if !vals.into_iter().all(f64::is_finite) {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if let Some(t) = self.top_z {
            if t < self.cut_z {
                return Err(GenError::Invalid("staged top below the finished height".into()));
            }
            if self.clearance_z <= t {
                // Equality is refused too: a level sitting ON the rapid
                // plane would plunge zero-depth, and the builder drops
                // zero-length moves - a silently missing plunge.
                return Err(GenError::Invalid("clearance plane not above the first cut level".into()));
            }
            if !(self.stepdown.is_some_and(|s| s > 0.0)) && t > self.cut_z {
                return Err(GenError::Invalid("staged facing needs a positive stepdown".into()));
            }
        }
        if self.stepdown.is_some_and(|s| s <= 0.0) {
            return Err(GenError::Invalid("stepdown must be positive".into()));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.blend_turns {
            super::fillet_tally::corner_arc_radius(self.blend_radius, self.tool_diameter).map_err(GenError::Invalid)?;
        }
        Ok(())
    }
}

/// Serpentine face passes over `region`. One level at `p.cut_z` when
/// `p.top_z` is `None` - the legacy facing, unchanged to the byte - or a
/// staged descent through `cut_levels_under(top_z, stock_top, cut_z,
/// stepdown)` when it is set: the rows are computed once and run at every level, each level
/// entered from the clearance plane the same way the single level was.
/// A stepover wider than the tool leaves an uncut band between rows -
/// visible in the path, never silently smoothed over, and never a crash:
/// facing simply cuts where it passes. With `p.blend_turns` each row-end
/// step becomes a tangent semicircle with the same endpoints, bulging into
/// the overrun, where `blend.rs`'s law proves it; every refusal keeps its
/// sharp corner and rides the path's notes. Rows never change.
pub fn face(region: &Region, p: &FaceParams) -> Result<Toolpath, GenError> {
    face_led(region, p, 0.0)
}

/// [`face`] with the entry run on by `lead` past its usual radius
/// overrun, along the first row's own direction (M11.3 owed): the plunge
/// stands `lead` farther out and the first row feeds in from there. Only
/// the entry point moves - rows, reversals and the exit are [`face`]'s.
/// `lead = 0` is [`face`] to the byte; a negative or non-finite lead is
/// refused.
pub fn face_led(region: &Region, p: &FaceParams, lead: f64) -> Result<Toolpath, GenError> {
    face_over(region, p, lead, None)
}

/// [`face_led`] told where material stands above the face: `solid` is
/// that outline (M13.11), read under [`FaceHoles::CutAir`] to tell
/// an air hole (cut straight across) from an island (lifted over);
/// `None` lifts over every hole and says so in the notes. Under
/// [`FaceHoles::Lift`] `solid` is the material standing at or above
/// the face (#2437): a hole it does not touch is below the cut plane
/// and is not bridged; `None` bridges every hole. Each row is
/// clipped to keep the tool centre a radius out of every bridged hole
/// (`face_holes`), the pieces are cut in row order, and the row crosses
/// a bridged hole through the clearance plane - up, across, plunge -
/// the facer's own level link. A hole that leaves material the clipped
/// rows cannot reach (a strip narrower than the tool) is refused by
/// name, never faced short.
pub fn face_over(
    region: &Region,
    p: &FaceParams,
    lead: f64,
    solid: Option<&Region>,
) -> Result<Toolpath, GenError> {
    face_over_kept(region, p, lead, solid, None)
}

/// [`face_over`] that keeps the whole tool clear of protected items
/// (#3343): at each level the rows are cut short at the [`FaceKeep`]
/// keep-out and the facer links over it at the clearance plane. A level
/// with no row piece left would leave the face above Cut Z, so the cut is
/// refused by name with the depth reached (#3526). `None` is
/// [`face_over`] to the byte.
pub fn face_over_kept(
    region: &Region,
    p: &FaceParams,
    lead: f64,
    solid: Option<&Region>,
    keep: Option<&FaceKeep<'_>>,
) -> Result<Toolpath, GenError> {
    face_picked(region, p, lead, solid, keep, &PathPicks::default())
}

/// [`face_over_kept`] under the user's start and exit picks (#3326), in
/// the region's frame: `face_raster` orders the same rows from the start
/// corner and refuses an exit the row count cannot reach. No pick: unchanged.
pub fn face_picked(
    region: &Region,
    p: &FaceParams,
    lead: f64,
    solid: Option<&Region>,
    keep: Option<&FaceKeep<'_>>,
    picks: &PathPicks,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    if !(lead.is_finite() && lead >= 0.0) {
        return Err(GenError::Invalid("the facing lead-in must be finite and not negative".into()));
    }
    let (lo, hi) = match region.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty footprint".into())),
    };
    let r = p.tool_diameter * 0.5;
    let mut ys = band(lo.y, hi.y, r, p.stepover)?;
    let (x_in, x_out) = (lo.x - r, hi.x + r);
    let zs = match (p.top_z, p.stepdown) {
        (Some(t), Some(s)) => cut_levels_under(t, p.stock_top, p.cut_z, s)?,
        _ => vec![p.cut_z],
    };

    // Rows are the same at every level, so the reversal plan is too:
    // decided once, its refusals stated once.
    let (mut turns, blend_notes) = plan_reversals(&ys, r, p.blend_turns, p.blend_radius);
    // #3326: the picks order the rows; the rows themselves never change.
    let raster = face_raster::plan(picks, (lo, hi), ys.len(), p.climb, p.cut_z)?;
    raster.order(&mut ys, &mut turns);
    // The rows' extents and their pieces around the bridged holes,
    // decided once like the reversals: a hole-free region keeps every
    // row whole, to the same numbers.
    let bridged = face_holes::forbidden(region, p.holes, solid, r)?;
    let (forbidden, hole_note) = (bridged.grown, bridged.note);
    let rows = raster.rows(&ys, x_in, x_out, lead);
    let pieces: Vec<Vec<(f64, f64)>> =
        rows.iter().map(|&(y, a, b)| face_holes::pieces(&forbidden, y, a, b)).collect();
    // Two bridged holes closer than the tool leave the strip between
    // them standing, with no row piece to cut it: refused by name.
    // Material beside a hole that only a tangent row would reach is the
    // rows' own limit (rows never change), stated in the notes like the
    // band a wide stepover leaves - visible, never silent.
    let mut unfaced = None;
    if !forbidden.is_empty() {
        if let Some((y, gap, x)) = face_holes::narrow_gap(&bridged.holes, &rows, &pieces, p.tool_diameter) {
            return Err(GenError::invalid_at(
                format!(
                    "two holes stand {gap:.2} mm apart on the row at Y={y:.2}, no wider than the {:.3} mm tool: the strip between them would stay standing - use a smaller tool, or cut through the holes",
                    p.tool_diameter
                ),
                Point3::new(x, y, p.cut_z),
            ));
        }
        unfaced = face_holes::uncut(region, &rows, &pieces, r)?;
    }
    let entry = pieces.iter().flatten().next().map(|&(x, _)| x).or(rows.first().map(|row| row.1)).unwrap_or(x_in);

    // The approach (#690): every mill op opens with a rapid to its start
    // on the clearance plane, so the post's first motion after the tool
    // call is a G00 there - never a feed from wherever the tool change
    // left the spindle. Facing starts the cursor where the others do.
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    for n in blend_notes.into_iter().chain(raster.note.clone()) {
        b.note(n);
    }
    if let Some(n) = hole_note {
        b.note(n);
    }
    if let Some(area) = unfaced {
        b.note(format!(
            "the rows leave {area:.2} mm^2 beside the bridged holes unfaced - no row runs tangent to them; a smaller stepover, or cutting through the holes, reaches it"
        ));
    }
    let mut lifted = false;
    let mut tally = face_keep::Tally::default();
    for &z in &zs {
        // #3343: the rows of this level, cut short at the protected keep-out.
        let Some(lv) = face_keep::level(keep, z, &rows, &pieces, (entry, ys[0]), &mut tally)? else {
            continue;
        };
        raster.held(&lv.pieces, z)?; // #3326: the picks, on the cut rows
        let (ex, ey) = lv.start;
        if tally.levels > 1 {
            // New material is engaged from the air, never from under:
            // lift to clearance and re-enter above the row's start.
            let at = b.at();
            b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
        }
        b.push(MoveKind::Rapid, Point3::new(ex, ey, p.clearance_z), None);
        b.push(MoveKind::Plunge, Point3::new(ex, ey, z), Some(p.plunge_feed));
        let pieces = lv.pieces.as_ref();
        for (i, y) in ys.iter().enumerate() {
            let (forward, ccw) = (raster.forward(i), raster.ccw(i));
            let end_x = rows[i].2;
            for &(x0, x1) in &pieces[i] {
                let at = b.at();
                if (at.x - x0).abs() > 0.0 || (at.y - *y).abs() > 0.0 || at.z != z {
                    // Across a bridged hole (or onto a row whose start
                    // was clipped): the level link, through clearance.
                    lifted = true;
                    link(&mut b, Point3::new(x0, *y, z), p);
                }
                b.push(MoveKind::Feed, Point3::new(x1, *y, z), Some(p.feed));
            }
            if let Some(&y1) = ys.get(i + 1) {
                let to = Point3::new(end_x, y1, z);
                let at = b.at();
                let next_whole = pieces[i + 1].first().is_some_and(|&(x0, _)| x0 == end_x);
                if at.x != end_x || at.y != *y || !next_whole || !lv.step_clear(end_x, *y, y1) {
                    // A hole or a keep-out took the row's end or the next
                    // row's start, or the step crosses a keep-out: no
                    // reversal to ride, the link takes the tool over.
                    lifted = true;
                    if let Some(&(x0, _)) = pieces[i + 1].first() {
                        link(&mut b, Point3::new(x0, y1, z), p);
                    }
                } else if let Some(turn) = turns.get(i).copied().flatten() {
                    // Blended reversal: the step's own endpoints, one
                    // semicircle bulging away from the footprint - a
                    // left (ccw) turn off a +X row that steps up, a
                    // right (cw) one off -X (`face_raster`).
                    match turn {
                        Turn::Semicircle => {
                            let center = Point2::new(end_x, (*y + y1) * 0.5);
                            b.push_arc(MoveKind::Feed, to, Some(p.feed), center, ccw);
                        }
                        // #1759: two quarter arcs of the radius asked
                        // for, the straight step between them `rho`
                        // farther out than the sharp step stood.
                        Turn::Corners(rho) => {
                            let out = end_x + if forward { rho } else { -rho };
                            let (c0, c1) = (Point2::new(end_x, *y + raster.step(rho)), Point2::new(end_x, y1 - raster.step(rho)));
                            b.push_arc(MoveKind::Feed, Point3::new(out, c0.y, z), Some(p.feed), c0, ccw);
                            b.push(MoveKind::Feed, Point3::new(out, c1.y, z), Some(p.feed));
                            b.push_arc(MoveKind::Feed, to, Some(p.feed), c1, ccw);
                        }
                    }
                } else {
                    b.push(MoveKind::Feed, to, Some(p.feed));
                }
            }
        }
    }
    if lifted && !forbidden.is_empty() {
        b.note("facing rows lifted over the region's holes at the clearance plane");
    }
    for n in tally.finish(keep)? {
        b.note(n);
    }
    let at = b.at();
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The facer's link from where the path stands to `to` on the cut
/// level: up to the clearance plane, across at it, and down at the
/// plunge feed - the level-to-level lift, used across a bridged hole.
fn link(b: &mut PathBuilder, to: Point3, p: &FaceParams) {
    let at = b.at();
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
    b.push(MoveKind::Rapid, Point3::new(to.x, to.y, p.clearance_z), None);
    b.push(MoveKind::Plunge, to, Some(p.plunge_feed));
}

/// Row centers banding [a + r, b - r]: start at the low edge's coverage
/// center, stride by `step`, and always end exactly at the high edge's
/// coverage center (a stride that would overshoot is replaced by that
/// snap; a band thinner than one tool diameter gets the single center
/// row). Never empty, never unsorted.
pub(super) fn band(a: f64, b: f64, r: f64, step: f64) -> Result<Vec<f64>, GenError> {
    let (first, last) = (a + r, b - r);
    let span = last - first;
    if span <= 0.0 {
        return Ok(vec![(a + b) * 0.5]);
    }
    pass_budget("stepover", span, step)?;
    let tol = span * 1e-9;
    let mut ys = vec![first];
    let mut y = first;
    while y + step < last - tol {
        y += step;
        ys.push(y);
    }
    if last - *ys.last().unwrap_or(&first) > tol {
        ys.push(last);
    }
    Ok(ys)
}

/// Most passes (rows or levels) one stepping ladder may lay (#691). A
/// step so small it would need more is refused by name before a single
/// entry is allocated: a 1e-9 mm stepover once asked for ~10^10 rows and
/// aborted the process on the failed allocation.
pub const MAX_PASSES: f64 = 100_000.0;

/// Refuses a `step` that would ladder `span` in more than [`MAX_PASSES`]
/// passes, or that cannot advance the coordinate at all (not finite, not
/// positive, or below the span's float resolution, where `y + step == y`
/// and the ladder would never end).
pub(super) fn pass_budget(what: &str, span: f64, step: f64) -> Result<(), GenError> {
    let n = span / step;
    let stalls = span + step == span;
    if !(step.is_finite() && step > 0.0) || !n.is_finite() || n > MAX_PASSES || stalls {
        return Err(GenError::Invalid(format!(
            "{what} {step} mm over {span:.3} mm needs more than {MAX_PASSES} passes - raise the {what}"
        )));
    }
    Ok(())
}
