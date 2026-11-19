//! ASCII DXF half-profile importer (#743): validates an explicit single
//! `CENTRELINE` line segment, explicit `$INSUNITS` length units, and one
//! open continuous `LINE`/`ARC` chain lying wholly on one side of the axis.
//! Provenance: Autodesk DXF Reference (ASCII DXF geometry and $INSUNITS) and
//! standard 2D vector projection; original implementation.

use std::f64::consts::PI;

use crate::spun::{MIN_SEGMENT, SpunPoint, SpunProfile};
use camcraft_geometry::dxf::{self, Primitive, Unit, Units, parse};
use camcraft_geometry::geo2d::Point2;

/// Layer containing the explicit tool axis in a DXF half-profile.
pub const CENTRELINE_LAYER: &str = "CENTRELINE";

const JOIN_TOL: f64 = 1e-4;
const CROSS_TOL: f64 = 1e-5;
const AXIS_SNAP_TOL: f64 = 1e-5;
const ARC_SAMPLE_STEPS: usize = 16;

/// Import an ASCII DXF half-profile requiring explicit `$INSUNITS`.
/// DXF +Y is tool +Z; the lower end of a Y-parallel centreline is z = 0.
pub fn dxf_to_spun(bytes: &[u8]) -> Result<SpunProfile, String> {
    dxf_to_spun_with_unit(bytes, None)
}

/// Import an ASCII DXF half-profile, using `default_unit` only when unitless.
pub fn dxf_to_spun_with_unit(
    bytes: &[u8],
    default_unit: Option<Unit>,
) -> Result<SpunProfile, String> {
    let doc = parse(bytes).map_err(|e| e.to_string())?;
    let scale = match doc.units {
        Units::Known(u) => u.millimeters_per_unit(),
        Units::MissingOrUnitless => match default_unit {
            Some(u) => u.millimeters_per_unit(),
            None => {
                return Err(
                    "DXF specifies no $INSUNITS header unit - set drawing units in the DXF or state millimetres/inches".into(),
                );
            }
        },
        Units::Unsupported(code) => {
            return Err(format!("DXF has unsupported $INSUNITS unit code {code}"));
        }
    };
    if !(scale.is_finite() && scale > 0.0) {
        return Err("DXF length unit scale is invalid".into());
    }
    let (cl, segs) = extract_segments(&doc, scale)?;
    let ordered = order_open_chain(&segs, cl.start, cl.axis)?;
    project_to_spun(&ordered, cl.start, cl.axis, cl.normal)
}

struct Centreline {
    start: Point2,
    axis: Point2,
    normal: Point2,
}

#[derive(Clone, Copy)]
enum SegKind {
    Line,
    Arc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
}

#[derive(Clone, Copy)]
struct Seg {
    a: Point2,
    b: Point2,
    kind: SegKind,
}

fn is_centreline_layer(layer: &str) -> bool {
    let t = layer.trim();
    t.eq_ignore_ascii_case(CENTRELINE_LAYER) || t.eq_ignore_ascii_case("CENTERLINE")
}

fn scale_pt(p: Point2, s: f64) -> Result<Point2, String> {
    let x = p.x * s;
    let y = p.y * s;
    if !(x.is_finite() && y.is_finite()) {
        return Err("DXF coordinates overflow millimetre conversion".into());
    }
    Ok(Point2::new(x, y))
}

fn extract_segments(doc: &dxf::Document, scale: f64) -> Result<(Centreline, Vec<Seg>), String> {
    let mut centrelines = Vec::new();
    let mut segs = Vec::new();
    for item in &doc.primitives {
        if matches!(item.primitive, Primitive::Circle { .. }) {
            return Err(
                "DXF contains a CIRCLE entity - expected a centreline LINE and one open half-profile".into(),
            );
        }
        if is_centreline_layer(&item.layer) {
            let Primitive::Line { start, end } = item.primitive else {
                return Err("DXF centreline entity must be a LINE".into());
            };
            centrelines.push((scale_pt(start, scale)?, scale_pt(end, scale)?));
            continue;
        }
        push_primitive(&mut segs, &item.primitive, scale)?;
    }
    match centrelines.as_slice() {
        [] => Err("DXF has no centreline - draw one LINE on layer 'CENTRELINE'".into()),
        [one] => {
            let dc = Point2::new(one.1.x - one.0.x, one.1.y - one.0.y);
            let len = dc.x.hypot(dc.y);
            if !(len.is_finite() && len >= MIN_SEGMENT) {
                return Err(format!("DXF centreline is shorter than {MIN_SEGMENT} mm"));
            }
            if segs.is_empty() {
                return Err("DXF has no half-profile geometry beside the centreline".into());
            }
            if dc.x != 0.0 {
                return Err("DXF centreline must be parallel to Y (DXF +Y is tool +Z)".into());
            }
            // A LINE's draw direction is invisible in CAD. Anchor the frame to
            // the lower endpoint, so reversing the LINE cannot move the holder.
            let start = if one.0.y <= one.1.y { one.0 } else { one.1 };
            let axis = Point2::new(0.0, 1.0);
            let normal = Point2::new(1.0, 0.0);
            Ok((
                Centreline {
                    start,
                    axis,
                    normal,
                },
                segs,
            ))
        }
        many => Err(format!(
            "DXF has {} centreline entities on layer 'CENTRELINE' - expected exactly one LINE",
            many.len()
        )),
    }
}

