// The preview march on the GPU (M7.2 B): camcraft_sim's
// `stock_sim::preview::march` laws, ported term for term. Provenance:
// Hart's sphere tracing (SIGGRAPH 1996) over the packed lattice
// (`preview::pack`); nothing from vendor binaries. Every constant here
// names its Rust twin - a change on one side is a parity-test failure
// on the other.

struct U {
    eye: vec4<f32>,      // xyz eye, w tan(half fov)
    fwd: vec4<f32>,      // xyz, w eps (cell * STEP_EPS)
    right: vec4<f32>,    // xyz, w stock cell
    up: vec4<f32>,       // xyz, w cap
    solid_lo: vec4<f32>, // xyz, w class tol (already gate-widened)
    solid_hi: vec4<f32>, // xyz, w has_target (0 / 1)
    world_lo: vec4<f32>, // xyz, w ortho half height (0 = perspective)
    world_hi: vec4<f32>,
    origin: vec4<i32>,   // stock lattice texel (0,0,0), w frame width
    dims: vec4<i32>,     // stock lattice extent, w frame height
    t_origin: vec4<i32>, // target lattice
    t_dims: vec4<i32>,
    t_cell: vec4<f32>,   // x target cell
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> mesh: array<f32>;
@group(0) @binding(2) var<storage, read> cut: array<f32>;
@group(0) @binding(3) var<storage, read> tgt: array<f32>;
// Per pixel: depth (t along the unit ray), class, shade, status
// (0 empty, 1 hit, 2 unreadable). Depth is -1 off a hit: WGSL has no
// portable NaN, so the host turns status into the NaN law.
@group(0) @binding(4) var<storage, read_write> out: array<vec4<f32>>;

const MAX_STEPS: i32 = 256; // march::MAX_STEPS (#3612: 64 holed grazing floors)
const BISECT: i32 = 6;      // march::BISECT
const RELAX: f32 = 0.9;     // march::RELAX
const LIGHT: vec3<f32> = vec3<f32>(-0.35, -0.5, 0.79); // march::LIGHT

// Texel at a cell key, clamped into the lattice (pack::slot).
fn texel(layer: u32, k: vec3<i32>) -> f32 {
    if (layer == 2u) {
        let d = u.t_dims.xyz;
        let i = clamp(k - u.t_origin.xyz, vec3<i32>(0), d - vec3<i32>(1));
        return tgt[u32((i.z * d.y + i.y) * d.x + i.x)];
    }
    let d = u.dims.xyz;
    let i = clamp(k - u.origin.xyz, vec3<i32>(0), d - vec3<i32>(1));
    let s = u32((i.z * d.y + i.y) * d.x + i.x);
    if (layer == 0u) {
        return mesh[s];
    }
    return cut[s];
}

// pack::trilinear: centers at (i + 0.5) * cell, blend z, then x, then y.
fn tri(layer: u32, p: vec3<f32>, cell: f32) -> f32 {
    let f = p / cell - vec3<f32>(0.5);
    let fl = floor(f);
    let i = vec3<i32>(fl);
    let w = f - fl;
    let m00 = mix(texel(layer, i + vec3<i32>(0, 0, 0)), texel(layer, i + vec3<i32>(0, 0, 1)), w.z);
    let m10 = mix(texel(layer, i + vec3<i32>(1, 0, 0)), texel(layer, i + vec3<i32>(1, 0, 1)), w.z);
    let m01 = mix(texel(layer, i + vec3<i32>(0, 1, 0)), texel(layer, i + vec3<i32>(0, 1, 1)), w.z);
    let m11 = mix(texel(layer, i + vec3<i32>(1, 1, 0)), texel(layer, i + vec3<i32>(1, 1, 1)), w.z);
    return mix(mix(m00, m10, w.x), mix(m01, m11, w.x), w.y);
}

// march::Eye::dist - blend each layer, then the max law.
fn dist(p: vec3<f32>) -> f32 {
    let cell = u.right.w;
    return max(tri(0u, p, cell), tri(1u, p, cell));
}

// march::outside_gap: the exact empty-ball radius outside the solid box.
fn outside_gap(p: vec3<f32>) -> f32 {
    let q = max(u.solid_lo.xyz - p, vec3<f32>(0.0)) + max(p - u.solid_hi.xyz, vec3<f32>(0.0));
    return length(q);
}

// march::trace. Returns (t, status).
fn trace(o: vec3<f32>, d: vec3<f32>) -> vec2<f32> {
    var t0 = -3.0e38;
    var t1 = 3.0e38;
    for (var a = 0; a < 3; a = a + 1) {
        let od = d[a];
        let oo = o[a];
        let lo = u.world_lo[a];
        let hi = u.world_hi[a];
        if (abs(od) < 1e-12) {
            if (oo < lo || oo > hi) {
                return vec2<f32>(0.0, 0.0);
            }
            continue;
        }
        let inv = 1.0 / od;
        let ta = (lo - oo) * inv;
        let tb = (hi - oo) * inv;
        t0 = max(t0, min(ta, tb));
        t1 = min(t1, max(ta, tb));
    }
    if (t1 < max(t0, 0.0)) {
        return vec2<f32>(0.0, 0.0);
    }
    let eps = u.fwd.w;
    let cell = u.right.w;
    var t = max(t0, 0.0);
    for (var s = 0; s < MAX_STEPS; s = s + 1) {
        let p = o + d * t;
        let phi = dist(p);
        if (phi != phi) {
            return vec2<f32>(0.0, 2.0);
        }
        if (phi <= eps) {
            // The hit law: material at the point or one cell deeper.
            if (phi < 0.0 || dist(p + d * cell) < 0.0) {
                return vec2<f32>(t, 1.0);
            }
            t = t + eps * 4.0;
            if (t > t1) {
                return vec2<f32>(0.0, 0.0);
            }
            continue;
        }
        let nt = t + max(phi, outside_gap(p)) * RELAX;
        if (nt <= t) {
            return vec2<f32>(0.0, 2.0);
        }
        if (nt > t1) {
            return vec2<f32>(0.0, 0.0);
        }
        t = nt;
    }
    return exhausted(o, d, t, cell);
}

// march::exhausted (#3612): the budget ran out at t. A hit stands only
// where material is proved on the ray (the last sample, or the hit
// law's probe one cell on, inside); bisect that bracket to the surface.
// Any other ray keeps status 2: a near miss is never painted.
fn exhausted(o: vec3<f32>, d: vec3<f32>, t: f32, cell: f32) -> vec2<f32> {
    let phi = dist(o + d * t);
    if (phi != phi) {
        return vec2<f32>(0.0, 2.0);
    }
    if (phi < 0.0) {
        return vec2<f32>(t, 1.0);
    }
    var hi = t + cell;
    let ahead = dist(o + d * hi);
    if (ahead != ahead || ahead >= 0.0) {
        return vec2<f32>(0.0, 2.0);
    }
    var lo = t;
    for (var b = 0; b < BISECT; b = b + 1) {
        let mid = 0.5 * (lo + hi);
        if (dist(o + d * mid) < 0.0) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    return vec2<f32>(hi, 1.0);
}

// march::shade: gradient normal, one-march ambient, lambert.
fn shade(p: vec3<f32>) -> f32 {
    let cell = u.right.w;
    let eps = u.fwd.w;
    let h = cell * 0.5;
    let g = vec3<f32>(
        dist(p + vec3<f32>(h, 0.0, 0.0)) - dist(p - vec3<f32>(h, 0.0, 0.0)),
        dist(p + vec3<f32>(0.0, h, 0.0)) - dist(p - vec3<f32>(0.0, h, 0.0)),
        dist(p + vec3<f32>(0.0, 0.0, h)) - dist(p - vec3<f32>(0.0, 0.0, h)),
    );
    let gl = length(g);
    if (!(gl > 1e-20)) {
        return 0.35;
    }
    let n = g / gl;
    var ao = 1.0;
    let span = 4.0 * cell;
    var s = cell * 0.1 + 2.0 * eps;
    for (var i = 0; i < 16; i = i + 1) {
        let d = dist(p + n * s);
        if (d != d) {
            break;
        }
        if (d <= eps) {
            ao = 1.0 - clamp(s / span, 0.0, 1.0);
            break;
        }
        s = s + max(d, cell * 0.01);
        if (s > span) {
            break;
        }
    }
    let lambert = clamp(dot(n, normalize(LIGHT)), 0.0, 1.0);
    return (0.18 + 0.82 * lambert) * (0.7 + 0.3 * ao);
}

// preview::deviation on the target's lattice: the GPU cannot afford
// the exact facet scan, and near the surface the band IS exact.
fn deviation(p: vec3<f32>) -> f32 {
    if (u.solid_hi.w < 0.5) {
        return 3.0;
    }
    let v = tri(2u, p, u.t_cell.x);
    if (v != v) {
        return 3.0;
    }
    let tol = u.solid_lo.w;
    if (v > tol) {
        return 1.0;
    }
    if (v < -tol) {
        return 2.0;
    }
    return 0.0;
}

@compute @workgroup_size(8, 8, 1)
fn cs_trace(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = u32(u.origin.w);
    let h = u32(u.dims.w);
    if (gid.x >= w || gid.y >= h) {
        return;
    }
    // PreviewCamera::ray - the one ray law both paths walk: the eye's
    // cone, or (#609) parallel rays from the eye plane in orthographic.
    let uu = 2.0 * ((f32(gid.x) + 0.5) / f32(w)) - 1.0;
    let vv = 1.0 - 2.0 * ((f32(gid.y) + 0.5) / f32(h));
    let oh = u.world_lo.w;
    var origin = u.eye.xyz;
    var dir = u.fwd.xyz;
    if (oh > 0.0) {
        origin = u.eye.xyz + u.right.xyz * (uu * oh * (f32(w) / f32(h))) + u.up.xyz * (vv * oh);
    } else {
        let tanh = u.eye.w;
        let a = tanh * (f32(w) / f32(h));
        dir = normalize(u.fwd.xyz + u.right.xyz * (uu * a) + u.up.xyz * (vv * tanh));
    }
    let r = trace(origin, dir);
    let i = gid.y * w + gid.x;
    if (r.y == 1.0) {
        let p = origin + dir * r.x;
        out[i] = vec4<f32>(r.x, deviation(p), shade(p), 1.0);
    } else {
        out[i] = vec4<f32>(-1.0, 3.0, 0.0, r.y);
    }
}
