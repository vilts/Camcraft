//! 3D viewport: indexed triangles with a z-buffer, rendered offscreen and
//! handed to the shell as a texture so the view docks like any panel (the
//! imgui external-texture bridge retired at M1.4.2 M-g; the wgpu types
//! come through `egui_wgpu` now - the same unified wgpu underneath).
//! Provenance: standard orbit camera + z-buffered indexed rendering from
//! public graphics literature.
//!
//! Selection is baked into the vertex stream (an extra float per vertex):
//! `Queue::write_buffer` lands at submit time, so a per-draw uniform tint
//! would silently take the last frame's color - one uniform means one
//! upload per frame and one draw call.


use egui_wgpu::wgpu;

/// vec3 position + f32 selection flag + f32 deviation class. Like `sel`,
/// the class rides baked in the vertex stream (same submit-time law as
/// above) and every `vs_main` pipeline reads all three attributes - the
/// layouts are one const, so a pipeline can never forget the third.
const VERTEX_STRIDE: u64 = 20;

/// The one vertex-buffer law, shared by every solid pipeline.
const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: VERTEX_STRIDE,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 12,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 16,
            shader_location: 2,
        },
    ],
};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// `struct U`: mvp + eye + wire (80 bytes), six palette vec4s, then the
/// wireframe edge ink of the live ground (#698).
const UNIFORM_SIZE: u64 = 80 + 7 * 16;

const SHADER: &str = "
// The verdict / selection palette rides the uniform (M13.4's
// colour-vision remap; Off = the literals these fields replaced).
struct U { mvp: mat4x4<f32>, eye: vec3<f32>, wire: f32,
           clean: vec4<f32>, rest: vec4<f32>, gouge: vec4<f32>,
           unjudged: vec4<f32>, hot: vec4<f32>, warm: vec4<f32>,
           wire_ink: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;

struct VSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) sel: f32,
    @location(1) world: vec3<f32>,
    @location(2) cls: f32,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) sel: f32,
           @location(2) cls: f32) -> VSOut {
    var out: VSOut;
    out.clip = u.mvp * vec4<f32>(pos, 1.0);
    out.sel = sel;
    out.world = pos;
    out.cls = cls;
    return out;
}

// Flat shading: STL/OBJ facets carry no smooth normals, so recover the
// face normal from screen-space derivatives of the world position.
// Key + fill give the broad gradient; a view-dependent specular adds the
// CAD-style sheen that makes facet angles readable at a glance.
@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let cr = cross(dpdx(in.world), dpdy(in.world));
    let len = length(cr);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), cr / len, len > 1e-9);
    let key_dir = normalize(vec3<f32>(0.45, 0.60, 0.66));
    // abs(): two-sided, so flipped windings never render unlit-black.
    let key = abs(dot(n, key_dir));
    let fill = abs(dot(n, normalize(vec3<f32>(-0.55, 0.25, 0.40))));
    let light = 0.34 + 0.52 * key + 0.18 * fill;
    let to_eye = normalize(u.eye - in.world);
    let halfv = normalize(key_dir + to_eye);
    let spec = pow(max(abs(dot(n, halfv)), 0.0), 24.0) * 0.35 * key;
    let base = vec3<f32>(0.66, 0.70, 0.76) * light + vec3<f32>(spec);
    let hot = vec3<f32>(0.98, 0.72, 0.20) * light + vec3<f32>(spec);
    // #339 slice 4: a fixture the selected op strikes wears the gouge
    // colour (class STRUCK_CLASS; plain models carry class 0).
    let struck = step(3.5, in.cls);
    let hit = u.gouge.xyz * light + vec3<f32>(spec);
    return vec4<f32>(mix(mix(base, hot, in.sel), hit, struck * 0.85), 1.0);
}

// Facet edges: dark over the shaded pass, warm when the part is selected.
// In wireframe mode the same wire runs over the ground instead of over
// lit metal, so it brightens to a cool gray on the dark ground and darkens
// to a slate on the light one (#698, `u.wire_ink`; the selection warm
// lifts to the same hot amber the fills use) - `u.wire` is the mode flag.
@fragment
fn fs_edge(in: VSOut) -> @location(0) vec4<f32> {
    let dark = vec3<f32>(0.06, 0.07, 0.09);
    let gray = u.wire_ink.xyz;
    let unlit = mix(dark, gray, u.wire);
    let sel = mix(u.warm.xyz, u.hot.xyz, u.wire);
    return vec4<f32>(mix(unlit, sel, in.sel), 1.0);
}

