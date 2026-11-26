//! Traced playback, measured (2026-11 perf report): the UI-side cost of
//! `ensure_field_view` per frame while the transport plays a dense
//! program, the time to a published view after a full-length jump, and
//! the bytes cloned per frame; plus the debug-lane law that the shown
//! stock follows the playhead. Provenance: our own fixtures (staged
//! facing programs) over the M7.1 field engine.
//!
//! Gate map (M4.4): covers ui::actions::ensure_field_view (playback), ui::egui_app::shell::cached_script, app::stock_play_worker.

use std::sync::Arc;
use std::time::{Duration, Instant};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, SetStock};
use camcraft_app::geometry::{Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_view::{CutLayer, FieldView};
use camcraft_app::tools::catalog;

use crate::actions::ensure_field_view;
use crate::egui_app::shell::cached_script;
use crate::state::{Sim, State};

/// A staged face over a `side` x `side` block at `stepover`.
fn faced_doc(side: f64, stepover: f64, stepdown: f64) -> State {
    let mut state = State::new();
    let w = &state.world;
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(w, StockShape::Box { dx: side, dy: side, dz: 20.0 }, loc).expect("box stock");
    state.history.execute(w, cmd);
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    state.history.execute(w, AddTool::new(w, "EM-0400-2FL", tool).expect("add tool"));
    let params = OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(14.0),
        top_z: Some(Height::fixed(19.0)),
        stepdown: Some(stepdown),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(w, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("create face");
    state.history.execute(w, cmd);
    state.history.execute(w, GenerateOperation::new(w, "Face1").expect("generate"));
    state.doc_version += 1;
    state
}

fn stats(v: &[Duration]) -> (f64, f64) {
    let ms = |d: &Duration| d.as_secs_f64() * 1e3;
    let mean = v.iter().map(ms).sum::<f64>() / v.len().max(1) as f64;
    (mean, v.iter().map(ms).fold(0.0, f64::max))
}

fn shown(state: &State) -> Option<usize> {
    state.field_play.as_ref().and_then(|w| w.shown_at())
}

/// What the UI thread pays for a frame of playback: the transport's
/// script, the field follow, and the eye's CPU side on a new view (a
/// same-session view re-sends its cut layer as bytes; any other is
/// packed here, as the eye does).
fn ui_frame(state: &mut State, last: &mut Option<Arc<FieldView>>, sent: &mut usize) -> Duration {
    let t0 = Instant::now();
    let _sc = cached_script(state);
    ensure_field_view(state);
    if let Some(Ok(fv)) = &state.field_view
        && !last.as_ref().is_some_and(|l| Arc::ptr_eq(l, fv))
    {
        match &fv.packed {
            Some(p) => {
                // The eye's choice (M13.1): the patch when its chain
                // covers the resident frame, else the whole layer.
                let held = last.as_ref().and_then(|l| l.packed.as_ref()).filter(|h| Arc::ptr_eq(&h.base, &p.base));
                let bytes = match &p.cut {
                    CutLayer::Patch { patch, .. } if held.is_some_and(|h| p.patches(h.seq)) => patch.bytes(),
                    _ => p.base.mesh.len() * 4,
                };
                *sent += bytes;
            }
            None => {
                let _ = fv.stock.field().and_then(camcraft_app::stock_sim::preview::pack_field);
            }
        }
        *last = Some(Arc::clone(fv));
    }
    t0.elapsed()
}

#[test]
#[ignore = "heavy lane (playback perf bench): dense program through traced playback, release ~30 s - run with -- --ignored --nocapture"]
fn traced_playback_cost_per_frame_and_per_jump() {
    let mut state = faced_doc(100.0, 0.5, 0.5);
    let sc = cached_script(&mut state);
    let (total, moves) = (sc.total, sc.segs.len());
    println!("program: {moves} moves, {total:.1} s");
    state.sim = Some(Sim::new());
    let (mut last, mut sent) = (None, 0usize);
    let frames = 300usize;
    let mut times = Vec::with_capacity(frames);
    let mut views = 0usize;
    for i in 0..frames {
        if let Some(s) = state.sim.as_mut() {
            s.t = total * (i as f64) / (frames as f64) * 0.5; // the first half, 300 frames
        }
        let before = last.clone();
        times.push(ui_frame(&mut state, &mut last, &mut sent));
        views += usize::from(!matches!((&before, &last), (Some(a), Some(b)) if Arc::ptr_eq(a, b)));
        std::thread::sleep(Duration::from_millis(16)); // the frame's remainder
    }
    let (mean, max) = stats(&times);
    println!("(a) ui frame while playing: mean {mean:.3} ms, max {max:.3} ms over {frames} frames; {views} new views shown, at move {:?}", shown(&state));
    let worker_bytes = match &state.field_view {
        Some(Ok(fv)) => fv.stock.field().map_or(0, |f| f.cut_layer_bytes()) + fv.packed.as_ref().map_or(0, |p| p.cut_bytes()),
        _ => 0,
    };
    let full = match &state.field_view {
        Some(Ok(fv)) => fv.packed.as_ref().map_or(0, |p| p.base.mesh.len() * 4),
        _ => 0,
    };
    println!(
        "(c) bytes cloned per frame on the ui thread: 0 (script and view are Arc hand-offs); cut layer sent to the GPU: {} per new view (a whole-layer upload is {full}); worker clones {worker_bytes} per publication (<= 20/s)",
        sent / views.max(1)
    );
    let end = state.field_play.as_ref().map_or(0, |w| w.index().total());
    // Let the worker catch the half-way playhead first: the jump is
    // timed from there, the same half-to-end span the old path cut.
    let half = {
        let sc = cached_script(&mut state);
        let t = state.sim.as_ref().map_or(0.0, |s| s.t);
        let p = camcraft_app::stock_play::progress(&sc, t);
        state.field_play.as_ref().and_then(|w| p.and_then(|p| w.index().index_at(&p))).unwrap_or(0)
    };
    let tc = Instant::now();
    while shown(&state) != Some(half) && tc.elapsed() < Duration::from_secs(120) {
        let _ = ui_frame(&mut state, &mut last, &mut sent);
        std::thread::sleep(Duration::from_millis(16));
    }
    println!("    caught up to move {half} in {:.1} s more", tc.elapsed().as_secs_f64());
    let t0 = Instant::now();
    if let Some(s) = state.sim.as_mut() {
        s.t = total;
    }
    let mut jump = Vec::new();
    while shown(&state) != Some(end) && t0.elapsed() < Duration::from_secs(120) {
        jump.push(ui_frame(&mut state, &mut last, &mut sent));
        std::thread::sleep(Duration::from_millis(16));
    }
    let (jm, jx) = stats(&jump);
    println!(
        "(b) full-length jump to a published view: {:.1} ms wall; ui frames meanwhile mean {jm:.3} ms, max {jx:.3} ms ({} frames)",
        t0.elapsed().as_secs_f64() * 1e3,
        jump.len()
    );
    assert_eq!(shown(&state), Some(end), "the jump lands");
}

/// M13.1 jump throughput: a full-length jump on the worker against the
/// pre-worker path (one `FieldPlay::seek` to the end on this thread),
/// both over the dense fixture. The seconds are the report's; the
/// assert is a generous ceiling so the test documents, never flakes.
#[test]
#[ignore = "heavy lane (playback perf bench): a full-length jump on the worker vs a direct seek, release ~40 s - run with --release -- --ignored --nocapture"]
fn a_full_length_jump_on_the_worker_costs_no_more_than_the_direct_seek() {
    use camcraft_app::stock_play::{FieldPlay, PlaySeed};
    use camcraft_app::stock_play_worker::PlayWorker;
    use camcraft_app::stock_view::DEFAULT_TEXELS;
    let state = faced_doc(100.0, 0.5, 0.5);
    let seed = || PlaySeed::gather(&state.world, false, DEFAULT_TEXELS).expect("seed");
    // The reference: the whole program in one seek, no worker.
    let mut play = FieldPlay::from_seed(seed()).expect("play");
    let total = play.total();
    let t0 = Instant::now();
    play.seek(total).expect("direct seek");
    let direct = t0.elapsed().as_secs_f64();
    println!("direct seek 0 -> {total}: {direct:.2} s");
    // The worker: the uncut frame first, then one jump to the end.
    let w = PlayWorker::spawn(seed()).expect("worker");
    let mut seen = 0u64;
    let t1 = Instant::now();
    while w.shown_at() != Some(0) && t1.elapsed() < Duration::from_secs(120) {
        seen = w.wait_published(seen, Duration::from_millis(100));
        let _ = w.take();
    }
    assert_eq!(w.shown_at(), Some(0), "the uncut stock is published first");
    let t2 = Instant::now();
    w.request(total);
    let mut frames = 0u32;
    while w.shown_at() != Some(total) && t2.elapsed() < Duration::from_secs(120) {
        seen = w.wait_published(seen, Duration::from_millis(100));
        frames += u32::from(matches!(w.take(), Some(Ok(_))));
    }
    let jump = t2.elapsed().as_secs_f64();
    println!("worker jump 0 -> {total}: {jump:.2} s ({frames} frames taken); direct {direct:.2} s; ratio {:.2}", jump / direct.max(1e-9));
    assert_eq!(w.shown_at(), Some(total), "the jump lands");
    assert!(jump <= 2.0 * direct.max(0.5), "a worker jump ({jump:.2} s) stays within 2x the direct seek ({direct:.2} s)");
}

#[test]
fn the_shown_stock_follows_the_playhead_without_blocking() {
    let mut state = faced_doc(12.0, 3.0, 2.5);
    let total = cached_script(&mut state).total;
    state.sim = Some(Sim::new());
    let (mut last, mut sent, mut ats) = (None, 0usize, Vec::new());
    for step in 0..=8 {
        if let Some(s) = state.sim.as_mut() {
            s.t = total * f64::from(step) / 8.0;
        }
        // Frames until the worker shows this playhead (never a wait
        // inside the frame itself).
        let t0 = Instant::now();
        let want = loop {
            let _ = ui_frame(&mut state, &mut last, &mut sent);
            let sc = cached_script(&mut state);
            let w = state.field_play.as_ref().expect("the transport is up: a worker runs");
            let p = camcraft_app::stock_play::progress(&sc, total * f64::from(step) / 8.0);
            let k = p.and_then(|p| w.index().index_at(&p)).unwrap_or(0);
            if shown(&state) == Some(k) || t0.elapsed() > Duration::from_secs(60) {
                break k;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(shown(&state), Some(want), "step {step}: the stock reached the playhead");
        assert!(matches!(state.field_view, Some(Ok(_))), "step {step}: a view is shown: {}", state.status);
        ats.push(want);
    }
    assert!(ats.windows(2).all(|w| w[0] <= w[1]), "the shown cut advances with the playhead: {ats:?}");
    assert!(ats.first() < ats.last(), "the model updates as the cutter moves: {ats:?}");
    // The script is built once for the document, not per frame.
    let a = cached_script(&mut state);
    assert!(Arc::ptr_eq(&a, &cached_script(&mut state)), "one timeline per document version");
    state.doc_version += 1;
    assert!(!Arc::ptr_eq(&a, &cached_script(&mut state)), "an edit rebuilds it");
    state.sim = None;
    ensure_field_view(&mut state);
    assert!(state.field_play.is_none(), "the transport down stops (and joins) the worker");
}
