//! #547: seeded fuzz over the cavity read and the electrode derivation.
//! A sound cavity damaged one named way is always refused by that name,
//! and pure garbage never panics and never answers a nonsense electrode
//! (Prime Directive: degenerate input errors, never crashes or hangs).
//!
//! Provenance: our own SYNTHETIC fixtures, built in code.

use crate::geometry::Triangle;
use crate::sinker_cavity::{cavity_of, PickedFace};
use crate::sinker_electrode::{derive, standing_in, ElectrodeSpec};
use crate::sinker_electrode_tests::{p, rect_cavity, spec};

/// xorshift64* (Vigna, "An experimental exploration of Marsaglia's
/// xorshift generators, scrambled", 2016) - a seeded, repeatable stream.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    /// Uniform in `[lo, hi)`.
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next() >> 11) as f64 / (1u64 << 53) as f64)
    }
}

/// One named way to damage a sound cavity, and the words its refusal
/// must carry.
fn damage(r: &mut Rng, f: &mut Vec<PickedFace>, kind: usize) -> &'static [&'static str] {
    let wi = 1 + r.below(4);
    let depth = f[1].outer[2].z - f[1].outer[0].z;
    match kind {
        0 => {
            let (fi, bad) = (r.below(f.len()), [f64::NAN, f64::INFINITY, f64::NEG_INFINITY][r.below(3)]);
            let vi = r.below(f[fi].outer.len());
            match r.below(3) {
                0 => f[fi].outer[vi].x = bad,
                1 => f[fi].outer[vi].y = bad,
                _ => f[fi].outer[vi].z = bad,
            }
            &["not a finite number"]
        }
        1 => {
            f[r.below(5)].normal.x = f64::NAN;
            &["not a finite number"]
        }
        2 => {
            f.remove(0);
            &["no floor is picked"]
        }
        3 => {
            f.remove(wi);
            &["do not close around the floor"]
        }
        4 => {
            f.truncate(1);
            &["only the floor"]
        }
        5 => {
            f.clear();
            &["no faces are picked"]
        }
        6 => {
            let fi = r.below(5);
            let a = f[fi].outer[0];
            f[fi].outer.iter_mut().for_each(|q| *q = a);
            &["zero area"]
        }
        7 => {
            let fi = r.below(5);
            f[fi].outer.truncate(r.below(3));
            &["open loop"]
        }
        8 => {
            f[r.below(5)].open = true;
            &["open loop"]
        }
        9 => {
            f[r.below(5)].curved = true;
            &["curved wall"]
        }
        10 => {
            let n = f[wi].normal;
            let d = r.range(0.001, 3.0);
            let q = &mut f[wi].outer[2];
            *q = p(q.x + n.x * d, q.y + n.y * d, q.z);
            &["is not planar"]
        }
        11 => {
            let t = r.range(0.01, 80.0).to_radians();
            let n = f[wi].normal;
            f[wi].normal = p(n.x * t.cos(), n.y * t.cos(), t.sin());
            for k in [2, 3] {
                let q = &mut f[wi].outer[k];
                *q = p(q.x - n.x * depth * t.tan(), q.y - n.y * depth * t.tan(), q.z);
            }
            &["leans"]
        }
        12 => {
            f[0].normal = p(0.0, 0.0, -1.0);
            &["faces down"]
        }
        13 => {
            let mut twin = f[0].clone();
            let dx = r.range(100.0, 900.0);
            twin.outer.iter_mut().for_each(|q| q.x += dx);
            f.push(twin);
            &["2 floors are picked"]
        }
        14 => {
            let (dx, dy) = (r.range(100.0, 900.0), r.range(100.0, 900.0));
            f[wi].outer.iter_mut().for_each(|q| *q = p(q.x + dx, q.y + dy, q.z));
            &["face of a different cavity"]
        }
        15 => {
            let dz = depth + r.range(0.01, 50.0);
            f[wi].outer.iter_mut().for_each(|q| q.z -= dz);
            &["below the picked floor"]
        }
        16 => {
            let n = f[wi].normal;
            f[wi].normal = p(-n.x, -n.y, 0.0);
            &["faces away from it"]
        }
        17 => {
            let fi = r.below(5);
            f[fi].outer[0].x = [1e7, -1e12, 1e300, f64::MAX][r.below(4)];
            &["outside the machinable range"]
        }
        18 => {
            // A wall that stops short of its edge's end.
            let (a, b) = (f[wi].outer[0], f[wi].outer[1]);
            let k = r.range(0.05, 0.95);
            let mid = p(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k, a.z);
            f[wi].outer[1] = mid;
            f[wi].outer[2] = p(mid.x, mid.y, f[wi].outer[2].z);
            &["do not close around the floor"]
        }
        _ => {
            // A bow-tie floor: its walls no longer stand on its edges,
            // or - should they - its outline crosses itself.
            f[0].outer.swap(1, 2);
            &["face of a different cavity", "self-intersecting", "do not close", "zero area", "faces away"]
        }
    }
}

