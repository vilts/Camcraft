//! The GPU side of the ground shadow and of the ambient occlusion (#2834):
//! two pipelines of the viewport's one 3D pass, and the two buffers that
//! hold the cached data. `draw_list` says when each one draws.
//!
//! - Ground shadow: the cached shadow mesh (`ground_shadow`), blended in
//!   black on the floor by the shade of each vertex.
//! - Ambient occlusion: the model faces through the face shader, with the
//!   cached value of each vertex (`occlusion`) as a second vertex buffer.
//!   The model vertex buffer itself does not change.
//!
//! Both use plain vertex buffers and alpha blending: no texture, no second
//! render target. The browser shell draws them like the native window.
//! Both draw inside the viewport's one 3D pass, into its colour and depth
//! targets, so both pipelines take the pass's sample count (#2958 MSAA) and
//! `View3D::set_samples` rebuilds them with the other viewport pipelines.
//!
//! Provenance: our own wgpu wiring, specced from the wgpu documentation.
//! The shadow is a blended planar shadow (Blinn 1988) and the occlusion is
//! a baked vertex term (Landis 2002); see `ground_shadow` and `occlusion`.
//! No vendor material.

use std::ops::Range;
use std::sync::Arc;

use egui_wgpu::wgpu;

use crate::draw_list::Effects;
use crate::ground_shadow::ShadowVertex;

/// The cached shade data of one scene: what `View3D::set_shade` uploads.
/// Each part is empty while its switch is off.
#[derive(Clone, Default)]
pub struct Shade {
    /// The ground shadow mesh: a triangle list on the floor.
    pub shadow: Arc<Vec<ShadowVertex>>,
    /// One occlusion value for each model vertex, in upload order.
    pub occlusion: Arc<Vec<f32>>,
}

impl Shade {
    /// Feed the content to a scene digest: new shade data is a new upload.
    pub(crate) fn digest(&self, eat: &mut impl FnMut(u64)) {
        eat(self.shadow.len() as u64);
        for f in self.shadow.iter().flatten() {
            eat(u64::from(f.to_bits()));
        }
        eat(self.occlusion.len() as u64);
        for f in self.occlusion.iter() {
            eat(u64::from(f.to_bits()));
        }
    }
}

/// The shader stages of the two passes, appended to the viewport shader:
/// they read its uniform `u` and its `lit` face function.
pub(crate) const WGSL: &str = "
// Occluded model faces (#2834): the face shader with the baked value of
// the vertex (1 = open, lower in a corner) on its light.
struct OccludedOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) sel: f32,
    @location(1) world: vec3<f32>,
    @location(2) cls: f32,
    @location(3) open: f32,
};

@vertex
fn vs_occluded(@location(0) pos: vec3<f32>, @location(1) sel: f32,
               @location(2) cls: f32, @location(3) open: f32) -> OccludedOut {
    var out: OccludedOut;
    out.clip = u.mvp * vec4<f32>(pos, 1.0);
    out.sel = sel;
    out.world = pos;
    out.cls = cls;
    out.open = open;
    return out;
}

@fragment
fn fs_occluded(in: OccludedOut) -> @location(0) vec4<f32> {
    return lit(in.world, in.sel, in.cls, in.open);
}

// Ground shadow (#2834): black, as dense as the shade of the vertex.
struct ShadowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) shade: f32,
};

@vertex
fn vs_shadow(@location(0) pos: vec3<f32>, @location(1) shade: f32) -> ShadowOut {
    var out: ShadowOut;
    out.clip = u.mvp * vec4<f32>(pos, 1.0);
    out.shade = shade;
    return out;
}

@fragment
fn fs_shadow(in: ShadowOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.6 * in.shade);
}
";

/// One occlusion value for each vertex: the second buffer of the occluded
/// face pipeline, at shader location 3.
const OCCLUSION_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 4,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 0, shader_location: 3 }],
};

/// A shadow vertex: vec3 position + f32 shade.
const SHADOW_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 16,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    ],
};

/// The pipelines, the buffers and the switches of the two passes.
pub(crate) struct ShadePass {
    occluded_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    /// The sample count the two pipelines are built for: the 3D pass's.
    samples: u32,
    shadow_buf: Option<wgpu::Buffer>,
    pub(crate) shadow_count: u32,
    occlusion_buf: Option<wgpu::Buffer>,
    pub(crate) occlusion_count: u32,
    /// The switches that the next render reads (`View3D::set_effects`).
    pub(crate) effects: Effects,
}