// Stock: same flat shading, cool steel tint, translucent - the model must
// read through the raw material. Selected stock is warmer and denser.
// The `cls` channel carries the deviation verdict (stock_sim's law: 0
// clean, 1 rest, 2 gouge, 3 unjudged) and paints it (our own display
// colors, the same reasoning as REGION_RGB's note): amber material left
// behind, loud red material cut too deep, flat gray where the kernel
// rendered no verdict - NaN is never green, never a clean bill. Plain
// stock parts upload cls = 0 everywhere and look exactly as before.
// Verdict columns keep their color even when selected (the amber lift is
// only for steel), and judged columns sit denser so the color reads.
// Class 4 is no verdict but the incoming stock a carried setup starts
// from (#406): a fixed teal, denser than steel, readable on the light and
// the dark viewport alike (our own display choice).
@fragment
fn fs_stock(in: VSOut) -> @location(0) vec4<f32> {
    let cr = cross(dpdx(in.world), dpdy(in.world));
    let len = length(cr);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), cr / len, len > 1e-9);
    let key_dir = normalize(vec3<f32>(0.45, 0.60, 0.66));
    let key = abs(dot(n, key_dir));
    let light = 0.55 + 0.45 * key;
    var tint = u.clean.xyz;
    var alpha = mix(0.30, 0.55, in.sel);
    switch (i32(in.cls + 0.5)) {
        case 1: { tint = u.rest.xyz; alpha = 0.75; }  // rest
        case 2: { tint = u.gouge.xyz; alpha = 0.85; }  // gouge
        case 3: { tint = u.unjudged.xyz; alpha = 0.25; }  // no verdict
        case 4: { tint = vec3<f32>(0.12, 0.66, 0.74); alpha = 0.45; }  // incoming stock (#406)
        default: {}
    }
    let judged = step(0.5, in.cls);
    let base = mix(u.clean.xyz, u.hot.xyz, in.sel * (1.0 - judged));
    return vec4<f32>(mix(base, tint, judged) * light, alpha);
}

// Cut-region fill: the selected operation's material area painted, not just
// outlined. A constant translucent warning orange (REGION_RGB in the app
// layer, alpha 0.22 - our own display choice) with no lighting: the fill
// marks a level, it is not a surface.
@fragment
fn fs_region(in: VSOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.95, 0.58, 0.15, 0.22);
}

// Overlay lines (WCS triads, world axes): position + explicit color, no
// lighting - reference geometry must stay legible at any zoom.
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rgb: vec3<f32>,
};

@vertex
fn vs_line(@location(0) pos: vec3<f32>, @location(1) rgb: vec3<f32>) -> LineOut {
    var out: LineOut;
    out.clip = u.mvp * vec4<f32>(pos, 1.0);
    out.rgb = rgb;
    return out;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.rgb, 1.0);
}
";

/// Overlay line vertex: vec3 position + vec3 color.
const OVERLAY_STRIDE: u64 = 24;

/// One triangle-soup part for the GPU: world-space vertices (transform
/// applied by the caller), plus selection and kind flags. Stock parts are
/// drawn blended after the models; region-fill parts after those (see
/// `set_document`'s models → stock → fill partition). A fill part is pure
/// translucent overlay: it never contributes welded wire.
/// The class a model part's vertices carry when it is a fixture the
/// selected op strikes (#339 slice 4): `fs_main` tints it in the gouge
/// colour. Above every stock class (0..=3), which only `fs_stock` reads.
pub const STRUCK_CLASS: f32 = 4.0;

pub struct GpuPart {
    pub verts: Vec<[f32; 3]>,
    pub selected: bool,
    pub stock: bool,
    pub fill: bool,
    /// Per-vertex deviation class (stock_sim's `color_classes` law: 0
    /// clean, 1 rest, 2 gouge, 3 unjudged; 4 the incoming-stock preview,
    /// #406), read only by `fs_stock`.
    /// Empty means "no verdict rendered" - the part uploads class 0
    /// everywhere and paints exactly as an ordinary stock part.
    pub classes: Vec<f32>,
}

