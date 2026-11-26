//! Provenance: our authored unit-box corpus and the ISO 10303 short-name registry.
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step.

mod step_util;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepModel, import_step, import_step_model};
use camcraft_kernel::{KernelError, Measure};
use step_util::{CASE_TIMEOUT, bounded_cancel};

const INCH: &str = include_str!("../../../testdata/models/short_units_inch.step");
const METRE: &str = include_str!("../../../testdata/models/short_units_metre.step");
const CONE: &str = include_str!("../../../testdata/models/short_units_degree_cone.step");

fn import(text: &str) -> Result<(Vec<Measure>, Vec<f64>), KernelError> {
    let bytes = text.as_bytes().to_vec();
    bounded_cancel("short-name units", CASE_TIMEOUT, move |stop| {
        let mut kernel = SolidcoreKernel::default();
        import_step(&mut kernel, &bytes, stop).map(|model| {
            (
                model.solids.iter().map(|solid| solid.measure).collect(),
                model.length_units_mm,
            )
        })
    })
}

fn assert_scale(solids: &[Measure], units: &[f64], size: f64) {
    assert_eq!(units, [size], "the declared unit must survive import");
    assert_eq!(solids.len(), 1, "the unit cube must remain one solid");
    let measure = solids[0];
    assert!(
        (measure.volume / size.powi(3) - 1.0).abs() < 1e-8,
        "wrong volume: {measure:?}, expected a {size} mm cube"
    );
    for axis in 0..3 {
        assert!(measure.min[axis].abs() < 1e-8, "wrong minimum: {measure:?}");
        assert!(
            (measure.max[axis] / size - 1.0).abs() < 1e-8,
            "wrong maximum: {measure:?}, expected {size} mm"
        );
    }
}

fn correct_scale(text: &str, size: f64) {
    let (solids, units) = import(text).expect("short-name unit entities must import");
    assert_scale(&solids, &units, size);
}

#[test]
fn short_name_inch_import_keeps_scale() {
    correct_scale(INCH, 25.4);
}

#[test]
fn short_name_metre_import_keeps_scale() {
    correct_scale(METRE, 1000.0);
}

#[test]
fn short_context_only_import_keeps_scale() {
    for (text, size) in [(INCH, 25.4), (METRE, 1000.0)] {
        correct_scale(&text.replace("SUNT(", "SI_UNIT("), size);
    }
}

#[test]
fn short_si_unit_only_import_keeps_scale() {
    for (text, size) in [(INCH, 25.4), (METRE, 1000.0)] {
        correct_scale(&text.replace("GUAC(", "GLOBAL_UNIT_ASSIGNED_CONTEXT("), size);
    }
}

/// The frustum of `short_units_degree_cone.step` (#2973): base radius 10 mm,
/// height 10 mm, semi-angle 1 degree. Its volume and top radius depend on the
/// angle, so an angle read in radians cannot pass.
fn assert_degree_cone(text: &str) {
    let (solids, units) = import(text).expect("the degree cone must import");
    assert_eq!(units, [1.0], "the file states millimetres");
    assert_eq!(solids.len(), 1, "the frustum must import as one solid");
    let m = solids[0];
    let (r0, h) = (10.0_f64, 10.0_f64);
    let r1 = r0 + h * 1.0_f64.to_radians().tan();
    let volume = std::f64::consts::PI * h / 3.0 * (r0 * r0 + r0 * r1 + r1 * r1);
    assert!(
        (m.volume / volume - 1.0).abs() < 1e-6,
        "wrong volume: {m:?}, expected {volume} mm^3 for a 1 degree cone"
    );
    assert!((m.max[0] - r1).abs() < 1e-6, "wrong top radius: {m:?}, expected {r1}");
    assert!((m.max[2] - h).abs() < 1e-9 && m.min[2].abs() < 1e-9, "wrong height: {m:?}");
}

#[test]
fn short_name_degree_cone_keeps_its_angle() {
    assert_degree_cone(CONE);
}

#[test]
fn long_name_degree_cone_control_keeps_its_angle() {
    let control = CONE
        .replace("PLANUN(", "PLANE_ANGLE_UNIT(")
        .replace("GUAC(", "GLOBAL_UNIT_ASSIGNED_CONTEXT(")
        .replace("GC(", "GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(")
        .replace("SUNT(", "SI_UNIT(");
    assert_degree_cone(&control);
}

