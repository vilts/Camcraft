//! Provenance: issue #2069's preview contract and this repository's committed milling streams.
//! Gate map: covers camcraft_post::post::preview, camcraft_post::post::js.

use camcraft_post::post::{self, cnc::{self, Record}, js::Script, preview::{BLANK_TEMPLATE, DEFAULT_VMC_JSON, SAMPLES}};

fn machine() -> serde_json::Value {
    serde_json::from_str(DEFAULT_VMC_JSON).expect("embedded preview machine is JSON")
}

#[test]
fn blank_template_compiles_and_emits_no_lines_or_warnings() {
    Script::load(BLANK_TEMPLATE).expect("blank template compiles");
    for sample in SAMPLES {
        let output = post::post(BLANK_TEMPLATE, &machine(), &sample.stream().unwrap()).unwrap();
        assert!(output.program.is_empty(), "{}: {:?}", sample.name, output.program);
        assert!(output.warnings.is_empty(), "{}: {:?}", sample.name, output.warnings);
    }
}

#[test]
fn preview_samples_post_cleanly_and_deterministically() {
    for sample in SAMPLES {
        let stream = sample.stream().unwrap();
        assert_eq!(stream, sample.stream().unwrap(), "{}", sample.name);
        for cps in [include_str!("../../../assets/posts/fanuc.cps"), include_str!("../../../assets/posts/mazak.cps")] {
            let output = post::post(cps, &machine(), &stream).unwrap();
            assert!(!output.program.is_empty(), "{}", sample.name);
            assert!(output.warnings.is_empty(), "{}: {:?}", sample.name, output.warnings);
            assert_eq!(output.program, post::post(cps, &machine(), &stream).unwrap().program, "{}", sample.name);
        }
    }
}

#[test]
fn preview_samples_carry_setup_operation_tool_spindle_and_cycle_feed() {
    assert!(!SAMPLES.is_empty());
    let mut cycle_count = 0;
    for sample in SAMPLES {
        let records = cnc::parse(&sample.stream().unwrap()).unwrap();
        for name in ["SetupName", "OperationName", "OperationKind", "ToolKind", "ToolDiameter", "SpindleSpeed"] {
            assert!(records.iter().any(|r| matches!(r, Record::Param { name: key, .. } if key == name)), "{}: missing {name}", sample.name);
        }
        assert!(records.iter().any(|r| matches!(r, Record::Tool { number: 1, .. })), "{}", sample.name);
        for record in records {
            if let Record::Cycle(cycle) = record {
                cycle_count += 1;
                assert!(cycle.feed.is_some_and(|feed| feed.is_finite() && feed > 0.0), "{}: {:?}", sample.name, cycle);
            }
        }
    }
    assert!(cycle_count > 0, "preview must exercise canned drilling");
}