pub struct View3D {
    pipeline: wgpu::RenderPipeline,
    /// Faces that only write depth - the wireframe mode's hidden-line
    /// prepass (same vertex stage and culling as `pipeline`, no color).
    depth_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    stock_pipeline: wgpu::RenderPipeline,
    region_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    /// The verdict/selection palette the next render uploads (M13.4;
    /// the shell sets it from the world's colour-vision mode).
    palette: crate::egui_app::colorvision::Semantic,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    /// Total indices; `[0, model_index_count)` are models,
    /// `[model_index_count, stock_index_end)` stock and the rest region
    /// fills (set_document partitions parts models-first).
    index_count: u32,
    model_index_count: u32,
    stock_index_end: u32,
    /// Where the overlay stock parts (the incoming stock, #406) start
    /// inside the stock range: `[overlay_index_start, stock_index_end)`
    /// is drawn even when the traced stock replaces the rest of the
    /// stock range - otherwise the preview vanished whenever toolpaths
    /// existed (hand check of #519 on a real GPU).
    overlay_index_start: u32,
    edge_buf: wgpu::Buffer,
    /// Pairs of u32 line indices; `[0, model_edge_count)` are model edges,
    /// the rest stock edges. Edges are per-part, so a stock face coplanar
    /// with a model face cannot weld the two into a seam-less blob.
    edge_count: u32,
    model_edge_count: u32,
    overlay_buf: wgpu::Buffer,
    overlay_count: u32,
    _color_tex: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl View3D {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport3d"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport3d-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport3d-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // mat4x4 (64) + eye vec3 + wire (80), then the six palette
        // vec4s (M13.4): UNIFORM_SIZE.
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-uniform"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport3d-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VERTEX_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Depth-only pass for wireframe mode: the same solid geometry,
        // rasterized with color writes off. Faces occlude the far wire
        // exactly as they occlude it when lit - the hidden-line law is
        // then one branch in `render`, not a second geometry.
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-depth"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VERTEX_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_edge"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Facet edges over the shaded pass. No culling (back-face wire is
        // hidden by depth anyway) and a negative slope bias so the lines
        // win the depth test against the faces they lie on.
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-lines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VERTEX_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // Lines may not carry a depth bias in wgpu 30, so edges win
                // ties against the faces they lie on via LessEqual instead.
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_edge"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Translucent stock: blended over whatever the opaque pass left,
        // never writing depth (models stay pickable-and-visible through it).
        let stock_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-stock"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VERTEX_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_stock"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
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
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Cut-region fill: the stock blend's shape (last, translucent, no
        // depth write) with a constant color - cloned, not shared, so a
        // future stock-material change cannot silently retint the fill.
        let region_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-region"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VERTEX_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_region"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
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
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Non-indexed lines for the WCS triads and world axes: their own
        // vertex layout (pos + rgb), no lighting, depth-tested but never
        // occluded-by-write.
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-overlay"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: OVERLAY_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // 4-byte placeholders keep the empty-document path simple.
        let dummy = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 4,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let vertex_buf = dummy("viewport3d-vbuf", wgpu::BufferUsages::VERTEX);
        let index_buf = dummy("viewport3d-ibuf", wgpu::BufferUsages::INDEX);
        let edge_buf = dummy("viewport3d-ebuf", wgpu::BufferUsages::INDEX);
        let overlay_buf = dummy("viewport3d-obuf", wgpu::BufferUsages::VERTEX);

        let targets = make_targets(device, format, width, height);

        View3D {
            pipeline,
            depth_pipeline,
            line_pipeline,
            stock_pipeline,
            region_pipeline,
            overlay_pipeline,
            bind_group,
            uniform,
            palette: crate::egui_app::colorvision::Semantic::OFF,
            vertex_buf,
            index_buf,
            index_count: 0,
            model_index_count: 0,
            stock_index_end: 0,
            overlay_index_start: 0,
            edge_buf,
            edge_count: 0,
            model_edge_count: 0,
            overlay_buf,
            overlay_count: 0,
            _color_tex: targets.color,
            color_view: targets.color_view,
            _depth_tex: targets.depth,
            depth_view: targets.depth_view,
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// The shell-facing color target; re-register/update after resize.
    #[must_use]
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if width == self.width && height == self.height {
            return;
        }
        let targets = make_targets(device, format, width, height);
        self._color_tex = targets.color;
        self.color_view = targets.color_view;
        self._depth_tex = targets.depth;
        self.depth_view = targets.depth_view;
        self.width = width;
        self.height = height;
    }

    /// Replace the GPU document. Parts are world-space triangle soup (the
    /// caller applies rigid transforms) tagged with selection and stock
    /// kind; partitioning for the draw order happens here, not in the
    /// caller, so a sloppy caller still gets models-first, stock-last.
    pub fn set_document(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, parts: &[GpuPart]) {
        let (order, overlay_from) = draw_order(parts);

        let mut verts: Vec<u8> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut edges: Vec<u32> = Vec::new();
        let mut vertex_count = 0u32;
        let mut model_indices = 0u32;
        let mut stock_end = 0u32;
        let mut overlay_start = None::<u32>;
        let mut model_edges = 0u32;
        for (pi, part) in order.iter().enumerate() {
            if pi == overlay_from {
                overlay_start = Some(idx.len() as u32);
            }
            let sel: f32 = if part.selected { 1.0 } else { 0.0 };
            for (vi, v) in part.verts.iter().enumerate() {
                for c in v {
                    verts.extend_from_slice(&c.to_le_bytes());
                }
                verts.extend_from_slice(&sel.to_le_bytes());
                // Absent class = clean, per the field's law; a class list
                // shorter than the vertex list is a caller bug we paint
                // over as steel, not a crash.
                let cls = part.classes.get(vi).copied().unwrap_or(0.0);
                verts.extend_from_slice(&cls.to_le_bytes());
            }
            let n = part.verts.len() as u32;
            idx.extend(vertex_count..vertex_count + n);
            // CAD-style wire per part: crease and boundary edges only, and
            // never welded across parts - a stock face coplanar with a
            // model face must not erase either solid's outline. Region
            // fills are a pure translucent overlay; their outline already
            // rides the overlay lines, so they add no wire.
            if !part.fill {
                edges.extend(feature_edges(&part.verts).into_iter().map(|i| i + vertex_count));
                stock_end = idx.len() as u32;
                if !part.stock {
                    model_indices = idx.len() as u32;
                    model_edges = edges.len() as u32;
                }
            }
            vertex_count += n;
        }
        if verts.is_empty() {
            self.index_count = 0; // render() then draws nothing, just clears
            self.model_index_count = 0;
            self.stock_index_end = 0;
            self.overlay_index_start = 0;
            self.edge_count = 0;
            self.model_edge_count = 0;
            return;
        }
        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-vbuf"),
            size: verts.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buf, 0, &verts);
        let idx_bytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-ibuf"),
            size: idx_bytes.len() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&index_buf, 0, &idx_bytes);
        let edge_bytes: Vec<u8> = edges.iter().flat_map(|i| i.to_le_bytes()).collect();
        let edge_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-ebuf"),
            size: (edge_bytes.len() as u64).max(4),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !edge_bytes.is_empty() {
            queue.write_buffer(&edge_buf, 0, &edge_bytes);
        }
        self.vertex_buf = vertex_buf;
        self.index_buf = index_buf;
        self.index_count = idx.len() as u32;
        self.model_index_count = model_indices;
        self.stock_index_end = stock_end;
        self.overlay_index_start = overlay_start.unwrap_or(stock_end).min(stock_end);
        self.edge_buf = edge_buf;
        self.edge_count = edges.len() as u32;
        self.model_edge_count = model_edges;
    }

