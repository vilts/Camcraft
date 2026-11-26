//! #3367: the tool meridian's convex atoms are the solids the solid law
//! builds today ([`Cutter::atoms`]), for each mill kind. Our own test.

use camcraft_tools::meridian::{Bound, MeridianAtom};
use camcraft_tools::undercut::cutter;

use super::probe::{Atom, Cutter};
use crate::tools::{Tool, ToolKind};

const TOL: f64 = 1e-9;

fn mill(kind: ToolKind, d: f64, corner_r: f64, tip_angle: f64, fl: f64) -> Tool {
    Tool { corner_r, ..cutter(kind, d, fl, d * 0.5, 30.0, tip_angle) }
}

fn cases() -> Vec<(&'static str, Tool)> {
    vec![
        ("end mill", mill(ToolKind::EndMill, 10.0, 0.0, 0.0, 20.0)),
        ("ball", mill(ToolKind::BallNose, 10.0, 5.0, 0.0, 20.0)),
        ("short ball", mill(ToolKind::BallNose, 10.0, 5.0, 0.0, 3.0)),
        ("bull", mill(ToolKind::BullNose, 10.0, 2.0, 0.0, 20.0)),
        ("short bull", mill(ToolKind::BullNose, 10.0, 3.0, 0.0, 2.0)),
        ("drill", mill(ToolKind::Drill, 8.0, 0.0, 118.0, 30.0)),
        ("chamfer", mill(ToolKind::Taper, 10.0, 0.0, 90.0, 20.0)),
        ("tall cone", mill(ToolKind::Taper, 10.0, 0.0, 30.0, 5.0)),
        ("t-slot", cutter(ToolKind::TSlot, 20.0, 5.0, 10.0, 10.0, 0.0)),
        ("keyseat", cutter(ToolKind::Keyseat, 25.0, 3.0, 10.0, 15.0, 0.0)),
        ("dovetail", cutter(ToolKind::Dovetail, 20.0, 5.0, 8.0, 10.0, 60.0)),
        ("lollipop", cutter(ToolKind::Lollipop, 10.0, 6.0, 4.0, 20.0, 0.0)),
    ]
}

fn same_solid(name: &str, m: &MeridianAtom, c: &Atom) {
    assert!((m.z0 - c.z0).abs() <= TOL, "{name}: z0 {} vs {}", m.z0, c.z0);
    assert!((m.z1 - c.z1).abs() <= TOL, "{name}: z1 {} vs {}", m.z1, c.z1);
    for i in 0..=64 {
        let z = m.z0 + (m.z1 - m.z0) * f64::from(i) / 64.0;
        let (a, b) = (m.rho(z), c.rho(z));
        assert!((a - b).abs() <= TOL, "{name}: rho({z}) {a} vs {b}");
    }
}

#[test]
fn meridian_atoms_match_cutter_atoms_for_each_mill_kind() {
    for (name, tool) in cases() {
        let m = tool.meridian().unwrap_or_else(|e| panic!("{name}: {e}"));
        let cut = Cutter::of(&tool).atoms();
        for bound in [Bound::Outer, Bound::Inner] {
            let atoms = m.atoms(bound, 1e-3).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!atoms.is_empty() && atoms.len() <= cut.len(), "{name}: {} atoms vs {}", atoms.len(), cut.len());
            assert!(atoms.iter().all(|a| a.cutting), "{name}: every atom of a mill cuts");
            for (a, c) in atoms.iter().zip(&cut) {
                same_solid(name, a, c);
            }
            // What the meridian leaves out is the shank and the holder.
            for c in &cut[atoms.len()..] {
                assert!(c.z0 >= tool.flute_len - TOL, "{name}: a cutting atom at {} is missing", c.z0);
            }
        }
    }
}
