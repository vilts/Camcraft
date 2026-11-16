//! Sinker EDM electrode wear compensation in Z (#546): the burn of
//! `sinker::burn_to_depth`, fed in layers, each layer's tip deepened by
//! the electrode length the stated axial wear rate says is gone by then
//! - so the worn face still reaches the stated floor.
//!
//! Provenance: general die-sink EDM practice from public machine-tool
//! handbooks - the electrode's end wear ratio (length lost per depth
//! burned) and feeding deeper to make it up; the layer model, the
//! refusals and the as-burned reading are our own design (#290 D1-D10).
//! No vendor wear table: a rate is the user's data (a condition's
//! `wear` in the `.mch`, or a wear they measured), never assumed.
//!
//! The law: with a rate `r`, a layer whose nominal floor lies `d` below
//! the work top has its tip programmed `r` times the depth that layer
//! actually burns deeper. The tip stands `s` above the floor
//! (`sinker::stand`: the spark gap, or #543's undersize stand-off) and
//! burning starts one gap above the top, so a layer burns `d - (s - gap)`
//! (never less than nothing): `tip = top - d + s - r * (d - (s - gap))`,
//! exactly where [`worn_tip`] reads the worn face back onto `floor + s`.
//! No burn is ever programmed below that tip at the full depth - an
//! over-deep move refuses by name. With no rate stated the burn is not
//! compensated, and says so; with no stand-off set `s` is the gap and
//! the law is #546's `r * d`.
//!
//! The orbit (#544) runs at the worn tip: each layer's orbit is expanded
//! at that layer's compensated Z, judged against the same deepest-tip
//! bound and the same electrode-length law as the plunge - wear never
//! lets an orbit move go below the compensated floor. With no rate and
//! one layer the moves are exactly `sinker::burn_to_depth`'s.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::sinker::{check, stand, SinkerParams, ON_FLOOR};
use crate::sinker_orbit::orbit_offsets;

/// What an uncompensated burn says of itself, on its page and its path.
pub const NOT_COMPENSATED: &str = "electrode wear: not compensated (no wear rate stated)";

/// The most layers one burn is split into: a finer step is refused by
/// name, never ground through.
pub const MAX_LAYERS: usize = 10_000;

/// A stated axial wear rate and where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct WearComp {
    /// Electrode length lost per unit of depth burned into the work.
    pub rate: f64,
    /// Where the rate is stated, as a note names it (e.g. "condition S1").
    pub from: String,
}

/// The compensation and layering of one burn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SinkerWear {
    /// `None`: no rate stated - the burn is not compensated.
    pub comp: Option<WearComp>,
    /// The depth of one layer, mm; `None` burns the whole depth as one.
    pub layer: Option<f64>,
}

/// The refusal of a rate that is not an axial wear rate, by name: not a
/// finite number, negative, or 1 and over (the electrode cannot lose
/// more length than the burn gains depth).
#[must_use]
pub fn rate_refusal(from: &str, rate: f64) -> Option<String> {
    (!(rate.is_finite() && (0.0..1.0).contains(&rate))).then(|| {
        format!(
            "sinker burn: the electrode wear rate {rate} ({from}) is not an axial wear rate - state a ratio from 0 up to (not including) 1: the electrode cannot wear more than the burn deepens"
        )
    })
}

/// What the burn's compensation says of itself.
#[must_use]
pub fn summary(comp: Option<&WearComp>) -> String {
    match comp {
        None => NOT_COMPENSATED.to_owned(),
        Some(c) => format!(
            "electrode wear: compensated in Z at {:.4} mm per mm burned ({})",
            c.rate, c.from
        ),
    }
}

/// The deepest Z the compensated tip may be programmed to: the stated
/// burn floor plus the stand-off (`sinker::stand`), less the rate times
/// the depth burned into the work - from one gap above the top down to
/// the stand-off above the floor (#543 meets #546).
#[must_use]
pub fn deepest_tip(p: &SinkerParams, rate: f64) -> f64 {
    let z = crate::sinker::burn_floor(p);
    let s = stand(p);
    z + s - rate * burned(p.top_z - z, s, p.gap)
}

