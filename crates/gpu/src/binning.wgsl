// Conservative facet binning (#2620): one invocation per facet box sets the
// facet's bit in every cell its box touches. Bit sets give each cell its
// facets once each, in ascending index, with no sort. The cell map is the
// f32 twin `cell32` in binning.rs. Provenance: uniform-grid partitioning
// from C. Ericson, "Real-Time Collision Detection" (2005), ch. 7.

struct Params {
    nx: u32,
    ny: u32,
    n: u32,
    words: u32,
    s: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> boxes: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> bits: array<atomic<u32>>;

fn cell(v: f32, n: u32) -> u32 {
    let t = floor(v / params.s);
    if (!(t > 0.0)) {
        return 0u;
    }
    return u32(min(t, f32(n - 1u)));
}

@compute @workgroup_size(64)
fn cs_bin(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.n) {
        return;
    }
    let b = boxes[i];
    let x0 = cell(b.x, params.nx);
    let x1 = cell(b.z, params.nx);
    let y0 = cell(b.y, params.ny);
    let y1 = cell(b.w, params.ny);
    let w = i / 32u;
    let m = 1u << (i % 32u);
    for (var cy = y0; cy <= y1; cy = cy + 1u) {
        for (var cx = x0; cx <= x1; cx = cx + 1u) {
            atomicOr(&bits[(cy * params.nx + cx) * params.words + w], m);
        }
    }
}
