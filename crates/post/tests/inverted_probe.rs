//! Probing on a machine with an inverted X / Y (#392): the posted Fanuc
//! and Haas probing blocks, run by a macro B controller (`macro_b`) on a
//! simulated machine whose inverted axis moves the other way, against a
//! part sitting off its model position, write the PHYSICALLY correct
//! work offset - the part datum in the controller's coordinates - for
//! face, edge, corner, bore and boss probing. The same reader on a
//! standard machine finds the same physical answer (the interpreter is
//! checked too); a part out of reach alarms; and the standard-sign
//! program, or a translation missing any one of its probing words, does
//! NOT find it on the inverted machine (the reader is no tautology).
//!
//! Provenance: Fanuc's published operator documentation (G31, custom
//! macro B, G10 L2); the Haas Mill Operator's Manual (G31 Feed Until
//! Skip, macro variables #5061-#5063 / #5221.., #3000 / #3006, G10 L2
//! P1..P6) - the same words with the same meaning, so one reader runs
//! both; touch-probe practice. Fixture values are our own.
//!
//! The probe cycles are recorded (#926): `recorded/inputs/inverted_probe_*`,
//! written and guarded by app's `recorded_post_streams` from
//! `motion::probing::probe_cycle`, dumped on G54 at `ORIGIN`, setting G55.
//!
//! Gate map (M4.4): covers post::invert (probing), post::js (manifest
//! flags), assets/posts fanuc.cps + haas.cps (probing hooks).

mod inverted_support;
mod macro_b;
mod recorded;

use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::{engine, invert};
use inverted_support::{cps, machine};
use macro_b::{Pt, Run, Stop};

/// The setup's work-offset origin in the model (CAM world) frame.
const ORIGIN: Pt = [12.5, -7.25, 3.0];
/// The machine's physical position of that origin as the model places
/// it: G54 holds it (negated on an inverted axis) before probing.
const DATUM: Pt = [-312.5, -201.25, -405.0];
/// Where the real part sits relative to the model: what probing finds.
const SHIFT: Pt = [0.37, -0.21, -0.13];
/// Stylus ball radius.
const R: f64 = 2.0;
/// G55 before the program writes it: an unwritten axis stays this.
const UNSET: f64 = 999.0;
/// Program words carry three decimals.
const TOL: f64 = 0.002;

const MACHINES: &[&[&str]] = &[&[], &["X"], &["Y"], &["X", "Y"]];
/// The posts that declare `probeAxisInversion = true`.
const POSTS: &[&str] = &["fanuc", "haas"];

/// The part, in model coordinates of the stylus ball CENTRE.
#[derive(Clone, Debug)]
enum Shape {
    /// The intersection of half-spaces: (surface point, outward normal).
    Solid(Vec<(Pt, Pt)>),
    /// Material outside a vertical bore (centre, radius) below `top`.
    Bore(Pt, f64),
    /// Material inside a vertical boss (centre, radius) below its top.
    Boss(Pt, f64),
}

impl Shape {
    fn holds(&self, c: Pt) -> bool {
        let rho = |o: Pt| ((c[0] - o[0]).powi(2) + (c[1] - o[1]).powi(2)).sqrt();
        match self {
            Shape::Solid(faces) => faces
                .iter()
                .all(|(q, n)| (c[0] - q[0]) * n[0] + (c[1] - q[1]) * n[1] + (c[2] - q[2]) * n[2] < R),
            Shape::Bore(o, rad) => rho(*o) > rad - R && c[2] - o[2] < R,
            Shape::Boss(o, rad) => rho(*o) < rad + R && c[2] - o[2] < R,
        }
    }
}

struct Case {
    name: &'static str,
    /// The first touch's travel direction (a unit vector).
    dir: Pt,
    /// The recorded stream under `recorded/inputs/`.
    input: &'static str,
    shape: Shape,
    /// The axes the probing finds (0 X, 1 Y, 2 Z).
    found: &'static [usize],
}

/// A wall the stylus meets travelling `d`: its surface point and its
/// outward normal (against the travel).
fn wall(t: Pt, d: Pt) -> (Pt, Pt) {
    (t, [-d[0], -d[1], -d[2]])
}

