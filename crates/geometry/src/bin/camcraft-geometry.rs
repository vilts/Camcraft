//! `camcraft-geometry` - headless mesh inspector (M13.3): reads an STL
//! (binary or ASCII) and prints triangle count, bounds and validity.
//! Provenance: the STL layout (80-byte header, u32 count, 50-byte
//! facets; the ASCII `facet`/`vertex` grammar) is the public 3D Systems
//! StereoLithography format description; this small reader is ours and
//! exists for the CLI only - the app's importers stay in app.

use camcraft_geometry::geometry::{repair, Mesh, Point3, Triangle};
use std::process::ExitCode;

const USAGE: &str = "\
camcraft-geometry - CamCraft mesh inspector (headless)

USAGE:
    camcraft-geometry <model.stl>

Reads a binary or ASCII STL and prints the triangle count, bounds,
degenerate facets and open edges. Exits non-zero on an unreadable,
truncated or malformed file, or one with non-finite coordinates.

OPTIONS:
    -h, --help        print this text
    --version         print the version
";

/// Refusal ceiling: a count past this is a corrupt header, not a model.
const MAX_TRIANGLES: usize = 50_000_000;

fn main() -> ExitCode {
    log_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Vec::new();
    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("camcraft-geometry {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            o if o.starts_with("--") => return die(format!("unknown option {o:?} (try --help)")),
            o => paths.push(o.to_string()),
        }
    }
    let [path] = paths.as_slice() else {
        return die("expected exactly one STL path (try --help)".into());
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return die(format!("cannot read {path}: {e}")),
    };
    let facets = match read_stl(&bytes) {
        Ok(f) => f,
        Err(e) => return die(format!("cannot parse {path}: {e}")),
    };
    let mesh = Mesh::from_facets(facets);
    if !mesh.all_finite() {
        return die(format!("{path}: non-finite coordinates"));
    }
    let Some(b) = mesh.bounds() else {
        return die(format!("{path}: no triangles"));
    };
    let degenerate = mesh.facets.iter().filter(|t| t.normal() == Point3::ZERO).count();
    let open = repair::open_edge_count(&mesh);
    println!("file: {path}");
    println!("triangles: {}", mesh.len());
    println!(
        "bounds: X {:.4}..{:.4}  Y {:.4}..{:.4}  Z {:.4}..{:.4}",
        b.min.x, b.max.x, b.min.y, b.max.y, b.min.z, b.max.z
    );
    println!("size: {:.4} x {:.4} x {:.4}", b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z);
    println!("degenerate facets: {degenerate}");
    println!("open edges: {open}");
    println!("closed: {}", open == 0);
    ExitCode::SUCCESS
}

fn die(msg: String) -> ExitCode {
    eprintln!("camcraft-geometry: {msg}");
    ExitCode::FAILURE
}

/// Binary when the header's count matches the file length exactly;
/// otherwise ASCII (which must open with `solid`).
fn read_stl(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    if bytes.len() >= 84 {
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if n.checked_mul(50).and_then(|x| x.checked_add(84)) == Some(bytes.len()) {
            return read_binary(&bytes[84..], n);
        }
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not a binary STL (size/count mismatch) and not UTF-8 text".to_string())?;
    if !text.trim_start().starts_with("solid") {
        return Err("not a binary STL (size/count mismatch) and no ASCII `solid` header".into());
    }
    read_ascii(text)
}

fn read_binary(body: &[u8], n: usize) -> Result<Vec<Triangle>, String> {
    if n > MAX_TRIANGLES {
        return Err(format!("{n} triangles exceeds the {MAX_TRIANGLES} ceiling"));
    }
    // Each 50-byte facet: normal (ignored, recomputed), three vertices
    // of three little-endian f32s, a 2-byte attribute word.
    let f = |c: &[u8; 50], o: usize| -> f64 {
        let mut w = [0u8; 4];
        if let Some(s) = c.get(o..o + 4) {
            w.copy_from_slice(s);
        }
        f64::from(f32::from_le_bytes(w))
    };
    let p = |c: &[u8; 50], o: usize| Point3::new(f(c, o), f(c, o + 4), f(c, o + 8));
    let (facets, _) = body.as_chunks::<50>();
    Ok(facets.iter().map(|c| Triangle::new(p(c, 12), p(c, 24), p(c, 36))).collect())
}

fn read_ascii(text: &str) -> Result<Vec<Triangle>, String> {
    let mut out = Vec::new();
    let mut verts: Vec<Point3> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("vertex") => {
                let w: Vec<&str> = it.collect();
                let [x, y, z] = w.as_slice() else {
                    return Err(format!("line {}: vertex needs three numbers", i + 1));
                };
                let mut c = [0.0f64; 3];
                for (slot, s) in c.iter_mut().zip([x, y, z]) {
                    *slot = s.parse::<f64>().map_err(|_| format!("line {}: bad number {s:?}", i + 1))?;
                }
                verts.push(Point3::new(c[0], c[1], c[2]));
            }
            Some("endfacet") => {
                let [a, b, c] = verts.as_slice() else {
                    return Err(format!("line {}: a facet needs exactly three vertices", i + 1));
                };
                out.push(Triangle::new(*a, *b, *c));
                verts.clear();
                if out.len() > MAX_TRIANGLES {
                    return Err(format!("more than {MAX_TRIANGLES} triangles"));
                }
            }
            _ => {}
        }
    }
    if !verts.is_empty() {
        return Err("file ends inside a facet".into());
    }
    Ok(out)
}

/// #163: the library's diagnostics are `tracing` events; print them on
/// stderr, filtered by `CAMCRAFT_LOG`, else `RUST_LOG`, else `info` - the
/// app binaries' rule. A second init is a no-op, never a panic.
fn log_init() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("CAMCRAFT_LOG")
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
}
