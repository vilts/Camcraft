//! Generation job orchestration and the 3D Pocket reach preflight.
//! Provenance: standard axial tool-stack clearance; issue #1844.

use super::*;

/// Run a fully-collected job: every world read happened in
/// [`collect_job`], on the caller's thread, and nothing here names a
/// `World`. Batch jobs run on worker threads, where a `&World` cannot
/// exist. The reach preflight refuses unsupported depths before any
/// generator builds a path.
pub fn run_job(mut job: GenJob) -> Result<(Toolpath, Vec<Warning>), GenError> {
    // The spindle law's set-aside note (issue #71 review) rides the
    // path whichever arm cut it: a note, never a warning - the post
    // is not held for features the op honestly leaves standing.
    let note = job.spindle_note.take();
    let carried = job.carried_overhang.take();
    // #3100: a user's own post taken at its declared order says so.
    let declared = job.post_order.custom.then(|| crate::operation::first_traverse_posed::DECLARED_NOTE.to_owned());
    let tool_r = job.tool.diameter * 0.5;
    let kind = job.params.kind();
    // #261 part 2: a refusal's spot is in the frame the generators ran
    // in (a 3+2 tilt's local one); the UI draws in world, so it is
    // carried out through the same frame the path would have been.
    let frame = job.frame;
    // #835: a refusal naming the holder of a tool with a body says it
    // judged the cylinder enclosing that body - #842: only where it did,
    // a body whose band stack does not fit the record.
    let judged_tool = job.tool.clone();
    let body = job.body.clone().filter(|p| crate::operation::body_judged::falls_back(&judged_tool, p));
    let outside_clearing = job
        .area
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(AreaCtx::outside_clearing_stock);
    let stock_band = StockBandRefusal::of(&job);
    let _span = tracing::info_span!("generate", op = %job.op_name).entered();
    let started = crate::clock::Instant::now();
    tracing::debug!(kind = %kind, "generation started");
    let (mut path, warns) = pocket_reach(&job).and_then(|()| run_job_arms(job))
        .map_err(|e| match stock_band { Some(band) => band.name(e), None => e })
        .map_err(|e| name_outside_entry_refusal(e, outside_clearing))
        .map_err(|e| crate::operation::body_judged::name_enclosure(e, &judged_tool, body.as_ref()))
        .map_err(|e| e.map_location(|p| frame.map_or(p, |f| f.to_world(p))))
        // #261 part 3: a refusal's ghost rides the same frame map as a path.
        .map_err(|e| match &frame {
            Some(f) => e.map_ghost(|t| f.world_path(t, machining2d::ARC_FIT_TOL)),
            None => e,
        })
        .inspect_err(|e| {
            tracing::warn!(kind = %kind, ms = crate::logging::elapsed_ms(started), "generation refused: {e}");
        })?;
    tracing::info!(
        kind = %kind,
        moves = path.moves.len(),
        warnings = warns.len(),
        ms = crate::logging::elapsed_ms(started),
        "generated"
    );
    let carried = carried.map(|o| o.note(&path, tool_r));
    for n in note.into_iter().chain(carried).chain(declared) {
        if !path.notes.contains(&n) {
            path.notes.push(n);
        }
    }
    Ok((path, warns))
}

/// #3791: document context for a refused stock-source clearing job.
/// Keep the generator's verdict and error location; only name its area.
struct StockBandRefusal {
    region: Region,
    area: Option<AreaCtx>,
    /// The stock's facets, in the frame of `region`: its outline tells a
    /// band around the parts from a closed pocket in a part.
    stock: Vec<Triangle>,
    diameter: f64,
    leave: f64,
    unit: crate::units::Unit,
}

impl StockBandRefusal {
    fn of(job: &GenJob) -> Option<Self> {
        if !matches!(job.src, SrcRef::Stock) || job.keep_parts.is_empty() {
            return None;
        }
        let leave = match &job.params {
            OpParams::Pocket { stock_to_leave, .. }
            | OpParams::Adaptive2D { stock_to_leave, .. }
            | OpParams::Offset2D { stock_to_leave, .. } => *stock_to_leave,
            _ => return None,
        };
        Some(Self {
            region: job.picked_region.as_ref()?.as_ref().ok()?.clone(),
            area: job.area.as_ref().and_then(|a| a.as_ref().ok()).cloned(),
            stock: job.frame.as_ref().map_or_else(|| job.material.facets.clone(), |f| f.local_facets(&job.material.facets)),
            diameter: job.tool.diameter,
            leave,
            unit: job.unit,
        })
    }

    fn name(self, e: GenError) -> GenError {
        e.map_msg(|m| {
            if m != "pocket is narrower than the tool" {
                return m;
            }
            // A closed pocket in a part keeps the generator's own words.
            if !surrounds_kept_material(&self.region, &self.stock) {
                return m;
            }
            let Ok(region) = apply_area_controls(self.region, self.area.as_ref()) else {
                return m;
            };
            // The existing band check must agree: avoid/rest refusals may
            // describe an area smaller than the document's stock band.
            if !region.offset(-(self.diameter * 0.5 + self.leave)).is_ok_and(|r| r.is_empty()) {
                return m;
            }
            let Ok(pieces) = machining2d::unreached_pieces(&region, self.diameter * 0.5 + self.leave) else {
                return m;
            };
            if pieces.is_empty() {
                return m;
            }
            let len = |v| format!("{} {}", crate::units::len(v, self.unit, 3), self.unit.suffix());
            let width = rectangular_band_width(&region)
                .map(|w| format!(" (minimum side width {})", len(w)))
                .unwrap_or_default();
            format!(
                "the stock band around the part{width} is narrower than the tool ({} diameter, {} stock to leave per wall): no cutter position fits. Use a smaller tool or a wider stock margin, or cut the outside with a 2D Contour.",
                len(self.diameter), len(self.leave),
            )
        })
    }
}

