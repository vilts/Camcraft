//! The viewport's render pipelines, built for one sample count (#2958):
//! every pipeline of the 3D pass must match the pass's targets, so a new
//! anti-aliasing choice rebuilds this whole set (and the traced-stock
//! composite, `trace::composite`) and nothing else.
//! Provenance: our own code, moved out of `viewport.rs` (the descriptors
//! are unchanged apart from the sample count and the wide overlay lines);
//! specced from the wgpu documentation.

use egui_wgpu::wgpu;

use super::wide::SEGMENT_LAYOUT;
use super::{DEPTH_FORMAT, VERTEX_LAYOUT};

pub(super) struct Pipes {
    pub pipeline: wgpu::RenderPipeline,
    /// Faces that only write depth - the wireframe mode's hidden-line
    /// prepass (same vertex stage and culling as `pipeline`, no color).
    pub depth_pipeline: wgpu::RenderPipeline,
    pub line_pipeline: wgpu::RenderPipeline,
    /// The model wire behind the faces ("Shaded, with hidden edges", #2818).
    pub hidden_line_pipeline: wgpu::RenderPipeline,
    pub stock_pipeline: wgpu::RenderPipeline,
    pub region_pipeline: wgpu::RenderPipeline,
    /// #2958: the overlay as wide anti-aliased quads, one instance per
    /// segment (`viewport::wide`).
    pub wide_pipeline: wgpu::RenderPipeline,
}

/// Translucent over what is there; the target's alpha stays as it is (the
/// egui texture keeps an opaque viewport).
const OVER: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// What differs between the viewport's pipelines.
struct Spec {
    label: &'static str,
    vs: &'static str,
    fs: &'static str,
    buffer: wgpu::VertexBufferLayout<'static>,
    topology: wgpu::PrimitiveTopology,
    cull: Option<wgpu::Face>,
    depth_write: bool,
    compare: wgpu::CompareFunction,
    blend: Option<wgpu::BlendState>,
    writes: wgpu::ColorWrites,
}

pub(super) fn build(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    samples: u32,
) -> Pipes {
    let make = |s: Spec| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(s.label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(s.vs),
                compilation_options: Default::default(),
                buffers: &[Some(s.buffer)],
            },
            primitive: wgpu::PrimitiveState {
                topology: s.topology,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: s.cull,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(s.depth_write),
                depth_compare: Some(s.compare),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(s.fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: s.blend, write_mask: s.writes })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let tri = wgpu::PrimitiveTopology::TriangleList;
    let back = Some(wgpu::Face::Back);
    let (less, less_eq) = (wgpu::CompareFunction::Less, wgpu::CompareFunction::LessEqual);
    let all = wgpu::ColorWrites::ALL;
    let solid = |label, fs, depth_write, blend, writes| Spec {
        label,
        vs: "vs_main",
        fs,
        buffer: VERTEX_LAYOUT,
        topology: tri,
        cull: back,
        depth_write,
        compare: less,
        blend,
        writes,
    };
    Pipes {
        pipeline: make(solid("viewport3d-pipe", "fs_main", true, None, all)),
        // Depth-only pass for wireframe mode: the same solid geometry,
        // rasterized with color writes off. Faces occlude the far wire
        // exactly as they occlude it when lit - the hidden-line law is
        // then one branch in `render`, not a second geometry.
        depth_pipeline: make(solid("viewport3d-depth", "fs_edge", true, None, wgpu::ColorWrites::empty())),
        // Facet edges over the shaded pass. No culling (back-face wire is
        // hidden by depth anyway). Lines may not carry a depth bias in
        // wgpu, so edges win ties against the faces they lie on via
        // LessEqual instead.
        line_pipeline: make(Spec {
            label: "viewport3d-lines",
            topology: wgpu::PrimitiveTopology::LineList,
            cull: None,
            depth_write: false,
            compare: less_eq,
            ..solid("", "fs_edge", false, None, all)
        }),
        // The hidden-edge pipeline (#2818) is the same wire with the depth
        // test reversed: it draws only where a face is in front, blended.
        hidden_line_pipeline: make(Spec {
            label: "viewport3d-hidden-lines",
            topology: wgpu::PrimitiveTopology::LineList,
            cull: None,
            depth_write: false,
            compare: wgpu::CompareFunction::Greater,
            ..solid("", "fs_edge_hidden", false, Some(wgpu::BlendState::ALPHA_BLENDING), all)
        }),
        // Translucent stock: blended over whatever the opaque pass left,
        // never writing depth (models stay pickable-and-visible through it).
        stock_pipeline: make(solid("viewport3d-stock", "fs_stock", false, Some(OVER), all)),
        // Cut-region fill: the stock blend's shape (last, translucent, no
        // depth write) with a constant color.
        region_pipeline: make(solid("viewport3d-region", "fs_region", false, Some(OVER), all)),
        // The overlay (toolpaths, height planes, outlines, triads, grid):
        // depth-tested, never writing depth, blended by its coverage.
        wide_pipeline: make(Spec {
            label: "viewport3d-wide",
            vs: "vs_wide",
            fs: "fs_wide",
            buffer: SEGMENT_LAYOUT,
            topology: tri,
            cull: None,
            depth_write: false,
            compare: less_eq,
            blend: Some(OVER),
            writes: all,
        }),
    }
}

/// The pass's targets for one sample count.
pub(super) struct Targets {
    /// The multisample colour target (`None` at 1 sample per pixel).
    pub msaa: Option<wgpu::Texture>,
    pub msaa_view: Option<wgpu::TextureView>,
    /// The single-sample colour the shell shows and tests read back.
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 }
}

pub(super) fn make_targets(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    samples: u32,
) -> Targets {
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport3d-color"),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        // COPY_SRC lets tests read the frame back (View3D::snapshot_rgba);
        // for the app the flag costs nothing at runtime.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let (msaa, depth) = make_sample_targets(device, format, width, height, samples);
    let msaa_view = msaa.as_ref().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    Targets { msaa, msaa_view, color, color_view, depth, depth_view }
}

/// The targets whose sample count follows the anti-aliasing: the
/// multisample colour (none at 1) and the depth.
pub(super) fn make_sample_targets(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    samples: u32,
) -> (Option<wgpu::Texture>, wgpu::Texture) {
    let target = |label, format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent(width, height),
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let msaa = (samples > 1).then(|| target("viewport3d-msaa", format, wgpu::TextureUsages::RENDER_ATTACHMENT));
    let depth = target("viewport3d-depth", DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
    (msaa, depth)
}
