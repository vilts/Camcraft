// Provenance: our own manifest and posting refusal contracts (#1335).
use super::*;

#[test]
fn waterjet_refuses_undeclared_posts_and_accepts_only_an_explicit_manifest() {
    for script in ["capabilities = 1;", "capabilities = 2;", "wireEdm = true;"] {
        assert!(!serves(script, Kind::Waterjet).expect("manifest loads"), "{script}");
    }
    assert!(serves("serves = [\"waterjet\"];", Kind::Waterjet).expect("explicit manifest"));
    assert!(!serves("serves = [\"mill\"];", Kind::Waterjet).expect("other explicit kind"));
    let world = crate::world::new_world();
    let machine = Machine { kind: Some("waterjet".into()), ..Default::default() };
    let why = refusal(&world, &["Jet setup".into()], &["Jet cut"],
        &("Waterjet machine".into(), Some(machine)), "capabilities = 1;", Some("mill.cps"))
        .expect("loads").expect("refuses");
    for word in ["Jet setup", "Jet cut", "mill.cps", "does not serve", "waterjet"] {
        assert!(why.contains(word), "{word}: {why}");
    }
}

#[test]
fn user_wire_manifest_serves_only_wire_edm() {
    let wire = "wireEdm = true;";
    assert!(serves(wire, Kind::WireEdm).expect("wire manifest loads"));
    for kind in [
        Kind::Mill,
        Kind::Lathe,
        Kind::MillTurn,
        Kind::Plasma,
        Kind::SinkerEdm,
    ] {
        assert!(
            !serves(wire, kind).expect("wire manifest loads"),
            "{kind:?}"
        );
    }
    for script in [
        "capabilities = 1;",
        "wireEdm = false;",
        "wireEdm = 1;",
        "wireEdm = \"true\";",
    ] {
        assert!(
            !serves(script, Kind::WireEdm).expect("manifest loads"),
            "{script}"
        );
    }
}

#[test]
fn user_wire_manifest_keeps_explicit_serves_restriction() {
    assert!(
        !serves("wireEdm = true; serves = [\"mill\"];", Kind::WireEdm).expect("manifest loads")
    );
}

#[test]
fn cutting_user_script_is_refused_on_wire_edm_by_name() {
    let world = crate::world::new_world();
    let machine = crate::machine::parse_mch(include_str!("../../tests/edm/wire-iso-fixture.mch"))
        .expect("fixture parses");
    let why = refusal(
        &world,
        &["Wire setup".to_owned()],
        &["Wire cut"],
        &("Wire machine".to_owned(), Some(machine)),
        "capabilities = 1;",
        Some("user-mill.cps"),
    )
    .expect("manifest loads")
    .expect("cutting script refuses");
    for word in [
        "Wire setup",
        "Wire cut",
        "user-mill.cps",
        "does not serve",
        "wire EDM",
    ] {
        assert!(why.contains(word), "{word:?} missing from {why}");
    }
}
