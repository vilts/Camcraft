// The stock removal kernel (#2627, GPU stock sim S2): one workgroup for
// each work item (one chord against one 8^3 brick), one invocation for
// each (x, y) column of the brick, the eight z cells in a loop. A claim
// folds into the lattice with atomicMax on the order-preserving key, so
// the result does not depend on the order of the items. Four entry
// points: cs_fill writes the -4 cap floor key, cs_encode turns an
// earlier layer into keys (#2629), cs_cut claims, cs_decode turns each
// key into the bits of its f32 (the pack_cut layout).
// The math is the CPU law of cut.rs, operation for operation.
// Provenance: the swept-solid distance laws of camcraft-sim's f32 twin
// (#2626): capsule closed form, upright flat cylinder in the (rho, z)
// half-plane, golden section (Kiefer 1953); the radix-sort float key.

struct Params {
    dims: vec3<u32>,
    n_tex: u32,
    first: u32,
    count: u32,
    row: u32,
    floor_key: u32,
    cell: f32,
    cap: f32,
    pad0: u32,
    pad1: u32,
}

struct Item {
    a: vec3<f32>,
    r: f32,
    b: vec3<f32>,
    flank: f32,
    base: vec3<i32>,
    ball: u32,
    lo: vec3<u32>,
    margin: f32,
    hi: vec3<u32>,
    pad: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> items: array<Item>;
@group(0) @binding(2) var<storage, read_write> keys: array<atomic<u32>>;

const GS_ITERS: u32 = 32u;
const PHI: f32 = 0.618034;
// The smallest denominator that the closed forms divide by; the CPU law
// gets a non-finite quotient (and takes t = 0) only below it.
const TINY: f32 = 1e-30;
const SIGN: u32 = 0x80000000u;

fn texel(wg: vec3<u32>, li: u32) -> u32 {
    return (wg.y * params.row + wg.x) * 64u + li;
}

@compute @workgroup_size(64)
fn cs_fill(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let i = texel(wg, li);
    if (i < params.n_tex) {
        atomicStore(&keys[i], params.floor_key);
    }
}

// #2629 (S3a): the incremental start. The buffer holds a layer (f32
// bits, the cs_decode output); turn each into its ordered key, so the
// cut folds onto the earlier layer instead of the floor.
@compute @workgroup_size(64)
fn cs_encode(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let i = texel(wg, li);
    if (i >= params.n_tex) {
        return;
    }
    let b = atomicLoad(&keys[i]);
    var key = b | SIGN;
    if ((b & SIGN) != 0u) {
        key = ~b;
    }
    atomicStore(&keys[i], key);
}

@compute @workgroup_size(64)
fn cs_decode(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let i = texel(wg, li);
    if (i >= params.n_tex) {
        return;
    }
    let u = atomicLoad(&keys[i]);
    var bits = ~u;
    if ((u & SIGN) != 0u) {
        bits = u & ~SIGN;
    }
    atomicStore(&keys[i], bits);
}

fn ordered(v: f32) -> u32 {
    let b = bitcast<u32>(v);
    if ((b & SIGN) != 0u) {
        return ~b;
    }
    return b | SIGN;
}

fn ball_dist(it: Item, q: vec3<f32>) -> f32 {
    let d = it.b - it.a;
    let v = q - it.a;
    let dd = dot(d, d);
    var t = 0.0;
    if (dd > 0.0) {
        t = clamp(dot(v, d) / dd, 0.0, 1.0);
    }
    let w = v - t * d;
    return sqrt(dot(w, w)) - it.r;
}

fn pose(it: Item, t: f32, q: vec3<f32>) -> f32 {
    let tip = it.a + t * (it.b - it.a);
    let x = q.x - tip.x;
    let y = q.y - tip.y;
    let zl = q.z - tip.z;
    let rho = sqrt(x * x + y * y);
    let e = vec3<f32>(rho - it.r, -zl, zl - it.flank);
    let m = max(max(e.x, e.y), e.z);
    if (m > 0.0) {
        let p = max(e, vec3<f32>(0.0));
        return sqrt(dot(p, p));
    }
    return m;
}

fn flat_dist(it: Item, q: vec3<f32>) -> f32 {
    let d = it.b - it.a;
    if (d.z == 0.0) {
        let dd = d.x * d.x + d.y * d.y;
        var t = 0.0;
        if (dd > TINY) {
            t = clamp(((q.x - it.a.x) * d.x + (q.y - it.a.y) * d.y) / dd, 0.0, 1.0);
        }
        return pose(it, t, q);
    }
    if (d.x == 0.0 && d.y == 0.0) {
        var t = 0.0;
        if (abs(d.z) > TINY) {
            t = clamp((q.z - it.a.z - 0.5 * it.flank) / d.z, 0.0, 1.0);
        }
        return pose(it, t, q);
    }
    var lo = 0.0;
    var hi = 1.0;
    var c = hi - PHI * (hi - lo);
    var e = lo + PHI * (hi - lo);
    var fc = pose(it, c, q);
    var fe = pose(it, e, q);
    for (var k = 0u; k < GS_ITERS; k = k + 1u) {
        if (fc < fe) {
            hi = e;
            e = c;
            fe = fc;
            c = hi - PHI * (hi - lo);
            fc = pose(it, c, q);
        } else {
            lo = c;
            c = e;
            fc = fe;
            e = lo + PHI * (hi - lo);
            fe = pose(it, e, q);
        }
    }
    return min(fc, fe);
}

@compute @workgroup_size(8, 8, 1)
fn cs_cut(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    if (wg.x >= params.count) {
        return;
    }
    let it = items[params.first + wg.x];
    for (var z = 0u; z < 8u; z = z + 1u) {
        let l = vec3<u32>(lid.x, lid.y, z);
        if (any(l < it.lo) || any(l > it.hi)) {
            continue;
        }
        let q = (vec3<f32>(l) + vec3<f32>(0.5)) * params.cell;
        var dist: f32;
        if (it.ball != 0u) {
            dist = ball_dist(it, q);
        } else {
            dist = flat_dist(it, q);
        }
        let phi = dist + it.margin;
        // written so that a NaN claims nothing, as on the CPU
        if (!(phi < params.cap)) {
            continue;
        }
        let at = vec3<i32>(l) + it.base;
        if (any(at < vec3<i32>(0)) || any(vec3<u32>(at) >= params.dims)) {
            continue; // unreachable: the host checked every item
        }
        let s = (u32(at.z) * params.dims.y + u32(at.y)) * params.dims.x + u32(at.x);
        atomicMax(&keys[s], ordered(min(-phi, params.cap)));
    }
}
