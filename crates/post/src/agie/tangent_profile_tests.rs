//! Native lines and arcs of a convex tangent DXF loop punch (#2983).
//! Provenance: the examples/agie rounded-punch and three-corner-punch drawings of #2983.
use super::round_profile::iso;
use super::tangent_profile::prepare;
use super::tests::punch;
use super::*;
use camcraft_geometry::dxf::{LayeredPrimitive, Primitive};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::sketch_chains::{ChainOptions, chain};
use camcraft_geometry::wire_round_profile::{TangentProfile, WireProfile};

fn tangent(primitives: Vec<Primitive>, shift: [f64; 2]) -> TangentProfile {
    let items: Vec<_> = primitives
        .into_iter()
        .map(|primitive| LayeredPrimitive {
            layer: "profile".into(),
            primitive,
        })
        .collect();
    let chains = chain(&items, ChainOptions::default()).unwrap();
    let mut shape = WireProfile::from_chain(&chains.closed[0]).unwrap();
    shape.place(|p| Point2::new(p.x + shift[0], p.y + shift[1]));
    match shape {
        WireProfile::Tangent(shape) => shape,
        WireProfile::Round(_) => panic!("a tangent loop"),
    }
}

fn line(a: [f64; 2], b: [f64; 2]) -> Primitive {
    Primitive::Line {
        start: Point2::new(a[0], a[1]),
        end: Point2::new(b[0], b[1]),
    }
}

fn arc(c: [f64; 2], radius: f64, from_deg: f64, to_deg: f64) -> Primitive {
    Primitive::Arc {
        center: Point2::new(c[0], c[1]),
        radius,
        start_angle: from_deg.to_radians(),
        sweep_angle: (to_deg - from_deg).to_radians(),
    }
}

/// The R6/R4 corners of examples/agie/rounded-punch.dxf.
fn rounded(shift: [f64; 2]) -> TangentProfile {
    tangent(
        vec![
            line([-16., -14.], [14., -14.]),
            arc([14., -8.], 6., 270., 360.),
            line([20., -8.], [20., 10.]),
            arc([16., 10.], 4., 0., 90.),
            line([16., 14.], [-14., 14.]),
            arc([-14., 8.], 6., 90., 180.),
            line([-20., 8.], [-20., -10.]),
            arc([-16., -10.], 4., 180., 270.),
        ],
        shift,
    )
}

fn job(start: [f64; 2]) -> AgieJob {
    AgieJob {
        contour: Vec::new(),
        start: Some(start),
        ..punch()
    }
}

#[test]
fn a_mixed_radius_loop_keeps_each_nominal_arc_and_its_straight_entry() {
    let (blocks, setup) = prepare(&job([0., -21.]), &rounded([0., 0.]), "ROUND20").unwrap();
    assert_eq!(setup.start_hole_um, [0, -21_000]);
    assert_eq!(
        iso("ROUND20.ISO", &blocks),
        "( ROUND20.ISO )\r\n\
         N0001 G00 X0.00000 Y-14.00000 ;\r\n\
         N0002 G01 X14.00000 Y-14.00000 ;\r\n\
         N0003 G03 X20.00000 Y-8.00000 I0.00000 J6.00000 ;\r\n\
         N0004 G01 X20.00000 Y10.00000 ;\r\n\
         N0005 G03 X16.00000 Y14.00000 I-4.00000 J0.00000 ;\r\n\
         N0006 G01 X-14.00000 Y14.00000 ;\r\n\
         N0007 G03 X-20.00000 Y8.00000 I0.00000 J-6.00000 ;\r\n\
         N0008 G01 X-20.00000 Y-10.00000 ;\r\n\
         N0009 G03 X-16.00000 Y-14.00000 I4.00000 J0.00000 ;\r\n\
         N0010 G01 X0.00000 Y-14.00000 ;\r\n\
         N0011 M02 ;\r\n"
    );
}

#[test]
fn an_oblique_tangent_side_joins_three_distinct_arcs() {
    let shape = tangent(
        vec![
            line([0., -5.], [24., -5.]),
            arc([24., 0.], 5., 270., 53.130_102_354_155_98 + 360.),
            line([27., 4.], [3., 22.]),
            arc([0., 18.], 5., 53.130_102_354_155_98, 180.),
            line([-5., 18.], [-5., 0.]),
            arc([0., 0.], 5., 180., 270.),
        ],
        [0., 0.],
    );
    let (blocks, _) = prepare(&job([12., -12.]), &shape, "THREEARC").unwrap();
    let text = iso("THREEARC.ISO", &blocks);
    assert_eq!(text.matches(" G03 ").count(), 3, "{text}");
    for word in [
        "N0001 G00 X12.00000 Y-5.00000 ;",
        "G03 X27.00000 Y4.00000 I0.00000 J5.00000 ;",
        "G01 X3.00000 Y22.00000 ;",
        "G03 X-5.00000 Y18.00000 I-3.00000 J-4.00000 ;",
        "G03 X0.00000 Y-5.00000 I5.00000 J0.00000 ;",
        "N0008 G01 X12.00000 Y-5.00000 ;",
    ] {
        assert!(text.contains(word), "{word}: {text}");
    }
}

#[test]
fn a_short_or_inner_entry_an_offgrid_joint_or_a_thin_stock_margin_is_refused() {
    let refused = |start: [f64; 2], shift: [f64; 2]| {
        prepare(&job(start), &rounded(shift), "ROUND20").unwrap_err()
    };
    assert!(refused([-15., -21.], [0., 0.]).contains("at least 3 mm"));
    assert!(refused([0., -12.], [0., 0.]).contains("at least 3 mm"));
    assert!(refused([0., -21.], [0.000_003, 0.]).contains("five-decimal"));
    // The stock is X-40..40: the shifted X39.8 extremum leaves 0.2 mm.
    assert!(refused([19.8, -21.], [19.8, 0.]).contains("0.4 mm"));
    // At X39.6 the extremum leaves exactly 0.4 mm.
    assert!(prepare(&job([19.6, -21.]), &rounded([19.6, 0.]), "R").is_ok());
    // A die, and a plate without a control check for native arcs.
    let die = AgieJob {
        side: AgieSide::Die,
        ..job([0., -21.])
    };
    assert!(prepare(&die, &rounded([0., 0.]), "R").is_err());
    let tall = AgieJob {
        stock_min: [-40., -30., -40.],
        ..job([0., -21.])
    };
    assert!(prepare(&tall, &rounded([0., 0.]), "R").is_err());
}
