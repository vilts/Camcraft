// Provenance: original CamCraft 3D rough-and-finish catalog contract (#2866).
mod catalog_3d_rough_finish {
    use super::*;

    #[test]
    fn regenerates() {
        run_case("3d", "rough-finish", "regenerates", regenerate);
    }

    #[test]
    fn rebuilds() {
        run_case("3d", "rough-finish", "rebuilds", rebuild);
    }

    #[test]
    fn interlevel_rapid_stops_at_feed_height_above_prior_floor() {
        // #3671: between levels the tool must not rapid down to the floor
        // of the cut level above with 0 margin. The rapid must stop at feed
        // height (plunge_height = 2.0 mm) above the last cut floor.
        let dir = repo("examples/catalog/3d/rough-finish");
        let nc = std::fs::read_to_string(dir.join("rough-finish.nc")).expect("read nc");
        let op_lines = section(&nc, "3D Pocket1");
        assert!(!op_lines.is_empty(), "3D Pocket1 section present");
        let mut lowest_rapid_z = f64::INFINITY;
        for line in &op_lines {
            if (line.starts_with("G00") || line.starts_with("G0 "))
                && let Some(z) = word(line, 'Z')
                && z < lowest_rapid_z
            {
                lowest_rapid_z = z;
            }
        }
        // The last level cuts at Z8. The level above it cut at Z10.
        // The rapid before the last level must stop at Z10 + 2.0 = Z12.
        assert!(
            lowest_rapid_z >= 12.0 - 1e-6,
            "lowest rapid in 3D Pocket1 was Z {lowest_rapid_z}, expected >= 12.0 (feed height above Z10)"
        );
    }
    /// #3852: in the posted 3D Pocket, no plunge feeds from the hop plane
    /// (none is longer than the 2 mm plunge height plus the 2 mm stepdown),
    /// and no linear move posts with zero length (the same X, Y and Z as
    /// the position before it).
    #[test]
    fn pocket_posts_no_hop_plane_plunge_and_no_zero_length_move() {
        let dir = repo("examples/catalog/3d/rough-finish");
        let nc = std::fs::read_to_string(dir.join("rough-finish.nc")).expect("read nc");
        let op_lines = section(&nc, "3D Pocket1");
        assert!(!op_lines.is_empty(), "3D Pocket1 section present");
        let mut at: Option<(f64, f64, f64)> = None;
        let (mut zero, mut long) = (Vec::new(), Vec::new());
        for line in &op_lines {
            let g = line.split_whitespace().next().unwrap_or("");
            if !matches!(g, "G00" | "G01" | "G02" | "G03") {
                continue;
            }
            let (Some(x), Some(y), Some(z)) = (word(line, 'X'), word(line, 'Y'), word(line, 'Z')) else {
                continue;
            };
            if g == "G01" && let Some((px, py, pz)) = at {
                if (px, py, pz) == (x, y, z) {
                    zero.push(*line);
                }
                if (px, py) == (x, y) && pz - z > 2.0 + 2.0 + 1e-6 {
                    long.push(*line);
                }
            }
            at = Some((x, y, z));
        }
        assert!(zero.is_empty(), "{} zero-length moves posted: {:?}", zero.len(), zero);
        assert!(long.is_empty(), "plunges that feed from the hop plane: {long:?}");
    }

    /// #3670: the 3D Pocket's entry is a helix. A pass that restarts in a
    /// level after a hop over the dome must not feed straight down into
    /// uncut stock: it takes the entry type, or it goes down only where
    /// the column is already cleared (the centre-cutting plunge law).
    #[test]
    fn pocket_re_entries_never_plunge_into_uncut_stock() {
        use camcraft_app::{model, operation, project, setup, world};
        use camcraft_motion::plunge_law::{Solid, first_solid_plunge};
        let dir = repo("examples/catalog/3d/rough-finish");
        let w = world::new_world();
        let notes = project::load_noted(&w, &dir.join("rough-finish.camproj")).expect("rough-finish project");
        assert!(notes.is_empty(), "load notes: {notes:?}");
        let (_, pocket) = setup::operation_order(&w)
            .into_iter()
            .find(|(n, _)| n == "3D Pocket1")
            .expect("the 3D Pocket of the case");
        let op = model::operation_of(&w, &pocket).expect("pocket operation");
        let (t, _) = operation::generate(&w, &op, "3D Pocket1").expect("the pocket generates");
        // Stock box: 22 mm tall, its top at Z22 (clearance 47 = top + 25).
        let solid = Solid { top: 22.0, footprint: None };
        assert_eq!(first_solid_plunge(&t, 3.0, &solid), None, "a re-entry plunges into uncut stock: {:?}", t.notes);
    }

