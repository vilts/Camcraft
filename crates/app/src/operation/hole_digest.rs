//! The hole-list words of an operation's stamp (#1560): a Drill, Bore
//! or Thread op's holes digest in the form the project file stores them,
//! snapped to the 1 nm (1e-6 mm) lattice ([`crate::cl::lattice`])
//! exactly as `project::params`'s `HoleDto` / `CrossHoleDto` write
//! them. A typed off-lattice value (`0.190 in - 0.040 in` =
//! 3.8099999999999996 mm) and the 3.81 the file reloads then stamp
//! alike, so an op saved Ok reopens Ok (the #169 law). On-lattice values
//! (what recognition emits) hash bit-for-bit as before; a change of one
//! lattice step (1 nm = 1e-6 mm, or anything past half a step) still
//! moves the stamp. Same remedy as #1351's path signature.
//!
//! Provenance: our own design (the stored form is the stamped form); no
//! vendor material.

use std::hash::Hasher;

use crate::cl::lattice;
use crate::drilling::{CrossHole, Hole};

/// One float as the project file stores it.
fn put(h: &mut impl Hasher, v: f64) {
    h.write(&lattice(v).to_bits().to_le_bytes());
}

/// One axial hole: the words and order the stamp always wrote.
pub(super) fn hole(h: &mut impl Hasher, x: &Hole) {
    put(h, x.xy.x);
    put(h, x.xy.y);
    put(h, x.radius);
    put(h, x.entry_z);
    put(h, x.floor_z);
    h.write_u8(u8::from(x.through));
    // Changes what the bore door says, so it belongs in the digest
    // (M8.3): a re-detect that folds a step marks the op Stale.
    h.write_u8(u8::from(x.stepped));
}