    /// Replace the overlay line set: pairs of `[x, y, z, r, g, b]` vertices
    /// drawn non-indexed after every solid - the WCS triads, world axes and
    /// cached toolpaths (the producers cap the list before it gets here).
    pub fn set_overlay(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, lines: &[[f32; 6]]) {
        if lines.is_empty() {
            self.overlay_count = 0; // render() then skips the draw entirely
            return;
        }
        let mut data = Vec::with_capacity(lines.len() * 24);
        for v in lines {
            for c in v {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-obuf"),
            size: data.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buf, 0, &data);
        self.overlay_buf = buf;
        self.overlay_count = lines.len() as u32;
    }

    /// The verdict/selection palette the next `render` uploads (M13.4's
    /// colour-vision remap). A uniform, not a vertex re-bake: a mode
    /// switch re-colours without touching the geometry buffers.
    pub fn set_palette(&mut self, p: &crate::egui_app::colorvision::Semantic) {
        self.palette = *p;
    }

    /// Encode the 3D pass into the offscreen target. `eye` is the camera
    /// position in world space, for the specular term; `display` selects
    /// shaded-with-wire or hidden-line wireframe (the buffers are identical
    /// either way - only the pass order and the `u.wire` flag differ).
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        mvp: &[[f32; 4]; 4],
        eye: [f32; 3],
        display: crate::state::DisplayMode,
    ) {
        self.render_with(queue, encoder, mvp, eye, display, None);
    }

    /// `render` with the stock slot handed to `stock` (M7.2 B3b's
    /// traced stock): when given, it draws in place of the stock sheet's
    /// faces and wire, in shaded mode only, and group 0 is re-bound after.
    pub fn render_with(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        mvp: &[[f32; 4]; 4],
        eye: [f32; 3],
        display: crate::state::DisplayMode,
        stock: Option<&dyn Fn(&mut wgpu::RenderPass<'_>)>,
    ) {
        let wireframe = display == crate::state::DisplayMode::Wireframe;
        // The live theme's viewport ground (#698): the clear colour and
        // the wireframe ink follow it, so a light theme gets a light ground.
        let ground = crate::egui_app::theme::current().viewport;
        let light = crate::egui_app::ground_ink::light_ground(ground);
        let wire = crate::egui_app::ground_ink::wire_ink(light);
        let mut data = [0u8; UNIFORM_SIZE as usize];
        let mut o = 0usize;
        for col in mvp {
            for v in col {
                data[o..o + 4].copy_from_slice(&v.to_le_bytes());
                o += 4;
            }
        }
        for v in eye {
            data[o..o + 4].copy_from_slice(&v.to_le_bytes());
            o += 4;
        }
        data[76..80].copy_from_slice(&(f32::from(u8::from(wireframe))).to_le_bytes());
        let p = &self.palette;
        o = 80;
        for c in [p.clean, p.rest, p.gouge, p.unjudged, p.select_hot, p.select_warm] {
            for v in [c[0], c[1], c[2], 1.0] {
                data[o..o + 4].copy_from_slice(&v.to_le_bytes());
                o += 4;
            }
        }
        for v in [wire[0], wire[1], wire[2], 1.0] {
            data[o..o + 4].copy_from_slice(&v.to_le_bytes());
            o += 4;
        }
        queue.write_buffer(&self.uniform, 0, &data);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewport3d-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(ground.r()) / 255.0,
                        g: f64::from(ground.g()) / 255.0,
                        b: f64::from(ground.b()) / 255.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // One bind group for the frame: every pipeline shares the layout,
        // so switching pipelines mid-pass needs no resource rebinds. Draw
        // order is opaque models, their wire, then translucent stock with
        // its wire on top, then the cut-region fills last among triangles -
        // a light film over everything it overlaps - then the reference-
        // geometry overlay lines on top of all of it. Wireframe keeps the
        // law, swaps the models' color pass for a depth-only one (so the
        // hidden-line cull still holds) and drops the stock faces - the
        // box is a volume of stock, not a surface, and its full wire reads
        // truer than an occluded one.
        pass.set_bind_group(0, &self.bind_group, &[]);
        if self.index_count > 0 {
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            if wireframe {
                if self.model_index_count > 0 {
                    pass.set_pipeline(&self.depth_pipeline);
                    pass.draw_indexed(0..self.model_index_count, 0, 0..1);
                }
            } else {
                pass.set_pipeline(&self.pipeline);
                pass.draw_indexed(0..self.model_index_count, 0, 0..1);
            }
            if self.model_edge_count > 0 {
                pass.set_index_buffer(self.edge_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_pipeline(&self.line_pipeline);
                pass.draw_indexed(0..self.model_edge_count, 0, 0..1);
            }
            let traced = stock.filter(|_| !wireframe);
            if let Some(draw) = traced {
                draw(&mut pass);
                pass.set_bind_group(0, &self.bind_group, &[]);
                // The overlay stock (the incoming stock, #406) is not the
                // traced stock's to replace: it still draws.
                if self.overlay_index_start < self.stock_index_end {
                    pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_pipeline(&self.stock_pipeline);
                    pass.draw_indexed(self.overlay_index_start..self.stock_index_end, 0, 0..1);
                }
            } else if self.model_index_count < self.stock_index_end && !wireframe {
                pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_pipeline(&self.stock_pipeline);
                pass.draw_indexed(self.model_index_count..self.stock_index_end, 0, 0..1);
            }
            if traced.is_none() && self.model_edge_count < self.edge_count {
                pass.set_index_buffer(self.edge_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_pipeline(&self.line_pipeline);
                pass.draw_indexed(self.model_edge_count..self.edge_count, 0, 0..1);
            }
            if self.stock_index_end < self.index_count {
                pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_pipeline(&self.region_pipeline);
                pass.draw_indexed(self.stock_index_end..self.index_count, 0, 0..1);
            }
        }
        if self.overlay_count > 0 {
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_vertex_buffer(0, self.overlay_buf.slice(..));
            pass.draw(0..self.overlay_count, 0..1);
        }
    }

    /// Read the current color target back to RGBA8 bytes (row-major, no
    /// padding) with its extent. Tests and diagnostics only - the app never
    /// calls it. Requires the 4-bytes-per-pixel formats we use; `None`
    /// anywhere the GPU refuses (no readback path, map failure).
    #[must_use]
    pub fn snapshot_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<(Vec<u8>, u32, u32)> {
        let (width, height) = (self.width, self.height);
        let row_bytes = (u64::from(width) * 4).next_multiple_of(256);
        let size = row_bytes * u64::from(height);
        if size == 0 || size > 64 * 1024 * 1024 {
            return None;
        }
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-snap"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("viewport3d-snap-copy"),
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self._color_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes.try_into().unwrap_or(u32::MAX)),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..)
            .map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
            return None;
        }
        if !matches!(rx.recv(), Ok(Ok(()))) {
            return None;
        }
        let raw = buf.slice(..).get_mapped_range().ok()?;
        let mut out = Vec::with_capacity(usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?);
        for row in 0..height {
            let off = usize::try_from(row_bytes * u64::from(row)).ok()?;
            out.extend_from_slice(&raw[off..off + (width * 4) as usize]);
        }
        drop(raw);
        buf.unmap();
        Some((out, width, height))
    }
}

