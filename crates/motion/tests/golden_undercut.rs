//! Committed golden CL dumps for the Slot Undercut generator (#1444):
//! a T-slot along a straight slot, a keyseat along a curved one, a
//! dovetail and a lollipop - the exact output pinned beside the
//! behavioural laws of `motion::machining2d::undercut`'s own tests.
//! Provenance: the project CL text grammar in docs/CNC-INTERMEDIATE.md.
//! Regenerate with `WRITE_GOLDEN=1 cargo test -p camcraft-motion --test golden_undercut`,
//! then review the diff in crates/motion/tests/golden/.
//!
//! Gate map: covers camcraft_motion::machining2d::undercut and camcraft_tools::undercut.

use camcraft_motion::cl::{text::to_text, Toolpath};
use camcraft_motion::geo2d::Point2;
use camcraft_motion::machining2d::{undercut, undercut_station, UndercutParams};
use camcraft_motion::tools::ToolKind;
use camcraft_tools::undercut::{cutter, Undercut};

/// Keep the fixtures in the generator crate and use the shared CL serializer.
fn write_golden(name: &str, path: &Toolpath) {
    let text = to_text(path).expect("the generated path has a valid CL text form");
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&file, text).expect("cannot write the golden CL file");
        return;
    }
    let want = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("cannot read golden {}: {e}", file.display()));
    assert_eq!(
        want.replace("\r\n", "\n"),
        text,
        "golden {} is stale",
        file.display()
    );
}

fn params(cutter: Undercut, width: f64, bulge: f64) -> UndercutParams {
    UndercutParams {
        cutter,
        width,
        bulge,
        run_in: 15.0,
        neck_slot: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        bottom_z: -12.0,
        stepover: 2.0,
        feed: 120.0,
        plunge_feed: 60.0,
        climb: true,
    }
}

/// The path, with the law every golden here must keep: Z changes at the
/// entry station and nowhere else.
fn cut(p: &UndercutParams) -> Toolpath {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(60.0, 0.0));
    let t = undercut(a, b, p).unwrap();
    let s = undercut_station(a, b, p).unwrap();
    for m in t.moves.iter().filter(|m| m.from.z != m.to.z) {
        assert!(
            (m.from.x - s.x).hypot(m.from.y - s.y) < 1e-9
                && (m.to.x - s.x).hypot(m.to.y - s.y) < 1e-9,
            "{m:?}"
        );
    }
    t
}

#[test]
fn slot_undercut_tslot_straight_matches_golden() {
    // A 20 x 6 head on an 8 mm neck in a 26 mm T-slot whose 14 mm neck
    // slot was cut earlier: the centre pass, then two widenings.
    let head = Undercut::of(&cutter(ToolKind::TSlot, 20.0, 6.0, 8.0, 30.0, 0.0)).unwrap();
    let t = cut(&UndercutParams {
        neck_slot: 14.0,
        ..params(head, 26.0, 0.0)
    });
    assert!(!t.has_arcs());
    write_golden("slot_undercut_tslot_straight.cl", &t);
}

#[test]
fn slot_undercut_keyseat_arc_matches_golden() {
    // A 16 x 3 keyseat cutter along an arc (bulge 0.3), conventional.
    let head = Undercut::of(&cutter(ToolKind::Keyseat, 16.0, 3.0, 6.0, 25.0, 0.0)).unwrap();
    let t = cut(&UndercutParams {
        climb: false,
        ..params(head, 20.0, 0.3)
    });
    assert!(t.has_arcs());
    write_golden("slot_undercut_keyseat_arc.cl", &t);
}

#[test]
fn slot_undercut_dovetail_straight_matches_golden() {
    // A 60 degree dovetail, 20 mm at its tip, in a 24 mm dovetail.
    let head = Undercut::of(&cutter(ToolKind::Dovetail, 20.0, 5.0, 8.0, 30.0, 60.0)).unwrap();
    let t = cut(&params(head, 24.0, 0.0));
    write_golden("slot_undercut_dovetail_straight.cl", &t);
}

#[test]
fn slot_undercut_lollipop_arc_matches_golden() {
    // A 10 mm ball on a 4 mm neck: the 14 mm groove is measured at the
    // ball's equator, so the passes shift 2 mm - not 7.
    let head = Undercut::of(&cutter(ToolKind::Lollipop, 10.0, 5.0, 4.0, 30.0, 0.0)).unwrap();
    let t = cut(&params(head, 14.0, -0.3));
    write_golden("slot_undercut_lollipop_arc.cl", &t);
}
