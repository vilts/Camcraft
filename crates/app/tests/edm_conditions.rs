//! #531 (part of #290): the EDM condition table, user data in the
//! `.mch` `edm.conditions[]`. CamCraft reads only the geometry a path
//! needs (spark gap, wire diameter, undersize) and names each condition
//! by the control's own id; nothing is shipped (D4). The lookup an EDM
//! op calls at command creation refuses by name a missing condition or
//! a missing field; a garbled table refuses the file; the table rides
//! the project file.
//!
//! Gate map (#531): covers app::project (Machine round trip). The pure
//! parser, lookup and `is_sane` cases live in camcraft-post
//! (`machine::edm_conditions::tests`).
//! Provenance: our own SYNTHETIC conditions below - invented numbers of
//! a plausible size, from no machine's manual.

use camcraft_app::machine::{parse_mch, ConditionField};

const WIRE: &str = include_str!("edm/wire-edm.mch");

fn with_conditions(conds: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(WIRE).expect("template json");
    v["edm"]["conditions"] = serde_json::from_str(conds).expect("conditions json");
    v.to_string()
}

/// Synthetic: a rough, a skim and a finish, the finish stating no gap.
const TABLE: &str = r#"[
    { "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" },
    { "id": "C210", "gap": 0.02, "wire_diameter": 0.25, "role": "skim", "notes": "synthetic" },
    { "id": 330, "wire_diameter": 0.25, "role": "finish" }
]"#;

#[test]
fn the_table_rides_the_project_file() {
    use camcraft_app::commands::{History, LoadMachine};
    use camcraft_app::world::new_world;
    use camcraft_app::{machine, project};
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(&with_conditions(TABLE)).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    let path = std::env::temp_dir().join(format!("camcraft-531-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let (_, m) = machine::of(&back);
    let e = m.and_then(|m| m.edm).expect("edm survives");
    assert_eq!(e.conditions.len(), 3);
    assert_eq!(e.conditions[1].gap, Some(0.02));
    assert!(e.condition("back", "C210", &[ConditionField::Gap]).is_ok());
}
