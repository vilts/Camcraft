//! Provenance: synthetic comment inputs from the Sodick owner report in #3213.
//! Gate map: covers assets/posts/sodick-wire.cps, camcraft_post::post::engine.

use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::{engine, js::ProgramId};
use serde_json::{json, Value};

#[test]
fn sodick_comment_escape_golden() {
    let records = vec![
        Record::Param {
            name: "OperationName".into(),
            value: json!(r"Sq #1 [die] \x"),
        },
        Record::Param {
            name: "OperationKind".into(),
            value: json!(r"Wire # [Contour] \kind"),
        },
        Record::Param {
            name: "SetupName".into(),
            value: json!(r"Fixture # [top] \setup"),
        },
        Record::Param {
            name: "WireSide".into(),
            value: json!(r"outside # [profile] \side"),
        },
        Record::Param {
            name: "WorkOffset".into(),
            value: json!("G54"),
        },
        Record::Param {
            name: "EdmCondition".into(),
            value: json!("C110"),
        },
        Record::Section { begin: true },
        Record::Section { begin: false },
    ];
    let machine =
        json!({"kind": "wire_edm", "name": r"Sodick # [machine] \name", "edm": {"awt": true}});
    let id = ProgramId {
        notes: vec![r"Check # [stock] \note before cutting".into()],
        ..ProgramId::default()
    };
    let out = engine::run_program_with(
        include_str!("../../../assets/posts/sodick-wire.cps"),
        &machine,
        &cnc::serialize(&records),
        &Value::Null,
        &id,
    )
    .expect("synthetic Sodick comments post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        out.program.trim_end(),
        include_str!("golden/sodick_comment_escape.nc").trim_end()
    );
}
