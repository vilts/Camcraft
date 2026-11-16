//! Built-in milling vises (#788, part of #339): generic parametric
//! vises built as geometry from a few dimensions, placed into a setup
//! through the fixture library's insert path (`commands::InsertVise`),
//! and judged by the same fixture laws as any meshed fixture - a vise
//! is a model wearing `FixtureDef`, its parameters riding the marker so
//! the jaw opening can be edited (`commands::SetVise`) and saved.
//!
//! The vise frame: x across the jaws (centred on the jaw width), y along
//! the screw - the fixed jaw's clamping face at y = 0, its body behind
//! it toward +y, the moving jaw's face at y = -opening - and z = 0 the
//! floor the part sits on, the base below. Placing the frame's origin
//! puts the fixed jaw's face, centred, at the floor. Each jaw is a solid
//! block `jaw_body_depth` deep (its plate plus the steel behind it) up
//! to jaw height - at least the real steel there, never less.
//!
//! Provenance: our own geometry and parameters; the size table's figures
//! are typical values from public catalog spec sheets (cited in
//! `docs/decisions/788-builtin-vises.md`), no vendor model copied.

use crate::geometry::{Aabb, Point3, Triangle, quat_conjugate, quat_rotate_point};
use crate::schema::Location;

pub mod mesh;
pub mod self_centering;
pub mod table;

pub use self_centering::SelfCentering;
pub use table::{SIZES, ViseSize, preset, presets, size};

/// Which kind of vise the parameters build (#787).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ViseStyle {
    /// A fixed jaw and a moving jaw on a screw (#788): the frame's origin
    /// is the fixed jaw's face.
    #[default]
    Standard,
    /// A 5-axis self-centering vise (#787): both jaws close on the centre,
    /// the frame's origin is that centre, no screw housing.
    SelfCentering(SelfCentering),
}

/// One vise's dimensions (millimetres) and its jaw opening.
#[derive(Clone, Debug, PartialEq)]
pub struct ViseParams {
    /// The table row it started from ("6 in"); free text once edited.
    pub size: String,
    pub jaw_width: f64,
    pub jaw_height: f64,
    /// Each jaw plate's depth along the screw.
    pub jaw_thickness: f64,
    /// Each jaw's solid body depth along the screw, plate included (at
    /// least the plate): the fixed jaw's steel behind its face and the
    /// moving jaw's block behind its face, modelled up to jaw height.
    pub jaw_body_depth: f64,
    pub max_opening: f64,
    /// Overall length along the screw, housing included.
    pub length: f64,
    /// Overall width across the jaws (at least the jaw width).
    pub width: f64,
    /// From the table to the floor the part sits on.
    pub base_height: f64,
    /// The screw and handle housing at the front: its length...
    pub housing_length: f64,
    /// ...and its height above the floor.
    pub housing_height: f64,
    /// The gap between the jaws' faces.
    pub opening: f64,
    /// Standard or self-centering (#787); the housing figures are zero on
    /// a self-centering vise.
    pub style: ViseStyle,
}

/// How far a part box may stand into the fixed jaw before "the stock is
/// not against the fixed jaw" (float dust on a placement made by fit).
const FACE_TOL: f64 = 1e-6;

