//! Provenance: Autodesk DXF LWPOLYLINE/POLYLINE/VERTEX records and group-42 bulges.
use super::entities::{common, coordinate, layer, point, segment};
use super::pairs::{Pairs, Record, integer, number};
use super::types::{DxfError, State, error};
use crate::geo2d::Point2;

#[derive(Clone, Copy)]
struct Vertex {
    point: Point2,
    bulge: f64,
}

#[derive(Default)]
struct Chain {
    first: Option<Vertex>,
    previous: Option<Vertex>,
    count: usize,
}

impl Chain {
    fn push(
        &mut self,
        vertex: Vertex,
        kind: &str,
        layer: &str,
        state: &mut State,
    ) -> Result<(), DxfError> {
        if let Some(previous) = self.previous {
            state.push(
                layer,
                segment(previous.point, vertex.point, previous.bulge, kind)?,
            );
        } else {
            self.first = Some(vertex);
        }
        self.previous = Some(vertex);
        self.count += 1;
        Ok(())
    }

    fn finish(
        self,
        closed: bool,
        kind: &str,
        layer: &str,
        state: &mut State,
    ) -> Result<(), DxfError> {
        if self.count < 2 {
            return Err(error(kind, "fewer than two vertices"));
        }
        if closed && let (Some(last), Some(first)) = (self.previous, self.first) {
            state.push(layer, segment(last.point, first.point, last.bulge, kind)?);
        }
        Ok(())
    }
}

fn closed(record: Record<'_>) -> Result<bool, DxfError> {
    let flags = record.integer(70, 0)?;
    if flags & !129 != 0 {
        return Err(error(
            record.kind,
            "3D, mesh or fitted polyline flags are unsupported",
        ));
    }
    Ok(flags & 1 != 0)
}

fn zero(value: f64, kind: &str, name: &str) -> Result<(), DxfError> {
    if value != 0.0 {
        return Err(error(kind, &format!("nonzero {name} is unsupported")));
    }
    Ok(())
}

#[derive(Default)]
struct Pending {
    x: Option<f64>,
    y: Option<f64>,
    bulge: Option<f64>,
}

impl Pending {
    fn take(&mut self, kind: &str) -> Result<Vertex, DxfError> {
        let x = self
            .x
            .take()
            .ok_or_else(|| error(kind, "vertex has no X coordinate"))?;
        let y = self
            .y
            .take()
            .ok_or_else(|| error(kind, "vertex has no Y coordinate"))?;
        let bulge = self.bulge.take().unwrap_or(0.0);
        Ok(Vertex {
            point: Point2::new(x, y),
            bulge,
        })
    }
}

pub(super) fn lightweight(record: Record<'_>, state: &mut State) -> Result<(), DxfError> {
    common(record)?;
    let layer = layer(record)?;
    let closed = closed(record)?;
    let expected = integer(record.required(90)?, record.kind)?;
    let expected =
        usize::try_from(expected).map_err(|_| error(record.kind, "negative vertex count"))?;
    if expected
        > state
            .limits
            .max_vertices
            .saturating_sub(state.doc.summary.vertices)
    {
        return Err(error(record.kind, "vertex count exceeds limit"));
    }
    let z = coordinate(record.number(38, Some(0.0))?, record.kind)?;
    state.elevation(record.kind, z)?;
    let mut chain = Chain::default();
    let mut vertex = Pending::default();
    let mut pairs = record.pairs();
    while let Some(pair) = pairs.next()? {
        match pair.code {
            10 => {
                if vertex.x.is_some() {
                    chain.push(vertex.take(record.kind)?, record.kind, layer, state)?;
                }
                state.vertex(record.kind)?;
                vertex.x = Some(coordinate(number(pair.value, record.kind)?, record.kind)?);
            }
            20 | 42 => {
                if vertex.x.is_none() {
                    return Err(error(record.kind, "vertex field before X coordinate"));
                }
                let slot = if pair.code == 20 {
                    &mut vertex.y
                } else {
                    &mut vertex.bulge
                };
                if slot.is_some() {
                    return Err(error(record.kind, "duplicate vertex field"));
                }
                let value = number(pair.value, record.kind)?;
                *slot = Some(if pair.code == 20 {
                    coordinate(value, record.kind)?
                } else {
                    value
                });
            }
            40 | 41 | 43 => zero(number(pair.value, record.kind)?, record.kind, "width")?,
            30 => {
                return Err(error(
                    record.kind,
                    "vertex Z is unsupported; use elevation group 38",
                ));
            }
            _ => {}
        }
    }
    if vertex.x.is_some() {
        chain.push(vertex.take(record.kind)?, record.kind, layer, state)?;
    }
    if chain.count != expected {
        return Err(error(record.kind, "vertex count does not match group 90"));
    }
    chain.finish(closed, record.kind, layer, state)
}

pub(super) fn legacy(
    record: Record<'_>,
    input: &mut Pairs<'_>,
    state: &mut State,
) -> Result<(), DxfError> {
    common(record)?;
    let layer = layer(record)?;
    let closed = closed(record)?;
    for code in [40, 41] {
        zero(record.number(code, Some(0.0))?, record.kind, "width")?;
    }
    for code in [10, 20] {
        zero(
            record.number(code, Some(0.0))?,
            record.kind,
            "dummy coordinate",
        )?;
    }
    if record.integer(75, 0)? != 0 {
        return Err(error(record.kind, "fitted surface is unsupported"));
    }
    let z = coordinate(record.number(30, Some(0.0))?, record.kind)?;
    state.elevation(record.kind, z)?;
    let mut chain = Chain::default();
    loop {
        let vertex = input
            .record()?
            .ok_or_else(|| error("POLYLINE", "missing SEQEND"))?;
        match vertex.kind {
            "SEQEND" => {
                state.entity("SEQEND")?;
                common(vertex)?;
                break;
            }
            "VERTEX" => {
                state.entity("VERTEX")?;
                state.vertex("POLYLINE VERTEX")?;
                common(vertex)?;
                if vertex.integer(70, 0)? != 0 {
                    return Err(error(
                        "POLYLINE VERTEX",
                        "3D, mesh or fitted vertex flags are unsupported",
                    ));
                }
                for code in [40, 41] {
                    zero(vertex.number(code, Some(0.0))?, "POLYLINE VERTEX", "width")?;
                }
                zero(vertex.number(30, Some(0.0))?, "POLYLINE VERTEX", "Z")?;
                if let Some(name) = vertex.field(8)?
                    && name != layer
                    && name != "0"
                {
                    return Err(error("POLYLINE VERTEX", "layer differs from POLYLINE"));
                }
                chain.push(
                    Vertex {
                        point: point(vertex, 10, 20)?,
                        bulge: vertex.number(42, Some(0.0))?,
                    },
                    "POLYLINE",
                    layer,
                    state,
                )?;
            }
            _ => return Err(error("POLYLINE", "expected VERTEX or SEQEND")),
        }
    }
    chain.finish(closed, record.kind, layer, state)
}
