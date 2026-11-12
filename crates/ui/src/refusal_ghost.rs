//! #261 part 3: the diagnostic ghost - the moves a refused generation had
//! built, drawn so the user sees what the operation was doing when it
//! stopped. It must never read as a toolpath: every move draws DASHED in
//! the palette's gouge colour, dimmed (rapids dimmer still), with no
//! direction chevrons, and only while the refusal stands. The moves come
//! off the refusal (`State::refusal_ghost`), never off the operation, so
//! drawing one changes nothing that caches, simulates or posts.
//! #871: the dimming is #698's ground-aware one - the dark ground scales
//! toward black as it always did; a light ground fades toward itself
//! (never toward black, which turns the red to ink), the cutting tone
//! keeping enough of the gouge colour to clear 3:1 there.
//! Provenance: our own display choice over our own refusal model; no
//! vendor material consulted.

use camcraft_app::cl::MoveKind;
use camcraft_app::geometry::Point3;
use camcraft_app::machining2d::Ghost;
use egui::Color32;

use crate::egui_app::colorvision::Semantic;
use crate::egui_app::ground_ink::{dimmed, faded, light_ground};
use crate::interact::{MAX_OVERLAY_VERTICES, arc_segments, push_line};

/// The ghost's cutting moves, as a share of the gouge colour.
const GHOST_DIM: f32 = 0.7;
/// Its rapids, dimmer: where the tool was going, not what it cut.
const GHOST_RAPID_DIM: f32 = 0.35;
/// On a light ground the cutting moves keep this much of the gouge
/// colour, faded toward the ground (#871): paler than a warned move,
/// still 3:1 on the light viewport in every colour-vision palette. The
/// rapids take #698's own dimmed-path fade, fainter still.
const GHOST_LIGHT_KEEP: f32 = 0.9;
/// One dash and one gap, in chevron arms.
const DASH_ARMS: f64 = 2.0;
/// The most dashes one segment breaks into: a long move takes longer
/// dashes rather than an unbounded vertex count.
const MAX_DASHES: usize = 64;

/// The colour a ghost move draws in, on the live theme's viewport.
#[must_use]
pub(crate) fn ghost_colour(pal: &Semantic, kind: MoveKind) -> [f32; 3] {
    ghost_ink(pal, kind, crate::egui_app::theme::current().viewport)
}

/// The colour a ghost move draws in on `ground` (see the module law):
/// the dark ground's tones are the gouge colour scaled, byte for byte
/// what they were; a light ground fades it toward itself.
#[must_use]
pub(crate) fn ghost_ink(pal: &Semantic, kind: MoveKind, ground: Color32) -> [f32; 3] {
    if kind == MoveKind::Rapid {
        dimmed(pal.gouge, GHOST_RAPID_DIM, ground)
    } else if light_ground(ground) {
        faded(pal.gouge, GHOST_LIGHT_KEEP, ground)
    } else {
        dimmed(pal.gouge, GHOST_DIM, ground)
    }
}

/// Append `ghost`'s moves to `out` as dashed lines (see the module law).
/// False once the overlay's vertex budget is spent - logged, never silent.
pub(crate) fn ghost_lines(out: &mut Vec<[f32; 6]>, ghost: &Ghost, pal: &Semantic, arm: f64) -> bool {
    let period = (arm * DASH_ARMS).max(f64::EPSILON);
    for m in ghost.moves() {
        let c = ghost_colour(pal, m.kind);
        let segs = if m.arc.is_some() { arc_segments(m) } else { vec![(m.from, m.to)] };
        for (a, b) in segs {
            if !dashed(out, a, b, period, c) {
                diag_warn!(
                    "overlay: the refusal's ghost outgrew the {MAX_OVERLAY_VERTICES}-vertex display budget; \
                     the rest is not drawn (a ghost is never a toolpath either way)"
                );
                out.truncate(out.len() & !1);
                return false;
            }
        }
    }
    true
}