struct Targets {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

fn make_targets(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Targets {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport3d-color"),
        size,
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
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport3d-depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    Targets {
        color,
        color_view,
        depth,
        depth_view,
    }
}

/// A world direction spelled as its three components - what `normalize`,
/// `fwd`, `basis_right` and `basis_up` all produce, so the orthonormal
/// basis `view_basis` returns is three of these.
type UnitVec = (f64, f64, f64);

/// How the viewport flattens the world to the screen (#434).
/// Ephemeral view state, like the camera itself: a direct field flip,
/// never a command, never saved in the project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectionMode {
    /// The house default: a 60-degree perspective cone, converging lines.
    #[default]
    Perspective,
    /// Parallel projection (issue #434): the view half-height is fixed
    /// at `dist * tan(30 deg)` - the height perspective sees at the
    /// target plane - so switching mid-session never jumps the framing
    /// and `dist` keeps spelling zoom for orbit, pan and `frame` alike.
    Orthographic,
}

/// Orbit camera around a target point, Z-up (the CAM convention).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub dist: f64,
    /// How this camera projects; carried by every camera transform so
    /// draw, pick and dolly math stay on one matrix by construction.
    pub projection: ProjectionMode,
    /// The orbit frame (#1993): which world axis stays up. Yaw and pitch
    /// are read in this frame; the eye and basis map back to the world.
    pub frame: crate::view_frame::ViewFrame,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: [0.0, 0.0, 0.0],
            yaw: 0.9,
            pitch: 0.5,
            dist: 80.0,
            projection: ProjectionMode::default(),
            frame: crate::view_frame::ViewFrame::default(),
        }
    }
}

impl Camera {
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= f64::from(dx) * 0.01;
        self.pitch = (self.pitch + f64::from(dy) * 0.01).clamp(-1.5, 1.5);
        if !self.yaw.is_finite() {
            self.yaw = self.yaw.rem_euclid(std::f64::consts::TAU);
            if !self.yaw.is_finite() {
                self.yaw = 0.0;
            }
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let scale = self.dist * 0.0015;
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        self.target = [
            self.target[0] - f64::from(dx) * scale * rx + f64::from(dy) * scale * ux,
            self.target[1] - f64::from(dx) * scale * ry + f64::from(dy) * scale * uy,
            self.target[2] - f64::from(dx) * scale * rz + f64::from(dy) * scale * uz,
        ];
        if self.target.iter().any(|t| !t.is_finite()) {
            self.target = [0.0, 0.0, 0.0];
        }
    }

    pub fn zoom(&mut self, wheel_steps: f32) {
        // One wheel click ~= 10% dolly; clamped to sane machining distances.
        let factor = (1.0 - f64::from(wheel_steps) * 0.1).clamp(0.5, 2.0);
        self.dist = (self.dist * factor).clamp(0.5, 100_000.0);
        if !self.dist.is_finite() {
            self.dist = 80.0;
        }
    }

