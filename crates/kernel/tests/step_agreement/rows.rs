//! The pinned table of the STEP bench (step_agreement.rs): per fixture its
//! length unit, the RECORDED truck reading and the pinned verdicts.
//! Provenance: our own bench design; the truck numbers are the bench's own
//! printout of truck-stepio 0.3.0 + truck-meshalgo 0.4.0 (ruststep at the
//! #887 git pin), taken at commit 4b694c7c7 before #2790 removed the reader
//! (docs/decisions/2790-remove-truck-step.md).
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (read_step,
//! import_step).

/// What truck-stepio made of a file when the bench last ran it: the mesh
/// numbers in millimetres at the app's chord target, or the refusal.
pub enum Truck {
    /// The first words of truck's refusal.
    Refused(&'static str),
    Read {
        /// Shells in the file's table (the app meshed each one).
        shells: usize,
        /// Sum of each shell's absolute mesh volume, mm^3.
        volume: f64,
        /// Sum of the triangle areas, mm^2.
        area: f64,
        min: [f64; 3],
        max: [f64; 3],
        /// Triangle sides used by one triangle only (0: watertight).
        open_edges: usize,
        /// The chord the meshes were made at, mm.
        chord_mm: f64,
    },
    /// A fixture added after #2790 removed the truck reader: there is no
    /// truck reading, and none is made up. Only the solidcore side and the
    /// length unit are judged.
    NotRecorded,
}

/// One fixture: its length unit (mm per unit, what crates/app's
/// `step_units` reads from it, pinned in crates/app/tests/import.rs), the
/// recorded truck reading, and the pinned verdicts of the importer
/// comparison and of the mesh comparison.
pub struct Row {
    pub file: &'static str,
    pub mm_per_unit: f64,
    pub truck: Truck,
    pub import: &'static str,
    pub mesh: &'static str,
}

/// A truck mesh as recorded, in the order the printout gave it.
const fn read(
    shells: usize,
    volume: f64,
    area: f64,
    min: [f64; 3],
    max: [f64; 3],
    open_edges: usize,
    chord_mm: f64,
) -> Truck {
    Truck::Read {
        shells,
        volume,
        area,
        min,
        max,
        open_edges,
        chord_mm,
    }
}

const UNIT_BOX: ([f64; 3], [f64; 3]) = ([0.0; 3], [1.0; 3]);

/// The pinned verdicts.
///
/// Both Milling-test files agree since the pin of #2781: the port's
/// "tight" bounding box took the whole cylinder of the trimmed face before
/// (Y -30..30 against the flat at Y = 19.719, #939), and now stops at the
/// flat as the file's own vertices and both meshes do.
///
/// #917: the pcurves with no closed form (a free-form edge on a spline face)
/// are made by the kernel (`solidcore_trims`), so bar_cross_holes, fig8 and
/// fillet_corner read; the closed forms are in `step_closed_form.rs`.
///
/// The three tool_csys files hold a coordinate system and no solid: both
/// importers refuse them by name.
pub const ROWS: &[Row] = &[
    Row {
        file: "Milling-test-inch.step",
        mm_per_unit: 25.4,
        truck: read(
            1,
            52802.576229,
            11142.078033,
            [-29.998750481868434, -30.000000000000014, -5.000000000000002],
            [29.998750481868434, 19.7190085969899, 20.000000000000004],
            830,
            0.005,
        ),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "Milling-test.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            52802.576229,
            11142.078033,
            [-29.99875048186842, -30.0, -5.0],
            [29.99875048186842, 19.7190085969899, 20.0],
            854,
            0.005,
        ),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "bad_entity_mid_data.step",
        mm_per_unit: 1.0,
        truck: Truck::Refused("syntax error: Error while tokenizing STEP input"),
        import: "both refuse (solidcore: read_step: invalid input: STEP syntax error)",
        mesh: "no exact solid to judge against",
    },
    // Refused at its pcurves before #917; the kernel makes them now. Truck's
    // mesh has 1090 open edges, so its volume is short: DISAGREEMENT.
    Row {
        file: "bar_cross_holes.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            45816.209845,
            11142.873937,
            [0.0, -24.996557626401895, -24.999928878018785],
            [40.00000000000001, 24.996557626401895, 24.9999288780188],
            1090,
            0.005,
        ),
        import: "disagree: volume, area",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "box.step",
        mm_per_unit: 1.0,
        truck: read(1, 1.0, 6.0, UNIT_BOX.0, UNIT_BOX.1, 0, 0.001),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    // #1697: truck's face meter refused face 6 (the B-spline that jumps);
    // solidcore refuses the solid by name: an edge 0.25 mm off its face is
    // over the cap of `solidcore_trims::MAX_EDGE_GAP_MM` (#917).
    Row {
        file: "bspline_discontinuous.step",
        mm_per_unit: 1.0,
        truck: Truck::Refused(
            "shell #17: face 6 of 6 (B-spline surface) not meshed: its subdivision did not converge within the evaluation cap",
        ),
        import: "both refuse (solidcore: #16 at read: pcurves failed on)",
        mesh: "no exact solid to judge against",
    },
    // FINDING (#1697): solidcore's mesh of the dome stands off the exact
    // volume by more than REL_TOL; the importers agree.
    Row {
        file: "bspline_dome.step",
        mm_per_unit: 1.0,
        truck: read(1, 1.022486, 6.007540, [0.0; 3], [1.0, 1.0, 1.05], 0, 0.001),
        import: "agree",
        mesh: "off: volume",
    },
    Row {
        file: "empty_list.step",
        mm_per_unit: 1.0,
        truck: read(1, 1.0, 6.0, UNIT_BOX.0, UNIT_BOX.1, 0, 0.001),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    // #2209 (PR #2215): two curved faces that meet at one smooth crease.
    // Refused at its pcurves before #917 (a NURBS on a spline surface);
    // the kernel makes them now.
    Row {
        file: "fig8.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            136543.723507,
            29334.831646,
            [-42.50000000000001, -29.99511031693924, 0.0],
            [42.50000000000003, 166.0153359361301, 20.000000000000018],
            4100,
            0.005,
        ),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    // The corner blend: refused at its pcurves before #917; the kernel makes
    // them now. Truck's mesh has 121 open edges, so its volume is short.
    Row {
        file: "fillet_corner.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            20651.676864,
            4036.404279,
            [-15.000000000000002, -15.0, -15.0],
            [15.0, 15.0, 15.0],
            121,
            0.005,
        ),
        import: "disagree: volume, area",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "hole_on_pocket_wall.step",
        mm_per_unit: 1.0,
        truck: Truck::NotRecorded,
        import: "no truck reading; unit agrees",
        mesh: "off: area",
    },
    Row {
        file: "lathe_holder_offset.step",
        mm_per_unit: 1.0,
        truck: read(1, 50200.0, 10740.0, [-21.0, 12.0, -7.0], [4.0, 130.0, 13.0], 0, 0.005),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "modular_clamp_unit.step",
        mm_per_unit: 1.0,
        truck: read(1, 16560.0, 3924.0, [-12.0, 0.0, -8.5], [12.0, 30.0, 14.5], 0, 0.005),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "modular_lathe_head.step",
        mm_per_unit: 1.0,
        truck: read(1, 2112.0, 1000.0, [2.0, -6.0, 1.0], [18.0, 6.0, 12.0], 0, 0.005),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    // #3034: the recorded QA part (a mould base with an ellipsoid boss
    // whose spline face ends in a pole). Added after #2790, so truck has
    // no recorded reading.
    Row {
        file: "q02_3d_mold.step",
        mm_per_unit: 1.0,
        truck: Truck::NotRecorded,
        import: "no truck reading; unit agrees",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "sample_ap214.step",
        mm_per_unit: 1000.0,
        truck: read(
            3,
            17149.486184,
            13874.403106,
            [-32.349603944426775, -4.940300000000888, -32.35702896644693],
            [32.359504099999995, 1.58750000000015, 85.85074105104079],
            3196,
            0.01,
        ),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    // #2973: a 1 degree draft frustum whose angle unit has the short name
    // PLANUN. It was added after #2790, so truck has no recorded reading.
    // The bench's raw tessellator does not expand the short names, so it
    // reads the angle in radians and its pcurve check refuses the cone.
    Row {
        file: "short_units_degree_cone.step",
        mm_per_unit: 1.0,
        truck: Truck::NotRecorded,
        import: "no truck reading; unit agrees",
        mesh: "no mesh: solid #16 not read: pcurves failed",
    },
    // #2919: the model reader expands unit aliases; the raw tessellator
    // still assumes millimetres. The geometry is in long form. These two
    // truck readings are from the live bench at 72ab66fc1, not 4b694c7c7.
    Row {
        file: "short_units_inch.step",
        mm_per_unit: 25.4,
        truck: Truck::Refused("a shell's mesh is not finite"),
        import: "truck refuses: a shell's mesh is not finite",
        mesh: "off: volume, area",
    },
    Row {
        file: "short_units_metre.step",
        mm_per_unit: 1000.0,
        truck: Truck::Refused("a shell's mesh is not finite"),
        import: "truck refuses: a shell's mesh is not finite",
        mesh: "off: volume, area",
    },
    // #3034: a rational B-spline dome that ends in a pole at a vertex. It
    // was added after #2790, so truck has no recorded reading.
    Row {
        file: "sphere_quarter_pole.step",
        mm_per_unit: 1.0,
        truck: Truck::NotRecorded,
        import: "no truck reading; unit agrees",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "spline_revolved.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            2725.306859,
            1193.769581,
            [-3.4542034091041917e-13, -2.4492935982947065e-15, 0.0],
            [21.0, 21.0, 11.000000000000004],
            370,
            0.005,
        ),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "tool_csys_box_iso13399.step",
        mm_per_unit: 1.0,
        truck: read(1, 16000.0, 4000.0, [-10.0, -10.0, -40.0], [10.0, 10.0, 0.0], 0, 0.005),
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "tool_csys_degenerate.step",
        mm_per_unit: 1.0,
        truck: Truck::Refused("no shells found"),
        import: "both refuse (solidcore: the file holds no solid)",
        mesh: "no exact solid to judge against",
    },
    Row {
        file: "tool_csys_iso13399.step",
        mm_per_unit: 1.0,
        truck: Truck::Refused("no shells found"),
        import: "both refuse (solidcore: the file holds no solid)",
        mesh: "no exact solid to judge against",
    },
    Row {
        file: "tool_csys_none.step",
        mm_per_unit: 1.0,
        truck: Truck::Refused("no shells found"),
        import: "both refuse (solidcore: the file holds no solid)",
        mesh: "no exact solid to judge against",
    },
    // #2780: a block with one strongly twisted B-spline flank (our own
    // fixture), and the same block with one vertex 0.5 mm off its curves.
    // DISAGREEMENT: truck read the flank block as an open shell of volume
    // 0; solidcore reads the closed solid at its exact 8000 mm^3. The block
    // with the vertex off is refused by solidcore by name.
    Row {
        file: "twisted_flank.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            0.0,
            5768.882041,
            [0.0, -2.7444713168731127e-12, -4.116706975310415e-12],
            [40.00000000000124, 40.00000000000401, 60.0000000000111],
            10,
            0.005,
        ),
        import: "disagree: volume, area",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "twisted_flank_off.step",
        mm_per_unit: 1.0,
        truck: read(
            1,
            16109.191950,
            5590.048263,
            [0.0; 3],
            [40.230769230752536, 40.230769230752536, 60.40946981286047],
            10,
            0.005,
        ),
        // #3767: solidcore fa67b3f5 builds the pcurves; its vertex check
        // refuses the solid (was "#117 at read: pcurves failed on").
        import: "solidcore refuses: #117 at check: a vertex sits",
        mesh: "no exact solid to judge against",
    },
];