/// `a -> b` as dashes: the first half of every `period` drawn, the
/// second left open. A segment shorter than one period is one dash over
/// its first half. False when the budget cannot take its dashes.
fn dashed(out: &mut Vec<[f32; 6]>, a: Point3, b: Point3, period: f64, c: [f32; 3]) -> bool {
    let d = b - a;
    let len = d.dot(d).sqrt();
    if !len.is_finite() {
        return true; // nothing finite to draw (a ghost is finite by law)
    }
    let n = ((len / period).ceil() as usize).clamp(1, MAX_DASHES);
    if out.len() + 2 * n > MAX_OVERLAY_VERTICES {
        return false;
    }
    let at = |t: f64| Point3::new(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t);
    for k in 0..n {
        let t0 = k as f64 / n as f64;
        let t1 = (k as f64 + 0.5) / n as f64;
        push_line(out, at(t0), at(t1), c);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::cl::{Move, Toolpath};

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn ghost() -> Ghost {
        let moves = vec![
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(0.0, 0.0, 2.0), None),
            Move::new(MoveKind::Feed, p(0.0, 0.0, -1.0), p(12.0, 0.0, -1.0), Some(500.0)),
        ];
        Ghost::of(&Toolpath { moves, ..Toolpath::default() }).expect("a finite ghost")
    }

    #[test]
    fn a_ghost_draws_dashed_in_the_dimmed_gouge_colour_and_never_in_a_path_colour() {
        let pal = &Semantic::RED_GREEN;
        let mut out = Vec::new();
        assert!(ghost_lines(&mut out, &ghost(), pal, 0.6));
        let feed = ghost_colour(pal, MoveKind::Feed);
        let rapid = ghost_colour(pal, MoveKind::Rapid);
        assert_eq!(feed, [pal.gouge[0] * GHOST_DIM, pal.gouge[1] * GHOST_DIM, pal.gouge[2] * GHOST_DIM]);
        for v in &out {
            let c = [v[3], v[4], v[5]];
            assert!(c == feed || c == rapid, "only the ghost's two tones: {c:?}");
            assert!(c != pal.feed && c != pal.rapid && c != pal.plunge && c != pal.gouge, "never a toolpath's own colour");
        }
        // The 12 mm feed at a 1.2 mm period: ten dashes, each 0.6 mm,
        // the gaps left open - dashed, so it never reads as a solid path.
        let feed_dashes: Vec<_> = out.as_chunks::<2>().0.iter().filter(|l| [l[0][3], l[0][4], l[0][5]] == feed).collect();
        assert_eq!(feed_dashes.len(), 10);
        let first = feed_dashes[0];
        assert_eq!([first[0][0], first[1][0]], [0.0, 0.6]);
        assert_eq!(feed_dashes[1][0][0], 1.2, "a gap between the dashes");
        // No chevrons: every line lies on the move it dashes (y = 0).
        assert!(out.iter().all(|v| v[1] == 0.0));
    }

    #[test]
    fn a_long_move_is_capped_in_dashes_not_in_length() {
        let moves = vec![Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(10_000.0, 0.0, 0.0), Some(500.0))];
        let g = Ghost::of(&Toolpath { moves, ..Toolpath::default() }).expect("ghost");
        let mut out = Vec::new();
        assert!(ghost_lines(&mut out, &g, &Semantic::OFF, 0.6));
        assert_eq!(out.len(), 2 * MAX_DASHES);
        let last = out[out.len() - 2..].to_vec();
        assert!(last[0][0] > 9_800.0, "the dashes still span the whole move: {last:?}");
    }

    #[test]
    fn the_ghost_reads_on_the_light_ground_and_keeps_its_dark_tones() {
        use crate::egui_app::colorvision::{ink, palette_for};
        use crate::egui_app::contrast;
        use crate::egui_app::theme::Tokens;
        use camcraft_app::schema::ColorVision;
        let light = Tokens::LIGHT.viewport;
        let dark = Tokens::DARK.viewport;
        let lg = [light.r(), light.g(), light.b()].map(|c| f32::from(c) / 255.0);
        for mode in ColorVision::ALL {
            // The dark ground: the gouge colour scaled, as #861 drew it.
            let pal = palette_for(mode, false);
            let k = |d: f32| [pal.gouge[0] * d, pal.gouge[1] * d, pal.gouge[2] * d];
            assert_eq!(ghost_ink(pal, MoveKind::Feed, dark), k(GHOST_DIM), "{mode:?}");
            assert_eq!(ghost_ink(pal, MoveKind::Plunge, dark), k(GHOST_DIM), "{mode:?}");
            assert_eq!(ghost_ink(pal, MoveKind::Rapid, dark), k(GHOST_RAPID_DIM), "{mode:?}");
            // The light ground: faded toward the ground, never toward
            // black - every channel between the gouge ink and the ground.
            let pal = palette_for(mode, true);
            let cut = ghost_ink(pal, MoveKind::Feed, light);
            let rapid = ghost_ink(pal, MoveKind::Rapid, light);
            for (what, c) in [("cut", cut), ("rapid", rapid)] {
                for i in 0..3 {
                    let (lo, hi) = (pal.gouge[i].min(lg[i]), pal.gouge[i].max(lg[i]));
                    assert!(c[i] >= lo && c[i] <= hi, "{mode:?} {what} channel {i} toward the ground: {c:?}");
                }
            }
            // WCAG 1.4.11: the cutting tone, a graphical object, clears
            // 3:1 on the light viewport; the rapid is fainter still.
            let r = contrast::ratio(ink(cut), light);
            assert!(r >= 3.0, "{mode:?} ghost cut reads {r:.2}:1 on the light viewport");
            let rr = contrast::ratio(ink(rapid), light);
            assert!(rr < r, "{mode:?} the ghost rapid ({rr:.2}:1) is fainter than its cut ({r:.2}:1)");
            // Never a toolpath's own colour on that ground either.
            for c in [cut, rapid] {
                assert!(c != pal.feed && c != pal.rapid && c != pal.plunge && c != pal.gouge, "{mode:?}: {c:?}");
            }
        }
    }
}