impl ViseParams {
    /// Every dimension named, as the refusals say it.
    fn named(&self) -> [(&'static str, f64); 11] {
        [
            ("jaw width", self.jaw_width),
            ("jaw height", self.jaw_height),
            ("jaw thickness", self.jaw_thickness),
            ("jaw body depth", self.jaw_body_depth),
            ("max opening", self.max_opening),
            ("overall length", self.length),
            ("overall width", self.width),
            ("base height", self.base_height),
            ("housing length", self.housing_length),
            ("housing height", self.housing_height),
            ("jaw opening", self.opening),
        ]
    }

    /// The vise law: every dimension a positive, finite length; the
    /// opening within the max; the body as wide as the jaws and long
    /// enough to carry both jaws at the max opening plus the housing.
    /// Refusals name the dimension.
    pub fn check(&self) -> Result<(), String> {
        if let ViseStyle::SelfCentering(sc) = &self.style {
            return self_centering::check(self, sc);
        }
        for (what, v) in self.named() {
            if !v.is_finite() {
                return Err(format!("vise {what} must be a finite length (got {v})"));
            }
            if v <= 0.0 {
                return Err(format!("vise {what} must be more than zero (got {v} mm)"));
            }
        }
        if self.opening > self.max_opening {
            return Err(format!(
                "vise jaw opening {:.3} mm is over the max opening {:.3} mm",
                self.opening, self.max_opening
            ));
        }
        if self.width < self.jaw_width {
            return Err(format!(
                "vise overall width {:.3} mm is narrower than its jaw width {:.3} mm",
                self.width, self.jaw_width
            ));
        }
        if self.jaw_body_depth < self.jaw_thickness {
            return Err(format!(
                "vise jaw body depth {:.3} mm is less than its jaw thickness {:.3} mm",
                self.jaw_body_depth, self.jaw_thickness
            ));
        }
        let carry = 2.0 * self.jaw_body_depth + self.max_opening + self.housing_length;
        if self.length <= carry {
            return Err(format!(
                "vise overall length {:.3} mm must be more than two jaw bodies, the max opening and the housing ({carry:.3} mm)",
                self.length
            ));
        }
        Ok(())
    }

    /// The same vise with the jaws `opening` apart.
    #[must_use]
    pub fn with_opening(&self, opening: f64) -> Self {
        Self { opening, ..self.clone() }
    }

    /// The checked vise's closed shell in its own frame.
    pub fn mesh(&self) -> Result<Vec<Triangle>, String> {
        self.check()?;
        match &self.style {
            ViseStyle::Standard => mesh::solid(self),
            ViseStyle::SelfCentering(sc) => self_centering::solid(self, sc),
        }
    }

    /// The opening that clamps stock box `stock` with the vise standing at
    /// `pose`, by its style: a standard vise's moving jaw against the
    /// stock's far side, a self-centering vise's jaws closed on a stock
    /// centred between them. Refused by name.
    pub fn opening_to_stock(&self, pose: &Location, stock: &Aabb) -> Result<f64, String> {
        match &self.style {
            ViseStyle::Standard => opening_from_stock(pose, stock),
            ViseStyle::SelfCentering(sc) => self_centering::opening_from_stock(self, sc, pose, stock),
        }
    }

    /// How far above the vise floor the stock's bottom sits: a
    /// self-centering grip jaw's ledge (#1274), else 0 (on the floor).
    /// The frame's z = 0 is that seat, the floor this far below it.
    #[must_use]
    pub fn seat_height(&self) -> f64 {
        match &self.style {
            ViseStyle::SelfCentering(sc) => sc.seat(self.jaw_height),
            ViseStyle::Standard => 0.0,
        }
    }

    /// The model name a fresh vise lands under (before the free-name
    /// suffix): "Vise 6 in".
    #[must_use]
    pub fn base_name(&self) -> String {
        let s = self.size.trim();
        if s.is_empty() { "Vise".to_owned() } else { format!("Vise {s}") }
    }
}

/// Fit a vise at identity rotation to stock box `stock` (a self-centering
/// vise: `self_centering::fit_to_stock`): the fixed jaw's
/// face on the stock's +y side, the jaw width centred on the stock in x,
/// the floor under the stock, and the opening the stock's y extent.
/// Returns the vise (opening set) and the pose of its frame. Refused by
/// name when the stock is wider than the vise opens.
pub fn fit_to_stock(p: &ViseParams, stock: &Aabb) -> Result<(ViseParams, Location), String> {
    if let ViseStyle::SelfCentering(sc) = &p.style {
        return self_centering::fit_to_stock(p, sc, stock);
    }
    let across = stock.max.y - stock.min.y;
    if !across.is_finite() || across <= 0.0 {
        return Err("the stock has no width to clamp".into());
    }
    if across > p.max_opening {
        return Err(format!(
            "the stock is {across:.3} mm across the jaws; the {} vise opens {:.3} mm at most",
            p.base_name(),
            p.max_opening
        ));
    }
    let fitted = p.with_opening(across);
    fitted.check()?;
    let pos = Point3::new(0.5 * (stock.min.x + stock.max.x), stock.max.y, stock.min.z);
    Ok((fitted, Location { pos, rot: crate::geometry::IDENTITY_Q }))
}

/// The opening that puts the moving jaw against stock box `stock` for a
/// vise standing at `pose`: the stock's far side from the fixed jaw,
/// measured along the vise's screw. Refused by name when the stock
/// reaches into the fixed jaw (the vise is not placed against it) or
/// lies behind it.
pub fn opening_from_stock(pose: &Location, stock: &Aabb) -> Result<f64, String> {
    let inv = quat_conjugate(pose.rot);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for k in 0..8 {
        let c = Point3::new(
            if k & 1 == 0 { stock.min.x } else { stock.max.x },
            if k & 2 == 0 { stock.min.y } else { stock.max.y },
            if k & 4 == 0 { stock.min.z } else { stock.max.z },
        );
        let local = quat_rotate_point(inv, Point3::new(c.x - pose.pos.x, c.y - pose.pos.y, c.z - pose.pos.z));
        lo = lo.min(local.y);
        hi = hi.max(local.y);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return Err("the stock box is not finite".into());
    }
    if hi > FACE_TOL {
        return Err(format!(
            "the stock reaches {hi:.3} mm into the fixed jaw - place the vise against the stock first"
        ));
    }
    if lo >= -FACE_TOL {
        return Err("the stock is not in front of the fixed jaw".into());
    }
    Ok(-lo)
}
