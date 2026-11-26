//! Wide anti-aliased overlay lines and the viewport's sample count (#2958).
//! A `wgpu` line is always one hardware pixel and is never smoothed, so
//! each overlay segment is drawn as a screen-facing quad: one 32-byte
//! instance per segment, the six corners made in the vertex shader. The
//! fragment shader fades the sides and the ends over one pixel, an end
//! reaches half the width past its point (two segments that meet close
//! the corner and do not overshoot it), and each vertex is lifted toward
//! the eye by a few line widths, so a line that lies on a face is not
//! half buried in that face by the depth test.
//! Provenance: the screen-space line quad and coverage fade are standard
//! real-time rendering practice (public graphics literature); the sample
//! counts follow the WebGPU and wgpu documentation. Our own code.

use egui_wgpu::wgpu;

/// One overlay segment on the GPU: two f32x3 points, two RGBA8 colours.
pub const SEGMENT_BYTES: u64 = 32;

/// The instance layout `vs_wide` reads.
pub(super) const SEGMENT_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: SEGMENT_BYTES,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Unorm8x4],
};

/// How far the lift pulls a vertex toward the eye, in line widths. Three
/// widths keep the full band over a face tilted up to about 79 degrees
/// from the screen; a line that is really behind a face by less than this
/// (a few pixels of depth on the screen) shows through.
pub const LIFT_WIDTHS: f32 = 3.0;

/// The narrowest and widest line the shader draws, in pixels.
pub const MIN_LINE_PX: f32 = 1.0;
pub const MAX_LINE_PX: f32 = 16.0;

/// The sample counts WebGPU guarantees for the viewport formats.
pub const GUARANTEED_SAMPLES: [u32; 2] = [1, 4];

/// Appended to the viewport shader: it shares `struct U` and reads
/// `u.vp` = (target width, target height, half width, half width + 1) px.
pub(super) const WIDE_SHADER: &str = "
struct WideOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rgb: vec3<f32>,
    @location(1) @interpolate(linear) dist: f32,
    @location(2) @interpolate(linear) along: f32,
    @location(3) @interpolate(flat) len: f32,
};

// The world distance one pixel spans at clip depth w: the y row of the
// projection scales world units to clip units (perspective and ortho).
fn world_px(w: f32) -> f32 {
    let p11 = length(vec3<f32>(u.mvp[0].y, u.mvp[1].y, u.mvp[2].y));
    return max(w, 1e-6) / max(u.vp.y * 0.5 * p11, 1e-9);
}

// Toward the eye: along the eye ray in perspective (the point stays on
// its pixel), against the depth gradient in ortho (no w row).
fn to_eye(p: vec3<f32>) -> vec3<f32> {
    let wrow = vec3<f32>(u.mvp[0].w, u.mvp[1].w, u.mvp[2].w);
    let zrow = vec3<f32>(u.mvp[0].z, u.mvp[1].z, u.mvp[2].z);
    let ortho = length(wrow) < 1e-12;
    let toward = select(u.eye - p, -zrow, ortho);
    let len = length(toward);
    return select(vec3<f32>(0.0), toward / len, len > 1e-12);
}

fn lifted(p: vec3<f32>) -> vec4<f32> {
    let c = u.mvp * vec4<f32>(p, 1.0);
    let lift = to_eye(p) * world_px(c.w) * u.vp.w * 3.0;
    return u.mvp * vec4<f32>(p + lift, 1.0);
}

