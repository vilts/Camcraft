//! Cutting data (M13.2): a small built-in table of workpiece material
//! groups with starting cutting speeds, feeds per tooth and the Kienzle
//! specific-cutting-force constants, plus the handbook formulas that
//! turn them into a spindle speed, a feed rate and a power estimate.
//!
//! Provenance: the material GROUPS are the public ISO 513 application
//! classification (P steel, M stainless, K cast iron, N non-ferrous,
//! plus plastics); the formulas are the textbook milling relations -
//! n = Vc*1000/(pi*D), vf = fz*z*n, the peripheral-milling engagement
//! angle phi = acos(1 - 2 ae/D) and its mean chip thickness
//! hm = fz*(2 ae/D)/phi (Altintas, *Manufacturing Automation*, ch. 2),
//! the radial chip-thinning factor hex/fz = sin(phi) = 2*sqrt(ae/D -
//! (ae/D)^2) for ae < D/2 (the chip-thinning relation published in
//! public tooling technical guides and machining handbooks), the
//! Kienzle law kc = kc1.1 * hm^-mc, and Pc = ae*ap*vf*kc/(60e6*eta).
//! Every NUMBER in the table is our own conservative starting value,
//! picked inside the ranges public handbooks (Machinery's Handbook
//! speed/feed chapters, general Kienzle tables) print for the group -
//! no vendor tool-data file or catalog table was copied.
//!
//! Pure module: no flecs, no I/O.

/// What the cutting edge is made of - the column the table reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMaterial {
    /// High-speed steel.
    Hss,
    /// Cemented (solid or brazed) carbide.
    Carbide,
}

impl ToolMaterial {
    pub const ALL: [ToolMaterial; 2] = [ToolMaterial::Hss, ToolMaterial::Carbide];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ToolMaterial::Hss => "HSS",
            ToolMaterial::Carbide => "Carbide",
        }
    }

    /// The word a project file writes.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            ToolMaterial::Hss => "hss",
            ToolMaterial::Carbide => "carbide",
        }
    }

    #[must_use]
    pub fn from_word(w: &str) -> Option<ToolMaterial> {
        ToolMaterial::ALL.into_iter().find(|m| m.word() == w)
    }
}

/// A workpiece material group of the built-in table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkMaterial {
    Aluminium6061,
    MildSteel,
    Stainless304,
    GreyCastIron,
    Brass,
    Plastics,
    // #3068: the router materials. Appended: the table is indexed in
    // ALL order and project files store the word, never the index.
    Softwood,
    Hardwood,
    Mdf,
    Plywood,
    Acrylic,
    Hdpe,
}

/// A cutting-speed (m/min) and feed-per-tooth (mm, at a 10 mm cutter)
/// range for one tool material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub vc: (f64, f64),
    pub fz10: (f64, f64),
}

/// One row of the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialRow {
    pub material: WorkMaterial,
    /// ISO 513 group letter ('O' for the plastics and wood rows, which
    /// ISO 513 does not class).
    pub iso: char,
    /// Specific cutting force at a 1 mm x 1 mm chip, N/mm^2 (Kienzle).
    pub kc11: f64,
    /// Kienzle exponent.
    pub mc: f64,
    pub hss: Window,
    pub carbide: Window,
}

const fn w(vc: (f64, f64), fz10: (f64, f64)) -> Window {
    Window { vc, fz10 }
}

