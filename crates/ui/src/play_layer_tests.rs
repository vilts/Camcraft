//! #3336 (GPU stock sim S3c): the Simulate view starts the layer
//! playback when the stock eye traces on the GPU and the shell gave a
//! cutter, and the CPU playback in every other case; a still frame cuts
//! nothing again; an eye that cannot hold a layer frame ends the layer
//! playback. The cutter here is the CPU twin law behind a counter (no
//! device in a lib test). Provenance: our own fixtures (the staged face
//! of `play_precalc_tests`) over the M7.1 field engine.
//!
//! Gate map (M4.4): covers ui::play_layer, ui::actions::ensure_field_view (the layer playback start), ui::play_layer (spawn, end).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use camcraft_app::stock_sim::preview::{pack_cut, LatticeMeta};
use camcraft_app::stock_sim::{cpu_layer_cutter, twin_items, twin_work, LayerCutter, TwinItem};
use camcraft_app::stock_view::{CutLayer, FieldView};

use crate::actions::ensure_field_view;
use crate::play_layer::{cut_input, cut_item, end as end_layer_playback, kernel_adapter, LayerPlayback};
use crate::play_precalc::{advance_playhead, holds_playhead};
use crate::play_precalc_tests::faced_state;
use crate::state::{Sim, State};

/// The CPU twin law behind a call counter: the layer cutter of a shell.
fn counting() -> (Arc<LayerCutter>, Arc<AtomicUsize>) {
    let (cpu, calls) = (cpu_layer_cutter(), Arc::new(AtomicUsize::new(0)));
    let seen = Arc::clone(&calls);
    let cutter: Arc<LayerCutter> = Arc::new(move |meta: &LatticeMeta, items: &[TwinItem], base: &[f32]| {
        seen.fetch_add(1, Ordering::SeqCst);
        cpu(meta, items, base)
    });
    (cutter, calls)
}

/// A faced document with the transport up and a shell's cutter.
fn playing(eye: bool) -> (State, Arc<AtomicUsize>) {
    let mut state = faced_state();
    let (cutter, calls) = counting();
    state.layer_play.cutter = Some(cutter);
    state.layer_play.eye = eye;
    state.sim = Some(Sim::new());
    (state, calls)
}

/// Texels drawn as material by the eye (`max(mesh, cut) < 0`) from the
/// packed frame alone, and by a reader of `FieldView::stock`; with the
/// uncut count.
fn drawn(fv: &FieldView) -> Option<(usize, usize, usize)> {
    let p = fv.packed.as_ref()?;
    let solid = |cut: &[f32]| p.base.mesh.iter().zip(cut).filter(|(m, c)| m.max(**c) < 0.0).count();
    let from_stock = pack_cut(fv.stock.field()?, &p.base.meta);
    let packed = match &p.cut {
        CutLayer::Full(c) => solid(c),
        CutLayer::Patch { .. } => solid(&from_stock),
    };
    Some((p.base.mesh.iter().filter(|m| **m < 0.0).count(), packed, solid(&from_stock)))
}

