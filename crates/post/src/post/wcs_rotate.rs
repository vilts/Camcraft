//! Carry a Z-rotated work offset (`WcsYaw`) into the CNC record stream (#760).
//! Provenance: standard 2D frame rotation in the XY plane about OriginX/Y.

use serde_json::{Value, json};

use super::PostError;
use super::cnc::Record;
use super::js::Script;

const TOL_RAD: f64 = 1e-9;

fn rotate_xy(x: f64, y: f64, ox: f64, oy: f64, s_y: f64, c_y: f64) -> (f64, f64) {
    let dx = x - ox;
    let dy = y - oy;
    (ox + dx * c_y + dy * s_y, oy - dx * s_y + dy * c_y)
}

pub(super) fn read_finite(v: &Value, name: &str) -> Result<f64, PostError> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| PostError::Refused(format!("stream parameter {name} must be a finite number")))
}

pub(super) fn chunk_bounds(records: &[Record]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (i, r) in records.iter().enumerate() {
        if matches!(r, Record::Section { begin: false }) {
            spans.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < records.len() {
        spans.push((start, records.len()));
    }
    spans
}

fn check_section_supported(chunk: &[Record]) -> Result<(), PostError> {
    for r in chunk {
        match r {
            Record::Touch(_) | Record::Guard(_) | Record::Retreat(_) | Record::Found(_) => {
                return Err(PostError::Refused(
                    "probing on a rotated work offset is not supported yet - post probing from an unrotated setup".into(),
                ));
            }
            Record::Thread(_) => {
                return Err(PostError::Refused(
                    "lathe threading on a rotated work offset is not supported - use an unrotated setup".into(),
                ));
            }
            Record::Param { name, .. } if name == "EdmCondition" || name == "SinkerCondition" => {
                return Err(PostError::Refused(
                    "EDM on a rotated work offset is not supported yet - use an unrotated setup".into(),
                ));
            }
            Record::Param { name, .. } if name == "LiveTool" => {
                return Err(PostError::Refused(
                    "C-axis live-tool work on a rotated work offset is not supported yet - use an unrotated setup".into(),
                ));
            }
            Record::Move(m) if m.abc.is_some() => {
                return Err(PostError::Refused(
                    "5-axis / rotary ABC moves inside a rotated work offset are not supported yet - use an unrotated setup".into(),
                ));
            }
            Record::Move(m) if m.center.is_some() && !m.plane.is_xy() => {
                return Err(PostError::Refused(
                    "an XZ/YZ (G18/G19) arc inside a rotated work offset cannot rotate in XY".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_control_capabilities(mch: &Value, s: &Script) -> Result<(), PostError> {
    let mch_ok = mch.get("coordinateRotation").and_then(Value::as_bool) == Some(true);
    if !mch_ok {
        let model = mch.get("name").and_then(Value::as_str).unwrap_or("mounted machine");
        return Err(PostError::Refused(format!(
            "WcsRotation Control (G68 / ROT) requires controller.coordinate_rotation: true in '{model}' - choose Bake into XY or declare coordinate_rotation in the machine file"
        )));
    }
    if !s.voices_wcs_rotation() {
        return Err(PostError::Refused(
            "WcsRotation Control (G68 / ROT) requires a post script declaring wcsRotation = true - choose Bake into XY or use Fanuc, Haas, Mazak, Doosan DNM, or Sinumerik".into(),
        ));
    }
    let has_inverted = mch
        .get("invertedAxes")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    if has_inverted {
        return Err(PostError::Refused(
            "WcsRotation Control (G68 / ROT) is not supported with inverted machine axes - choose Bake into XY".into(),
        ));
    }
    Ok(())
}

fn parse_mode(param_mode: Option<&Value>, options: &Value) -> Result<bool, PostError> {
    let word = match param_mode {
        Some(v) => v
            .as_str()
            .ok_or_else(|| PostError::Refused(format!("WcsRotationMode must be a string, got {v}")))?,
        None => match options.get("WcsRotation") {
            None | Some(Value::Null) => return Ok(false),
            Some(v) => v
                .as_str()
                .ok_or_else(|| PostError::Refused(format!("WcsRotation option must be a string, got {v}")))?,
        },
    };
    match word {
        "bake" => Ok(false),
        "control" => Ok(true),
        other => Err(PostError::Refused(format!(
            "unknown WCS rotation mode {other:?} - expected \"bake\" or \"control\""
        ))),
    }
}

/// Transform every Control-mode section carrying `PARAM WcsYaw` from Setup/Table
/// coordinates into its rotated WCS XY frame about modal `OriginX`/`OriginY`
/// (`p' = O + Rz(-yaw) * (p - O)`), keeping record count and indices intact
/// for `viewer_map`. Bake mode checks safety and keeps Setup/Table coordinates
/// untouched so a translation-only control drives the same physical table path.
pub fn apply(mut records: Vec<Record>, mch: &Value, s: &Script, options: &Value) -> Result<Vec<Record>, PostError> {
    let mut org_x = 0.0_f64;
    let mut org_y = 0.0_f64;
    let spans = chunk_bounds(&records);
    for (start, end) in spans {
        let chunk = &mut records[start..end];
        let mut yaw_deg: Option<f64> = None;
        let mut param_mode: Option<Value> = None;
        let mut start_x: Option<f64> = None;
        let mut start_y: Option<f64> = None;
        let mut stock = [None::<f64>; 4];
        for r in chunk.iter() {
            if let Record::Param { name, value } = r {
                match name.as_str() {
                    "OriginX" => org_x = read_finite(value, "OriginX")?,
                    "OriginY" => org_y = read_finite(value, "OriginY")?,
                    "WcsYaw" => yaw_deg = Some(read_finite(value, "WcsYaw")?),
                    "WcsRotationMode" => param_mode = Some(value.clone()),
                    "StartX" => start_x = Some(read_finite(value, "StartX")?),
                    "StartY" => start_y = Some(read_finite(value, "StartY")?),
                    "StockXYLeft" => stock[0] = Some(read_finite(value, "StockXYLeft")?),
                    "StockXYRight" => stock[1] = Some(read_finite(value, "StockXYRight")?),
                    "StockXYFront" => stock[2] = Some(read_finite(value, "StockXYFront")?),
                    "StockXYBack" => stock[3] = Some(read_finite(value, "StockXYBack")?),
                    _ => {}
                }
            }
        }
        let Some(deg) = yaw_deg else {
            continue;
        };
        let rad = deg.to_radians();
        if rad.abs() <= TOL_RAD {
            continue;
        }
        let control = parse_mode(param_mode.as_ref(), options)?;
        check_section_supported(chunk)?;
        if !control {
            continue;
        }
        check_control_capabilities(mch, s)?;

        let (s_y, c_y) = rad.sin_cos();
        let rot_start = start_x.zip(start_y).map(|(x, y)| rotate_xy(x, y, org_x, org_y, s_y, c_y));
        let rot_stock = match stock {
            [Some(l), Some(r), Some(f), Some(b)] => {
                let corners = [(l, f), (r, f), (r, b), (l, b)];
                let (mut lx, mut rx, mut fy, mut by) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
                for (cx, cy) in corners {
                    let (ux, uy) = rotate_xy(cx, cy, org_x, org_y, s_y, c_y);
                    lx = lx.min(ux);
                    rx = rx.max(ux);
                    fy = fy.min(uy);
                    by = by.max(uy);
                }
                Some([lx, rx, fy, by])
            }
            _ => None,
        };

        for r in chunk.iter_mut() {
            match r {
                Record::Param { name, value } => match name.as_str() {
                    "StartX" if rot_start.is_some() => {
                        if let Some((x, _)) = rot_start {
                            *value = json!(x);
                        }
                    }
                    "StartY" if rot_start.is_some() => {
                        if let Some((_, y)) = rot_start {
                            *value = json!(y);
                        }
                    }
                    "StockXYLeft" if rot_stock.is_some() => {
                        if let Some(st) = rot_stock {
                            *value = json!(st[0]);
                        }
                    }
                    "StockXYRight" if rot_stock.is_some() => {
                        if let Some(st) = rot_stock {
                            *value = json!(st[1]);
                        }
                    }
                    "StockXYFront" if rot_stock.is_some() => {
                        if let Some(st) = rot_stock {
                            *value = json!(st[2]);
                        }
                    }
                    "StockXYBack" if rot_stock.is_some() => {
                        if let Some(st) = rot_stock {
                            *value = json!(st[3]);
                        }
                    }
                    _ => {}
                },
                Record::Move(m) => {
                    let (nx, ny) = rotate_xy(m.x, m.y, org_x, org_y, s_y, c_y);
                    m.x = nx;
                    m.y = ny;
                    if let Some((cx, cy)) = m.center {
                        let (ncx, ncy) = rotate_xy(cx, cy, org_x, org_y, s_y, c_y);
                        m.center = Some((ncx, ncy));
                    }
                }
                Record::Cycle(c) => {
                    let (nx, ny) = rotate_xy(c.x, c.y, org_x, org_y, s_y, c_y);
                    c.x = nx;
                    c.y = ny;
                }
                _ => {}
            }
        }
    }
    Ok(records)
}