#[test]
fn fuzz_a_damaged_cavity_is_always_refused_by_name_never_a_panic() {
    let mut r = Rng(0x5147_0547_D1E5_1ACE);
    for round in 0..6000 {
        let lo = (r.range(-200.0, 200.0), r.range(-200.0, 200.0));
        let hi = (lo.0 + r.range(0.5, 300.0), lo.1 + r.range(0.5, 300.0));
        let floor = r.range(-100.0, 100.0);
        let mut faces = rect_cavity(lo, hi, floor, floor + r.range(0.1, 80.0));
        let u = r.range(0.0, 0.2);
        // Sound first: the electrode is the cavity less the undersize.
        let sound = cavity_of(&faces).and_then(|c| derive(&c, &spec(u)));
        let e = sound.unwrap_or_else(|e| panic!("round {round}: a sound cavity was refused: {e}"));
        assert!((e.face.0 - (hi.0 - lo.0 - 2.0 * u)).abs() < 1e-9 && (e.face.1 - (hi.1 - lo.1 - 2.0 * u)).abs() < 1e-9, "round {round}: {e:?}");
        assert!(e.face.0 > 0.0 && e.face.1 > 0.0 && e.length > e.depth && e.depth > 0.0);
        let kind = r.below(20);
        let want = damage(&mut r, &mut faces, kind);
        match cavity_of(&faces).and_then(|c| derive(&c, &spec(u))) {
            Ok(e) => panic!("round {round}, damage {kind}: an electrode came out of a damaged cavity: {e:?}"),
            Err(err) => {
                let text = err.to_string();
                assert!(text.starts_with("invalid machining parameters: electrode from faces: "), "round {round}: {text}");
                assert!(want.iter().any(|w| text.contains(w)), "round {round}, damage {kind}: wanted one of {want:?}, got: {text}");
            }
        }
    }
}

#[test]
fn fuzz_garbage_faces_never_panic_and_never_answer_a_nonsense_electrode() {
    let mut r = Rng(0x0BAD_FACE_5EED_0547);
    let num = |r: &mut Rng| match r.below(12) {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => f64::NEG_INFINITY,
        3 => 0.0,
        4 => -0.0,
        5 => f64::MAX,
        6 => f64::MIN_POSITIVE,
        7 => r.range(-1e9, 1e9),
        8 => [0.0, 1.0, 5.0, 10.0][r.below(4)],
        _ => r.range(-50.0, 50.0),
    };
    let unit = |r: &mut Rng| match r.below(8) {
        0 => p(0.0, 0.0, 1.0),
        1 => p(0.0, 0.0, -1.0),
        2 => p(1.0, 0.0, 0.0),
        3 => p(0.0, -1.0, 0.0),
        _ => p(0.0, 0.0, 0.0),
    };
    let mut answered = 0;
    for round in 0..20_000 {
        let mut faces = Vec::new();
        for k in 0..r.below(8) {
            let normal = if r.below(3) == 0 { p(num(&mut r), num(&mut r), num(&mut r)) } else { unit(&mut r) };
            let outer = (0..r.below(7)).map(|_| p(num(&mut r), num(&mut r), num(&mut r))).collect();
            faces.push(PickedFace { label: format!("face {k}"), normal, curved: r.below(9) == 0, open: r.below(9) == 0, outer });
        }
        // Sometimes the garbage is laid over a sound cavity.
        if r.below(4) == 0 {
            faces.extend(rect_cavity((0.0, 0.0), (10.0, 5.0), 0.0, 5.0));
        }
        let s = ElectrodeSpec { undersize: num(&mut r), orbit: (r.below(2) == 0).then(|| num(&mut r)), stub: num(&mut r), pickup: num(&mut r) };
        match cavity_of(&faces).and_then(|c| derive(&c, &s)) {
            Err(e) => assert!(!e.to_string().is_empty()),
            Ok(e) => {
                answered += 1;
                let nums = [e.face.0, e.face.1, e.center.0, e.center.1, e.cavity.0, e.cavity.1, e.depth, e.length, e.pickup, e.inset];
                assert!(nums.iter().all(|v| v.is_finite()), "round {round}: {e:?}");
                assert!(e.face.0 > 0.0 && e.face.1 > 0.0 && e.face.0 <= e.cavity.0 && e.face.1 <= e.cavity.1, "round {round}: {e:?}");
                assert!(e.depth > 0.0 && e.length > e.depth, "round {round}: {e:?}");
                assert!(e.tool().is_sane(), "round {round}: {e:?}");
                let _ = standing_in(&e, &[Triangle::new(p(num(&mut r), 1.0, 1.0), p(2.0, num(&mut r), 1.0), p(1.0, 2.0, num(&mut r)))]);
            }
        }
    }
    // Bounded, not vacuous: almost everything is refused.
    assert!(answered < 2000, "{answered} electrodes out of garbage");
}