    /// Ray through an NDC cursor position (x right, y up, -1..1):
    /// (origin, normalized direction), or None on degenerate math.
    /// In [`ProjectionMode::Orthographic`] the ray is parallel to the
    /// view axis and its origin is the cursor's point on the eye plane
    /// - the mirror of the ortho box [`Camera::mvp`] draws, so a pick
    ///   through the same matrix hits what the screen shows (#434).
    #[must_use]
    pub fn ray_through(&self, ndc: [f32; 2], aspect: f64) -> Option<([f64; 3], [f64; 3])> {
        let (fx, fy, fz) = self.fwd();
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        let g = (std::f64::consts::FRAC_PI_6).tan(); // half-height at d = 1
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        let (nx, ny) = (f64::from(ndc[0]), f64::from(ndc[1]));
        if self.projection == ProjectionMode::Orthographic {
            let h = self.ortho_half_height();
            let w = h * aspect;
            let e = self.eye();
            let origin = [
                e[0] + nx * w * rx + ny * h * ux,
                e[1] + nx * w * ry + ny * h * uy,
                e[2] + nx * w * rz + ny * h * uz,
            ];
            let dir = [fx, fy, fz];
            if !origin.iter().all(|v| v.is_finite())
                || !dir.iter().all(|v| v.is_finite())
                || dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2] < 1e-24
            {
                return None;
            }
            return Some((origin, dir));
        }
        let dir = [
            fx + nx * g * aspect * rx + ny * g * ux,
            fy + nx * g * aspect * ry + ny * g * uy,
            fz + nx * g * aspect * rz + ny * g * uz,
        ];
        let len2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        if !len2.is_finite() || len2 < 1e-24 {
            return None;
        }
        let inv = 1.0 / len2.sqrt();
        let dir = [dir[0] * inv, dir[1] * inv, dir[2] * inv];
        let origin = self.eye();
        if !dir.iter().all(|v| v.is_finite()) || !origin.iter().all(|v| v.is_finite()) {
            return None;
        }
        Some((origin, dir))
    }

    /// Dolly toward the cursor, keeping the world point under it fixed.
    /// `ndc` is the cursor in normalized device coords (x right, y up,
    /// both -1..1) and `aspect` the viewport width/height. Returns the new
    /// target, or None on a degenerate ray (caller keeps plain behavior).
    /// In [`ProjectionMode::Orthographic`] the fixed point is the
    /// cursor's own point on the target plane - ortho's parallel rays
    /// have no "closest point to target", and shrinking `dist` shrinks
    /// the ortho box by the same factor the target moves, so the point
    /// under the cursor is fixed exactly (#434).
    pub fn dolly_cursor(
        &mut self,
        ndc: [f32; 2],
        aspect: f64,
        wheel_steps: f32,
    ) -> Option<[f64; 3]> {
        let (fx, fy, fz) = self.fwd();
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        let g = (std::f64::consts::FRAC_PI_6).tan(); // half-height at d = 1
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        let (nx, ny) = (f64::from(ndc[0]), f64::from(ndc[1]));
        if !nx.is_finite() || !ny.is_finite() {
            return None;
        }
        let (tx, ty, tz) = (self.target[0], self.target[1], self.target[2]);
        let (px, py, pz) = if self.projection == ProjectionMode::Orthographic {
            let h = self.ortho_half_height();
            let w = h * aspect;
            (
                tx + nx * w * rx + ny * h * ux,
                ty + nx * w * ry + ny * h * uy,
                tz + nx * w * rz + ny * h * uz,
            )
        } else {
            let (ex, ey, ez) = (self.eye()[0], self.eye()[1], self.eye()[2]);
            let dir = [
                fx + nx * g * aspect * rx + ny * g * ux,
                fy + nx * g * aspect * ry + ny * g * uy,
                fz + nx * g * aspect * rz + ny * g * uz,
            ];
            // Closest point on the ray to the current target.
            let (wx, wy, wz) = (tx - ex, ty - ey, tz - ez);
            let dd = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
            if !dd.is_finite() || dd <= 1e-12 {
                return None;
            }
            let s = (wx * dir[0] + wy * dir[1] + wz * dir[2]) / dd;
            (ex + dir[0] * s, ey + dir[1] * s, ez + dir[2] * s)
        };
        let old_dist = self.dist;
        self.zoom(wheel_steps);
        let k = self.dist / old_dist;
        if !k.is_finite() || k <= 0.0 {
            return None;
        }
        self.target = [px + (tx - px) * k, py + (ty - py) * k, pz + (tz - pz) * k];
        if self.target.iter().any(|t| !t.is_finite()) {
            return None;
        }
        Some(self.target)
    }

    /// Center the camera on a bounding sphere.
    #[must_use]
    pub fn frame(center: [f64; 3], radius: f64) -> Camera {
        Camera {
            target: center,
            yaw: 0.9,
            pitch: 0.5,
            dist: (radius * 3.2).clamp(1.0, 50_000.0),
            projection: ProjectionMode::default(),
            frame: crate::view_frame::ViewFrame::default(),
        }
    }

    /// The same camera in orbit frame `frame`, at that frame's home
    /// (yaw, pitch) - the view a newly mounted machine kind opens on
    /// (#1993). Target, distance and projection are kept.
    #[must_use]
    pub fn with_frame(&self, frame: crate::view_frame::ViewFrame) -> Camera {
        let (yaw, pitch) = frame.home();
        Camera { frame, ..self.oriented(yaw, pitch) }
    }

    /// Look at the same target, from the same distance, along the world
    /// direction `d` (the eye stands at `target + dist * d`): the view-cube
    /// law of [`crate::egui_app::viewcube`], and the one place a
    /// standard orientation turns yaw/pitch. Non-finite `d` is a no-op
    /// that answers `self` - the cube's classification is finite by
    /// construction, so this is the Prime Directive's belt, not a path.
    /// The straight-up/straight-down cases need a special yaw: `d.x`,
    /// `d.y` vanish there, so `atan2` answers 0 and every pole click
    /// would land on one canonical roll, spinning a yawed approach up to
    /// half a turn on screen (#622). Instead the pole takes the *pre-
    /// click* yaw snapped to the nearest quarter turn - the roll the view
    /// had, kept; `basis_right`'s zero-guard then evaluates that yaw, so
    /// a pole view rolls like its yaw and a second pole click changes
    /// nothing (`docs/decisions/622-viewcube-roll-snap.md`).
    #[must_use]
    pub fn toward(&self, d: [f64; 3]) -> Camera {
        if d.iter().any(|v| !v.is_finite()) {
            return *self;
        }
        let mut n = d;
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        // Degenerate or non-finite length is a no-op (a finite `d` can
        // overflow its squares to infinity, never to NaN).
        if l <= f64::EPSILON || l.is_nan() {
            return *self;
        }
        n = [n[0] / l, n[1] / l, n[2] / l];
        // The cube names world directions; yaw and pitch live in the
        // orbit frame (#1993).
        let (a, b, c) = self.frame.to_local((n[0], n[1], n[2]));
        n = [a, b, c];
        // At an exact Z pole the atan2 vanishes; take the roll from the
        // yaw this camera already had, snapped to the nearest quarter
        // turn, so the click levels the view without turning it (#622).
        let quarter = std::f64::consts::FRAC_PI_2;
        let yaw = if n[2].abs() == 1.0 {
            (self.yaw / quarter).round() * quarter
        } else {
            n[1].atan2(n[0])
        };
        Camera {
            target: self.target,
            yaw,
            pitch: n[2].clamp(-1.0, 1.0).asin(),
            dist: self.dist,
            projection: self.projection,
            frame: self.frame,
        }
    }

    /// The same camera at an exact (yaw, pitch) - the house isometric the
    /// frame default keeps, spelled for the nav bar.
    #[must_use]
    pub fn oriented(&self, yaw: f64, pitch: f64) -> Camera {
        Camera {
            target: self.target,
            yaw,
            pitch,
            dist: self.dist,
            projection: self.projection,
            frame: self.frame,
        }
    }

    /// The camera's orthonormal world basis (right, up, forward) - the
    /// rotation a view-cube projection is drawn through, and the frame
    /// an edge or corner hit is classified in.
    #[must_use]
    pub fn view_basis(&self) -> (UnitVec, UnitVec, UnitVec) {
        (self.basis_right(), self.basis_up(), self.fwd())
    }

    #[must_use]
    pub(crate) fn eye(&self) -> [f64; 3] {
        let cp = self.pitch.cos();
        let (ox, oy, oz) = self.frame.to_world((
            self.dist * cp * self.yaw.cos(),
            self.dist * cp * self.yaw.sin(),
            self.dist * self.pitch.sin(),
        ));
        [self.target[0] + ox, self.target[1] + oy, self.target[2] + oz]
    }

    /// Camera position as f32, for the shader's specular term.
    #[must_use]
    pub fn eye_f32(&self) -> [f32; 3] {
        let finite = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
        std::array::from_fn(|i| finite(self.eye()[i]))
    }

    /// Forward from eye toward target, normalized.
    #[must_use]
    fn fwd(&self) -> (f64, f64, f64) {
        let e = self.eye();
        normalize([self.target[0] - e[0], self.target[1] - e[1], self.target[2] - e[2]])
    }

    /// Right = normalize(cross(forward, up)), with up the orbit frame's
    /// local +Z (world Z on a mill, #1993); at an exact pole the cross
    /// vanishes, and the guard evaluates the same expression at the camera's
    /// (quarter-snapped, see `toward`) yaw instead - the limit every
    /// near-pole view already rides (#622).
    #[must_use]
    fn basis_right(&self) -> (f64, f64, f64) {
        let (fx, fy, _) = self.frame.to_local(self.fwd());
        let (x, y, _) = normalize([fy, -fx, 0.0]);
        if x == 0.0 && y == 0.0 {
            // looking straight along the frame's up axis
            return self.frame.to_world((-self.yaw.sin(), self.yaw.cos(), 0.0));
        }
        self.frame.to_world((x, y, 0.0))
    }

    /// Up = cross(right, forward).
    #[must_use]
    fn basis_up(&self) -> (f64, f64, f64) {
        let f = {
            let (x, y, z) = self.fwd();
            [x, y, z]
        };
        let (rx, ry, rz) = self.basis_right();
        normalize([ry * f[2] - rz * f[1], rz * f[0] - rx * f[2], rx * f[1] - ry * f[0]])
    }

    /// Column-major model-view-projection (right-handed, camera looks -Z).
    #[must_use]
    pub fn mvp(&self, aspect: f64) -> [[f32; 4]; 4] {
        let e = self.eye();
        let (f0, f1, f2) = self.fwd();
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        let view = [
            [rx, ux, -f0, 0.0],
            [ry, uy, -f1, 0.0],
            [rz, uz, -f2, 0.0],
            [
                -(rx * e[0] + ry * e[1] + rz * e[2]),
                -(ux * e[0] + uy * e[1] + uz * e[2]),
                f0 * e[0] + f1 * e[1] + f2 * e[2],
                1.0,
            ],
        ];
        let near = 0.1f64;
        let far = (self.dist * 40.0).max(1000.0);
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        let proj = if self.projection == ProjectionMode::Orthographic {
            // Parallel box (#434): half-height is what perspective sees at
            // the target plane, so `dist` keeps spelling zoom and the
            // mode switch never jumps the framing. Depth is linear and
            // clip.w is 1 everywhere, so every screen-space consumer that
            // divides by w degenerates cleanly onto this matrix too.
            // The depth row spans near -> 0.0 and far -> 1.0, NOT the
            // OpenGL [-1, +1] span: WebGPU clips on 0 <= z <= w, and with
            // clip.w held at 1 the OpenGL span puts every point nearer
            // than (near + far) / 2 - everything in view - at negative
            // clip z and blanks the viewport (#600).
            let h = self.ortho_half_height();
            let w = h * aspect;
            [
                [1.0 / w, 0.0, 0.0, 0.0],
                [0.0, 1.0 / h, 0.0, 0.0],
                [0.0, 0.0, 1.0 / (near - far), 0.0],
                [0.0, 0.0, near / (near - far), 1.0],
            ]
        } else {
            let g = (std::f64::consts::FRAC_PI_6).tan().recip(); // 60 deg vertical fov
            [
                [g / aspect, 0.0, 0.0, 0.0],
                [0.0, g, 0.0, 0.0],
                [0.0, 0.0, (far + near) / (near - far), -1.0],
                [0.0, 0.0, 2.0 * far * near / (near - far), 0.0],
            ]
        };
        mat_mul(proj, view)
    }

    /// Orthographic view half-height: the height the perspective cone
    /// sees at the target plane, so both modes frame identically at the
    /// same `dist` (#434). `dist` is clamped >= 0.5 by `zoom`, so this
    /// never vanishes.
    #[must_use]
    pub(crate) fn ortho_half_height(&self) -> f64 {
        self.dist * (std::f64::consts::FRAC_PI_6).tan()
    }
}