#[test]
fn unmarked_angle_unit_is_refused_by_name() {
    // The degree unit #13 without its plane angle marker: the reader cannot
    // know that it is an angle unit, and must not read angles in radians.
    let text = CONE.replace("NMDUNT(*) PLANUN() );", "NMDUNT(*) );");
    assert_ne!(text, CONE, "the fixture must have the marker to remove");
    match import(&text) {
        Err(KernelError::InvalidInput { detail, .. }) => assert!(
            detail.contains("#13") && detail.contains("plane angle"),
            "the refusal must name the unit: {detail}"
        ),
        other => panic!("an unmarked angle unit must be refused, got {other:?}"),
    }
}

/// The import must be refused, and the refusal must contain `named`.
fn refused(text: &str, named: &str) {
    let err = import(text).expect_err("a length unit read at 1 mm per unit must be refused");
    let msg = err.to_string();
    assert!(msg.contains(named), "the refusal must name `{named}`: {msg}");
}

/// #2919: a length unit, but no context that the reader can find states it.
#[test]
fn length_unit_without_unit_context_is_refused() {
    for text in [INCH, METRE] {
        let no_context = text.replace("GUAC((#12, #13, #14))", "");
        assert!(!no_context.contains("GUAC("), "the fixture must lose its unit context");
        refused(&no_context, "length unit #12 is in the file");
    }
}

/// #2919 review: a unit context that names a unit missing from the file.
#[test]
fn unit_context_naming_missing_unit_is_refused() {
    refused(
        &INCH.replace("GUAC((#12, #13, #14))", "GUAC((#12, #13, #14, #777))"),
        "unit context #11 names unit #777",
    );
}

/// #2919 review: the solid's own context states no length unit, while a
/// different context of the file states one.
#[test]
fn solid_in_unitless_context_is_refused() {
    let text = INCH
        .replace(
            "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11);",
            "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #902);\n\
             #902 = ( GEOMETRIC_REPRESENTATION_CONTEXT(3) REPRESENTATION_CONTEXT('', '') );",
        );
    assert!(text.contains("#902 ="), "the fixture must move the solid to a context with no unit");
    refused(&text, "context #902 states no length unit");
}

/// #2919 review (P1, case 1): the file states a length unit, but no
/// representation lists the solid, so the reader reads it at 1 mm per unit.
#[test]
fn solid_listed_by_no_representation_is_refused() {
    let text = INCH.replace(
        "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11);",
        "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (), #11);",
    );
    assert!(text.contains("('', (), #11)"), "the fixture must unlist the solid");
    refused(&text, "no representation gives it a context");
}

/// The unit cube as a surface model that the topology reader splits into
/// three pieces: closed shell #17 (a solid) and open shells #903 and #904
/// (two sheets), with representation #10 in context `ctx`.
fn split_model(ctx: &str) -> String {
    let mut text = INCH.replace(
        "#16 = MANIFOLD_SOLID_BREP('', #17);",
        "#16 = SHELL_BASED_SURFACE_MODEL('', (#17, #903, #904));\n\
         #903 = OPEN_SHELL('', (#18));\n\
         #904 = OPEN_SHELL('', (#25));",
    );
    if ctx != "#11" {
        text = text.replace(
            "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11);",
            &format!(
                "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), {ctx});\n\
                 {ctx} = ( GEOMETRIC_REPRESENTATION_CONTEXT(3) REPRESENTATION_CONTEXT('', '') );"
            ),
        );
    }
    assert!(text.contains("#904 = OPEN_SHELL"), "the fixture must hold a split surface model");
    text
}

fn import_model(text: &str) -> Result<StepModel, KernelError> {
    let bytes = text.as_bytes().to_vec();
    bounded_cancel("short-name units, model", CASE_TIMEOUT, move |stop| {
        let mut kernel = SolidcoreKernel::default();
        import_step_model(&mut kernel, &bytes, stop)
    })
}

