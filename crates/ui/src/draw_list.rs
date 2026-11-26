//! The viewport's draw list (#2818): which pipeline draws which index range,
//! in which order, for each display mode. Pure arithmetic on the counts that
//! `View3D::set_document` already cached, so a frame builds no geometry and a
//! test can read the list without a GPU.
//! Provenance: our own pass order, moved out of `View3D::render_with`; the
//! hidden-edge pass is the reversed depth test of published hidden-line
//! practice (Foley, van Dam et al., "Computer Graphics: Principles and
//! Practice"). The ground shadow pass and the occlusion pass (#2834) are
//! listed here too; `shade_pass` has their sources. No vendor material.

use std::ops::Range;

use crate::state::DisplayMode;

/// One pipeline of the 3D pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pipe {
    /// Lit model faces.
    Faces,
    /// Lit model faces, darkened by the cached occlusion values (#2834).
    FacesOccluded,
    /// The cached ground shadow mesh, blended on the floor (#2834). Its
    /// range counts vertices of the shadow buffer, not indices.
    GroundShadow,
    /// Model faces to the depth buffer only: the hidden-line prepass.
    FacesDepthOnly,
    /// Feature edges in front of the faces.
    Edges,
    /// Feature edges behind the faces, in a lighter tone.
    HiddenEdges,
    /// Translucent stock faces.
    Stock,
    /// The traced (machined) stock, drawn by its own pass.
    TracedStock,
    /// Translucent cut-region fills.
    Region,
}

impl Pipe {
    /// True when the step reads the edge index buffer, not the face one.
    #[must_use]
    pub const fn uses_edge_buffer(self) -> bool {
        matches!(self, Pipe::Edges | Pipe::HiddenEdges)
    }
}

/// One draw call: a pipeline and the index range that it draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub pipe: Pipe,
    pub range: Range<u32>,
}

/// The cached partition of the document's index buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// All face indices: models, then stock, then region fills.
    pub index_count: u32,
    pub model_index_count: u32,
    pub stock_index_end: u32,
    /// Start of the overlay stock (the incoming stock) in the stock range.
    pub overlay_index_start: u32,
    /// All edge indices: model edges, then stock edges.
    pub edge_count: u32,
    pub model_edge_count: u32,
    /// Vertices of the cached ground shadow mesh (#2834); 0 with no data.
    pub shadow_vertex_count: u32,
    /// Cached occlusion values (#2834): one for each model vertex, or 0.
    pub occlusion_vertex_count: u32,
}

/// The shading switches of the Display menu (#2834). View state: the draw
/// list reads them, nothing in the document does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub ground_shadow: bool,
    pub ambient_occlusion: bool,
}

/// The most steps that one frame can hold.
const MAX_STEPS: usize = 8;

/// A fixed-size draw list: no allocation on a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    steps: [Option<Step>; MAX_STEPS],
    len: usize,
}

impl Plan {
    fn new() -> Self {
        Plan { steps: [const { None }; MAX_STEPS], len: 0 }
    }

    /// Add a step. An empty range draws nothing and is not listed.
    fn push(&mut self, pipe: Pipe, range: Range<u32>) {
        if range.start >= range.end && pipe != Pipe::TracedStock {
            return;
        }
        if let Some(slot) = self.steps.get_mut(self.len) {
            *slot = Some(Step { pipe, range });
            self.len += 1;
        }
    }

    /// The steps in draw order.
    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().take(self.len).flatten()
    }

    /// The pipelines in draw order: what a test reads.
    #[must_use]
    pub fn pipes(&self) -> Vec<Pipe> {
        self.steps().map(|s| s.pipe).collect()
    }
}

/// The draw list of one frame. `traced` says that the caller has a traced
/// stock to draw in the stock slot; only the shaded modes use it.
///
/// - Shaded, with edges: faces, edges, stock, stock edges, regions.
/// - Shaded: the same without the edges of the model and of the stock.
/// - Wireframe: depth-only faces, edges, stock edges, regions. No stock
///   faces: the stock is a volume and its full wire reads truer.
/// - Shaded, with hidden edges: as "with edges", and the model edges draw a
///   second time behind the faces.
/// - Wireframe, see-through (#2636): edges, stock edges, regions. No face
///   step at all, so the depth buffer stays empty and each edge draws.
#[must_use]
pub fn plan(display: DisplayMode, c: &Counts, traced: bool) -> Plan {
    plan_with(display, c, traced, Effects::default())
}

/// [`plan`] with the shading switches (#2834). The two passes belong to the
/// shaded modes: wireframe draws no lit face and no shadow of one.
///
/// - Ambient occlusion: the occluded face pass takes the model face slot. It
///   needs one cached value for each model vertex; data that does not fit is
///   never read, and the plain face pass draws.
/// - Ground shadow: the cached shadow mesh draws after the model (so that
///   the model hides the part below it) and before the translucent stock.
#[must_use]
pub fn plan_with(display: DisplayMode, c: &Counts, traced: bool, fx: Effects) -> Plan {
    let mut p = Plan::new();
    if c.index_count == 0 {
        return p;
    }
    let wireframe = display.is_wireframe();
    let models = 0..c.model_index_count;
    let model_edges = 0..c.model_edge_count;
    let occluded = fx.ambient_occlusion && c.occlusion_vertex_count == c.model_index_count;
    let faces = match (wireframe, occluded) {
        (true, _) => Pipe::FacesDepthOnly,
        (false, true) => Pipe::FacesOccluded,
        (false, false) => Pipe::Faces,
    };
    // See-through writes no depth: a depth prepass would hide the far edges,
    // the stock wire and the toolpath lines behind the model.
    if !display.sees_through() {
        p.push(faces, models);
    }
    // "Shaded" draws no wire: not on the model and not on the stock.
    let wire = display != DisplayMode::Shaded;
    if wire {
        p.push(Pipe::Edges, model_edges.clone());
    }
    if display == DisplayMode::ShadedHiddenEdges {
        p.push(Pipe::HiddenEdges, model_edges);
    }
    if fx.ground_shadow && !wireframe {
        p.push(Pipe::GroundShadow, 0..c.shadow_vertex_count);
    }
    let traced = traced && !wireframe;
    if traced {
        p.push(Pipe::TracedStock, 0..0);
        // The overlay stock is not the traced stock's to replace.
        p.push(Pipe::Stock, c.overlay_index_start..c.stock_index_end);
    } else {
        if !wireframe {
            p.push(Pipe::Stock, c.model_index_count..c.stock_index_end);
        }
        if wire {
            p.push(Pipe::Edges, c.model_edge_count..c.edge_count);
        }
    }
    p.push(Pipe::Region, c.stock_index_end..c.index_count);
    p
}

#[cfg(test)]
#[path = "draw_list_tests.rs"]
mod tests;
