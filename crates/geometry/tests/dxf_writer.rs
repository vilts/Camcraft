//! #743: clean-room ASCII DXF writer round-trip and named refusal pins.
//! Gate map (M4.4): covers camcraft_geometry::dxf.

use camcraft_geometry::dxf::{
    Document, DxfError, Layer, LayeredPrimitive, Primitive, Unit, Units, parse, write, write_bytes,
};
use camcraft_geometry::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn write_and_parse_round_trip_lines_arcs_circles_layers_and_units() {
    let doc = Document {
        units: Units::Known(Unit::Millimeters),
        elevation: Some(0.0),
        layers: vec![
            Layer {
                name: "CENTRELINE".into(),
                flags: 0,
                color: 5,
                line_type: "CENTER".into(),
            },
            Layer {
                name: "PROFILE".into(),
                flags: 0,
                color: 7,
                line_type: "CONTINUOUS".into(),
            },
        ],
        primitives: vec![
            LayeredPrimitive {
                layer: "CENTRELINE".into(),
                primitive: Primitive::Line {
                    start: Point2::new(0.0, 0.0),
                    end: Point2::new(0.0, 85.5),
                },
            },
            LayeredPrimitive {
                layer: "PROFILE".into(),
                primitive: Primitive::Line {
                    start: Point2::new(0.0, 25.0),
                    end: Point2::new(12.5, 25.0),
                },
            },
            LayeredPrimitive {
                layer: "PROFILE".into(),
                primitive: Primitive::Arc {
                    center: Point2::new(12.5, 35.0),
                    radius: 10.0,
                    start_angle: -FRAC_PI_2,
                    sweep_angle: FRAC_PI_2,
                },
            },
            LayeredPrimitive {
                layer: "PROFILE".into(),
                primitive: Primitive::Arc {
                    center: Point2::new(32.5, 45.0),
                    radius: 10.0,
                    start_angle: PI * 0.75,
                    sweep_angle: PI * 0.75,
                },
            },
            LayeredPrimitive {
                layer: "PROFILE".into(),
                primitive: Primitive::Circle {
                    center: Point2::new(50.0, 50.0),
                    radius: 3.25,
                },
            },
        ],
        ..Document::default()
    };

    let text = write(&doc).expect("writer writes valid document");
    let bytes = write_bytes(&doc).expect("write_bytes matches");
    assert_eq!(text.as_bytes(), bytes.as_slice());

    let reparsed = parse(&bytes).expect("reader parses written DXF");
    assert_eq!(reparsed.units, Units::Known(Unit::Millimeters));
    assert_eq!(reparsed.layers, doc.layers);
    assert_eq!(reparsed.elevation, Some(0.0));
    assert_eq!(reparsed.primitives.len(), doc.primitives.len());

    for (got, want) in reparsed.primitives.iter().zip(doc.primitives.iter()) {
        assert_eq!(got.layer, want.layer);
        match (&got.primitive, &want.primitive) {
            (
                Primitive::Line { start: gs, end: ge },
                Primitive::Line { start: ws, end: we },
            ) => {
                near(gs.x, ws.x);
                near(gs.y, ws.y);
                near(ge.x, we.x);
                near(ge.y, we.y);
            }
            (
                Primitive::Arc {
                    center: gc,
                    radius: gr,
                    start_angle: gsa,
                    sweep_angle: gsw,
                },
                Primitive::Arc {
                    center: wc,
                    radius: wr,
                    start_angle: wsa,
                    sweep_angle: wsw,
                },
            ) => {
                near(gc.x, wc.x);
                near(gc.y, wc.y);
                near(*gr, *wr);
                let delta = (*gsa - wsa.rem_euclid(TAU)).rem_euclid(TAU);
                assert!(delta < 1e-10 || (TAU - delta) < 1e-10, "{gsa} vs {wsa}");
                near(*gsw, *wsw);
            }
            (
                Primitive::Circle { center: gc, radius: gr },
                Primitive::Circle { center: wc, radius: wr },
            ) => {
                near(gc.x, wc.x);
                near(gc.y, wc.y);
                near(*gr, *wr);
            }
            other => panic!("mismatched primitive kind: {other:?}"),
        }
    }
}

#[test]
fn writer_refuses_invalid_inputs_by_name() {
    let mut doc = Document::default();
    doc.primitives.push(LayeredPrimitive {
        layer: "PROFILE".into(),
        primitive: Primitive::Line {
            start: Point2::new(1.0, 2.0),
            end: Point2::new(1.0, 2.0),
        },
    });
    assert!(matches!(write(&doc), Err(DxfError(msg)) if msg.contains("LINE")));

    let mut bad_arc = Document::default();
    bad_arc.primitives.push(LayeredPrimitive {
        layer: "PROFILE".into(),
        primitive: Primitive::Arc {
            center: Point2::new(0.0, 0.0),
            radius: 5.0,
            start_angle: 0.0,
            sweep_angle: TAU,
        },
    });
    assert!(matches!(write(&bad_arc), Err(DxfError(msg)) if msg.contains("ARC")));

    let mut nan_circle = Document::default();
    nan_circle.primitives.push(LayeredPrimitive {
        layer: "0".into(),
        primitive: Primitive::Circle {
            center: Point2::new(f64::NAN, 0.0),
            radius: 5.0,
        },
    });
    assert!(matches!(write(&nan_circle), Err(DxfError(msg)) if msg.contains("CIRCLE")));

    let mut bad_layer = Document::default();
    bad_layer.layers.push(Layer {
        name: "bad\nlayer".into(),
        flags: 0,
        color: 7,
        line_type: "CONTINUOUS".into(),
    });
    assert!(matches!(write(&bad_layer), Err(DxfError(msg)) if msg.contains("LAYER")));

    let unsupported_unit = Document {
        units: Units::Unsupported(999),
        ..Document::default()
    };
    assert!(matches!(write(&unsupported_unit), Err(DxfError(msg)) if msg.contains("$INSUNITS")));
}

#[test]
fn merged_export_writer_retains_main_zero_length_line_refusal() {
    use camcraft_geometry::dxf::{ExportEntity, to_text};

    let origin = Point2::new(0.0, 0.0);
    let entity = ExportEntity::Line {
        layer: "PROFILE".into(),
        start: origin,
        end: origin,
    };
    let err = to_text(Some(Unit::Millimeters), &[], &[entity], &[])
        .expect_err("main's writer must still refuse zero-length LINE after the merge");
    assert!(err.0.contains("LINE") && err.0.contains("zero-length"), "{err}");
}