/// #2919 review (P1, case 2): each piece split from a surface model sits in
/// the model's context. A unitless context refuses every piece by name.
#[test]
fn split_surface_model_pieces_in_unitless_context_are_refused() {
    let text = split_model("#902");
    assert!(text.contains("#902 = ("), "the fixture must move the model to a context with no unit");
    let err = import_model(&text).expect_err("pieces read at 1 mm per unit must be refused");
    let msg = err.to_string();
    assert!(msg.contains("3 of 3"), "each of the three pieces must be refused: {msg}");
    assert!(msg.contains("first #17"), "the solid piece must be refused: {msg}");
    assert!(msg.contains("context #902 states no length unit"), "the refusal must name the context: {msg}");
}

/// Control for the case above: the same split model in the inch context
/// keeps each piece, at the inch scale.
#[test]
fn split_surface_model_pieces_in_inch_context_keep_scale() {
    let model = import_model(&split_model("#11")).expect("the split model in the inch context must import");
    assert_eq!(model.length_units_mm, [25.4], "the declared unit must survive import");
    assert_eq!(model.solids.len(), 1, "closed shell #17 must stay one solid");
    assert_eq!(model.sheets.len(), 2, "open shells #903 and #904 must stay two sheets");
    let measure = model.solids[0].measure;
    assert!((measure.volume / 25.4f64.powi(3) - 1.0).abs() < 1e-8, "wrong volume: {measure:?}");
    for axis in 0..3 {
        assert!((measure.max[axis] / 25.4 - 1.0).abs() < 1e-8, "wrong maximum: {measure:?}");
    }
}

/// The inch fixture restated in millimetres: the one unit the file states
/// is the reader's own default.
fn millimetre(text: &str) -> String {
    let out = text.replace(
        "#12 = ( CNBSUN('INCH', #900) LNGUNT() NMDUNT(*) );",
        "#12 = ( LNGUNT() NMDUNT(*) SUNT(.MILLI.,.METRE.) );",
    );
    assert_ne!(out, text, "the fixture must restate its unit in millimetres");
    out
}

/// Stack-gate finding on the refusals above: when every unit the file
/// states is the millimetre, the reader's 1 mm default is the file's own
/// unit. An unlisted solid and the pieces of a model in a context with no
/// unit then import at the right scale, and are not refused.
#[test]
fn millimetre_file_keeps_items_with_no_unit_context() {
    let unlisted = millimetre(&INCH.replace(
        "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11);",
        "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (), #11);",
    ));
    let (solids, units) = import(&unlisted).expect("an unlisted solid of a mm file must import");
    assert_scale(&solids, &units, 1.0);
    let model = import_model(&millimetre(&split_model("#902")))
        .expect("the pieces of a mm file in a context with no unit must import");
    assert_eq!(model.length_units_mm, [1.0], "the file states millimetres");
    assert_eq!(model.solids.len(), 1, "closed shell #17 must stay one solid");
    assert_eq!(model.sheets.len(), 2, "open shells #903 and #904 must stay two sheets");
    let measure = model.solids[0].measure;
    assert!((measure.volume - 1.0).abs() < 1e-8, "wrong volume: {measure:?}");
}

/// The representation lists the placement #70 in place of the solid #16, so
/// no representation lists the solid and it has no unit context.
fn unlisted(text: &str) -> String {
    let out = text.replace(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11)",
        "ADVANCED_BREP_SHAPE_REPRESENTATION('', (#70), #11)",
    );
    assert_ne!(out, text, "the fixture must list the solid #16");
    out
}

/// #3079: the file states degrees, and the cone #16 has no context. The
/// reader would read its semi-angle in radians, so it must refuse #16.
#[test]
fn unlisted_item_in_a_degree_file_is_refused_by_name() {
    match import(&unlisted(CONE)) {
        Err(KernelError::InvalidResult { detail, .. }) => assert!(
            detail.contains("#16") && detail.contains("angle"),
            "the refusal must name the item and the angle unit: {detail}"
        ),
        other => panic!("an unlisted item in a degree file must be refused, got {other:?}"),
    }
}