@vertex
fn vs_wide(@builtin(vertex_index) vi: u32,
           @location(0) a: vec3<f32>, @location(1) b: vec3<f32>,
           @location(2) rgb_a: vec4<f32>, @location(3) rgb_b: vec4<f32>) -> WideOut {
    var out: WideOut;
    // Corners 0..5 of two triangles: side -1/+1 across, end a/b along.
    let side = select(-1.0, 1.0, vi == 1u || vi == 2u || vi == 4u);
    let at_b = vi == 2u || vi == 4u || vi == 5u;
    var qa = lifted(a);
    var qb = lifted(b);
    // Clip to the near plane (wgpu clip z >= 0) before the screen
    // direction is taken: an end behind the eye must not flip the quad.
    if (qa.z < 0.0 && qb.z < 0.0) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        out.rgb = vec3<f32>(0.0);
        out.dist = 0.0;
        out.along = 0.0;
        out.len = 0.0;
        return out;
    }
    if (qa.z < 0.0) { qa = mix(qa, qb, qa.z / (qa.z - qb.z)); }
    if (qb.z < 0.0) { qb = mix(qb, qa, qb.z / (qb.z - qa.z)); }
    let half_vp = u.vp.xy * 0.5;
    let sa = qa.xy / qa.w * half_vp;
    let sb = qb.xy / qb.w * half_vp;
    var d = sb - sa;
    let len = length(d);
    if (len < 1e-6) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let ext = u.vp.w;
    let me = select(qa, qb, at_b);
    let along = select(-d, d, at_b) * ext;
    let off = (n * side * ext + along) / half_vp * me.w;
    out.clip = me + vec4<f32>(off, 0.0, 0.0);
    out.rgb = select(rgb_a.rgb, rgb_b.rgb, at_b);
    out.dist = side * ext;
    out.along = select(-ext, len + ext, at_b);
    out.len = len;
    return out;
}

// Coverage: one pixel of fade at the sides and past each end; the end
// reaches half the width past its point.
@fragment
fn fs_wide(in: WideOut) -> @location(0) vec4<f32> {
    let hw = u.vp.z;
    let side = clamp(hw + 0.5 - abs(in.dist), 0.0, 1.0);
    let beyond = max(-in.along, in.along - in.len);
    let cap = clamp(hw + 0.5 - beyond, 0.0, 1.0);
    return vec4<f32>(in.rgb, side * cap);
}
";

/// The overlay vertex pairs (`[x, y, z, r, g, b]`, two per segment) as
/// instance bytes: the points as f32, each end's colour as RGBA8. A
/// trailing odd vertex is dropped (never half a line).
#[must_use]
pub fn segment_bytes(lines: &[[f32; 6]]) -> Vec<u8> {
    let rgba = |v: &[f32; 6]| -> [u8; 4] {
        // `as u8` saturates and maps NaN to 0.
        let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        [c(v[3]), c(v[4]), c(v[5]), 255]
    };
    let mut out = Vec::with_capacity(lines.len() / 2 * SEGMENT_BYTES as usize);
    let (segments, _odd) = lines.as_chunks::<2>();
    for [a, b] in segments {
        for x in a[..3].iter().chain(&b[..3]) {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&rgba(a));
        out.extend_from_slice(&rgba(b));
    }
    out
}

/// The line width in target pixels for `points` at `scale` pixels per
/// point, held to `MIN_LINE_PX..=MAX_LINE_PX` (a bad scale draws 1 px).
#[must_use]
pub fn line_px(points: f32, scale: f64) -> f32 {
    let px = f64::from(points) * scale;
    if px.is_finite() { (px as f32).clamp(MIN_LINE_PX, MAX_LINE_PX) } else { MIN_LINE_PX }
}

/// The sample count to build: `want` when the device offers it, else the
/// highest offered count below it (never more memory than was asked for),
/// else 1.
#[must_use]
pub fn effective_samples(want: u32, offered: &[u32]) -> u32 {
    offered.iter().copied().filter(|n| *n <= want).max().unwrap_or(1)
}

/// The sample counts `adapter` offers for the colour `format` and the
/// depth target together. Without the adapter-specific format feature on
/// the device, only WebGPU's guaranteed pair is valid.
#[must_use]
pub fn offered_samples(adapter: &wgpu::Adapter, device_features: wgpu::Features, format: wgpu::TextureFormat) -> Vec<u32> {
    if !device_features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES) {
        return GUARANTEED_SAMPLES.to_vec();
    }
    let both = |n: u32| {
        [format, super::DEPTH_FORMAT]
            .iter()
            .all(|f| adapter.get_texture_format_features(*f).flags.sample_count_supported(n))
    };
    let mut out: Vec<u32> = [1, 2, 4].into_iter().filter(|n| *n == 1 || both(*n)).collect();
    if !out.contains(&4) {
        // WebGPU guarantees 4 for these formats; keep it offered.
        out.push(4);
    }
    out
}

#[cfg(test)]
#[path = "wide_tests.rs"]
mod tests;
