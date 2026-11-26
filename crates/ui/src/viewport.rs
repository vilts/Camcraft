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

mod pipes;
mod wide;

pub use wide::{
    GUARANTEED_SAMPLES, LIFT_WIDTHS, MAX_LINE_PX, MIN_LINE_PX, SEGMENT_BYTES, effective_samples, line_px,
    offered_samples, segment_bytes,
};

/// vec3 position + f32 selection flag + f32 deviation class. Like `sel`,
/// the class rides baked in the vertex stream (same submit-time law as
/// above) and every `vs_main` pipeline reads all three attributes - the
/// layouts are one const, so a pipeline can never forget the third.
const VERTEX_STRIDE: u64 = 20;

/// The one vertex-buffer law, shared by every solid pipeline.
pub(crate) const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
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

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// `struct U`: mvp + eye + wire (80 bytes), six palette vec4s, the
/// wireframe edge ink of the live ground (#698), then the overlay line
/// geometry `vp` (#2958: target size, half width, half width + 1 px).
const UNIFORM_SIZE: u64 = 80 + 8 * 16;

const SHADER: &str = "
// The verdict / selection palette rides the uniform (M13.4's
// colour-vision remap; Off = the literals these fields replaced).
struct U { mvp: mat4x4<f32>, eye: vec3<f32>, wire: f32,
           clean: vec4<f32>, rest: vec4<f32>, gouge: vec4<f32>,
           unjudged: vec4<f32>, hot: vec4<f32>, warm: vec4<f32>,
           wire_ink: vec4<f32>, vp: vec4<f32> };
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
// `open` scales the light: 1 on the plain face pass, the baked occlusion
// value of the vertex on the occluded face pass (#2834, `shade_pass`).
fn lit(world: vec3<f32>, sel: f32, cls: f32, open: f32) -> vec4<f32> {
    let cr = cross(dpdx(world), dpdy(world));
    let len = length(cr);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), cr / len, len > 1e-9);
    let key_dir = normalize(vec3<f32>(0.45, 0.60, 0.66));
    // abs(): two-sided, so flipped windings never render unlit-black.
    let key = abs(dot(n, key_dir));
    let fill = abs(dot(n, normalize(vec3<f32>(-0.55, 0.25, 0.40))));
    let light = (0.34 + 0.52 * key + 0.18 * fill) * open;
    let to_eye = normalize(u.eye - world);
    let halfv = normalize(key_dir + to_eye);
    let spec = pow(max(abs(dot(n, halfv)), 0.0), 24.0) * 0.35 * key * open;
    let base = vec3<f32>(0.66, 0.70, 0.76) * light + vec3<f32>(spec);
    let hot = vec3<f32>(0.98, 0.72, 0.20) * light + vec3<f32>(spec);
    // #339 slice 4: a fixture the selected op strikes wears the gouge
    // colour (class STRUCK_CLASS; plain models carry class 0).
    let struck = step(3.5, cls);
    let hit = u.gouge.xyz * light + vec3<f32>(spec);
    return vec4<f32>(mix(mix(base, hot, sel), hit, struck * 0.85), 1.0);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    return lit(in.world, in.sel, in.cls, 1.0);
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