/// The depth a tip `d` below the top plus the stand-off `s` actually
/// burns into the work: burning starts one gap above the top, so it is
/// `d - (s - gap)`, never less than nothing (a tip above the work burns
/// and wears nothing, as [`worn_tip`] reads it).
fn burned(d: f64, s: f64, gap: f64) -> f64 {
    (d - (s - gap)).max(0.0)
}

/// Where a programmed tip `t` burns with an electrode worn by `rate`:
/// the as-burned tip the stock sim judges. Above the work (top plus the
/// gap) nothing is burned and nothing is worn.
#[must_use]
pub fn worn_tip(t: f64, top: f64, gap: f64, rate: f64) -> f64 {
    let entry = top + gap;
    if t >= entry || rate <= 0.0 {
        t
    } else {
        (t + rate * entry) / (1.0 + rate)
    }
}

/// The burn in layers, each tip deepened by the wear the rate states
/// (none when no rate is stated); approach, the layer feeds - each
/// followed by the orbit at that layer's compensated tip when one is
/// stated - retract.
///
/// # Errors
/// Every refusal of `sinker::burn_to_depth` (the orbit's included), a
/// rate that is not an axial wear rate, a layer step that is not a
/// positive finite number or cuts more than [`MAX_LAYERS`] layers, a
/// compensated burn deeper than the electrode is long, or a move (plunge
/// or orbit) past the compensated bound.
pub fn burn_compensated(p: &SinkerParams, w: &SinkerWear) -> Result<Toolpath, GenError> {
    let z = check(p)?;
    let orbit = p.orbit.as_ref().map(orbit_offsets).transpose()?;
    let rate = match &w.comp {
        Some(c) => match rate_refusal(&c.from, c.rate) {
            Some(why) => return Err(GenError::Invalid(why)),
            None => c.rate,
        },
        None => 0.0,
    };
    let total = p.top_z - z;
    let step = match w.layer {
        None => total,
        Some(s) if s.is_finite() && s > 0.0 => s,
        Some(s) => {
            return Err(GenError::Invalid(format!(
                "sinker burn: the layer step {s} must be a positive depth"
            )));
        }
    };
    let layers = (total / step - 1e-9).ceil().max(1.0);
    if !layers.is_finite() || layers > MAX_LAYERS as f64 {
        return Err(GenError::Invalid(format!(
            "sinker burn: a layer step of {step} cuts the {total:.3} burn into more than {MAX_LAYERS} layers - state a larger step"
        )));
    }
    let s = stand(p);
    let deepest = deepest_tip(p, rate);
    let buried = p.top_z - deepest;
    if buried > p.electrode_len + ON_FLOOR {
        return Err(GenError::Invalid(format!(
            "sinker burn: with wear compensation the tip is programmed {buried:.3} below the work top, but the electrode is only {:.3} long - its holder would enter the work",
            p.electrode_len
        )));
    }
    let mut b = PathBuilder::approached(Point3::new(p.x, p.y, p.approach_z));
    let mut prev = f64::INFINITY;
    for i in 1..=(layers as usize) {
        let d = (i as f64 * step).min(total);
        let tip = (p.top_z - d) + s - rate * burned(d, s, p.gap);
        // The bound and the order, re-read on every move: never past the
        // compensated floor, never back up between layers.
        if tip < deepest - ON_FLOOR || !tip.is_finite() {
            return Err(GenError::Invalid(format!(
                "sinker burn: layer {i} would reach Z {tip:.3}, over-deep past the compensated floor Z {deepest:.3}"
            )));
        }
        if tip < prev {
            b.push(MoveKind::Feed, Point3::new(p.x, p.y, tip), Some(p.feed));
            prev = tip;
            // The orbit at this layer's worn tip (#544 meets #546): every
            // move at `tip`, which the bound above has just admitted.
            for &(dx, dy) in orbit.iter().flatten() {
                b.push(MoveKind::Feed, Point3::new(p.x + dx, p.y + dy, tip), Some(p.feed));
            }
        }
    }
    if let (Some(o), Some(pts)) = (p.orbit.as_ref(), orbit.as_ref()) {
        b.note(format!(
            "sinker orbit: {} of radius {:.3} at each layer's compensated tip, {} moves per layer",
            o.kind.word(),
            o.radius,
            pts.len()
        ));
    }
    b.push(MoveKind::Rapid, Point3::new(p.x, p.y, p.approach_z), None);
    b.note(format!(
        "sinker burn: {} from the {} to Z {z:.3}, tip at Z {deepest:.3} in {layers} layer(s) (spark gap {:.3}; cavity floor datum Z {:.3})",
        p.depth,
        p.datum.label(),
        p.gap,
        p.floor_z
    ));
    b.note(summary(w.comp.as_ref()));
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinker::{burn_to_depth, SinkerDatum};

    fn params() -> SinkerParams {
        SinkerParams {
            x: 0.0,
            y: 0.0,
            approach_z: 25.0,
            top_z: 20.0,
            floor_z: 10.0,
            model_floor_z: Some(0.0),
            datum: SinkerDatum::WorkTop,
            depth: 6.0,
            electrode_len: 30.0,
            feed: 40.0,
            gap: 0.05,
            stand: None,
            orbit: None,
        }
    }

    fn feeds(t: &Toolpath) -> Vec<f64> {
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to.z).collect()
    }

    #[test]
    fn sinker_wear_off_is_the_plain_burn_and_says_not_compensated() {
        let p = params();
        let off = burn_compensated(&p, &SinkerWear::default()).expect("burns");
        let plain = burn_to_depth(&p).expect("burns");
        assert_eq!(off.moves, plain.moves);
        assert!(off.notes.iter().any(|n| n.contains("not compensated")), "{:?}", off.notes);
    }

    #[test]
    fn sinker_wear_deepens_each_layer_by_rate_times_depth() {
        let p = params();
        let w = SinkerWear { comp: Some(WearComp { rate: 0.02, from: "test".into() }), layer: Some(2.0) };
        let z = feeds(&burn_compensated(&p, &w).expect("burns"));
        // Layers at 2, 4, 6 below the top: 18 - 0.04, 16 - 0.08, 14 - 0.12, + gap.
        let want = [18.05 - 0.04, 16.05 - 0.08, 14.05 - 0.12];
        assert_eq!(z.len(), 3);
        for (a, b) in z.iter().zip(want) {
            assert!((a - b).abs() < 1e-9, "{z:?}");
        }
        // The stock sim's reading of the worn electrode lands on the floors.
        for (t, n) in z.iter().zip([18.05, 16.05, 14.05]) {
            assert!((worn_tip(*t, 20.0, 0.05, 0.02) - n).abs() < 1e-9);
        }
    }

    // #543 meets #546: a rough's undersize stand-off `s` (0.3 over a 0.05
    // gap) is where the compensated tip stops - never the gap.
    fn stood() -> SinkerParams {
        SinkerParams { stand: Some(0.3), ..params() }
    }

    #[test]
    fn sinker_wear_stand_off_without_a_rate_stops_at_floor_plus_stand() {
        let p = stood();
        let path = burn_compensated(&p, &SinkerWear::default()).expect("burns");
        assert_eq!(feeds(&path), [14.3], "floor 14 + stand-off 0.3");
        assert_eq!(path.moves, burn_to_depth(&p).expect("burns").moves);
    }

    #[test]
    fn sinker_wear_stand_off_with_a_rate_wears_onto_floor_plus_stand() {
        // 0.1 layers: the first few stand above the work and wear nothing.
        let p = stood();
        let w = SinkerWear { comp: Some(WearComp { rate: 0.02, from: "test".into() }), layer: Some(0.1) };
        let path = burn_compensated(&p, &w).expect("burns");
        let z = feeds(&path);
        assert_eq!(z.len(), 60);
        for (i, t) in z.iter().enumerate() {
            let d = ((i + 1) as f64 * 0.1).min(6.0);
            let worn = worn_tip(*t, 20.0, 0.05, 0.02);
            assert!((worn - (20.0 - d + 0.3)).abs() < 1e-9, "layer {d}: worn {worn} programmed {t}");
        }
        let deepest = deepest_tip(&p, 0.02);
        assert!((z[59] - deepest).abs() < 1e-9 && (worn_tip(deepest, 20.0, 0.05, 0.02) - 14.3).abs() < 1e-9);
        assert!(path.moves.iter().all(|m| m.to.z >= deepest - 1e-9));
    }

    #[test]
    fn sinker_wear_refuses_a_bad_rate_step_or_reach_by_name() {
        let mut p = params();
        for r in [f64::NAN, -0.1, 1.0, 1.5] {
            let w = SinkerWear { comp: Some(WearComp { rate: r, from: "condition S1".into() }), layer: None };
            let e = burn_compensated(&p, &w).expect_err("refused");
            assert!(e.to_string().contains("not an axial wear rate"), "{e}");
        }
        let w = SinkerWear { comp: None, layer: Some(0.0) };
        assert!(burn_compensated(&p, &w).expect_err("step").to_string().contains("layer step"));
        let w = SinkerWear { comp: None, layer: Some(1e-6) };
        assert!(burn_compensated(&p, &w).expect_err("too fine").to_string().contains("layers"));
        // 6 mm burn at 0.5: programmed 9 below the top, the electrode is 8.
        p.electrode_len = 8.0;
        let w = SinkerWear { comp: Some(WearComp { rate: 0.5, from: "t".into() }), layer: None };
        let e = burn_compensated(&p, &w).expect_err("holder");
        assert!(e.to_string().contains("with wear compensation"), "{e}");
    }

    fn orbit() -> crate::sinker_orbit::Orbit {
        crate::sinker_orbit::Orbit { kind: crate::sinker_orbit::OrbitKind::Circular, radius: 0.2, vectors: 4, turns: 1 }
    }

    #[test]
    fn sinker_wear_orbit_runs_at_each_worn_layer_tip_above_the_deepest() {
        let p = SinkerParams { orbit: Some(orbit()), ..params() };
        let w = SinkerWear { comp: Some(WearComp { rate: 0.02, from: "test".into() }), layer: Some(2.0) };
        let path = burn_compensated(&p, &w).expect("burns");
        let deepest = deepest_tip(&p, 0.02);
        let layer_z = [18.05 - 0.04, 16.05 - 0.08, 14.05 - 0.12];
        let n = crate::sinker_orbit::orbit_offsets(&orbit()).expect("orbit").len();
        let fs: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(fs.len(), 3 * (1 + n), "a plunge and a full orbit per layer");
        for (i, want) in layer_z.iter().enumerate() {
            for m in &fs[i * (1 + n)..(i + 1) * (1 + n)] {
                assert!((m.to.z - want).abs() < 1e-9, "layer {i}: {} vs {want}", m.to.z);
            }
        }
        // No move, plunge or orbit, below the deepest compensated tip; the
        // last layer's orbit sits on it.
        assert!(path.moves.iter().all(|m| m.to.z >= deepest - 1e-9), "{deepest}");
        assert!((layer_z[2] - deepest).abs() < 1e-9);
        assert!(fs.iter().any(|m| (m.to.x - 0.2).abs() < 1e-9), "the orbit reaches its radius");
    }

    #[test]
    fn sinker_wear_off_with_an_orbit_is_mains_orbit_burn() {
        let p = SinkerParams { orbit: Some(orbit()), ..params() };
        let off = burn_compensated(&p, &SinkerWear::default()).expect("burns");
        let plain = burn_to_depth(&p).expect("burns");
        assert_eq!(off.moves, plain.moves);
    }

    #[test]
    fn sinker_wear_orbit_keys_the_electrode_law_on_the_worn_tip() {
        // Nominal tip buried 5.95 (fits a 6.0 electrode); worn at 0.05 it
        // is programmed 6.25 below the top - refused, orbit or not.
        let p = SinkerParams { orbit: Some(orbit()), electrode_len: 6.0, ..params() };
        assert!(burn_to_depth(&p).is_ok(), "the nominal burn fits");
        let w = SinkerWear { comp: Some(WearComp { rate: 0.05, from: "t".into() }), layer: None };
        let e = burn_compensated(&p, &w).expect_err("worn tip buries the holder");
        assert!(e.to_string().contains("with wear compensation"), "{e}");
    }
}