    /// The review of #3670 found stock outside the core but inside the
    /// cutter at this point. Read the mesh and previous swept cuts directly;
    /// do not call the production core or full-disc clearance predicates.
    #[test]
    fn pocket_re_entry_keeps_the_outer_stock_clear() {
        use camcraft_app::{geometry::import::obj, model, operation, project, setup, world};
        use camcraft_motion::cl::MoveKind;
        use camcraft_motion::geometry::Point3;
        let dir = repo("examples/catalog/3d/rough-finish");
        let witness = Point3::new(-0.138840, -17.8, 10.0);
        let mesh = obj::parse(&fs::read(dir.join("model/rough-finish.obj")).expect("catalog OBJ")).expect("OBJ mesh");
        let surface = mesh.facets.iter().filter_map(|t| {
            let (ux, uy) = (t.b.x - t.a.x, t.b.y - t.a.y);
            let (vx, vy) = (t.c.x - t.a.x, t.c.y - t.a.y);
            let (px, py) = (witness.x - t.a.x, witness.y - t.a.y);
            let det = ux * vy - uy * vx;
            if det.abs() < 1e-12 { return None; }
            let (u, v) = ((px * vy - py * vx) / det, (ux * py - uy * px) / det);
            (u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9)
                .then_some(t.a.z + u * (t.b.z - t.a.z) + v * (t.c.z - t.a.z))
        }).reduce(f64::max).expect("mesh surface under witness");
        assert!((surface - 8.582312).abs() < 1e-5, "surface Z{surface}");
        assert!(surface < witness.z && witness.z < 22.0, "the witness is stock above the model");
        let w = world::new_world();
        let notes = project::load_noted(&w, &dir.join("rough-finish.camproj")).expect("rough-finish project");
        assert!(notes.is_empty(), "load notes: {notes:?}");
        let (_, pocket) = setup::operation_order(&w).into_iter().find(|(n, _)| n == "3D Pocket1").expect("pocket");
        let op = model::operation_of(&w, &pocket).expect("operation");
        let (path, _) = operation::generate(&w, &op, "3D Pocket1").expect("pocket generates");
        let mut cleared = false;
        for (i, m) in path.moves.iter().enumerate().filter(|(_, m)| m.kind != MoveKind::Rapid) {
            let axial = m.arc.is_none() && (m.from.x - m.to.x).hypot(m.from.y - m.to.y) < 1e-6
                && m.from.z > witness.z && m.to.z <= witness.z;
            let reaches = (m.to.x - witness.x).hypot(m.to.y - witness.y) < 3.0;
            assert!(!axial || !reaches || cleared, "move {i} plunges through outer stock at {witness:?}: {m:?}");
            // A chord wholly below the witness removes its stock. Shrink
            // the cutter by more than the chord sagitta to prove coverage.
            cleared |= m.segments3(0.001).iter().any(|(a, b)| {
                if a.z.max(b.z) > witness.z { return false; }
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                let f = if len2 > 0.0 {
                    (((witness.x - a.x) * dx + (witness.y - a.y) * dy) / len2).clamp(0.0, 1.0)
                } else { 0.0 };
                (witness.x - a.x - f * dx).hypot(witness.y - a.y - f * dy) < 2.998
            });
        }
        assert!(cleared, "a cutting entry eventually removes the witness stock");
    }

}