/// The table. Our own numbers inside public handbook ranges (module
/// provenance); kc1.1 sits at the high side of the printed values so
/// the power estimate errs toward a warning, never toward a stall.
/// The router rows (#3068) read their 10 mm chip load through the
/// router curve ([`crate::cutting_router::router_scale`]); their speeds
/// sit at the low side of the woodworking and plastics ranges, and HSS
/// takes about 0.7 of the carbide chip load.
pub const TABLE: [MaterialRow; 12] = [
    MaterialRow { material: WorkMaterial::Aluminium6061, iso: 'N', kc11: 700.0, mc: 0.25,
        hss: w((100.0, 250.0), (0.04, 0.08)), carbide: w((250.0, 600.0), (0.05, 0.10)) },
    MaterialRow { material: WorkMaterial::MildSteel, iso: 'P', kc11: 1500.0, mc: 0.25,
        hss: w((20.0, 35.0), (0.03, 0.06)), carbide: w((120.0, 250.0), (0.04, 0.08)) },
    MaterialRow { material: WorkMaterial::Stainless304, iso: 'M', kc11: 2000.0, mc: 0.21,
        hss: w((10.0, 20.0), (0.02, 0.04)), carbide: w((60.0, 120.0), (0.03, 0.06)) },
    MaterialRow { material: WorkMaterial::GreyCastIron, iso: 'K', kc11: 1100.0, mc: 0.28,
        hss: w((15.0, 30.0), (0.03, 0.06)), carbide: w((80.0, 180.0), (0.05, 0.10)) },
    MaterialRow { material: WorkMaterial::Brass, iso: 'N', kc11: 780.0, mc: 0.18,
        hss: w((60.0, 100.0), (0.04, 0.08)), carbide: w((150.0, 300.0), (0.05, 0.10)) },
    MaterialRow { material: WorkMaterial::Plastics, iso: 'O', kc11: 250.0, mc: 0.25,
        hss: w((100.0, 250.0), (0.05, 0.12)), carbide: w((200.0, 500.0), (0.06, 0.15)) },
    MaterialRow { material: WorkMaterial::Softwood, iso: 'O', kc11: 80.0, mc: 0.25,
        hss: w((150.0, 350.0), (0.17, 0.25)), carbide: w((300.0, 700.0), (0.24, 0.36)) },
    MaterialRow { material: WorkMaterial::Hardwood, iso: 'O', kc11: 150.0, mc: 0.25,
        hss: w((120.0, 300.0), (0.13, 0.20)), carbide: w((250.0, 600.0), (0.19, 0.29)) },
    MaterialRow { material: WorkMaterial::Mdf, iso: 'O', kc11: 120.0, mc: 0.25,
        hss: w((150.0, 350.0), (0.17, 0.25)), carbide: w((300.0, 700.0), (0.24, 0.36)) },
    MaterialRow { material: WorkMaterial::Plywood, iso: 'O', kc11: 120.0, mc: 0.25,
        hss: w((120.0, 300.0), (0.15, 0.22)), carbide: w((250.0, 600.0), (0.21, 0.32)) },
    MaterialRow { material: WorkMaterial::Acrylic, iso: 'O', kc11: 350.0, mc: 0.25,
        hss: w((80.0, 200.0), (0.10, 0.15)), carbide: w((150.0, 400.0), (0.14, 0.22)) },
    MaterialRow { material: WorkMaterial::Hdpe, iso: 'O', kc11: 200.0, mc: 0.25,
        hss: w((100.0, 250.0), (0.12, 0.18)), carbide: w((200.0, 500.0), (0.17, 0.26)) },
];

impl WorkMaterial {
    pub const ALL: [WorkMaterial; 12] = [
        WorkMaterial::Aluminium6061,
        WorkMaterial::MildSteel,
        WorkMaterial::Stainless304,
        WorkMaterial::GreyCastIron,
        WorkMaterial::Brass,
        WorkMaterial::Plastics,
        WorkMaterial::Softwood,
        WorkMaterial::Hardwood,
        WorkMaterial::Mdf,
        WorkMaterial::Plywood,
        WorkMaterial::Acrylic,
        WorkMaterial::Hdpe,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            WorkMaterial::Aluminium6061 => "Aluminium 6061",
            WorkMaterial::MildSteel => "Mild steel",
            WorkMaterial::Stainless304 => "Stainless 304",
            WorkMaterial::GreyCastIron => "Grey cast iron",
            WorkMaterial::Brass => "Brass",
            WorkMaterial::Plastics => "Plastics (POM)",
            WorkMaterial::Softwood => "Softwood (pine, spruce)",
            WorkMaterial::Hardwood => "Hardwood (oak, maple)",
            WorkMaterial::Mdf => "MDF",
            WorkMaterial::Plywood => "Plywood",
            WorkMaterial::Acrylic => "Acrylic (PMMA)",
            WorkMaterial::Hdpe => "HDPE",
        }
    }

    /// The word a project file writes - names survive table growth.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            WorkMaterial::Aluminium6061 => "aluminium_6061",
            WorkMaterial::MildSteel => "mild_steel",
            WorkMaterial::Stainless304 => "stainless_304",
            WorkMaterial::GreyCastIron => "grey_cast_iron",
            WorkMaterial::Brass => "brass",
            WorkMaterial::Plastics => "plastics_pom",
            WorkMaterial::Softwood => "softwood",
            WorkMaterial::Hardwood => "hardwood",
            WorkMaterial::Mdf => "mdf",
            WorkMaterial::Plywood => "plywood",
            WorkMaterial::Acrylic => "acrylic",
            WorkMaterial::Hdpe => "hdpe",
        }
    }

    #[must_use]
    pub fn from_word(w: &str) -> Option<WorkMaterial> {
        WorkMaterial::ALL.into_iter().find(|m| m.word() == w)
    }

    #[must_use]
    pub fn row(self) -> &'static MaterialRow {
        // The table is indexed in ALL order (pinned by a test).
        &TABLE[self as usize]
    }

    /// A router material (#3068): its chip load reads the router curve
    /// by cutter diameter, not the metal bands of [`fz_scale`].
    #[must_use]
    pub const fn router(self) -> bool {
        matches!(
            self,
            WorkMaterial::Softwood
                | WorkMaterial::Hardwood
                | WorkMaterial::Mdf
                | WorkMaterial::Plywood
                | WorkMaterial::Acrylic
                | WorkMaterial::Hdpe
        )
    }

    /// The chip-load scale for a cutter of diameter `d` mm, relative to
    /// the row's 10 mm feed per tooth: the router curve for a router
    /// material, the metal bands for the others.
    #[must_use]
    pub fn chip_scale(self, d: f64) -> f64 {
        if self.router() { crate::cutting_router::router_scale(d) } else { fz_scale(d) }
    }

    #[must_use]
    pub fn window(self, tm: ToolMaterial) -> Window {
        match tm {
            ToolMaterial::Hss => self.row().hss,
            ToolMaterial::Carbide => self.row().carbide,
        }
    }
}