// Hidden edges (#2818): the same wire where it lies behind a face, blended
// thin over the lit metal so that it reads lighter than the front wire.
@fragment
fn fs_edge_hidden(in: VSOut) -> @location(0) vec4<f32> {
    let dark = vec3<f32>(0.06, 0.07, 0.09);
    return vec4<f32>(mix(dark, u.warm.xyz, in.sel), 0.5);
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

";

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
    /// Every pipeline of the pass, built for `samples` (#2958).
    pipes: pipes::Pipes,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    /// Samples per pixel of the pass targets: 1 draws straight into
    /// `color_view`; more renders into `msaa_view` and resolves there.
    samples: u32,
    /// The overlay line width in target pixels (#2958).
    line_px: f32,
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
    /// Overlay segments (instances), not vertices (#2958).
    overlay_count: u32,
    /// The ground shadow pass and the occlusion pass (#2834).
    shade: crate::shade_pass::ShadePass,
    _msaa_tex: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
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
            source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{}{}", wide::WIDE_SHADER, crate::shade_pass::WGSL).into()),
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

        let pipes = pipes::build(device, &shader, &layout, format, 1);

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

        let targets = pipes::make_targets(device, format, width, height, 1);
        // #2834 + #2958: the shade pipelines draw in this pass, so they take
        // its sample count (1 until `set_samples`).
        let shade = crate::shade_pass::ShadePass::new(device, &shader, &layout, format, 1);

        View3D {
            pipes,
            shader,
            layout,
            format,
            samples: 1,
            line_px: line_px(1.5, 1.0),
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
            shade,
            _msaa_tex: targets.msaa,
            msaa_view: targets.msaa_view,
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
        let targets = pipes::make_targets(device, format, width, height, self.samples);
        self._color_tex = targets.color;
        self.color_view = targets.color_view;
        self._depth_tex = targets.depth;
        self.depth_view = targets.depth_view;
        self._msaa_tex = targets.msaa;
        self.msaa_view = targets.msaa_view;
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
        // #2958: one 32-byte instance per segment, drawn as a wide quad.
        let data = segment_bytes(lines);
        if data.is_empty() {
            self.overlay_count = 0; // render() then skips the draw entirely
            return;
        }
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-obuf"),
            size: data.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buf, 0, &data);
        self.overlay_buf = buf;
        self.overlay_count = u32::try_from(lines.len() / 2).unwrap_or(u32::MAX);
    }

    /// The cached index partition that the draw list reads (#2818).
    #[must_use]
    pub const fn counts(&self) -> crate::draw_list::Counts {
        crate::draw_list::Counts {
            index_count: self.index_count,
            model_index_count: self.model_index_count,
            stock_index_end: self.stock_index_end,
            overlay_index_start: self.overlay_index_start,
            edge_count: self.edge_count,
            model_edge_count: self.model_edge_count,
            shadow_vertex_count: self.shade.shadow_count,
            occlusion_vertex_count: self.shade.occlusion_count,
        }
    }

    /// Replace the cached shade data (#2834): the ground shadow mesh and
    /// the occlusion values of the model vertices of the last
    /// `set_document`. Call it with each new document.
    pub fn set_shade(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, shade: &crate::shade_pass::Shade) {
        self.shade.set(device, queue, shade);
    }

    /// The shading switches that the next `render` reads (#2834). They pick
    /// draw list steps; they do not touch a buffer.
    pub fn set_effects(&mut self, effects: crate::draw_list::Effects) {
        self.shade.effects = effects;
    }

    /// The overlay segment count of the last upload.
    #[must_use]
    pub const fn overlay_segments(&self) -> u32 {
        self.overlay_count
    }

    /// #2958: the overlay line width in target pixels (`line_px` holds it
    /// to `MIN_LINE_PX..=MAX_LINE_PX`). A uniform: nothing re-uploads.
    pub fn set_line_px(&mut self, px: f32) {
        self.line_px = if px.is_finite() { px.clamp(MIN_LINE_PX, MAX_LINE_PX) } else { MIN_LINE_PX };
    }

    #[must_use]
    pub const fn line_width_px(&self) -> f32 {
        self.line_px
    }

    #[must_use]
    pub const fn samples(&self) -> u32 {
        self.samples
    }

    /// The sample count of the ground shadow and occluded face pipelines
    /// (#2834). They draw in the 3D pass, so it is always `samples()`.
    #[must_use]
    pub const fn shade_samples(&self) -> u32 {
        self.shade.samples()
    }

    /// #2958: rebuild the pipelines and the multisample targets for
    /// `samples` per pixel. The resolved colour target is kept, so the
    /// shell's egui texture stays bound. The caller passes only a count
    /// the device offers (`effective_samples`). True when it changed.
    pub fn set_samples(&mut self, device: &wgpu::Device, samples: u32) -> bool {
        let samples = samples.max(1);
        if samples == self.samples {
            return false;
        }
        self.pipes = pipes::build(device, &self.shader, &self.layout, self.format, samples);
        // #2834: the shadow and the occluded faces draw in the same pass.
        self.shade.rebuild(device, &self.shader, &self.layout, self.format, samples);
        let (msaa, depth) = pipes::make_sample_targets(device, self.format, self.width, self.height, samples);
        self.msaa_view = msaa.as_ref().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
        self._msaa_tex = msaa;
        self.depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        self._depth_tex = depth;
        self.samples = samples;
        true
    }

    /// The verdict/selection palette the next `render` uploads (M13.4's
    /// colour-vision remap). A uniform, not a vertex re-bake: a mode
    /// switch re-colours without touching the geometry buffers.
    pub fn set_palette(&mut self, p: &crate::egui_app::colorvision::Semantic) {
        self.palette = *p;
    }

    /// Encode the 3D pass into the offscreen target. `eye` is the camera
    /// position in world space, for the specular term; `display` selects
    /// the display mode (the buffers are identical in each mode - only the
    /// draw list, `crate::draw_list::plan`, and the `u.wire` flag differ).
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
        let wireframe = display.is_wireframe();
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
        // #2958: the overlay geometry - target size, half width, and the
        // quad's reach (half width plus the one-pixel fade).
        let hw = self.line_px * 0.5;
        for v in [self.width as f32, self.height as f32, hw, hw + 1.0] {
            data[o..o + 4].copy_from_slice(&v.to_le_bytes());
            o += 4;
        }
        queue.write_buffer(&self.uniform, 0, &data);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewport3d-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.msaa_view.as_ref().unwrap_or(&self.color_view),
                resolve_target: self.msaa_view.as_ref().map(|_| &self.color_view),
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
        // The order itself is `draw_list::plan` (#2818): one list for each
        // display mode, read from the cached index partition.
        pass.set_bind_group(0, &self.bind_group, &[]);
        use crate::draw_list::Pipe;
        let plan = crate::draw_list::plan_with(display, &self.counts(), stock.is_some(), self.shade.effects);
        if self.index_count > 0 {
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        }
        for step in plan.steps() {
            let pipeline = match step.pipe {
                Pipe::Faces => &self.pipes.pipeline,
                // No occlusion buffer: the plain faces (the list never asks).
                Pipe::FacesOccluded => self.shade.bind_occluded(&mut pass).unwrap_or(&self.pipes.pipeline),
                Pipe::GroundShadow => {
                    self.shade.draw_shadow(&mut pass, step.range.clone());
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    continue;
                }
                Pipe::FacesDepthOnly => &self.pipes.depth_pipeline,
                Pipe::Edges => &self.pipes.line_pipeline,
                Pipe::HiddenEdges => &self.pipes.hidden_line_pipeline,
                Pipe::Stock => &self.pipes.stock_pipeline,
                Pipe::Region => &self.pipes.region_pipeline,
                Pipe::TracedStock => {
                    if let Some(draw) = stock {
                        draw(&mut pass);
                        pass.set_bind_group(0, &self.bind_group, &[]);
                    }
                    continue;
                }
            };
            let indices = if step.pipe.uses_edge_buffer() { &self.edge_buf } else { &self.index_buf };
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(pipeline);
            pass.draw_indexed(step.range.clone(), 0, 0..1);
        }
        if self.overlay_count > 0 {
            pass.set_pipeline(&self.pipes.wide_pipeline);
            pass.set_vertex_buffer(0, self.overlay_buf.slice(..));
            pass.draw(0..6, 0..self.overlay_count);
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

/// The distance behind the target the orthographic camera sits at. Its
/// framing is independent of distance (the half-height is `dist * tan
/// (fov/2)`), so nothing useful comes of pushing the eye toward the
/// model - dollying along the view direction with `dist` only swallowed
/// the model's near faces behind the camera as the zoom shrank. The
/// ortho `dist` is a zoom factor; the eye stays at a nominal distance.
pub(crate) const ORTHO_EYE_DIST: f64 = 100.0;

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
    /// view axis and its origin is the cursor's point on the near plane,
    /// [`Camera::ortho_behind_eye`] behind the eye plane - the mirror of
    /// the ortho box [`Camera::mvp`] draws, so a pick through the same
    /// matrix hits what the screen shows (#434), nearer faces included (#2375).
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
            let back = self.ortho_behind_eye();
            let origin = [
                e[0] + nx * w * rx + ny * h * ux - back * fx,
                e[1] + nx * w * ry + ny * h * uy - back * fy,
                e[2] + nx * w * rz + ny * h * uz - back * fz,
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
        let d = if self.projection == ProjectionMode::Orthographic {
            ORTHO_EYE_DIST
        } else {
            self.dist
        };
        let cp = self.pitch.cos();
        let (ox, oy, oz) = self.frame.to_world((
            d * cp * self.yaw.cos(),
            d * cp * self.yaw.sin(),
            d * self.pitch.sin(),
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
        // The near plane rides the view distance (minimally far from
        // the eye): a fixed 0.1 clipped against any part smaller than
        // half a millimetre near the camera, so zooming magnified cut
        // the near faces away at the surface.
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        // Perspective clips 0.1 mm in front of the eye - enough to kill
        // the near-plane tunnel problem without a visible crop at sane
        // distances. `dist` clamps to 0.5 mm, so the 0.1 mm plane still
        // sits inside the clip volume at every zoom.
        let near = (self.dist * 0.001).clamp(1e-5, 0.1);
        let far = (self.dist * 40.0).max(1000.0);
        let proj = if self.projection == ProjectionMode::Orthographic {
            // The eye stays a fixed nominal distance behind the target,
            // so a near plane in front of it would sit 99.9 mm ahead of
            // the target plane and any nearer geometry would vanish.
            // The near plane reaches far behind the eye plane so nothing
            // inside the visible box clips away.
            let near = -self.ortho_behind_eye();
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

    /// How far the orthographic near plane sits behind the eye plane, mm:
    /// [`Camera::mvp`] clips there and [`Camera::ray_through`] starts there.
    #[must_use]
    pub(crate) fn ortho_behind_eye(&self) -> f64 {
        (self.dist * 20.0).max(1000.0) - ORTHO_EYE_DIST
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
