//! #3662: the cutters that a converter reads from another tool library
//! travel in a `.camtools` file. A single-form thread mill that states
//! no pitch range reads as a range of its one pitch. The synthetic form
//! mill of #3662 (one V tooth on a necked shank) rides the file as a
//! single-form thread mill with its drawn cutter. The synthetic tapered
//! mills keep their flat and ball tips. Provenance: our own tests over
//! our own format; the numbers are the synthetic example of #3662.

use super::{cat, cutter_named, import, world_with};
use camcraft_app::commands::{AddTool, History, SetToolCutter};
use camcraft_app::spun::SpunPoint;
use camcraft_app::thread_mill;
use camcraft_app::tool_library::{TIP_FORM_VERSION, export_tool_library, parse_tool_library};
use camcraft_app::tools::{TaperTip, Tool, ToolKind};
use serde_json::{Value, json};

/// The file text of `tools`, with the `pitch_min` key taken out of each
/// definition (what a converter writes when it knows only the tooth).
fn without_range(tools: &[(&str, Tool)], cutter: Option<(&str, Vec<SpunPoint>)>) -> String {
    let (w, mut h) = world_with(tools);
    if let Some((n, pts)) = cutter {
        h.execute(&w, SetToolCutter::new(&w, n, Some(pts)).expect("drawn cutter"));
    }
    let mut v: Value = serde_json::from_str(&export_tool_library(&w).expect("export")).expect("json");
    for row in v["tools"].as_array_mut().expect("tools") {
        if let Some(def) = row["definition"].as_object_mut() {
            def.remove("pitch_min");
        }
    }
    v.to_string()
}

#[test]
fn a_single_form_thread_mill_without_a_pitch_range_loads() {
    let stated = cat("TMS-0600-P050-150");
    assert_eq!(stated.kind, ToolKind::ThreadMillSingle);
    let text = without_range(&[("TMS", stated.clone())], None);
    assert!(!text.contains("pitch_min"), "{text}");
    let lib = parse_tool_library(&text).expect("a single-form thread mill without a range loads");
    let t = &lib[0].tool;
    assert_eq!(t.pitch, stated.pitch);
    assert_eq!(t.pitch_min, t.pitch, "no range stated reads as the one stated pitch");
    // The operation cuts that pitch and refuses every other pitch by name.
    assert_eq!(thread_mill::resolve_pitch(t, Some(t.pitch)), Ok(t.pitch));
    for other in [stated.pitch_min, t.pitch * 2.0] {
        let why = thread_mill::resolve_pitch(t, Some(other)).expect_err("another pitch");
        assert!(why.contains("outside"), "{why}");
    }
    // It lands, and the file this build writes states the range, so an
    // older build reads it: no version step.
    let (w, mut h) = world_with(&[]);
    import(&w, &mut h, &text).expect("import");
    let again: Value = serde_json::from_str(&export_tool_library(&w).expect("export")).expect("json");
    assert_eq!(again["tools"][0]["definition"]["pitch_min"], json!(stated.pitch));
    assert!(again["version"].as_u64().is_some_and(|v| v < TIP_FORM_VERSION), "{again}");
}

/// The #3662 form mill, `(radius, height)`: (0,0) (2,0) (4,0.8) (2,1.6)
/// (2,9) (4,11) (4,40) (0,40), here as `(z, r)` points from the tip.
fn tooth_on_a_neck() -> Vec<SpunPoint> {
    [(0.0, 0.0), (0.0, 2.0), (0.8, 4.0), (1.6, 2.0), (9.0, 2.0), (11.0, 4.0), (40.0, 4.0), (40.0, 0.0)]
        .into_iter()
        .map(|(z, r)| SpunPoint::line(z, r))
        .collect()
}

#[test]
fn the_3662_form_mill_rides_the_file_as_a_necked_single_form_thread_mill() {
    // One V tooth: 2 mm out over 0.8 mm up each flank, 43.6 degrees.
    let flank = 2.0 * (0.8f64 / 2.0).atan().to_degrees();
    let tm = Tool {
        kind: ToolKind::ThreadMillSingle,
        diameter: 8.0,
        tip_angle: flank,
        pitch: 1.6,
        pitch_min: 1.6,
        flute_len: 1.6,
        shank_d: 8.0,
        shank_len: 38.4,
        ..cat("TMS-0600-P050-150")
    };
    assert_eq!(tm.why_not_sane(), None);
    let text = without_range(&[("V", tm.clone())], Some(("V", tooth_on_a_neck())));
    let lib = parse_tool_library(&text).expect("the form mill reads back");
    assert_eq!(lib[0].tool, tm, "no range reads as the one pitch: the same tool");
    let drawn = lib[0].cutter.clone().expect("the drawn tooth and neck ride the file");
    let (w, mut h) = world_with(&[]);
    import(&w, &mut h, &text).expect("import");
    assert_eq!(cutter_named(&w, "V"), Some(drawn), "the same drawn cutter lands");
    // The thread operation refuses its 43.6 degree form by name.
    assert!(thread_mill::form_refusal(&tm).is_some_and(|m| m.contains("43.6")));
}

#[test]
fn the_3662_tapered_mills_round_trip_with_their_tips() {
    // Tip 1 mm across (flat) or a 0.5 mm ball, 30 degrees each side,
    // 10 mm cutting length, 12 mm at the shoulder.
    let base = Tool { tip_angle: 60.0, diameter: 12.0, flute_len: 10.0, shank_d: 12.0, ..cat("CM-0600-90") };
    let flat = Tool { tip_form: TaperTip::Flat, tip_diameter: 1.0, ..base.clone() };
    let ball = Tool { tip_form: TaperTip::Ball, corner_r: 0.5, ..base };
    for t in [&flat, &ball] {
        assert_eq!(t.why_not_sane(), None, "{t:?}");
    }
    let w = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "FLAT", flat.clone()).expect("add"));
    h.execute(&w, AddTool::new(&w, "BALL", ball.clone()).expect("add"));
    let text = export_tool_library(&w).expect("export");
    let v: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["version"], json!(TIP_FORM_VERSION));
    let lib = parse_tool_library(&text).expect("this build reads its own file");
    let by = |n: &str| lib.iter().find(|t| t.name == n).map(|t| t.tool.clone());
    assert_eq!(by("FLAT"), Some(flat));
    assert_eq!(by("BALL"), Some(ball));
}