/// Feed-per-tooth scale by cutter diameter: the handbook tables' trend
/// (small cutters take thin chips), as five bands around the 10 mm
/// reference the table states. Our own factors.
#[must_use]
pub fn fz_scale(d: f64) -> f64 {
    if d <= 3.0 {
        0.25
    } else if d <= 6.0 {
        0.5
    } else if d <= 12.0 {
        1.0
    } else if d <= 20.0 {
        1.4
    } else {
        1.8
    }
}

/// Spindle speed for a cutting speed: n = Vc*1000/(pi*D), rpm.
#[must_use]
pub fn rpm_for(vc: f64, d: f64) -> f64 {
    vc * 1000.0 / (std::f64::consts::PI * d)
}

/// Feed rate: vf = fz*z*n, mm/min.
#[must_use]
pub fn feed_for(fz: f64, z: u32, rpm: f64) -> f64 {
    fz * f64::from(z) * rpm
}

/// The smallest ae/D the chip-thinning compensation reads: below it the
/// factor runs away (1/sqrt) and a suggestion would be a feed no table
/// vouches for, so the compensation holds at this load's value.
pub const MIN_THINNING_LOAD: f64 = 0.05;

/// Radial chip thinning: the max chip thickness a peripheral cut takes
/// at fz is hex = fz * sin(phi), phi = acos(1 - 2 ae/D); for ae >= D/2
/// the factor is 1. Returns the factor `hex / fz` in (0, 1], with ae/D
/// held at [`MIN_THINNING_LOAD`] from below.
#[must_use]
pub fn thinning_factor(load: f64) -> f64 {
    let r = load.clamp(MIN_THINNING_LOAD, 0.5);
    2.0 * (r - r * r).sqrt()
}

/// Mean chip thickness of a peripheral cut, mm: hm = fz*(2 ae/D)/phi
/// with phi = acos(1 - 2 ae/D) in radians (square-shoulder edge,
/// kappa_r = 90 deg). `None` for a load outside (0, 1].
#[must_use]
pub fn mean_chip(fz: f64, load: f64) -> Option<f64> {
    if !(load.is_finite() && load > 0.0 && load <= 1.0 && fz.is_finite() && fz > 0.0) {
        return None;
    }
    let phi = (1.0 - 2.0 * load).clamp(-1.0, 1.0).acos();
    (phi > 0.0).then(|| fz * 2.0 * load / phi)
}

/// Kienzle specific cutting force, N/mm^2: kc = kc1.1 * hm^-mc.
#[must_use]
pub fn kc(row: &MaterialRow, hm: f64) -> f64 {
    row.kc11 * hm.powf(-row.mc)
}

/// The spindle efficiency the power estimate divides by - the typical
/// value public machining handbooks quote for a milling spindle drive.
pub const SPINDLE_EFFICIENCY: f64 = 0.8;

/// Net spindle power, kW: Pc = ae*ap*vf*kc / (60e6 * eta).
#[must_use]
pub fn power_kw(ae: f64, ap: f64, vf: f64, kc: f64) -> f64 {
    ae * ap * vf * kc / (60.0e6 * SPINDLE_EFFICIENCY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_indexed_in_all_order_and_words_round_trip() {
        for m in WorkMaterial::ALL {
            assert_eq!(m.row().material, m);
            assert_eq!(WorkMaterial::from_word(m.word()), Some(m));
            for tm in ToolMaterial::ALL {
                let w = m.window(tm);
                assert!(w.vc.0 > 0.0 && w.vc.0 < w.vc.1);
                assert!(w.fz10.0 > 0.0 && w.fz10.0 < w.fz10.1);
            }
        }
        for tm in ToolMaterial::ALL {
            assert_eq!(ToolMaterial::from_word(tm.word()), Some(tm));
        }
        assert_eq!(WorkMaterial::from_word("unobtainium"), None);
    }

    #[test]
    fn formulas_match_hand_calculation() {
        // Vc 100 m/min on a 10 mm cutter: 100000 / (pi*10) = 3183.1 rpm.
        assert!((rpm_for(100.0, 10.0) - 3_183.098_861_837_907).abs() < 1e-9);
        assert!((feed_for(0.05, 4, 3000.0) - 600.0).abs() < 1e-12);
        // Full slot: hm = 2 fz / pi.
        let hm = mean_chip(0.05, 1.0).unwrap_or(0.0);
        assert!((hm - 0.1 / std::f64::consts::PI).abs() < 1e-12);
        // Half immersion: thinning factor 1; quarter: 2*sqrt(0.1875).
        assert!((thinning_factor(0.5) - 1.0).abs() < 1e-12);
        assert!((thinning_factor(0.25) - 0.866_025_403_784_438_6).abs() < 1e-12);
        assert!(mean_chip(0.05, 0.0).is_none());
    }
}