fn cases() -> Vec<Case> {
    let (xn, xp, yn, yp, zn) = ([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]);
    let up = |t: Pt| (t, [0.0, 0.0, 1.0]);
    let (tz, tx, te) = ([10.0, 5.0, 20.0], [40.0, 5.0, 15.0], [20.0, 40.0, 20.0]);
    let (tc, tn, tb) = ([0.0, 0.0, 20.0], [80.0, 60.0, 20.0], [30.0, 30.0, 20.0]);
    vec![
        Case {
            name: "face -Z",
            dir: zn,
            input: "inverted_probe_face_zneg.cnc",
            shape: Shape::Solid(vec![up(tz)]),
            found: &[2],
        },
        Case {
            name: "face -X",
            dir: xn,
            input: "inverted_probe_face_xneg.cnc",
            shape: Shape::Solid(vec![wall(tx, xn)]),
            found: &[0],
        },
        Case {
            name: "edge -Y",
            dir: yn,
            input: "inverted_probe_edge_yneg.cnc",
            shape: Shape::Solid(vec![wall(te, yn), up(te)]),
            found: &[1],
        },
        Case {
            name: "corner +X+Y",
            dir: xp,
            input: "inverted_probe_corner_pos.cnc",
            shape: Shape::Solid(vec![wall(tc, xp), wall(tc, yp), up(tc)]),
            found: &[0, 1],
        },
        Case {
            name: "corner -X-Y",
            dir: xn,
            input: "inverted_probe_corner_neg.cnc",
            shape: Shape::Solid(vec![wall(tn, xn), wall(tn, yn), up(tn)]),
            found: &[0, 1],
        },
        Case { name: "bore", dir: zn, input: "inverted_probe_bore.cnc", shape: Shape::Bore(tb, 10.0), found: &[0, 1] },
        Case { name: "boss", dir: zn, input: "inverted_probe_boss.cnc", shape: Shape::Boss(tb, 8.0), found: &[0, 1] },
    ]
}

/// The case's probing records: set G55 from the trips, tolerance 0.5.
fn records(c: &Case) -> Vec<Record> {
    recorded::input_records(c.input)
}

fn post_for(post: &str, records: &[Record], letters: &[&str]) -> String {
    post_with(&cps(post), records, letters)
}

fn post_with(script: &str, records: &[Record], letters: &[&str]) -> String {
    let out = engine::run(script, &machine(letters), &cnc::serialize(records))
        .unwrap_or_else(|e| panic!("inverted {letters:?}: {e}"));
    assert!(out.warnings.is_empty(), "inverted {letters:?}: {:?}", out.warnings);
    out.program
}

/// `post`'s script with its `probeAxisInversion` line taken out.
fn without_probe_flag(post: &str) -> String {
    let s = cps(post);
    let line = s.lines().find(|l| l.starts_with("probeAxisInversion = true;")).expect("declares the flag");
    s.replacen(line, "", 1)
}

fn sign(flip: [bool; 3]) -> Pt {
    flip.map(|f| if f { -1.0 } else { 1.0 })
}

fn flip_of(letters: &[&str]) -> [bool; 3] {
    [letters.contains(&"X"), letters.contains(&"Y"), false]
}

/// Run `program` on the machine with `flip` inverted, the part shifted
/// by `shift` off its model position.
fn simulate(program: &str, flip: [bool; 3], shape: &Shape, shift: Pt) -> Run {
    let s = sign(flip);
    let mut offsets = [[UNSET; 3]; 6];
    offsets[0] = [0, 1, 2].map(|a| s[a] * DATUM[a]);
    // Physical tip -> model coordinates of the ball centre, with the
    // part moved by `shift` (so the part's model point m sits at m+shift).
    let material = |p: Pt| {
        let c = [0, 1, 2].map(|a| p[a] - DATUM[a] + ORIGIN[a] - shift[a] + if a == 2 { R } else { 0.0 });
        shape.holds(c)
    };
    macro_b::run(program, flip, offsets, &material).unwrap_or_else(|e| panic!("{e}\n{program}"))
}

/// The register the probing must write: the real part's datum in the
/// controller's coordinates (negated on an inverted axis).
fn want(flip: [bool; 3], shift: Pt) -> Pt {
    let s = sign(flip);
    [0, 1, 2].map(|a| s[a] * (DATUM[a] + shift[a]))
}

/// True when the run ends cleanly with G55 right on every found axis
/// and G54 untouched.
fn correct(run: &Run, c: &Case, flip: [bool; 3], shift: Pt) -> bool {
    let (w, g54) = (want(flip, shift), want(flip, [0.0; 3]));
    run.stop == Stop::End
        && c.found.iter().all(|&a| (run.offsets[1][a] - w[a]).abs() <= TOL)
        && (0..3).all(|a| c.found.contains(&a) || run.offsets[1][a] == UNSET)
        && (0..3).all(|a| (run.offsets[0][a] - g54[a]).abs() < 1e-12)
}