/// The occluded face pipeline and the shadow pipeline for `samples` per
/// pixel. Both draw into the 3D pass's own targets (no target of their
/// own), so `samples` is always the pass's count.
fn pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    samples: u32,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let pipe = |label, vs, fs, buffers: &[Option<wgpu::VertexBufferLayout<'_>>], cull, write, blend| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: cull,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::viewport::DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    // The face pipeline of the viewport, with the occlusion buffer.
    let occluded_pipeline = pipe(
        "viewport3d-occluded",
        "vs_occluded",
        "fs_occluded",
        &[Some(crate::viewport::VERTEX_LAYOUT), Some(OCCLUSION_LAYOUT)],
        Some(wgpu::Face::Back),
        true,
        None,
    );
    // The shadow shows from above and from below, writes no depth and
    // keeps the alpha of the target, like the translucent stock.
    let shadow_pipeline = pipe(
        "viewport3d-ground-shadow",
        "vs_shadow",
        "fs_shadow",
        &[Some(SHADOW_LAYOUT)],
        None,
        false,
        Some(wgpu::BlendState {
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
        }),
    );
    (occluded_pipeline, shadow_pipeline)
}

impl ShadePass {
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let samples = samples.max(1);
        let (occluded_pipeline, shadow_pipeline) = pipelines(device, shader, layout, format, samples);
        ShadePass {
            occluded_pipeline,
            shadow_pipeline,
            samples,
            shadow_buf: None,
            shadow_count: 0,
            occlusion_buf: None,
            occlusion_count: 0,
            effects: Effects::default(),
        }
    }

    /// Build the two pipelines again for `samples` per pixel (#2958): the
    /// pass's targets changed their sample count. The buffers stay.
    pub(crate) fn rebuild(
        &mut self,
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        samples: u32,
    ) {
        let samples = samples.max(1);
        let (occluded, shadow) = pipelines(device, shader, layout, format, samples);
        self.occluded_pipeline = occluded;
        self.shadow_pipeline = shadow;
        self.samples = samples;
    }

    /// The sample count of the two pipelines.
    pub(crate) const fn samples(&self) -> u32 {
        self.samples
    }

    /// Replace the two buffers. Empty data frees its buffer.
    pub(crate) fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, shade: &Shade) {
        let upload = |label, floats: &mut dyn Iterator<Item = f32>| -> Option<wgpu::Buffer> {
            let bytes: Vec<u8> = floats.flat_map(f32::to_le_bytes).collect();
            if bytes.is_empty() {
                return None;
            }
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buf, 0, &bytes);
            Some(buf)
        };
        self.shadow_buf = upload("viewport3d-shadow", &mut shade.shadow.iter().flatten().copied());
        self.shadow_count = if self.shadow_buf.is_some() { u32::try_from(shade.shadow.len()).unwrap_or(0) } else { 0 };
        self.occlusion_buf = upload("viewport3d-occlusion", &mut shade.occlusion.iter().copied());
        self.occlusion_count =
            if self.occlusion_buf.is_some() { u32::try_from(shade.occlusion.len()).unwrap_or(0) } else { 0 };
    }

    /// The occluded face pipeline with its occlusion buffer bound, or `None`
    /// when there is no buffer (the draw list then never asks for it).
    pub(crate) fn bind_occluded(&self, pass: &mut wgpu::RenderPass<'_>) -> Option<&wgpu::RenderPipeline> {
        let buf = self.occlusion_buf.as_ref()?;
        pass.set_vertex_buffer(1, buf.slice(..));
        Some(&self.occluded_pipeline)
    }

    /// Draw `range` of the shadow mesh. The caller binds its own vertex
    /// buffer again after this.
    pub(crate) fn draw_shadow(&self, pass: &mut wgpu::RenderPass<'_>, range: Range<u32>) {
        let Some(buf) = &self.shadow_buf else { return };
        pass.set_pipeline(&self.shadow_pipeline);
        pass.set_vertex_buffer(0, buf.slice(..));
        pass.draw(range.start..range.end.min(self.shadow_count), 0..1);
    }
}