/// True when a piece of `region` (the stock outline less the standing
/// parts, before the area controls) is a band of stock around kept
/// material: its outer loop is a loop of the stock outline, and material
/// stands inside that loop. A closed pocket fails the first test, since
/// part walls bound it. A strip beside a flush part fails the second.
fn surrounds_kept_material(region: &Region, stock: &[Triangle]) -> bool {
    /// Below this (mm^2) a difference is tessellation noise (`area.rs`).
    const NOISE: f64 = 1e-4;
    let Ok(outline) = slice::project(stock, FOOTPRINT_SAMPLES) else {
        return false;
    };
    let fill = |l: &crate::geo2d::Loop| Region::from_slices(vec![l.clone()]).ok();
    let outline: Vec<Region> = outline.loops().iter().filter(|l| l.is_ccw()).filter_map(fill).collect();
    let small = |a: &Region, b: &Region| a.difference(b).is_ok_and(|d| d.area().abs() < NOISE);
    region.loops().iter().filter(|l| l.is_ccw()).filter_map(fill).any(|outer| {
        outline.iter().any(|o| small(&outer, o) && small(o, &outer))
            && outer.difference(region).is_ok_and(|kept| kept.area().abs() >= NOISE)
    })
}

/// Only quote a side margin for a rectangular stock/part ring. Bounds
/// alone cannot measure curved, concave or multiple-island bands.
fn rectangular_band_width(region: &Region) -> Option<f64> {
    if region.loops().len() != 2 {
        return None;
    }
    let bounds = |ccw| {
        let ring = region.loops().iter().find(|l| l.is_ccw() == ccw)?;
        let (lo, hi) = Region::new(vec![ring.clone()]).ok()?.bounds()?;
        let area = (hi.x - lo.x) * (hi.y - lo.y);
        ((ring.area().abs() - area).abs() < 1e-6).then_some((lo, hi))
    };
    let (lo, hi) = bounds(true)?;
    let (inner_lo, inner_hi) = bounds(false)?;
    let width = (inner_lo.x - lo.x).min(inner_lo.y - lo.y)
        .min(hi.x - inner_hi.x).min(hi.y - inner_hi.y);
    (width.is_finite() && width > 0.0).then_some(width)
}

/// The ladder proves only cutter-radius clearance. Until it can widen
/// each level for the assembly, require every wider band to stay above
/// the material at Bottom Z. This is deliberately conservative: no XY
/// clearance or previously removed stock is credited by this preflight.
fn pocket_reach(job: &GenJob) -> Result<(), GenError> {
    if !matches!(job.params, OpParams::Pocket3D { .. }) || job.frame.is_some() {
        // Tilted whole-model pockets already refuse in open_footprint;
        // their existing diagnostic keeps precedence.
        return Ok(());
    }
    let bottom = req_z(job.hs.bottom, "Bottom of Part Z")?;
    let top = job.material.top.max(job.stock_aabb.map_or(job.material.top, |(_, hi)| hi.z));
    let depth = top - bottom;
    let geom = job.tool.geom();
    let stack = crate::tool_body::stack(&geom, job.body.as_ref()).with_neck(job.neck.clone());
    // Read exactly the bands used by verification, including its
    // enclosing-cylinder fallback for a body that does not fit.
    let cutter = verify::Cutter::of(&job.tool).with_body(&geom, &stack);
    for (name, bands) in [("shank", cutter.shank_bands()), ("holder", cutter.holder_bands())] {
        for (above, radius, len) in bands {
            if len > 0.0 && radius > cutter.radius && depth >= above {
                return Err(GenError::Invalid(format!(
                    "3D Pocket cannot prove {name} clearance for tool '{}': Bottom of Part Z is {:.3} mm below the material top, but its wider {name} starts {:.3} mm above the tip. Raise Bottom of Part Z above {:.3} mm or use a longer-reaching tool. This conservative reach check does not credit lateral clearance or previously removed stock.",
                    job.tool_name, depth, above, top - above,
                )));
            }
        }
    }
    Ok(())
}

/// Annotate a narrow-area or missing-entry refusal when a clearing kind
/// ran with `Containment::Outside`, naming the `Extension` control and
/// setup stock (#2232).
fn name_outside_entry_refusal(e: GenError, outside_stock: Option<bool>) -> GenError {
    let Some(has_stock) = outside_stock else {
        return e;
    };
    e.map_msg(|m| {
        if m.contains("no room for an entry")
            || m.contains("narrower than the tool")
            || m.contains("too narrow")
        {
            if has_stock {
                format!(
                    "{m} - the setup stock leaves no room past the selection; enlarge the stock or set an Extension (Boundary)"
                )
            } else {
                format!(
                    "{m} - this setup has no stock, so Outside containment leaves a band one tool wide; set an Extension (Boundary) or add setup stock"
                )
            }
        } else {
            m
        }
    })
}