#[test]
fn every_mode_sets_the_physical_offset_on_every_machine() {
    for (post, c) in POSTS.iter().flat_map(|p| cases().into_iter().map(move |c| (*p, c))) {
        let recs = records(&c);
        let touches = recs.iter().filter(|r| matches!(r, Record::Touch(_))).count();
        for letters in MACHINES {
            let flip = flip_of(letters);
            let program = post_for(post, &recs, letters);
            let run = simulate(&program, flip, &c.shape, SHIFT);
            let what = format!("{post}: {} on inverted {letters:?}", c.name);
            assert_eq!(run.stop, Stop::End, "{what}\n{program}");
            assert_eq!(run.trips.len(), touches, "{what}: every touch trips once\n{program}");
            assert!(
                correct(&run, &c, flip, SHIFT),
                "{what}: G55 {:?}, want {:?} on axes {:?}\n{program}",
                run.offsets[1],
                want(flip, SHIFT),
                c.found
            );
        }
    }
}

#[test]
fn a_part_where_the_model_says_sets_the_unchanged_datum() {
    for (post, c) in POSTS.iter().flat_map(|p| cases().into_iter().map(move |c| (*p, c))) {
        for letters in MACHINES {
            let flip = flip_of(letters);
            let run = simulate(&post_for(post, &records(&c), letters), flip, &c.shape, [0.0; 3]);
            assert!(correct(&run, &c, flip, [0.0; 3]), "{post}: {} on {letters:?}: {:?}", c.name, run);
        }
    }
}

#[test]
fn a_part_out_of_reach_alarms_no_contact_and_writes_nothing() {
    let c = cases().into_iter().find(|c| c.name == "face -X").expect("face -X");
    // Travelling -X onto a part 7 mm further -X: past the 5 mm overtravel.
    for (post, letters) in POSTS.iter().flat_map(|p| MACHINES.iter().map(move |l| (*p, *l))) {
        let run = simulate(&post_for(post, &records(&c), letters), flip_of(letters), &c.shape, [-7.0, 0.0, 0.0]);
        assert_eq!(run.stop, Stop::Alarm(2, "PROBE NO CONTACT".into()), "{post} {letters:?}");
        assert_eq!(run.offsets[1], [UNSET; 3], "{post} {letters:?}: no offset is written");
    }
}

#[test]
fn a_part_off_by_more_than_the_tolerance_alarms() {
    let c = cases().into_iter().find(|c| c.name == "bore").expect("bore");
    for (post, letters) in POSTS.iter().flat_map(|p| MACHINES.iter().map(move |l| (*p, *l))) {
        let run = simulate(&post_for(post, &records(&c), letters), flip_of(letters), &c.shape, [0.8, 0.0, 0.0]);
        assert_eq!(run.stop, Stop::Alarm(1, "PROBE X OUT OF TOLERANCE".into()), "{post} {letters:?}");
        assert_eq!(run.offsets[1], [UNSET; 3], "{post} {letters:?}");
    }
}

#[test]
fn the_standard_program_on_an_inverted_machine_does_not_find_the_part() {
    for (post, c) in POSTS.iter().flat_map(|p| cases().into_iter().map(move |c| (*p, c))) {
        if !c.found.iter().any(|&a| a < 2) {
            continue;
        }
        let program = post_for(post, &records(&c), &[]);
        for letters in &MACHINES[1..] {
            let flip = flip_of(letters);
            if !c.found.iter().any(|&a| flip[a]) {
                continue; // nothing measured on the inverted axis
            }
            let run = simulate(&program, flip, &c.shape, SHIFT);
            assert!(!correct(&run, &c, flip, SHIFT), "{post}: {} untranslated on {letters:?} passed: {run:?}", c.name);
        }
    }
}

/// What a probing program must do on the machine `flip`: set the right
/// offset for a part within reach, and alarm "no contact" (writing
/// nothing) for one 7 mm past the first touch's overtravel.
fn behaves(program: &str, c: &Case, flip: [bool; 3]) -> bool {
    let u = c.dir;
    let away = [7.0 * u[0], 7.0 * u[1], 7.0 * u[2]];
    let far = simulate(program, flip, &c.shape, away);
    correct(&simulate(program, flip, &c.shape, SHIFT), c, flip, SHIFT)
        && far.stop == Stop::Alarm(2, "PROBE NO CONTACT".into())
        && far.offsets[1] == [UNSET; 3]
}

/// Undo one word of the translation in a record.
type Undo = fn(&mut Record);