fn normalize(v: [f64; 3]) -> (f64, f64, f64) {
    let l = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if l <= f64::EPSILON {
        return (0.0, 0.0, 0.0);
    }
    let l = l.sqrt();
    (v[0] / l, v[1] / l, v[2] / l)
}

/// Wireframe of a triangle soup: boundary edges and creases only, facet
/// seams inside a flat or smoothly-bent run stay hidden. The rule itself
/// lives in the app layer (`geometry::feature`, shared with M1.5 face and
/// chain topology); this is the adapter into the GPU index list. Welding
/// is per part, as the caller splits the soup that way.
fn feature_edges(verts: &[[f32; 3]]) -> Vec<u32> {
    let pts: Vec<[f64; 3]> = verts
        .iter()
        .map(|v| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])])
        .collect();
    camcraft_app::geometry::feature::feature_edges(&pts)
        .into_iter()
        .flat_map(|e| [e.i, e.j])
        .collect()
}

/// Column-major matrix product: `out[c][r] = sum_k a[k][r] * b[c][k]`.
/// Non-finite results become 0 rather than poisoning the whole matrix.
fn mat_mul(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f32; 4]; 4] {
    std::array::from_fn(|c| {
        std::array::from_fn(|r| {
            let mut s = 0.0f64;
            for k in 0..4 {
                s += a[k][r] * b[c][k];
            }
            if s.is_finite() { s as f32 } else { 0.0 }
        })
    })
}

/// The upload order: models, then stock, then the overlay stock (which
/// the traced stock never replaces), then region fills; and the index in
/// that order where the overlay stock starts.
pub(crate) fn draw_order(parts: &[GpuPart]) -> (Vec<&GpuPart>, usize) {
    let mut order: Vec<&GpuPart> = parts.iter().filter(|p| !p.stock && !p.fill).collect();
    order.extend(parts.iter().filter(|p| p.stock && !p.fill && !is_overlay(p)));
    let overlay_from = order.len();
    order.extend(parts.iter().filter(|p| p.stock && !p.fill && is_overlay(p)));
    order.extend(parts.iter().filter(|p| p.fill));
    (order, overlay_from)
}

/// An overlay stock part: drawn in the stock pass but never replaced by
/// the traced stock (today only the incoming stock, #406).
pub(crate) fn is_overlay(p: &GpuPart) -> bool {
    p.classes.first().is_some_and(|c| (*c - crate::incoming_view::INCOMING_CLASS).abs() < 0.5)
}
