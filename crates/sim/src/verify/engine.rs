//! The `verify` dispatcher: runs the verifier's laws (a)-(f2) in their fixed order, each specced from the public sources named in `verify`'s module law (split from `verify.rs` by M5.0.1 and into law modules by M13.7; structure only).

use super::types::{VerifyContext, Warning};
use super::laws::{bands_vs_fixtures, rapids_vs_fixtures_spindle};
use super::axis_door::axis_door;
use super::rest::Rest;
use super::{crossing, finish, rapids, reach, section};

use crate::cl::Toolpath;

/// Verify a generated path. Grid-bucketed in feed count (crossings), linear
/// per sample for the rest; called by the operation layer after
/// generation, never on a systems hot path.
pub fn verify(path: &Toolpath, cx: &VerifyContext) -> Vec<Warning> {
    verify_bodies(path, cx, &[])
}

/// [`verify`] of a path cut on a model that is the union of touching
/// bodies (M13.11): `bodies` are the per-solid index ranges of
/// `cx.model`, and every plane-section reader (the gouge law's islands,
/// the column surface, the posed rapid's overhead) reads the plane as
/// the 2D union of the bodies' own sections - a seam of two coincident
/// walls between two models is interior, never a failed slice that
/// convicts every move at that plane. Fewer than two ranges is
/// [`verify`], byte for byte.
pub fn verify_bodies(
    path: &Toolpath,
    cx: &VerifyContext,
    bodies: &[std::ops::Range<usize>],
) -> Vec<Warning> {
    let mut warns: Vec<Warning> = Vec::new();

    // The axis door (#1312): a non-finite or zero-length tool axis is
    // NonFinite's, and a non-unit one is normalised once, so no law
    // below reads a scaled or unreadable pose.
    let door = axis_door(path);
    let path: &Toolpath = &door.path;
    for (i, m) in path.moves.iter().enumerate() {
        if !(m.from.is_finite() && m.to.is_finite()) || door.refused.contains(&i) {
            warns.push(Warning::NonFinite { at: i });
        }
    }

    // The chord budget is shared by laws (a)-(f), so it is bound here,
    // outside every mode gate.
    let chord_tol = cx.gouge_tol;

    // (a) Crossing - [`crossing::crossings`].
    warns.extend(crossing::crossings(path, cx, chord_tol));

    // The plane-section cache and rest-material surface shared by laws
    // (b), (c), (c3) and (d) - [`Rest`].
    let rest = Rest::new(path, cx, chord_tol, bodies);

    // (b) Gouge, then (b2) the solid law, with the op's real tip
    // (M13.7) - [`section`].
    let cutter = super::Cutter::judged(cx.tool, cx.cutter);
    let lift = cutter.tip_height();
    if section::judged(cx.mode) {
        section::tangency(path, cx, &rest, lift, chord_tol, &mut warns);
        section::solid(path, cx, &cutter, &mut warns);
    }

    // (c) Shank/holder reach and (c3) the mid-turn sweep - [`reach`].
    reach::band_reach(path, cx, &rest, chord_tol, &mut warns);

    // (c2) Bands against fixtures (M3.2 stage B) - the law lives in
    // [`bands_vs_fixtures`] so the post gate can run it without a
    // region. Run for EVERY mode: steel has no tangency courtesy.
    if !cx.fixtures.is_empty() {
        warns.extend(bands_vs_fixtures(
            path,
            cx.tool,
            cx.fixtures,
            cx.spindle,
            chord_tol,
        ));
    }

    // (d) Rapids in stock - [`rapids`].
    rapids::rapids_in_stock(path, cx, &rest, &cutter, &mut warns);

    // (d2) Rapids against fixtures (M3.2 stage B) - the law lives in
    // [`rapids_vs_fixtures`] so the post gate can run it without a
    // region.
    if !cx.fixtures.is_empty() {
        warns.extend(rapids_vs_fixtures_spindle(path, cx.tool, cx.fixtures, cx.spindle));
    }

    // (e) Cycle reach - [`reach::cycle_reach`].
    reach::cycle_reach(path, cx, &mut warns);

    // (f) Finish envelope and (f2) posed contact - [`finish`].
    finish::finish_envelope(path, cx, chord_tol, &mut warns);

    warns
}
