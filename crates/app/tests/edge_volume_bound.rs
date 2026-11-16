//! Provenance: the cached-record operation bound from #529, extended for #1891.
//! Gate map: covers app::stock_sim (breaks::bounded, declared_width).

use camcraft_app::cl::{BreakFrame, BreakProfile, EdgeBreak, EdgeBreakVolume};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType};
use camcraft_app::stock_sim::breaks;

fn params(ty: OpType) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(), tilt: None, stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0), top_z: None, stepdown: None,
        feed: 1200.0, plunge_feed: 300.0, climb: true, blend_turns: false,
    };
    operation::retype_params(&base, ty.ordinal())
}

fn record(width: f64) -> EdgeBreak {
    EdgeBreak {
        edge: vec![Point3::new(0.0, 0.0, 10.0), Point3::new(10.0, 0.0, 10.0)],
        depth: width / 2.0, slope: 0.0,
        volume: Some(EdgeBreakVolume {
            width, profile: BreakProfile::Flat,
            frames: vec![BreakFrame { outward: [0.0, 0.0, 1.0], across: [0.0, 1.0, 0.0] }],
        }),
    }
}

#[test]
fn framed_records_obey_their_operations_width_and_kind() {
    for ty in [OpType::Chamfer2D, OpType::Deburr] {
        let params = params(ty);
        let width = breaks::declared_width(&params).expect("width");
        let good = record(width);
        let wide = record(width * 2.0);
        assert_eq!(breaks::bounded("edge op", &params, vec![good.clone(), wide]), vec![good]);
    }
    assert!(breaks::bounded("face", &params(OpType::Face), vec![record(0.1)]).is_empty());
}

#[test]
fn corrupted_frames_and_excess_depth_are_dropped_before_verification() {
    let params = params(OpType::Deburr);
    let width = breaks::declared_width(&params).expect("width");
    let good = record(width);
    let mut deep = good.clone();
    deep.depth = width * 1.01;
    let mut frame = good.clone();
    frame.volume.as_mut().expect("volume").frames[0].outward = [0.0, 0.0, 10.0];
    assert_eq!(breaks::bounded("broken edge op", &params, vec![deep, frame, good.clone()]), vec![good]);
}