fn push_primitive(segs: &mut Vec<Seg>, p: &Primitive, scale: f64) -> Result<(), String> {
    match *p {
        Primitive::Line { start, end } => {
            let a = scale_pt(start, scale)?;
            let b = scale_pt(end, scale)?;
            segs.push(Seg {
                a,
                b,
                kind: SegKind::Line,
            });
        }
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => {
            let c = scale_pt(center, scale)?;
            let r = radius * scale;
            if !(r.is_finite() && r > 0.0) {
                return Err("DXF arc radius is out of range in millimetres".into());
            }
            // Spun arcs encode a minor arc by signed radius. Split major arcs
            // in either direction so conversion preserves their full extent.
            if sweep_angle.abs() > PI + 1e-6 {
                let half = sweep_angle * 0.5;
                push_arc(segs, c, r, start_angle, half);
                push_arc(segs, c, r, start_angle + half, half);
            } else {
                push_arc(segs, c, r, start_angle, sweep_angle);
            }
        }
        Primitive::Circle { .. } => {}
    }
    Ok(())
}

fn push_arc(segs: &mut Vec<Seg>, c: Point2, r: f64, start: f64, sweep: f64) {
    let end = start + sweep;
    let a = Point2::new(c.x + r * start.cos(), c.y + r * start.sin());
    let b = Point2::new(c.x + r * end.cos(), c.y + r * end.sin());
    segs.push(Seg {
        a,
        b,
        kind: SegKind::Arc {
            center: c,
            radius: r,
            start_angle: start,
            sweep_angle: sweep,
        },
    });
}

fn dot2(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn sub2(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x - b.x, a.y - b.y)
}

#[derive(Clone, Copy)]
struct OrderedSeg {
    start: Point2,
    end: Point2,
    kind: SegKind,
}