/// Play frames (the transport clock and the eye's ask) until the packed
/// frame shows a removal; the counts of `drawn` at that frame.
fn play_to_a_cut(state: &mut State) -> (usize, usize, usize) {
    ensure_field_view(state);
    let total = crate::egui_app::shell::cached_script(state).total;
    let t0 = Instant::now();
    loop {
        assert!(t0.elapsed() < Duration::from_secs(300), "no drawn removal: {:?}", state.status);
        if !holds_playhead(state) {
            advance_playhead(state, 0.1, total);
        }
        ensure_field_view(state);
        if let Some(Ok(fv)) = &state.field_view
            && let Some(d) = drawn(fv)
            && d.1 < d.0
        {
            return d;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Ask for the same playhead until the worker has landed on it and the
/// view took its frame.
fn settle(state: &mut State) {
    let t0 = Instant::now();
    loop {
        ensure_field_view(state);
        if state.field_play.as_ref().is_some_and(|p| !p.needs_poll()) {
            return;
        }
        assert!(t0.elapsed() < Duration::from_secs(120), "the playback does not land");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_transport_starts_the_layer_playback_when_the_eye_traces_on_the_gpu() {
    let (mut state, calls) = playing(true);
    let (uncut, packed, from_stock) = play_to_a_cut(&mut state);
    assert!(state.layer_play.on, "the worker runs in the layer mode");
    assert!(calls.load(Ordering::SeqCst) > 0, "the shell's cutter cut the face op");
    assert!(packed < uncut, "the packed frame shows the removal");
    assert_eq!(from_stock, uncut, "the f64 field did not cut the layer op: the cutter did the work");
    let fv = state.field_view.as_ref().and_then(|r| r.as_ref().ok()).expect("a frame");
    let whole = fv.packed.as_ref().is_some_and(|p| matches!(p.cut, CutLayer::Full(_)));
    assert!(whole, "a layer frame carries its whole cut layer: the eye never repacks it from the stock");
}

/// The counter law of #3336 on the layer path: a still frame cuts
/// nothing again and publishes nothing.
#[test]
fn a_still_frame_of_the_layer_playback_cuts_nothing_again() {
    let (mut state, calls) = playing(true);
    play_to_a_cut(&mut state);
    settle(&mut state);
    let cut = calls.load(Ordering::SeqCst);
    assert!(cut > 0, "the layer playback cut up to the playhead");
    let published = state.field_play.as_ref().map(|p| p.published());
    for _ in 0..20 {
        ensure_field_view(&mut state);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(calls.load(Ordering::SeqCst), cut, "a still frame calls the cutter again");
    assert_eq!(state.field_play.as_ref().map(|p| p.published()), published, "a still frame publishes again");
}

/// The CPU fallback of #3336: no GPU eye (no adapter, a refused trace
/// pass, a headless run) keeps the CPU playback, and so does a shell
/// that gave no cutter.
#[test]
fn an_eye_that_does_not_trace_on_the_gpu_keeps_the_cpu_playback() {
    let (mut state, calls) = playing(false);
    let (uncut, packed, from_stock) = play_to_a_cut(&mut state);
    assert!(!state.layer_play.on, "no layer mode without a GPU eye");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the cutter is not called");
    assert!(from_stock < uncut && packed < uncut, "the f64 field shows the removal: {from_stock} / {packed} of {uncut}");

    let mut none = faced_state();
    none.layer_play.eye = true; // a GPU eye, but the shell gave no cutter
    none.sim = Some(Sim::new());
    let (uncut, _, from_stock) = play_to_a_cut(&mut none);
    assert!(!none.layer_play.on && from_stock < uncut, "no cutter: the CPU playback");
}

#[test]
fn an_eye_that_cannot_hold_the_layer_frame_ends_the_layer_playback() {
    let (mut state, calls) = playing(true);
    play_to_a_cut(&mut state);
    assert!(state.layer_play.on);
    // The view pass: the eye holds the frame, but the CPU marches it.
    assert!(state.layer_play.must_end(true, false));
    end_layer_playback(&mut state);
    assert!(state.field_play.is_none() && state.field_view.is_none(), "the layer worker and its frame retire");
    assert!(state.layer_play.refused && !state.layer_play.on);
    assert!(state.status.contains("CPU"), "the fallback is said: {}", state.status);
    let before = calls.load(Ordering::SeqCst);
    let (uncut, _, from_stock) = play_to_a_cut(&mut state);
    assert!(!state.layer_play.on, "the CPU playback starts in its place");
    assert!(from_stock < uncut, "and `FieldView::stock` holds the cut that the CPU march reads");
    assert_eq!(calls.load(Ordering::SeqCst), before, "the cutter is not called again");
}

#[test]
fn the_layer_playback_ends_only_for_a_held_frame_off_the_gpu() {
    let on = LayerPlayback { on: true, ..LayerPlayback::default() };
    assert!(on.must_end(true, false));
    assert!(!on.must_end(true, true), "a frame on the GPU is what the layer mode is for");
    assert!(!on.must_end(false, false), "the eye holds no frame yet: nothing is marched");
    assert!(!LayerPlayback::default().must_end(true, false), "the CPU playback has the cut in its stock");
    let (cutter, _) = counting();
    let mut lp = LayerPlayback { cutter: Some(cutter), eye: true, ..LayerPlayback::default() };
    assert!(lp.start_cutter().is_some());
    lp.refused = true;
    assert!(lp.start_cutter().is_none(), "a refused layer playback is not started again");
}

#[test]
fn a_twin_item_maps_to_the_kernel_item_field_by_field() {
    use camcraft_app::cl::{Move, MoveKind};
    use camcraft_app::geometry::Point3;
    use camcraft_app::stock_sim::SdfTool;
    let meta = LatticeMeta { cell: 0.5, origin: [0; 3], dims: [24, 24, 24], solid: ([0.0; 3], [12.0; 3]), cap: 2.0 };
    let tool = SdfTool::Ball { r: 2.0 };
    let (from, to) = (Point3::new(3.0, 3.0, 6.0), Point3::new(9.0, 8.0, 5.0));
    let moves = [Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(600.0), arc: None, axis: None }];
    let work = twin_work(&moves, &tool, &meta).expect("the twin cuts a ball move");
    let items = twin_items(&work, &meta).expect("items");
    assert!(!items.is_empty());
    let input = cut_input(&meta, &items);
    assert_eq!((input.grid.dims, input.grid.cell, input.grid.cap), ([24, 24, 24], 0.5, 2.0));
    assert_eq!(input.items.len(), items.len());
    input.check().expect("the kernel takes the batch");
    for (t, k) in items.iter().zip(&input.items) {
        assert_eq!(*k, cut_item(t));
        assert_eq!((k.a, k.b, k.r, k.flank, k.margin, k.ball), (t.tile.a, t.tile.b, t.tile.r, t.tile.flank, t.tile.margin, t.tile.ball));
        assert_eq!((k.base, k.lo, k.hi), (t.base, t.lo, t.hi));
    }
}

#[test]
fn a_software_adapter_gets_no_kernel_by_name() {
    use egui_wgpu::wgpu::DeviceType;
    let err = kernel_adapter(DeviceType::Cpu).expect_err("a software adapter");
    assert!(err.contains("software adapter"), "{err}");
    for kind in [DeviceType::DiscreteGpu, DeviceType::IntegratedGpu, DeviceType::VirtualGpu, DeviceType::Other] {
        assert_eq!(kernel_adapter(kind), Ok(()), "{kind:?}");
    }
}
