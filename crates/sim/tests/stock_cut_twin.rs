//! The f32 cut twin (#2626, GPU stock sim S1) against the f64 field
//! cut: one-sided parity on the same moves, the order-preserving key,
//! the move-order law and the refusals.
//! Provenance: this codebase's own f64 field cut (`SdfField::
//! subtract_batch`, `preview::pack_cut`) as the reference; nothing
//! from vendor binaries.
//!
//! The layer playback (#2629, S3b) at the end: seeks, keyframes and the
//! move in flight against the one-batch twin.
//!
//! Gate map: covers camcraft_sim::stock_sim::cut_twin,
//! camcraft_sim::stock_sim::cut_twin_work and
//! camcraft_sim::stock_sim::layer_play.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::preview::{pack_cut, pack_field, LatticeMeta};
use camcraft_sim::stock_sim::{
    cpu_layer_cutter, from_ordered_bits, layer_tool, merge_max, ordered_bits, partial_move, run_twin, run_twin_items_onto,
    twin_cut, twin_items, twin_work, LayerPlay, LayerSpan, SdfField, SdfTool, SimOp, Stock, StockError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// A 16 x 16 x 8 block on a cell that is not a power of two (0.7),
/// so the tile-relative rounding is real; band 4 -> cap 2.8.
fn block() -> (SdfField, LatticeMeta) {
    let f = SdfField::from_mesh(&cube(p(0.0, 0.0, 0.0), p(16.0, 16.0, 8.0)).facets, 0.7, 4).expect("the block builds");
    let meta = pack_field(&f).expect("the block packs").meta;
    (f, meta)
}

/// A level feed, a plunge, two ramps (the flat's golden-section path)
/// and a rapid that must cut nothing.
fn program() -> Vec<Move> {
    let mut rapid = feed(p(-5.0, -5.0, 3.0), p(20.0, 20.0, 3.0));
    rapid.kind = MoveKind::Rapid;
    vec![
        feed(p(2.0, 3.0, 5.0), p(13.0, 4.0, 5.0)),
        feed(p(13.0, 4.0, 5.0), p(6.0, 12.0, 3.5)),
        feed(p(6.0, 12.0, 9.0), p(6.0, 12.0, 4.0)),
        feed(p(1.0, 1.0, 7.0), p(15.0, 14.0, 2.0)),
        rapid,
    ]
}

const BALL: SdfTool = SdfTool::Ball { r: 2.5 };
const FLAT: SdfTool = SdfTool::Flat { r: 3.0, flank: 10.0 };

/// The f64 reference: the field's own cut, packed (f64 -> f32 rounds
/// to nearest, which keeps order, so the one-sided bound stays exact).
fn reference(tool: &SdfTool, moves: &[Move]) -> (Vec<f32>, LatticeMeta) {
    let (mut f, meta) = block();
    f.subtract_batch(moves, tool, 0.01).expect("the f64 cut");
    (pack_cut(&f, &meta), meta)
}

fn parity(tool: SdfTool) {
    let moves = program();
    let (want, meta) = reference(&tool, &moves);
    let got = twin_cut(&moves, &tool, &meta).expect("the twin cuts");
    assert_eq!(got.len(), want.len());
    // A cut below -cap never shows: the mesh layer is at least -cap,
    // and the march reads max(mesh, cut). Both sides clamp there, so a
    // claim that sits just at the cap threshold on one side only
    // compares by its visible value.
    let cap = meta.cap as f32;
    // Lower side: the twin margin is TWIN_ULPS (32) f32 epsilons of the
    // tile scale (reach + r + flank + 8 cells, under 40 mm here); the
    // f32 distance error is inside that margin, so the gap is at most
    // twice it. 96 eps * 64 mm covers that with room. The flat's
    // golden-section path adds 1e-6 of its length (under 20 mm) to the
    // margin: 1e-4 covers twice that.
    let tol = 96.0 * f32::EPSILON * 64.0 + 1e-4;
    let (mut claimed, mut deep) = (0, 0);
    for (n, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert!(g.is_finite() && w.is_finite(), "texel {n}: {g} / {w}");
        let (g, w) = (g.max(-cap), w.max(-cap));
        assert!(g <= w, "texel {n}: the twin removes more than the f64 cut: {g} > {w}");
        assert!(g >= w - tol, "texel {n}: the twin lags the f64 cut by more than {tol}: {g} < {w}");
        if w > -cap {
            claimed += 1;
        }
        if w >= 1.0 {
            deep += 1;
        }
    }
    // fixture sanity: the program claims many cells, and many of them
    // lie more than 1 mm inside the swept solid (the ball's 2.5 mm
    // radius never reaches the 2.8 mm cap, so "full" is no test here)
    assert!(claimed > 200 && deep > 200, "{tool:?}: {claimed} claimed and {deep} deep texels");
}

#[test]
fn the_ball_twin_never_removes_more_than_the_f64_cut() {
    parity(BALL);
}

#[test]
fn the_flat_twin_never_removes_more_than_the_f64_cut() {
    parity(FLAT);
}

#[test]
fn the_ordered_key_keeps_the_order_of_negative_zero_and_positive() {
    let vals = [
        f32::NEG_INFINITY,
        -3.0e38,
        -2.5,
        -1.0e-30,
        -f32::from_bits(1), // the smallest negative subnormal
        -0.0,
        0.0,
        f32::from_bits(1),
        1.0e-30,
        1.0,
        2.5,
        3.0e38,
        f32::INFINITY,
    ];
    for w in vals.windows(2) {
        assert!(ordered_bits(w[0]) < ordered_bits(w[1]), "{} vs {}", w[0], w[1]);
    }
    for v in vals {
        assert_eq!(from_ordered_bits(ordered_bits(v)).to_bits(), v.to_bits(), "round trip of {v}");
    }
}

#[test]
fn the_batch_answers_bit_for_bit_in_any_move_order() {
    let (_, meta) = block();
    for tool in [BALL, FLAT] {
        let moves = program();
        let base = twin_cut(&moves, &tool, &meta).expect("the twin cuts");
        let mut rev = moves.clone();
        rev.reverse();
        let mut rot = moves.clone();
        rot.rotate_left(2);
        for other in [rev, rot] {
            let got = twin_cut(&other, &tool, &meta).expect("the twin cuts");
            let same = base.iter().zip(got.iter()).all(|(a, b)| a.to_bits() == b.to_bits());
            assert!(same, "{tool:?}: the move order changed the result");
        }
    }
}

#[test]
fn the_work_list_covers_every_claimed_cell_inside_its_bricks() {
    let (_, meta) = block();
    let mut moves = program();
    moves.push(feed(p(200.0, 200.0, 5.0), p(210.0, 200.0, 5.0))); // outside the lattice
    let work = twin_work(&moves, &FLAT, &meta).expect("the work list");
    assert_eq!(work.chords.len(), 5, "four feeds and the far feed; the rapid has no chord");
    assert!(work.items.iter().all(|w| w.chord != 4), "the far feed touches no brick");
    assert!(work.items.windows(2).all(|w| (w[0].brick, w[0].chord) < (w[1].brick, w[1].chord)));
    let hi = |a: usize| meta.origin[a] + meta.dims[a] as i32 - 1;
    for w in &work.items {
        for a in 0..3 {
            let b0 = w.brick[a] * 8;
            assert!(b0 <= w.lo[a] && w.lo[a] <= w.hi[a] && w.hi[a] <= b0 + 7, "{w:?}");
            assert!(meta.origin[a] <= w.lo[a] && w.hi[a] <= hi(a), "{w:?}");
        }
    }
    let cut = twin_cut(&moves, &FLAT, &meta).expect("the twin cuts");
    let floor = (-4.0 * meta.cap) as f32;
    let [dx, dy, _] = meta.dims.map(|d| d as usize);
    for (n, v) in cut.iter().enumerate() {
        if *v == floor {
            continue;
        }
        let key = [
            meta.origin[0] + (n % dx) as i32,
            meta.origin[1] + (n / dx % dy) as i32,
            meta.origin[2] + (n / (dx * dy)) as i32,
        ];
        let covered = work.items.iter().any(|w| (0..3).all(|a| w.lo[a] <= key[a] && key[a] <= w.hi[a]));
        assert!(covered, "claimed cell {key:?} is in no work item");
    }
}

/// The text of the GPU kernel's parity fixture (#2627): the kernel
/// items of the twin, the twin's layer and the f64 layer, as f32 bit
/// patterns, sparse (only texels off the `-4 cap` floor).
fn gpu_fixture(tool: SdfTool, name: &str) -> String {
    // the level feed, the ramp and the plunge: the three flat paths
    let moves = program()[..3].to_vec();
    let (want, meta) = reference(&tool, &moves);
    let work = twin_work(&moves, &tool, &meta).expect("the work list");
    let items = twin_items(&work, &meta).expect("the kernel items");
    let twin = run_twin(&work, &meta).expect("the twin cuts");
    let h = |v: f32| format!("{:08x}", v.to_bits());
    // FNV-1a over the twin layer's bits, in slot order
    let fnv = twin.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(0xcbf2_9ce4_8422_2325u64, |acc, b| {
        (acc ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let mut s = format!(
        "# {name}: the f32 cut twin (#2626) on the block and the first three moves\n\
         # of crates/sim/tests/stock_cut_twin.rs, for the GPU kernel parity of #2627.\n\
         # Written by the_gpu_kernel_fixtures_match_the_twin; CAMCRAFT_BLESS=1 writes it again.\n\
         # f32 values are hex bit patterns. grid: dims cell cap. item: a b r flank margin\n\
         # ball base lo hi. twin: FNV-1a 64 of the twin layer's bits (slot order, LE).\n\
         # t: slot step from the last t line, f64 reference (texels off the floor).\n"
    );
    let [dx, dy, dz] = meta.dims;
    s += &format!("grid {dx} {dy} {dz} {} {}\n", h(meta.cell as f32), h(meta.cap as f32));
    for it in &items {
        let t = &it.tile;
        let v3 = |p: [f32; 3]| format!("{} {} {}", h(p[0]), h(p[1]), h(p[2]));
        let i3 = |p: [i64; 3]| format!("{} {} {}", p[0], p[1], p[2]);
        s += &format!(
            "item {} {} {} {} {} {} {} {} {}\n",
            v3(t.a),
            v3(t.b),
            h(t.r),
            h(t.flank),
            h(t.margin),
            u8::from(t.ball),
            i3(it.base.map(i64::from)),
            i3(it.lo.map(i64::from)),
            i3(it.hi.map(i64::from)),
        );
    }
    s += &format!("twin {fnv:016x}\n");
    let floor = (-4.0 * meta.cap) as f32;
    let mut last = 0;
    for (n, w) in want.iter().enumerate() {
        if w.to_bits() != floor.to_bits() {
            s += &format!("t {} {}\n", n - last, h(*w));
            last = n;
        }
    }
    s
}

/// The fixtures that `crates/gpu/tests/cut_parity.rs` reads are the
/// twin's current output (the `facet_grid_2619` pattern of #2620).
#[test]
fn the_gpu_kernel_fixtures_match_the_twin() {
    for (tool, name) in [(BALL, "cut_twin_ball_2627"), (FLAT, "cut_twin_flat_2627")] {
        let path = format!("{}/../gpu/tests/fixtures/{name}.txt", env!("CARGO_MANIFEST_DIR"));
        let text = gpu_fixture(tool, name);
        if std::env::var("CAMCRAFT_BLESS").is_ok_and(|v| v == "1") {
            std::fs::write(&path, &text).expect("write the fixture");
            continue;
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert!(have == text, "{path} is stale or missing; CAMCRAFT_BLESS=1 writes it again");
    }
}

fn refusal(moves: &[Move], tool: &SdfTool, needle: &str) {
    let (_, meta) = block();
    match twin_cut(moves, tool, &meta) {
        Err(StockError::Invalid(msg)) => assert!(msg.contains(needle), "{msg:?} does not name {needle:?}"),
        other => panic!("expected a refusal naming {needle:?}, got {other:?}"),
    }
}

#[test]
fn degenerate_input_refuses_with_a_clean_error() {
    let ok = [feed(p(2.0, 2.0, 5.0), p(9.0, 2.0, 5.0))];
    refusal(&[feed(p(4.0, 4.0, 5.0), p(4.0, 4.0, 5.0))], &BALL, "zero-length");
    refusal(&ok, &SdfTool::Ball { r: 0.0 }, "radius");
    refusal(&ok, &SdfTool::Flat { r: 0.0, flank: 10.0 }, "radius");
    refusal(&ok, &SdfTool::Ball { r: f64::NAN }, "radius");
    refusal(&ok, &SdfTool::Flat { r: 3.0, flank: f64::NAN }, "flank");
    refusal(&[feed(p(f64::NAN, 2.0, 5.0), p(9.0, 2.0, 5.0))], &FLAT, "non-finite");
    let mut tilted = ok[0];
    tilted.axis = Some([0.0, 0.6, 0.8]);
    refusal(&[tilted], &BALL, "upright");
}

#[test]
fn every_other_tool_refuses_by_name() {
    let ok = [feed(p(2.0, 2.0, 5.0), p(9.0, 2.0, 5.0))];
    refusal(&ok, &SdfTool::Shanked { r: 2.0, shank: 10.0 }, "shanked ball");
    refusal(&ok, &SdfTool::Taper { r: 2.0, cot: 1.0 }, "taper");
    refusal(&ok, &SdfTool::Drill { r: 2.0, cot: 0.6, flank: 10.0 }, "drill");
    refusal(&ok, &SdfTool::BullNose { r: 3.0, corner: 1.0, flank: 10.0 }, "bull nose");
}

/// The one-batch twin of the moves before `k` of each span, merged: the
/// layer that the playback must show at `k`.
fn one_batch(spans: &[LayerSpan<'_>], k: usize, meta: &LatticeMeta) -> Vec<f32> {
    let n = meta.dims.iter().map(|d| *d as usize).product();
    let mut out = vec![(-4.0 * meta.cap) as f32; n];
    for s in spans {
        let upto = k.saturating_sub(s.start).min(s.moves.len());
        if upto > 0 {
            merge_max(&mut out, &twin_cut(&s.moves[..upto], s.tool, meta).expect("the twin")).expect("same lattice");
        }
    }
    out
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn the_layer_playback_is_the_one_batch_twin_at_every_seek() {
    let (_, meta) = block();
    let moves = program();
    let spans = [
        LayerSpan { start: 0, moves: &moves, tool: &BALL },
        LayerSpan { start: moves.len(), moves: &moves, tool: &FLAT },
    ];
    let total = 2 * moves.len();
    let mut play = LayerPlay::new(meta.clone(), cpu_layer_cutter(), 2).expect("the layer");
    // forward, a jump, backward past keyframes, the end, and back to 0
    for k in [1, 2, 3, 7, total, 4, 9, 0, total, 5] {
        play.seek(k, &spans).expect("the seek");
        assert_eq!(play.at(), k);
        let got = play.frame(k, 0.0, &spans).expect("the frame");
        assert_eq!(bits(&got), bits(&one_batch(&spans, k, &meta)), "the layer at {k}");
    }
    assert!(play.keyframes() > 2, "the seeks keep keyframes: {}", play.keyframes());
}

#[test]
fn the_layer_frame_shows_the_travelled_part_of_the_move_in_flight() {
    let (_, meta) = block();
    let moves = program();
    let spans = [LayerSpan { start: 0, moves: &moves, tool: &FLAT }];
    let mut play = LayerPlay::new(meta.clone(), cpu_layer_cutter(), 3).expect("the layer");
    let k = 3; // the second ramp, a golden-section move
    let half = play.frame(k, 0.5, &spans).expect("the half frame");
    let sub = partial_move(&moves[k], 0.5).expect("a travelled part");
    let mut with = moves[..k].to_vec();
    with.push(sub);
    let want = twin_cut(&with, &FLAT, &meta).expect("the twin");
    assert_eq!(bits(&half), bits(&want), "the move in flight");
    let whole = play.frame(k, 0.0, &spans).expect("the whole frame");
    assert_eq!(bits(&whole), bits(&one_batch(&spans, k, &meta)), "the partial cut is on a copy");
    assert_ne!(bits(&half), bits(&whole), "the fixture's half move cuts something");
}

#[test]
fn an_op_without_a_twin_stays_on_the_field_by_name() {
    let (f, _) = block();
    let stock = Stock::Field(f);
    let moves = program();
    let ball = SimOp { moves: &moves, radius: 2.5, ball: true, cone: None, corner: None };
    assert!(matches!(layer_tool(&ball, &stock), Ok(SdfTool::Ball { .. })));
    let flat = SimOp { ball: false, ..ball };
    assert!(matches!(layer_tool(&flat, &stock), Ok(SdfTool::Flat { .. })));
    let bull = SimOp { corner: Some(0.5), ..flat };
    let err = layer_tool(&bull, &stock).expect_err("a bull nose").to_string();
    assert!(err.contains("bull nose"), "{err}");
    let mut arc = moves.clone();
    arc[0].arc = Some(camcraft_sim::cl::ArcMove { center: camcraft_sim::geo2d::Point2::new(8.0, 8.0), ccw: false, plane: Default::default() });
    let err = layer_tool(&SimOp { moves: &arc, ..ball }, &stock).expect_err("an arc").to_string();
    assert!(err.contains("arc"), "{err}");
}

#[test]
fn a_bad_base_layer_is_refused_by_name() {
    let (_, meta) = block();
    let work = twin_work(&program(), &BALL, &meta).expect("the work");
    let items = twin_items(&work, &meta).expect("the items");
    let n: usize = meta.dims.iter().map(|d| *d as usize).product();
    let err = run_twin_items_onto(&items, &meta, &vec![0.0; n - 1]).expect_err("a short base").to_string();
    assert!(err.contains("base layer has"), "{err}");
    let mut nan = vec![-1.0; n];
    nan[3] = f32::NAN;
    let err = run_twin_items_onto(&items, &meta, &nan).expect_err("a NaN").to_string();
    assert!(err.contains("not finite"), "{err}");
}
