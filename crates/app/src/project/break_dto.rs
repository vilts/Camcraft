//! The cached edge break (#303, `crate::cl::EdgeBreak`) in the project
//! file. Our own format; the additive law of the comps, threads and
//! probes keys: absent is none - every path but a chamfer's or a
//! deburr's, and every cache before the record existed (such a cached
//! chamfer is judged against the sharp model until it regenerates -
//! stricter, never looser).

use serde::{Deserialize, Serialize};

use crate::cl::{lattice, BreakFrame, BreakProfile, EdgeBreak, EdgeBreakVolume};
use crate::geometry::Point3;

/// One cached edge break: the edge polyline on the micron lattice, the
/// bevel's depth at the edge and its slope.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EdgeBreakDto {
    pub edge: Vec<[f64; 3]>,
    pub depth: f64,
    pub slope: f64,
    /// Raw until validated: a corrupt extension drops only this record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VolumeDto {
    width: f64,
    frames: Vec<FrameDto>,
    profile: ProfileDto,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameDto {
    outward: [f64; 3],
    across: [f64; 3],
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ProfileDto {
    Flat,
    ConcaveBall { radius: f64 },
    ConvexRound { radius: f64 },
}

impl VolumeDto {
    fn from_volume(v: &EdgeBreakVolume) -> Self {
        Self {
            width: v.width,
            frames: v.frames.iter().map(|f| FrameDto { outward: f.outward, across: f.across }).collect(),
            profile: match v.profile {
                BreakProfile::Flat => ProfileDto::Flat,
                BreakProfile::ConcaveBall { radius } => ProfileDto::ConcaveBall { radius },
                BreakProfile::ConvexRound { radius } => ProfileDto::ConvexRound { radius },
            },
        }
    }

    fn into_volume(self) -> EdgeBreakVolume {
        EdgeBreakVolume {
            width: self.width,
            frames: self.frames.into_iter().map(|f| BreakFrame { outward: f.outward, across: f.across }).collect(),
            profile: match self.profile {
                ProfileDto::Flat => BreakProfile::Flat,
                ProfileDto::ConcaveBall { radius } => BreakProfile::ConcaveBall { radius },
                ProfileDto::ConvexRound { radius } => BreakProfile::ConvexRound { radius },
            },
        }
    }
}

impl EdgeBreakDto {
    pub(super) fn from_break(b: &EdgeBreak) -> Self {
        EdgeBreakDto {
            // Framed caps need the original endpoints: snapping a short
            // oblique edge would make its frame non-orthogonal on load.
            edge: b.edge.iter().map(|p| {
                if b.volume.is_some() { [p.x, p.y, p.z] } else { [lattice(p.x), lattice(p.y), lattice(p.z)] }
            }).collect(),
            depth: b.depth,
            slope: b.slope,
            // A serialization failure keeps an invalid extension rather
            // than turning the record into a legacy cone.
            volume: b.volume.as_ref().map(|v| serde_json::to_value(VolumeDto::from_volume(v)).unwrap_or(serde_json::Value::Bool(false))),
        }
    }
}

/// The breaks of a loaded path; a record that fails the sanity door is
/// dropped (its columns are then judged against the sharp model).
pub(super) fn load(dtos: &[EdgeBreakDto]) -> Vec<EdgeBreak> {
    dtos.iter().enumerate()
        .filter_map(|(index, d)| {
            let volume = match &d.volume {
                None => None,
                Some(v) => match serde_json::from_value::<VolumeDto>(v.clone()) {
                    Ok(v) => Some(v.into_volume()),
                    Err(why) => {
                        tracing::warn!(record = index, %why, "edge-break volume malformed - dropped, judged sharp");
                        return None;
                    }
                },
            };
            let b = EdgeBreak {
                edge: d.edge.iter().map(|p| Point3::new(p[0], p[1], p[2])).collect(),
                depth: d.depth,
                slope: d.slope,
                volume,
            };
            if !b.is_sane() {
                tracing::warn!(record = index, "edge-break record has invalid dimensions, profile or frame - dropped, judged sharp");
                return None;
            }
            Some(b)
        })
        .collect()
}

#[cfg(test)]
mod volume_tests {
    use super::*;
    use crate::cl::{BreakFrame, BreakProfile, EdgeBreakVolume};

    #[test]
    fn a_framed_break_survives_the_project_record_reader() {
        let record: EdgeBreakDto = serde_json::from_value(serde_json::json!({
            "edge": [[0.0, 0.0, 10.0], [10.0, 0.0, 10.0]],
            "depth": 0.5,
            "slope": 0.0,
            "volume": {
                "width": 1.0,
                "frames": [{"outward": [0.0, 0.0, 1.0], "across": [0.0, 1.0, 0.0]}],
                "profile": {"kind": "flat"}
            }
        })).expect("the framed record is valid project JSON");
        let loaded = load(&[record]);
        assert_eq!(loaded.len(), 1, "a valid 3D edge break must survive loading");
        assert!(loaded[0].is_sane());
    }

    fn framed_json() -> serde_json::Value {
        serde_json::json!({
            "edge": [[0.0, 0.0, 10.0], [10.0, 0.0, 10.0]],
            "depth": 0.5, "slope": 0.0,
            "volume": { "width": 1.0,
                "frames": [{"outward": [0.0, 0.0, 1.0], "across": [0.0, 1.0, 0.0]}],
                "profile": {"kind": "flat"}
            }
        })
    }

    #[test]
    fn legacy_records_keep_their_wire_shape_and_sanity() {
        let old = serde_json::json!({
            "edge": [[0.0, 0.0, 10.0], [10.0, 0.0, 10.0]], "depth": 0.5, "slope": 1.0
        });
        let dto: EdgeBreakDto = serde_json::from_value(old.clone()).expect("legacy record");
        let loaded = load(&[dto]);
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].volume.is_none());
        assert_eq!(serde_json::to_value(EdgeBreakDto::from_break(&loaded[0])).expect("write"), old);
        assert_eq!(loaded[0].floor_at(0.0, 0.25), Some((9.75, 10.0)));
    }

    #[test]
    fn every_profile_round_trips_and_older_readers_reject_its_zero_slope() {
        for kind in ["flat", "concave_ball", "convex_round"] {
            let mut json = framed_json();
            json["volume"]["profile"] = if kind == "flat" {
                serde_json::json!({"kind": kind})
            } else { serde_json::json!({"kind": kind, "radius": 0.75}) };
            let dto: EdgeBreakDto = serde_json::from_value(json).expect("framed record");
            let loaded = load(&[dto]);
            assert_eq!(loaded.len(), 1);
            let saved = EdgeBreakDto::from_break(&loaded[0]);
            let text = serde_json::to_string(&saved).expect("write");
            let saved: EdgeBreakDto = serde_json::from_str(&text).expect("read");
            assert_eq!(load(&[saved]), loaded, "{kind}");
            assert_eq!(loaded[0].slope, 0.0, "old readers must drop 3D records instead of applying a cone");
        }
    }

    #[test]
    fn malformed_extensions_drop_only_the_record_and_never_fall_back_to_a_cone() {
        let payloads = [
            serde_json::json!(false),
            serde_json::json!({}),
            serde_json::json!({"width":1,"frames":[],"profile":{"kind":"flat"}}),
            serde_json::json!({"width":1,"frames":[{"outward":[0,0,1],"across":[0,1,0]}],"profile":{"kind":"unknown"}}),
            serde_json::json!({"width":1,"frames":[{"outward":[0,1],"across":[0,1,0]}],"profile":{"kind":"flat"}}),
            serde_json::json!({"width":1,"frames":[{"outward":[0,0,1],"across":[0,1,0]}],"profile":{"kind":"flat"},"extra":1}),
            serde_json::json!({"width":1,"frames":[{"outward":[0,0,1],"across":[0,1,0],"extra":1}],"profile":{"kind":"flat"}}),
            serde_json::json!({"width":1,"frames":[{"outward":[0,0,1],"across":[0,1,0]}],"profile":{"kind":"concave_ball","radius":0.75,"extra":1}}),
        ];
        for payload in payloads {
            let mut json = framed_json();
            json["volume"] = payload;
            // Positive legacy slope is deliberate: corrupt extensions
            // must not turn into a plausible vertical-axis declaration.
            json["slope"] = 1.0.into();
            let bad: EdgeBreakDto = serde_json::from_value(json).expect("raw record");
            let good: EdgeBreakDto = serde_json::from_value(framed_json()).expect("good record");
            let loaded = load(&[bad, good]);
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].slope, 0.0);
        }
    }

    #[test]
    fn framed_endpoints_keep_precision_needed_for_short_oblique_edges() {
        use crate::cl::TiltedFrame;
        let frame = TiltedFrame::new(Point3::new(0.123456789, 0.234567891, 0.345678912), [1.0, 2.0, 3.0]).expect("frame");
        let end = frame.to_world(Point3::new(0.0, 0.0, 0.000012345));
        let b = EdgeBreak {
            edge: vec![frame.origin, end], depth: 0.5, slope: 0.0,
            volume: Some(EdgeBreakVolume {
                width: 1.0, profile: BreakProfile::Flat,
                frames: vec![BreakFrame { outward: frame.u, across: frame.v }],
            }),
        };
        assert!(b.is_sane());
        let dto = EdgeBreakDto::from_break(&b);
        let dto = serde_json::from_str(&serde_json::to_string(&dto).expect("write")).expect("read");
        assert_eq!(load(&[dto]), vec![b]);
    }
}