/// One radial hole (M5.2 F2). The direction is stored verbatim by the
/// file (recognition emits axis-aligned unit vectors), so it hashes
/// verbatim; every other float takes the file's snap.
pub(super) fn cross(h: &mut impl Hasher, c: &CrossHole) {
    put(h, c.origin.x);
    put(h, c.origin.y);
    put(h, c.origin.z);
    for v in [c.dir.x, c.dir.y, c.dir.z] {
        h.write(&v.to_bits().to_le_bytes());
    }
    for v in [c.radius, c.entry_d, c.floor_d] {
        put(h, v);
    }
    h.write_u8(u8::from(c.through));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drilling::CycleChoice;
    use crate::geo2d::Point2;
    use crate::geometry::Point3;
    use crate::operation::{Height, OpParams};

    fn sig(x: &Hole) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        hole(&mut h, x);
        h.finish()
    }

    fn at(floor_z: f64) -> Hole {
        Hole { xy: Point2::new(10.0, -5.0), radius: 3.0, entry_z: 15.334, floor_z, through: false, stepped: false }
    }

    #[test]
    fn a_typed_floor_stamps_like_the_one_the_file_reloads() {
        let typed: f64 = 0.190 * 25.4 - 0.040 * 25.4;
        assert_ne!(typed, 3.81, "the probe must be off the lattice");
        assert_eq!(sig(&at(typed)), sig(&at(3.81)));
        // One micron (1000 lattice steps) or one lattice step (1 nm = 1e-6 mm) is a different hole.
        assert_ne!(sig(&at(3.81)), sig(&at(3.811)));
        assert_ne!(sig(&at(3.81)), sig(&at(3.81 + 1e-6)));
    }

    fn cross_sig(c: &CrossHole) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        cross(&mut h, c);
        h.finish()
    }

    fn drill_stamp(world: &flecs_ecs::prelude::World, c: &CrossHole) -> u64 {
        let params = OpParams::Drill {
            break_through: None,
            clearance_z: Height::fixed(10.0),
            retract_z: None,
            top_z: Height::fixed(0.0),
            bottom_z: None,
            cycle: CycleChoice::Drill,
            peck: 2.0,
            dwell: 0.0,
            feed: 200.0,
            plunge_feed: 100.0,
            holes: Vec::new(),
            cross: vec![*c],
        };
        let mut h = std::hash::DefaultHasher::new();
        params.stamp_with(world, None, None, &mut h);
        h.finish()
    }

    fn base_cross() -> CrossHole {
        CrossHole {
            origin: Point3::new(10.0, -5.0, 2.5),
            dir: Point3::new(-1.0, 0.0, 0.0),
            radius: 3.0,
            entry_d: 0.0,
            floor_d: -3.81,
            through: false,
        }
    }

    #[test]
    fn cross_hole_stamp_moves_on_every_field() {
        let world = crate::world::new_world();
        let base = base_cross();
        let base_digest = cross_sig(&base);
        let base_stamp = drill_stamp(&world, &base);
        let step = 1e-6; // one lattice step (1 nm = 1e-6 mm) past rounding

        let variants: [(&str, CrossHole); 10] = [
            ("origin.x", CrossHole { origin: Point3::new(base.origin.x + step, base.origin.y, base.origin.z), ..base }),
            ("origin.y", CrossHole { origin: Point3::new(base.origin.x, base.origin.y + step, base.origin.z), ..base }),
            ("origin.z", CrossHole { origin: Point3::new(base.origin.x, base.origin.y, base.origin.z + step), ..base }),
            ("dir.x", CrossHole { dir: Point3::new(base.dir.x + step, base.dir.y, base.dir.z), ..base }),
            ("dir.y", CrossHole { dir: Point3::new(base.dir.x, base.dir.y + step, base.dir.z), ..base }),
            ("dir.z", CrossHole { dir: Point3::new(base.dir.x, base.dir.y, base.dir.z + step), ..base }),
            ("radius", CrossHole { radius: base.radius + step, ..base }),
            ("entry_d", CrossHole { entry_d: base.entry_d + step, ..base }),
            ("floor_d", CrossHole { floor_d: base.floor_d - step, ..base }),
            ("through", CrossHole { through: !base.through, ..base }),
        ];
        for (name, moved) in variants {
            assert_ne!(cross_sig(&moved), base_digest, "cross digest did not move for {name}");
            assert_ne!(drill_stamp(&world, &moved), base_stamp, "drill stamp did not move for {name}");
        }
    }

    #[test]
    fn cross_hole_off_lattice_floor_stamps_like_reloaded_and_dir_is_verbatim() {
        let world = crate::world::new_world();
        let base = base_cross();
        let typed = -(0.190 * 25.4 - 0.040 * 25.4);
        assert_ne!(typed, -3.81, "the probe must be off the 1e-6 mm lattice");
        let off_lattice = CrossHole { floor_d: typed, ..base };
        assert_eq!(cross_sig(&off_lattice), cross_sig(&base));
        assert_eq!(drill_stamp(&world, &off_lattice), drill_stamp(&world, &base));

        // Sub-half-step noise on every snapped scalar stays on the same stamp;
        // one lattice step (1 nm = 1e-6 mm) past rounding moves it.
        let noise = 2e-7;
        let snapped: [(&str, CrossHole); 6] = [
            ("origin.x", CrossHole { origin: Point3::new(base.origin.x + noise, base.origin.y, base.origin.z), ..base }),
            ("origin.y", CrossHole { origin: Point3::new(base.origin.x, base.origin.y + noise, base.origin.z), ..base }),
            ("origin.z", CrossHole { origin: Point3::new(base.origin.x, base.origin.y, base.origin.z + noise), ..base }),
            ("radius", CrossHole { radius: base.radius + noise, ..base }),
            ("entry_d", CrossHole { entry_d: base.entry_d + noise, ..base }),
            ("floor_d", CrossHole { floor_d: base.floor_d - noise, ..base }),
        ];
        for (name, noisy) in snapped {
            assert_eq!(drill_stamp(&world, &noisy), drill_stamp(&world, &base), "sub-half-step noise on {name} moved the stamp");
        }
        let next_step = CrossHole { floor_d: base.floor_d - 1e-6, ..base };
        assert_ne!(drill_stamp(&world, &next_step), drill_stamp(&world, &base));

        // `dir` is stored verbatim by CrossHoleDto, so sub-lattice changes still move the stamp.
        let verbatim: [(&str, CrossHole); 3] = [
            ("dir.x", CrossHole { dir: Point3::new(base.dir.x + noise, base.dir.y, base.dir.z), ..base }),
            ("dir.y", CrossHole { dir: Point3::new(base.dir.x, base.dir.y + noise, base.dir.z), ..base }),
            ("dir.z", CrossHole { dir: Point3::new(base.dir.x, base.dir.y, base.dir.z + noise), ..base }),
        ];
        for (name, tweaked) in verbatim {
            assert_ne!(drill_stamp(&world, &tweaked), drill_stamp(&world, &base), "sub-lattice change on {name} failed to move the stamp");
        }
    }
}
