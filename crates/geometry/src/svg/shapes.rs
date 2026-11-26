//! The SVG basic shapes and paths drawn into a [`Sink`].
//! Provenance: the W3C SVG 1.1 basic-shapes chapter (rect with the rx/ry auto and clamp rules,
//! circle, ellipse, line, polyline, polygon) and path chapter; original implementation.

use super::emit::Sink;
use super::number::{list, user_length};
use super::path::{self, Cmd};
use super::xml::Node;
use super::{SvgError, invalid};
use crate::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// What drawing one element did.
pub(super) enum Drawn {
    Shape,
    /// A zero size: SVG draws nothing (counted as skipped).
    Empty,
}

fn len(node: &Node, name: &str) -> Result<f64, SvgError> {
    let what = format!("<{}> {name}", node.name);
    node.attr(name).map_or(Ok(0.0), |v| user_length(v, &what))
}

fn size(node: &Node, name: &str) -> Result<Option<f64>, SvgError> {
    let what = format!("<{}> {name}", node.name);
    let Some(v) = node.attr(name).filter(|v| v.trim() != "auto") else { return Ok(None) };
    let v = user_length(v, &what)?;
    if v < 0.0 {
        return Err(invalid(format!("{what} is negative ({v})")));
    }
    Ok(Some(v))
}

/// Draw `node` (one of the shape elements) into `sink`; `max_cmds` bounds path data.
pub(super) fn draw(node: &Node, sink: &mut Sink, max_cmds: usize) -> Result<Drawn, SvgError> {
    let p = Point2::new;
    match node.name.as_str() {
        "line" => {
            sink.line(p(len(node, "x1")?, len(node, "y1")?), p(len(node, "x2")?, len(node, "y2")?))?;
        }
        "rect" => {
            let (x, y) = (len(node, "x")?, len(node, "y")?);
            let (w, h) = (size(node, "width")?.unwrap_or(0.0), size(node, "height")?.unwrap_or(0.0));
            if w == 0.0 || h == 0.0 {
                return Ok(Drawn::Empty);
            }
            let (rx, ry) = match (size(node, "rx")?, size(node, "ry")?) {
                (None, None) => (0.0, 0.0),
                (Some(r), None) | (None, Some(r)) => (r, r),
                (Some(a), Some(b)) => (a, b),
            };
            let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
            if rx == 0.0 || ry == 0.0 {
                let c = [p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h)];
                for i in 0..4 {
                    sink.line(c[i], c[(i + 1) % 4])?;
                }
            } else {
                // Clockwise on the page from the top edge, each corner a quarter ellipse.
                sink.line(p(x + rx, y), p(x + w - rx, y))?;
                sink.ellipse(p(x + w - rx, y + ry), rx, ry, 0.0, -FRAC_PI_2, FRAC_PI_2)?;
                sink.line(p(x + w, y + ry), p(x + w, y + h - ry))?;
                sink.ellipse(p(x + w - rx, y + h - ry), rx, ry, 0.0, 0.0, FRAC_PI_2)?;
                sink.line(p(x + w - rx, y + h), p(x + rx, y + h))?;
                sink.ellipse(p(x + rx, y + h - ry), rx, ry, 0.0, FRAC_PI_2, FRAC_PI_2)?;
                sink.line(p(x, y + h - ry), p(x, y + ry))?;
                sink.ellipse(p(x + rx, y + ry), rx, ry, 0.0, PI, FRAC_PI_2)?;
            }
        }
        "circle" | "ellipse" => {
            let centre = p(len(node, "cx")?, len(node, "cy")?);
            let (rx, ry) = if node.name == "circle" {
                let r = size(node, "r")?.unwrap_or(0.0);
                (r, r)
            } else {
                let (a, b) = (size(node, "rx")?, size(node, "ry")?);
                let r = a.or(b).unwrap_or(0.0);
                (a.unwrap_or(r), b.unwrap_or(r))
            };
            if rx == 0.0 || ry == 0.0 {
                return Ok(Drawn::Empty);
            }
            sink.ellipse(centre, rx, ry, 0.0, 0.0, TAU)?;
        }
        "polyline" | "polygon" => {
            let what = format!("<{}> points", node.name);
            let v = list(node.attr("points").unwrap_or(""), &what)?;
            if v.len() % 2 != 0 {
                return Err(invalid(format!("{what} has an odd count of numbers ({})", v.len())));
            }
            let pts: Vec<Point2> = v.as_chunks::<2>().0.iter().map(|&[x, y]| p(x, y)).collect();
            for pair in pts.windows(2) {
                sink.line(pair[0], pair[1])?;
            }
            if let (true, Some(&last), Some(&first)) = (node.name == "polygon" && pts.len() > 2, pts.last(), pts.first()) {
                sink.line(last, first)?;
            }
            if pts.len() < 2 {
                return Ok(Drawn::Empty);
            }
        }
        "path" => draw_path(node.attr("d").unwrap_or(""), sink, max_cmds)?,
        _ => return Ok(Drawn::Empty),
    }
    Ok(Drawn::Shape)
}

fn draw_path(d: &str, sink: &mut Sink, max_cmds: usize) -> Result<(), SvgError> {
    let mut cur = Point2::default();
    let mut start = Point2::default();
    for cmd in path::parse(d, max_cmds)? {
        match cmd {
            Cmd::Move(p) => {
                start = p;
                cur = p;
            }
            Cmd::Line(p) => {
                sink.line(cur, p)?;
                cur = p;
            }
            Cmd::Cubic(c1, c2, p) => {
                sink.cubic(cur, c1, c2, p)?;
                cur = p;
            }
            Cmd::Quad(c, p) => {
                sink.quad(cur, c, p)?;
                cur = p;
            }
            Cmd::Arc { rx, ry, phi, large, sweep, to } => {
                sink.arc(cur, rx, ry, phi, large, sweep, to)?;
                cur = to;
            }
            Cmd::Close => {
                sink.line(cur, start)?;
                cur = start;
            }
        }
    }
    Ok(())
}