/// Each probing word the translation negates is load-bearing: undo one
/// (after the engine's own translation) and the inverted machine
/// crashes, alarms or sets a wrong offset - or, for the touch target,
/// probes past its overtravel instead of alarming "no contact" (the
/// skip runs on toward the wrong end, so a part out of reach is driven
/// into rather than reported).
#[test]
fn every_translated_probing_word_is_needed() {
    let (x, flip) = (invert::Flip { x: true, y: false, z: false }, [true, false, false]);
    let corners = || cases().into_iter().filter(|c| c.name.starts_with("corner"));
    for (post, c) in POSTS.iter().flat_map(|p| corners().map(move |c| (*p, c))) {
        let mirrored = invert::translate_with(records(&c), x, true).expect("translates");
        // The pre-translated stream on a standard post is the engine's
        // own (less the header line naming the machine's inverted axis).
        let body = |p: String| p.lines().filter(|l| !l.starts_with("(MACHINE ")).collect::<Vec<_>>().join("\n");
        assert_eq!(body(post_for(post, &mirrored, &[])), body(post_for(post, &records(&c), &["X"])), "{post}: {}", c.name);
        assert!(behaves(&post_for(post, &mirrored, &[]), &c, flip), "{post}: {}: the full translation", c.name);
        let undo: [(&str, Undo); 3] = [
            ("touch target", |r| {
                if let Record::Touch(t) = r {
                    t.target[0] = -t.target[0];
                }
            }),
            ("back-off delta", |r| {
                if let Record::Retreat(t) = r {
                    t.delta[0] = -t.delta[0];
                }
            }),
            ("found expected", |r| {
                if let Record::Found(f) = r {
                    f.axes.iter_mut().filter(|a| a.axis == 0).for_each(|a| a.expected = -a.expected);
                }
            }),
        ];
        for (what, f) in undo {
            let mut broken = mirrored.clone();
            broken.iter_mut().for_each(f);
            assert!(!behaves(&post_for(post, &broken, &[]), &c, flip), "{post}: {}: passes without the {what}", c.name);
        }
    }
}

/// Every shipped post that translates an inverted axis now also
/// declares its probing verified, so the refusal is pinned on a shipped
/// script with the flag taken out: still refused by name.
#[test]
fn probing_through_an_unverified_script_still_refuses_by_name() {
    let c = cases().into_iter().find(|c| c.name == "bore").expect("bore");
    let stream = cnc::serialize(&records(&c));
    for post in POSTS {
        let e = engine::run(&without_probe_flag(post), &machine(&["X"]), &stream)
            .map(|o| o.program)
            .expect_err("refused")
            .to_string();
        assert_eq!(
            e,
            "machine VMC 3-AXIS has an inverted X axis: operation Probe1: probing with a post script that does \
             not declare probeAxisInversion = true (Fanuc and Haas do) is not supported for an inverted axis yet",
            "{post}"
        );
    }
    let probing = invert::translate(records(&c), invert::Flip { x: false, y: true, z: false });
    assert!(probing.is_err_and(|e| e.iter().any(|l| l.contains("probeAxisInversion"))), "plain translate refuses");
}

/// Declaring the flag changes nothing on a standard machine: every
/// case posts byte for byte as the script without it (the Haas probe
/// golden from #173 stays as it was, too).
#[test]
fn an_uninverted_program_is_unchanged_by_the_flag() {
    for (post, c) in POSTS.iter().flat_map(|p| cases().into_iter().map(move |c| (*p, c))) {
        let recs = records(&c);
        assert_eq!(post_for(post, &recs, &[]), post_with(&without_probe_flag(post), &recs, &[]), "{post}: {}", c.name);
    }
}

/// The reader itself: both deviation forms (main's `a--b` and #377's
/// folded `a+b`) read alike, brackets and ABS nest, IF..GOTO jumps, and
/// an unmodelled word is an error rather than a pass.
#[test]
fn the_macro_reader_reads_both_sign_forms() {
    let program = "#100=5\n#101=#100--2.5\n#102=#100+2.5\n#103=[[#101+#102]/2]-ABS[-1]\n\
                   IF[#101EQ#102]GOTO7\n#3000=9(NOT EQUAL)\nN7G10L2P3X[#101]Y[#102]Z#103\nM30";
    let run = macro_b::run(program, [false; 3], [[0.0; 3]; 6], &|_| false).expect("reads");
    assert_eq!(run.stop, Stop::End);
    assert_eq!(run.offsets[2], [7.5, 7.5, 6.5]);
    let bad = macro_b::run("G90 G02 X1 Y1 I1", [false; 3], [[0.0; 3]; 6], &|_| false);
    assert!(bad.is_err_and(|e| e.contains("G2 is not modelled")), "an arc is not a probing block");
}
