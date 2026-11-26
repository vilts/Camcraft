//! The draw list of each display mode (#2818 "Done means": a test shows the
//! draw list for each display mode).
//! Provenance: our own tests over our own pass order.
//! Gate map: covers ui::draw_list.

use super::{Counts, Effects, Pipe, Step, plan, plan_with};
use crate::state::DisplayMode;

/// A model, a stock and one region fill: each range is not empty.
fn scene() -> Counts {
    Counts {
        index_count: 90,
        model_index_count: 36,
        stock_index_end: 72,
        overlay_index_start: 72,
        edge_count: 48,
        model_edge_count: 24,
        ..Counts::default()
    }
}

#[test]
fn shaded_with_edges_draws_faces_then_edges() {
    let p = plan(DisplayMode::ShadedEdges, &scene(), false);
    assert_eq!(p.pipes(), [Pipe::Faces, Pipe::Edges, Pipe::Stock, Pipe::Edges, Pipe::Region]);
    let steps: Vec<Step> = p.steps().cloned().collect();
    assert_eq!(steps[0].range, 0..36, "the model faces");
    assert_eq!(steps[1].range, 0..24, "the model edges");
    assert_eq!(steps[2].range, 36..72, "the stock faces");
    assert_eq!(steps[3].range, 24..48, "the stock edges");
    assert_eq!(steps[4].range, 72..90, "the region fills");
}

#[test]
fn shaded_draws_faces_and_no_edges() {
    // The mode applies to the model and to the stock in the same way.
    let p = plan(DisplayMode::Shaded, &scene(), false);
    assert_eq!(p.pipes(), [Pipe::Faces, Pipe::Stock, Pipe::Region]);
    assert!(!p.pipes().contains(&Pipe::Edges), "no wire on the model or on the stock");
    assert!(!p.pipes().contains(&Pipe::HiddenEdges));
}

#[test]
fn wireframe_draws_depth_only_faces_and_each_edge() {
    let p = plan(DisplayMode::Wireframe, &scene(), false);
    assert_eq!(p.pipes(), [Pipe::FacesDepthOnly, Pipe::Edges, Pipe::Edges, Pipe::Region]);
    assert!(!p.pipes().contains(&Pipe::Faces), "no lit faces");
    assert!(!p.pipes().contains(&Pipe::Stock), "no stock faces");
}

#[test]
fn shaded_with_hidden_edges_draws_the_model_edges_two_times() {
    let p = plan(DisplayMode::ShadedHiddenEdges, &scene(), false);
    assert_eq!(
        p.pipes(),
        [Pipe::Faces, Pipe::Edges, Pipe::HiddenEdges, Pipe::Stock, Pipe::Edges, Pipe::Region]
    );
    let hidden = p.steps().find(|s| s.pipe == Pipe::HiddenEdges).cloned();
    assert_eq!(hidden.map(|s| s.range), Some(0..24), "the same cached model edge list");
}

#[test]
fn the_traced_stock_takes_the_stock_slot_in_the_shaded_modes_only() {
    let mut c = scene();
    c.overlay_index_start = 54;
    for mode in [DisplayMode::ShadedEdges, DisplayMode::Shaded, DisplayMode::ShadedHiddenEdges] {
        let p = plan(mode, &c, true);
        let pipes = p.pipes();
        assert!(pipes.contains(&Pipe::TracedStock), "{mode:?}");
        let stock = p.steps().find(|s| s.pipe == Pipe::Stock).cloned();
        assert_eq!(stock.map(|s| s.range), Some(54..72), "{mode:?}: only the overlay stock");
        let stock_wire = p.steps().any(|s| s.pipe == Pipe::Edges && s.range == (24..48));
        assert!(!stock_wire, "{mode:?}: the traced stock replaces the stock wire");
    }
    let p = plan(DisplayMode::Wireframe, &c, true);
    assert!(!p.pipes().contains(&Pipe::TracedStock), "wireframe draws the stock wire");
}

#[test]
fn an_empty_document_draws_nothing_and_empty_ranges_are_not_listed() {
    for mode in DisplayMode::ALL {
        assert!(plan(mode, &Counts::default(), true).pipes().is_empty(), "{mode:?}");
    }
    // A model only: no stock, no region, no stock edges.
    let c = Counts { index_count: 36, model_index_count: 36, stock_index_end: 36, overlay_index_start: 36, edge_count: 24, model_edge_count: 24, ..Counts::default() };
    assert_eq!(plan(DisplayMode::ShadedEdges, &c, false).pipes(), [Pipe::Faces, Pipe::Edges]);
    assert_eq!(plan(DisplayMode::Shaded, &c, false).pipes(), [Pipe::Faces]);
}

// ---- #2834: the ground shadow pass and the occlusion pass ----

const SHADOW: Effects = Effects { ground_shadow: true, ambient_occlusion: false };
const OCCLUSION: Effects = Effects { ground_shadow: false, ambient_occlusion: true };
const BOTH: Effects = Effects { ground_shadow: true, ambient_occlusion: true };

/// `scene` with cached shadow data and occlusion data for the 36 model vertices.
fn lit() -> Counts {
    Counts { shadow_vertex_count: 600, occlusion_vertex_count: 36, ..scene() }
}