/// #3079: the same cone, with a semi-angle that reads cleanly in radians.
/// The reader builds the solid, so the refusal must come after the build
/// and not only from a failed read.
#[test]
fn unlisted_item_that_reads_cleanly_is_refused_by_name() {
    let base = unlisted(CONE);
    let text = base.replace(
        "CONICAL_SURFACE('', #70, 10.0, 1.0)",
        "CONICAL_SURFACE('', #70, 10.0, 0.017453292519943295)",
    );
    assert_ne!(text, base, "the fixture must have the cone");
    match import(&text) {
        Err(KernelError::InvalidResult { detail, .. }) => assert!(
            detail.contains("#16") && detail.contains("plane angle unit #13"),
            "the refusal must name the item and the angle unit: {detail}"
        ),
        other => panic!("an unlisted item in a degree file must be refused, got {other:?}"),
    }
}

/// #3079 control: a millimetre file that states the radian keeps an
/// unlisted item, because the default angle unit is the stated one.
#[test]
fn unlisted_item_in_a_radian_file_is_kept() {
    let mm = METRE.replace("SUNT($,.METRE.)", "SUNT(.MILLI.,.METRE.)");
    assert_ne!(mm, METRE, "the fixture must state the metre");
    let text = mm.replace(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11)",
        "ADVANCED_BREP_SHAPE_REPRESENTATION('', (#81), #11)",
    );
    assert_ne!(text, mm, "the fixture must list the solid #16");
    let (solids, _) = import(&text).expect("a radian file keeps its unlisted item");
    assert_eq!(solids.len(), 1, "the unlisted cube must import");
    assert!((solids[0].volume - 1.0).abs() < 1e-8, "a 1 mm cube: {:?}", solids[0]);
}

/// Representation #10 moved to the context #990, which states no unit.
fn in_unitless_context(text: &str) -> String {
    let out = text.replace(
        "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #11);",
        "#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#16), #990);\n\
         #990 = ( GEOMETRIC_REPRESENTATION_CONTEXT(3) REPRESENTATION_CONTEXT('', '') );",
    );
    assert_ne!(out, text, "the fixture must list the solid #16 in context #11");
    out
}

/// #3079 review: the cone #16 sits in the context #990, which states no
/// angle unit, while the context #11 states degrees. The reader would read
/// its semi-angle in radians, so it must refuse #16, whether the cone then
/// fails to read (1.0 read as radians) or reads cleanly (1 degree written
/// as radians).
#[test]
fn item_in_a_context_with_no_angle_unit_in_a_degree_file_is_refused_by_name() {
    let base = in_unitless_context(CONE);
    let clean = base.replace(
        "CONICAL_SURFACE('', #70, 10.0, 1.0)",
        "CONICAL_SURFACE('', #70, 10.0, 0.017453292519943295)",
    );
    assert_ne!(clean, base, "the fixture must have the cone");
    for text in [base, clean] {
        match import(&text) {
            Err(KernelError::InvalidResult { detail, .. }) => assert!(
                detail.contains("#16") && detail.contains("context #990 states no plane angle unit"),
                "the refusal must name the item and its context: {detail}"
            ),
            other => panic!("an item with no angle unit in a degree file must be refused, got {other:?}"),
        }
    }
}

/// #3079 review control: in a millimetre file that states the radian, an
/// item in a context with no unit is kept: the default angle unit is the
/// stated one.
#[test]
fn item_in_a_context_with_no_angle_unit_in_a_radian_file_is_kept() {
    let mm = METRE.replace("SUNT($,.METRE.)", "SUNT(.MILLI.,.METRE.)");
    assert_ne!(mm, METRE, "the fixture must state the metre");
    let (solids, units) = import(&in_unitless_context(&mm)).expect("a radian file keeps the item");
    assert_eq!(units, [1.0], "the file states millimetres");
    assert_eq!(solids.len(), 1, "the cube must import");
    assert!((solids[0].volume - 1.0).abs() < 1e-8, "a 1 mm cube: {:?}", solids[0]);
}

#[test]
fn long_name_context_controls_import_with_correct_scale() {
    for (text, size) in [(INCH, 25.4), (METRE, 1000.0)] {
        let control = text
            .replace("GUAC(", "GLOBAL_UNIT_ASSIGNED_CONTEXT(")
            .replace("SUNT(", "SI_UNIT(");
        let (solids, units) = import(&control).expect("long-name control must import");
        assert_scale(&solids, &units, size);
    }
}