fn order_open_chain(segs: &[Seg], c0: Point2, axis: Point2) -> Result<Vec<OrderedSeg>, String> {
    let mut nodes: Vec<Point2> = Vec::new();
    let mut intern = |p: Point2| -> usize {
        if let Some(i) = nodes
            .iter()
            .position(|n| sub2(p, *n).x.hypot(sub2(p, *n).y) <= JOIN_TOL)
        {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let edges: Vec<(usize, usize)> = segs.iter().map(|s| (intern(s.a), intern(s.b))).collect();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (ei, &(u, v)) in edges.iter().enumerate() {
        if u == v {
            return Err("DXF half-profile has a zero-length or closed loop segment".into());
        }
        adj[u].push(ei);
        adj[v].push(ei);
    }
    if let Some((vi, list)) = adj.iter().enumerate().find(|(_, l)| l.len() > 2) {
        return Err(format!(
            "DXF half-profile branches ({} segments meet at ({:.3}, {:.3}))",
            list.len(),
            nodes[vi].x,
            nodes[vi].y
        ));
    }
    let ends: Vec<usize> = adj
        .iter()
        .enumerate()
        .filter(|(_, l)| l.len() == 1)
        .map(|(i, _)| i)
        .collect();
    if ends.is_empty() {
        return Err(
            "DXF profile forms a closed loop instead of an open half-profile between axis ends"
                .into(),
        );
    }
    if ends.len() != 2 {
        return Err(format!(
            "DXF half-profile is disconnected ({} open endpoints instead of 2)",
            ends.len()
        ));
    }
    let z_of = |vi: usize| dot2(sub2(nodes[vi], c0), axis);
    let mut cur = if z_of(ends[0]) <= z_of(ends[1]) {
        ends[0]
    } else {
        ends[1]
    };
    let mut used = vec![false; segs.len()];
    let mut chain = Vec::with_capacity(segs.len());
    while let Some(&ei) = adj[cur].iter().find(|&&e| !used[e]) {
        used[ei] = true;
        let (u, v) = edges[ei];
        let s = segs[ei];
        let (start, end, next) = if u == cur {
            (s.a, s.b, v)
        } else {
            (s.b, s.a, u)
        };
        chain.push(OrderedSeg {
            start,
            end,
            kind: s.kind,
        });
        cur = next;
    }
    if chain.len() != segs.len() {
        return Err("DXF half-profile has disconnected extra segments".into());
    }
    Ok(chain)
}

fn project_to_spun(
    chain: &[OrderedSeg],
    c0: Point2,
    axis: Point2,
    n_ref: Point2,
) -> Result<SpunProfile, String> {
    let mut min_sr = f64::INFINITY;
    let mut max_sr = f64::NEG_INFINITY;
    for s in chain {
        for p in sample_seg_points(s) {
            let sr = dot2(sub2(p, c0), n_ref);
            min_sr = min_sr.min(sr);
            max_sr = max_sr.max(sr);
        }
    }
    if max_sr > CROSS_TOL && min_sr < -CROSS_TOL {
        return Err(format!(
            "DXF half-profile crosses the centreline (points reach {min_sr:.3} mm and {max_sr:.3} mm)"
        ));
    }
    if max_sr.abs() <= CROSS_TOL && min_sr.abs() <= CROSS_TOL {
        return Err("DXF half-profile lies flat along the centreline (zero radius)".into());
    }
    let n_r = if max_sr > CROSS_TOL {
        n_ref
    } else {
        Point2::new(-n_ref.x, -n_ref.y)
    };
    let proj = |p: Point2| (dot2(sub2(p, c0), axis), dot2(sub2(p, c0), n_r));
    let mut pts = Vec::with_capacity(chain.len() + 1);
    for (i, s) in chain.iter().enumerate() {
        let (za, mut ra) = proj(s.start);
        let (zb, mut rb) = proj(s.end);
        if i == 0 && ra.abs() <= AXIS_SNAP_TOL {
            ra = 0.0;
        }
        if i + 1 == chain.len() && rb.abs() <= AXIS_SNAP_TOL {
            rb = 0.0;
        }
        let za = if za.abs() <= 1e-12 { 0.0 } else { za };
        let arc = match s.kind {
            SegKind::Line => 0.0,
            SegKind::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let len = (zb - za).hypot(rb - ra);
                if len <= 0.0 {
                    return Err("DXF half-profile contains a zero-length arc".into());
                }
                let mid_ang = start_angle + 0.5 * sweep_angle;
                let mid = Point2::new(
                    center.x + radius * mid_ang.cos(),
                    center.y + radius * mid_ang.sin(),
                );
                let (zm, rm) = proj(mid);
                let n_zr = (-(rb - ra) / len, (zb - za) / len);
                let c_mid = ((za + zb) * 0.5, (ra + rb) * 0.5);
                let dot_out = (zm - c_mid.0) * n_zr.0 + (rm - c_mid.1) * n_zr.1;
                let r_eff = if len * 0.5 > radius && (len * 0.5 - radius) <= 1e-5 {
                    len * 0.5
                } else {
                    radius
                };
                if dot_out >= 0.0 { r_eff } else { -r_eff }
            }
        };
        pts.push(SpunPoint { z: za, r: ra, arc });
        if i + 1 == chain.len() {
            let zb = if zb.abs() <= 1e-12 { 0.0 } else { zb };
            pts.push(SpunPoint::line(zb, rb));
        }
    }
    SpunProfile::new(pts).map_err(|e| format!("DXF half-profile invalid: {e}"))
}

fn sample_seg_points(s: &OrderedSeg) -> Vec<Point2> {
    match s.kind {
        SegKind::Line => vec![s.start, s.end],
        SegKind::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => {
            let mut out = Vec::with_capacity(ARC_SAMPLE_STEPS + 1);
            for i in 0..=ARC_SAMPLE_STEPS {
                let th = start_angle + sweep_angle * (i as f64 / ARC_SAMPLE_STEPS as f64);
                out.push(Point2::new(
                    center.x + radius * th.cos(),
                    center.y + radius * th.sin(),
                ));
            }
            out
        }
    }
}