#[test]
fn the_ground_shadow_pass_draws_after_the_model_and_before_the_stock() {
    let on = plan_with(DisplayMode::ShadedEdges, &lit(), false, SHADOW);
    assert_eq!(on.pipes(), [Pipe::Faces, Pipe::Edges, Pipe::GroundShadow, Pipe::Stock, Pipe::Edges, Pipe::Region]);
    let shadow = on.steps().find(|s| s.pipe == Pipe::GroundShadow).cloned();
    assert_eq!(shadow.map(|s| s.range), Some(0..600), "the cached shadow vertices");
    // Off: the list is the list of #2818.
    let off = plan_with(DisplayMode::ShadedEdges, &lit(), false, Effects::default());
    assert_eq!(off, plan(DisplayMode::ShadedEdges, &lit(), false));
    assert!(!off.pipes().contains(&Pipe::GroundShadow), "the switch is off");
    // On, but no shadow data: nothing to draw.
    assert!(!plan_with(DisplayMode::ShadedEdges, &scene(), false, SHADOW).pipes().contains(&Pipe::GroundShadow));
    // Wireframe draws no faces, so it draws no shadow of them.
    for mode in [DisplayMode::Wireframe, DisplayMode::WireframeSeeThrough] {
        assert!(!plan_with(mode, &lit(), false, SHADOW).pipes().contains(&Pipe::GroundShadow), "{mode:?}");
    }
    for mode in [DisplayMode::Shaded, DisplayMode::ShadedHiddenEdges] {
        assert!(plan_with(mode, &lit(), false, SHADOW).pipes().contains(&Pipe::GroundShadow), "{mode:?}");
    }
}

#[test]
fn the_occlusion_pass_takes_the_model_face_slot() {
    let on = plan_with(DisplayMode::ShadedEdges, &lit(), false, OCCLUSION);
    assert_eq!(on.pipes(), [Pipe::FacesOccluded, Pipe::Edges, Pipe::Stock, Pipe::Edges, Pipe::Region]);
    assert_eq!(on.steps().next().map(|s| s.range.clone()), Some(0..36), "the model faces");
    let off = plan_with(DisplayMode::ShadedEdges, &lit(), false, Effects::default());
    assert_eq!(off.pipes().first(), Some(&Pipe::Faces), "the switch is off");
    // Occlusion data that does not fit the model vertices is never read.
    for n in [0, 35, 37] {
        let c = Counts { occlusion_vertex_count: n, ..lit() };
        assert_eq!(plan_with(DisplayMode::ShadedEdges, &c, false, OCCLUSION).pipes().first(), Some(&Pipe::Faces), "{n}");
    }
    assert_eq!(plan_with(DisplayMode::Wireframe, &lit(), false, OCCLUSION).pipes().first(), Some(&Pipe::FacesDepthOnly));
    // See-through draws no face at all: occlusion does not add a face pass.
    let see = plan_with(DisplayMode::WireframeSeeThrough, &lit(), false, BOTH);
    assert_eq!(see, plan(DisplayMode::WireframeSeeThrough, &lit(), false));
    assert_eq!(see.pipes(), [Pipe::Edges, Pipe::Edges, Pipe::Region]);
}

#[test]
fn the_longest_draw_list_holds_each_pass() {
    let p = plan_with(DisplayMode::ShadedHiddenEdges, &lit(), false, BOTH);
    assert_eq!(
        p.pipes(),
        [Pipe::FacesOccluded, Pipe::Edges, Pipe::HiddenEdges, Pipe::GroundShadow, Pipe::Stock, Pipe::Edges, Pipe::Region]
    );
    let c = Counts { overlay_index_start: 54, ..lit() };
    let p = plan_with(DisplayMode::ShadedHiddenEdges, &c, true, BOTH);
    assert_eq!(
        p.pipes(),
        [Pipe::FacesOccluded, Pipe::Edges, Pipe::HiddenEdges, Pipe::GroundShadow, Pipe::TracedStock, Pipe::Stock, Pipe::Region]
    );
}

/// #2636: "Wireframe, see-through" draws each edge, front and back. No face
/// goes to the color buffer or to the depth buffer, so the model hides
/// nothing behind it: not the stock, not a fixture, not a toolpath.
#[test]
fn see_through_wireframe_draws_each_edge_and_no_face() {
    let mode = DisplayMode::WireframeSeeThrough;
    let p = plan(mode, &scene(), false);
    assert_eq!(p.pipes(), [Pipe::Edges, Pipe::Edges, Pipe::Region]);
    assert!(!p.pipes().contains(&Pipe::FacesDepthOnly), "no depth prepass: nothing is hidden");
    let steps: Vec<Step> = p.steps().cloned().collect();
    assert_eq!(steps[0].range, 0..24, "the cached model edges");
    assert_eq!(steps[1].range, 24..48, "the cached stock edges");
    // The traced stock is a shaded surface: this mode draws the stock wire.
    let traced = plan(mode, &scene(), true);
    assert_eq!(traced.pipes(), [Pipe::Edges, Pipe::Edges, Pipe::Region]);
}
