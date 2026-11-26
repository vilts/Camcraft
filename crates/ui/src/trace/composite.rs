//! The traced stock drawn into the 3D pass (M7.2 B3b): a fullscreen
//! pipeline over the trace frame's per-pixel vec4s, writing the hit's
//! projected depth so the machined volume sorts with models and lines.
//! Provenance: deferred composite of a ray-marched depth image over
//! textbook z/w rasterization depth; our own Rust/WGSL.
//!
//! ## The laws here
//!
//! - **Same eye, same depth.** The fragment rebuilds its ray from the
//!   viewport camera's own basis and field of view (`Camera`'s 60° law;
//!   in orthographic the parallel ray from the eye plane, #609)
//!   and projects the hit through the very `mvp` the meshes use - the
//!   traced surface can neither float over nor sink under a model.
//! - **Opaque, and only where it hit.** Status 1 draws; a miss or a
//!   budget bail discards, so nothing invented paints material.
//! - **Refuse, never black-screen.** A device that cannot read storage
//!   from a fragment refuses at construction by name, and the shell
//!   keeps the stock sheet.
//! - **One sample count with the pass.** The composite draws into the
//!   viewport's pass, so its pipeline follows the pass's anti-aliasing
//!   (#2958, `set_samples`). It writes one depth for each pixel, so the
//!   traced stock's own edges are not smoothed; its cost does not change.

use egui_wgpu::wgpu;

const SHADER: &str = include_str!("composite.wgsl");

/// `struct C` in composite.wgsl: a mat4 and nine vec4s (five of frame
/// geometry, four of verdict palette - M13.4).
const UNIFORM_SIZE: u64 = 64 + 9 * 16;

/// The viewport depth format - `viewport::DEPTH_FORMAT`'s twin.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// What the uniform needs besides the `mvp`: the eye in the trace's
/// own terms and the two frame sizes.
#[derive(Clone, Copy, Debug)]
pub struct FrameGeom {
    pub eye: [f32; 3],
    pub fwd: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub tan_half_fov: f32,
    /// The orthographic view half height (#609); 0 for perspective.
    pub ortho_half_height: f32,
    /// Viewport extent in pixels.
    pub view: (u32, u32),
    /// Viewport pixels per trace pixel (>= 1).
    pub k: u32,
    /// Trace frame extent in pixels.
    pub trace: (u32, u32),
    /// The verdict palette (M13.4's colour-vision remap): clean, rest,
    /// gouge, unjudged - `fs_stock`'s, so sheet and trace agree.
    pub verdict: [[f32; 3]; 4],
}

/// The composite pipeline plus its uniform.
pub struct Composite {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    /// Kept to rebuild the pipeline for a new sample count (#2958).
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    samples: u32,
}

impl Composite {
    /// Build the pipeline for the viewport's color `format`. `Err` by
    /// name where the device cannot bind storage to a fragment shader.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        pollster::block_on(Self::new_async(device, format))
    }

    /// `new`, awaiting the device's verdict instead of blocking on it: a
    /// browser answers the error scope from its event loop (#31).
    pub async fn new_async(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        if device.limits().max_storage_buffers_per_shader_stage == 0 {
            return Err("the device binds no storage buffers to a fragment shader".into());
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("trace-composite"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("trace-composite-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("trace-composite-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = pipeline_for(device, &module, &layout, format, 1);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trace-composite-uniform"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if let Some(err) = scope.pop().await {
            return Err(format!("the device refused the composite pipeline ({err})"));
        }
        Ok(Composite { pipeline, bgl, uniform, module, layout, format, samples: 1 })
    }

    /// #2958: rebuild the pipeline for the viewport pass's sample count.
    /// The caller passes only a count the device offers (the viewport's
    /// `effective_samples`); the same count builds the viewport's own.
    pub fn set_samples(&mut self, device: &wgpu::Device, samples: u32) {
        let samples = samples.max(1);
        if samples != self.samples {
            self.pipeline = pipeline_for(device, &self.module, &self.layout, self.format, samples);
            self.samples = samples;
        }
    }

    #[must_use]
    pub const fn samples(&self) -> u32 {
        self.samples
    }

    /// Bind a trace frame (one vec4 per trace pixel, STORAGE usage).
    pub fn bind(&self, device: &wgpu::Device, frame: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("trace-composite-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: frame.as_entire_binding() },
            ],
        })
    }

    /// Stage this frame's uniform (lands at submit, before the pass).
    pub fn write(&self, queue: &wgpu::Queue, mvp: &[[f32; 4]; 4], g: &FrameGeom) {
        queue.write_buffer(&self.uniform, 0, &uniform_bytes(mvp, g));
    }

    /// Draw the composite into an open 3D pass. The caller re-binds
    /// its own group 0 afterwards.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, bind: &wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// The composite pipeline for `samples` per pixel.
fn pipeline_for(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    samples: u32,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("trace-composite-pipe"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_full"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_composite"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// `struct C`, field by field.
#[must_use]
pub fn uniform_bytes(mvp: &[[f32; 4]; 4], g: &FrameGeom) -> Vec<u8> {
    let mut b = Vec::with_capacity(UNIFORM_SIZE as usize);
    for col in mvp {
        for v in col {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut v4 = |v: [f32; 3], w: f32| {
        for x in [v[0], v[1], v[2], w] {
            b.extend_from_slice(&x.to_le_bytes());
        }
    };
    v4(g.eye, g.tan_half_fov);
    v4(g.fwd, g.view.0 as f32);
    v4(g.right, g.view.1 as f32);
    v4(g.up, g.k as f32);
    v4([g.trace.0 as f32, g.trace.1 as f32, g.ortho_half_height], 0.0);
    for c in g.verdict {
        v4(c, 1.0);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uniform_is_exactly_struct_c() {
        let g = FrameGeom {
            eye: [1.0, 2.0, 3.0],
            fwd: [0.0, 0.0, -1.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            tan_half_fov: 0.5,
            ortho_half_height: 7.5,
            view: (640, 480),
            k: 2,
            trace: (320, 240),
            verdict: [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6], [0.7, 0.8, 0.9], [1.0, 0.0, 0.5]],
        };
        let b = uniform_bytes(&[[0.0; 4]; 4], &g);
        assert_eq!(b.len() as u64, UNIFORM_SIZE);
        let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        assert_eq!(f(64 + 12), 0.5); // eye.w = tan
        assert_eq!(f(80 + 12), 640.0); // fwd.w = view width
        assert_eq!(f(112 + 12), 2.0); // up.w = k
        assert_eq!(f(128), 320.0); // frame.x
        assert_eq!(f(128 + 8), 7.5); // frame.z = ortho half height (#609)
        assert_eq!(f(144 + 16 * 2 + 4), 0.8); // gouge.g
        assert_eq!(f(144 + 16 * 3 + 12), 1.0); // unjudged.w
    }
}
