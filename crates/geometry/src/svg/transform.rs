//! 2D affine transforms and the SVG `transform` attribute.
//! Provenance: the W3C SVG 1.1 coordinate-systems chapter (the matrix form
//! `[a c e; b d f; 0 0 1]` and the six transform functions); original implementation.

use super::number::{Nums, finite};
use super::{SvgError, invalid};
use crate::geo2d::Point2;

/// `x' = a x + c y + e`, `y' = b x + d y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine { e: x, f: y, ..Affine::IDENTITY }
    }

    pub fn scale(x: f64, y: f64) -> Affine {
        Affine { a: x, d: y, ..Affine::IDENTITY }
    }

    pub fn rotate_deg(deg: f64) -> Affine {
        let (s, c) = deg.to_radians().sin_cos();
        Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }

    /// `self` after `inner`: a point goes through `inner` first.
    pub fn then(self, inner: Affine) -> Affine {
        Affine {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    pub fn apply(&self, p: Point2) -> Point2 {
        Point2::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    pub fn det(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f].iter().all(|v| v.is_finite())
    }
}

/// The `transform` attribute: a list of functions applied right to left to a point.
pub(super) fn parse(text: &str) -> Result<Affine, SvgError> {
    let bad = |why: &str| invalid(format!("transform \"{text}\": {why}"));
    let mut nums = Nums::new(text);
    let mut out = Affine::IDENTITY;
    loop {
        nums.sep();
        if nums.done() {
            break;
        }
        let start = nums.at;
        while nums.letter().is_some() {
            nums.bump();
        }
        let name = text.get(start..nums.at).unwrap_or("").trim().to_owned();
        if name.is_empty() {
            return Err(bad("a transform function name is missing"));
        }
        nums.sep();
        if text.as_bytes().get(nums.at) != Some(&b'(') {
            return Err(bad("'(' is missing"));
        }
        nums.bump();
        let mut args = Vec::new();
        while let Some(v) = nums.number() {
            args.push(finite(v, "a transform value")?);
            nums.sep();
        }
        nums.done();
        if text.as_bytes().get(nums.at) != Some(&b')') {
            return Err(bad("')' is missing"));
        }
        nums.bump();
        let step = match (name.as_str(), args.as_slice()) {
            ("matrix", &[a, b, c, d, e, f]) => Affine { a, b, c, d, e, f },
            ("translate", &[x]) => Affine::translate(x, 0.0),
            ("translate", &[x, y]) => Affine::translate(x, y),
            ("scale", &[s]) => Affine::scale(s, s),
            ("scale", &[x, y]) => Affine::scale(x, y),
            ("rotate", &[deg]) => Affine::rotate_deg(deg),
            ("rotate", &[deg, cx, cy]) => {
                Affine::translate(cx, cy).then(Affine::rotate_deg(deg)).then(Affine::translate(-cx, -cy))
            }
            ("skewX", &[deg]) => Affine { c: deg.to_radians().tan(), ..Affine::IDENTITY },
            ("skewY", &[deg]) => Affine { b: deg.to_radians().tan(), ..Affine::IDENTITY },
            _ => return Err(bad(&format!("{name} with {} values is not a transform function", args.len()))),
        };
        out = out.then(step);
    }
    // A transform that collapses the drawing onto a line or a point cannot be cut.
    if !out.is_finite() || out.det().abs() < 1e-12 {
        return Err(bad("the transform is singular or not finite"));
    }
    Ok(out)
}
