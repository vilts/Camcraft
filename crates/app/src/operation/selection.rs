

use super::*;

/// Largest tilt from horizontal, in degrees, a Face operation accepts for
/// a selected planar face: past it the facing footprint would lie about
/// the area being cleared.
pub(super) const FACE_TILT_MAX_DEG: f64 = 60.0;
/// Shoelace area below which a projected loop is a traced line rather
/// than a boundary. 1e-9 mm² is far under weld-lattice noise and far
/// above an actual face.
pub(super) const DEGENERATE_AREA: f64 = 1e-9;
/// The material-side rule's threshold (M1.5.1 issue C): when the area
/// inside a chain-only Face region holds less than this fraction of the
/// material the region disagrees with - inside vs outside - the outside
/// is taken instead. 0.5 means "strictly less material inside than
/// outside": a bore rim rings air (inside ≈ 0) and machines the annulus;
/// a box-top ring holds all its material inside (ratio ≈ 1) and stays.
pub(super) const OUTSIDE_BIAS: f64 = 0.5;

/// What a selection resolves to: the boundary loops and centerline runs
/// it spells, plus Auto's verdict for a Contour (the level law, #1933;
/// Outside for every other kind and for a side the user named). Named
/// so the job struct (`super::job`) stores the same shape it reads -
/// one truth for the tuple, not a second spelling to drift.
pub(super) type SelectionGeometry = (Vec<Loop>, Vec<Vec<Point2>>, machining2d::AutoSide);

/// The region loops and centerline runs a feature selection resolves to,
/// as the world stands now: each picked face gives its closed boundary
/// loops, each closed chain its ring, projected to XY, coincident rims
/// counted once. An open chain is no boundary at all: for a Contour it
/// comes back in the second half - the one kind that machines a run -
/// and for a Face it runs out to the stock edge, the run acting as the
/// limit (M1.5.1 issue C). Refusals are the Prime Directive's kind - a
/// tilted face for a facing pass, an open chain for a Pocket or a Trace,
/// an open sheet, a run-out whose side cannot be told, each named in the
/// message, each with the fix stated. Never a silent drop: an unconsumed
/// run either bounds something or the message says why it cannot.
///
/// Picked open runs are stitched before this routing (M1.5.1 issue C):
/// several runs that meet end to end spell one ring, and a ring is a
/// ring however it is spelled - see `stitch_open_runs`. A facing region
/// spelled only by chains is then clipped to the material that actually
/// stands there - and taken on the far side when the picked rings bound
/// air, which is what "machine out to the stock, the run is the limit"
/// means on a bore rim (the material-side rule, [`OUTSIDE_BIAS`]). A
/// contour with `Side::Auto` asks the same rings a narrower question -
/// which side of them holds the material at their own level - in
/// the third slot of [`selection_geometry_impl`]'s tuple; this wrapper
/// hands out only the halves the region-shaped kinds want.
pub(super) fn selection_geometry(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<(Vec<Loop>, Vec<Vec<Point2>>), GenError> {
    let (loops, chains, _) = selection_geometry_impl(world, setup, items, params, None)?;
    Ok((loops, chains))
}

/// [`side_sane`]'s question, answered for one concrete source.
pub(crate) fn side_sane(params: &OpParams, src: &SrcRef) -> Option<String> {
    let OpParams::Contour { side, .. } = params else {
        return None;
    };
    if *side != Side::Inside || matches!(src, SrcRef::Selection(_)) {
        return None;
    }
    Some(
        "Side 'Inside' needs picked rings: this source machines the whole model section, whose \
         hole walls are already cut from inside - switch Side to Auto, or pick the wall's edges"
            .into(),
    )
}

/// The side [`Side::Auto`] resolves to for this operation's contour,
/// asked of the very walk generation runs ([`selection_geometry_impl`]) -
/// the same law [`machining2d::PocketStrategy::resolve`] is for the
/// pocket's Auto, so a panel note and a generated path can never
/// disagree. `None` when there is no verdict to quote yet: picks that
/// resolve to open runs only (the side is inert there), or geometry that
/// refuses. A whole-section source always answers Outside - its `+` law
/// is already parity-correct over holes, and Inside there is a named
/// refusal, never an Auto verdict.
pub fn contour_auto_side(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Option<bool> {
    match src {
        SrcRef::Stock | SrcRef::Entity(_) => Some(false),
        // The display's ask, one walk per input (#2249): generation
        // reads the side from its own walk in `job`, never from here.
        SrcRef::Selection(items) => super::selection_memo::answer(world, setup, items, params).inside,
    }
}

/// The drive chains, in WORLD 3D (M5.1 J): the same entity/name/
/// hash-id resolution [`selection_geometry_impl`]'s walk does, read
/// through the transform instead of projected to XY - for the
/// drive-curve contour the chain is the path where it STANDS, and a
/// shadow of a space curve is not the curve. A closed chain and an
/// open run are both welcome: the ball rides the points either way,
/// and the kernel's winding law reads the rest. Face and Patch picks
/// are refused by name - this kind asks for curves, not boundaries.
/// Resolution errors quote the same labels the 2D walk quotes, so a
/// re-pick instruction reads identically in both.
pub(super) fn drive_chains(world: &World, items: &[GeoSel]) -> Result<Vec<Vec<Point3>>, GenError> {
    let mut out: Vec<Vec<Point3>> = Vec::new();
    for g in items {
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(GenError::Invalid(format!(
                "the selection names '{}', which is gone",
                g.entity
            )));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(GenError::Invalid(format!(
                "'{}' has no geometry to select from",
                g.entity
            )));
        };
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let label = format!("{} of '{}'", g.feature, g.entity);
        match g.feature {
            FeatureSel::Chain(cid) => {
                let Some(c) = crate::model::resolve_chain(&features, cid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let pts: Vec<Point3> = feature::chain_points(&features, &c)
                    .iter()
                    .map(|p| transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot))
                    .collect();
                if pts.len() < 2 {
                    // A one-point chain cannot carry a pass: the
                    // kernel's refusal would speak of drive curves
                    // while the pick is the one to blame - name it here.
                    return Err(GenError::Invalid(format!(
                        "{label} holds a single point - re-pick a real edge"
                    )));
                }
                out.push(pts);
            }
            FeatureSel::Face(_) | FeatureSel::Patch(_) => {
                return Err(GenError::Invalid(format!(
                    "{label} is a face - Multi-Axis Contour machines the picked EDGE CHAINS where they stand; machine this face's boundary as a chain, or use 5-Axis Contour for the model's own pattern"
                )));
            }
            FeatureSel::Vertex(_) => {
                return Err(GenError::Invalid(format!(
                    "{label} is a point - Multi-Axis Contour machines picked EDGE CHAINS; pick the edges through it"
                )));
            }
        }
    }
    if out.is_empty() {
        return Err(GenError::Empty(
            "Multi-Axis Contour machines picked edge chains - pick edges in the Geometry section"
                .into(),
        ));
    }
    Ok(out)
}

/// [`selection_geometry`] with a human-readable account of what the
/// selection resolved to (`notes`) accumulated on the side, so the
/// account can never disagree with the geometry - they are one walk.
/// The third return value is Auto's verdict for a Contour (the section
/// at the picked rings' own level, #1933); it is Outside for every other
/// kind and for a contour whose side the user already named.
pub(super) fn selection_geometry_impl(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
    notes: Option<&mut Vec<String>>,
) -> Result<SelectionGeometry, GenError> {
    selection_geometry_in(world, setup, items, params, notes, None)
}

/// A world point in the plane the walk projects to: world XY, or the
/// 3+2 index's tilted plane (M11.5 owed) - the frame's `to_local` is
/// the one door `run_job` reads the material through, so a boundary
/// pick lands exactly where the tilted region's own outline lands.
pub(super) fn plane_xy(w: Point3, frame: Option<&crate::cl::TiltedFrame>) -> Point2 {
    match frame {
        None => Point2::new(w.x, w.y),
        Some(f) => {
            let l = f.to_local(w);
            Point2::new(l.x, l.y)
        }
    }
}

/// [`selection_geometry_impl`] with the projection plane named: `None`
/// is world XY (every operation's own walk - a picked SOURCE under a
/// tilt is refused upstream); `Some` projects the picks into the
/// tilted frame, the boundary picks of a 3+2-indexed op.
pub(super) fn selection_geometry_in(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
    mut notes: Option<&mut Vec<String>>,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<SelectionGeometry, GenError> {
    let facing = matches!(params, OpParams::Face { .. });
    // The pocket law is the roughing twins' law too (M2.4 A): all
    // three hollow a closed region, so none may end mid-edge.
    let pocket = matches!(
        params,
        OpParams::Pocket { .. } | OpParams::Adaptive2D { .. } | OpParams::Offset2D { .. }
    );
    // The plasma profile takes open runs as the contour does (#670).
    let contour = matches!(params, OpParams::Contour { .. } | OpParams::PlasmaProfile(_) | OpParams::WireContour(_));
    let contour_side = match params {
        OpParams::Contour { side, .. } => *side,
        _ => Side::Auto,
    };
    // The two silhouettes a run-out consults, computed once and shared
    // with the assembly below: the source's own footprint (the Face arm
    // of `region_for` and the verification project, so preview and path
    // agree) and the stock around it (what a run runs out to, where
    // stock exists).
    let opframe = OpFrame::new(setup.map(str::to_owned), None);
    let mut material_sil = None;
    let mut envelope = None;
    let mut chain_only = false;
    if facing {
        chain_only = items.iter().all(|g| !matches!(g.feature, FeatureSel::Face(_)));
        // A picked face supplies its own closed boundary. The material and
        // stock silhouettes are only needed to extend an open chain or to
        // decide which side of a chain-only selection is metal. Projecting
        // both entire solids here made a face click on a rotated part spend
        // its time in 16 full-mesh sections before it could even check the
        // face's orientation.
        if chain_only || items.iter().any(|g| matches!(g.feature, FeatureSel::Chain(_))) {
            let material = material_of(world, setup, &SrcRef::Selection(items.to_vec()))?;
            let sil = silhouette(&material.facets);
            let stock_name = crate::setup::stock_entity_name(world, setup);
            envelope = model::find_by_name(world, &stock_name)
                .and_then(|e| opframe.furniture_facets(world, &e))
                .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
                .filter(|r| !r.is_empty())
                .or_else(|| sil.clone());
            material_sil = sil;
        }
    }
    let mut loops: Vec<Loop> = Vec::new();
    let mut chains: Vec<Vec<Point2>> = Vec::new();
    let mut strips: Vec<Loop> = Vec::new();
    let mut rims: Vec<Vec<(i64, i64)>> = Vec::new();
    // Per entity first, then across entities (M13.11); the cross walk's
    // refusals - a three-way junction, runs on different planes - are
    // the Prime Directive's kind and come back by name.
    let (rings, stitched) = stitch_open_runs(world, setup, items, frame)?;
    let mut stitched_runs = 0usize;
    let mut stitched_rings = 0usize;
    for (label, pts, runs) in rings {
        if runs > 1 {
            stitched_runs += runs;
            stitched_rings += 1;
        }
        push_loop(&label, pts, &mut loops, &mut rims)?;
    }
    if let Some(n) = &mut notes
        && stitched_runs > 0
    {
        n.push(format!(
            "{stitched_runs} open run{} stitched into {stitched_rings} ring{}",
            if stitched_runs == 1 { "" } else { "s" },
            if stitched_rings == 1 { "" } else { "s" },
        ));
    }
    // #1735: two open edges of a pocket-family pick are a band's walls.
    let band = open_band::of_selection(world, setup, items, params, frame, &stitched, &mut loops, notes.as_deref_mut())?;
    for (gi, g) in items.iter().enumerate() {
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(GenError::Invalid(format!(
                "the selection names '{}', which is gone",
                g.entity
            )));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(GenError::Invalid(format!(
                "'{}' has no geometry to select from",
                g.entity
            )));
        };
        let loc = opframe.pose_in_setup(world, &entity).unwrap_or_default();
        let label = format!("{} of '{}'", g.feature, g.entity);
        match g.feature {
            FeatureSel::Vertex(_) => {
                // A point bounds no region: refuse by name, never drop it.
                return Err(GenError::Invalid(format!(
                    "{label} is a point - a boundary needs faces or edge chains; pick those, or use the vertex as a height or origin reference"
                )));
            }
            FeatureSel::Face(fid) => {
                let Some(f) = features.faces.iter().find(|f| f.id == fid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let n = transform_point(
                    Point3::new(f.normal[0], f.normal[1], f.normal[2]),
                    Point3::ZERO,
                    loc.rot,
                );
                if facing && n.z.abs() < FACE_TILT_MAX_DEG.to_radians().cos() {
                    // #1510: a face level only in a tilted WCS says why.
                    if let Some(why) = crate::setup_wcs_tilt::setup_problem(world, setup) {
                        return Err(GenError::Invalid(format!("{label} is not horizontal - a facing pass needs a floor on the machine, and {why}")));
                    }
                    return Err(GenError::Invalid(format!(
                        "{label} is not horizontal - a facing pass needs a floor; machine its boundary with a 2D Contour instead"
                    )));
                }
                if f.loops.iter().all(|l| matches!(l.kind, LoopKind::Open)) {
                    return Err(GenError::Invalid(format!(
                        "{label} is an open sheet; select its edge chain instead"
                    )));
                }
                for l in &f.loops {
                    if matches!(l.kind, LoopKind::Open) {
                        continue;
                    }
                    let pts: Vec<Point2> = l
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize))
                        .map(|p| {
                            plane_xy(transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot), frame)
                        })
                        .collect();
                    push_loop(&label, pts, &mut loops, &mut rims)?;
                }
            }
            FeatureSel::Chain(cid) => {
                let Some(c) = crate::model::resolve_chain(&features, cid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let xy: Vec<Point2> = feature::chain_points(&features, &c)
                    .iter()
                    .map(|p| plane_xy(transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot), frame))
                    .collect();
                if !c.closed {
                    if stitched.contains_key(&gi) {
                        // The stitch pass closed this run's ends into a
                        // ring with its siblings; the ring is already
                        // in `loops`, routed like any closed chain.
                        continue;
                    }
                    if pocket {
                        if band {
                            continue;
                        }
                        return Err(open_band::mid_edge(&label, params));
                    }
                    if contour {
                        // The run's consumer: a centerline pass along
                        // these exact points, XY like everything else
                        // here - the Z ladder is the operation's.
                        chains.push(xy);
                    } else if facing {
                        // M1.5.1 issue C: the run is the limit - face
                        // from it out to the stock edge. No stock and
                        // no source footprint means nowhere to run to,
                        // said by name rather than dropped.
                        let Some(env) = &envelope else {
                            return Err(GenError::Invalid(format!(
                                "{label} is an open run and nothing bounds the plane around it - set up stock or close the pick into a chain"
                            )));
                        };
                        strips.push(extend_run_to_footprint(
                            &label,
                            &xy,
                            env,
                            material_sil.as_ref(),
                        )?);
                    } else {
                        // Trace used to drop the run in silence; the
                        // Prime Directive's answer is a named refusal -
                        // and the name is the kind the user picked:
                        // Engrave is the trace's twin (M2.4 B), and a
                        // trace row's truth is this row's truth.
                        return Err(GenError::Invalid(format!(
                            "{label} is an open run - {} machines a closed boundary; use a 2D Contour or close the pick into a chain",
                            params.kind().info().label
                        )));
                    }
                    continue;
                }
                push_loop(&label, xy, &mut loops, &mut rims)?;
            }
            FeatureSel::Patch(pid) => {
                let Some(rings) = feature::patch_rings(&features, pid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                // The Face arm's horizontal-tilt rule deliberately does not
                // apply: a patch is exactly how one asks for a wall. The
                // wall's rims go through the same seams as any closed
                // boundary - `push_loop`'s canonical-rim dedupe drops the
                // bottom rim of a vertical wall as the XY twin of the top,
                // so the region is the footprint the weld encloses, never
                // an even-odd sliver of its own skin - and the facing
                // material-side rule below turns a bore wall's rim around
                // the air it rings into the annulus the cut needs.
                let mut drew = false;
                for r in rings.iter().filter(|r| r.closed) {
                    let pts: Vec<Point2> = r
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize))
                        .map(|p| {
                            plane_xy(transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot), frame)
                        })
                        .collect();
                    push_loop(&label, pts, &mut loops, &mut rims)?;
                    drew = true;
                }
                if !drew {
                    return Err(GenError::Invalid(format!(
                        "{label} bounds no closed ring - re-pick it"
                    )));
                }
            }
        }
    }
    if facing && (chain_only || !strips.is_empty()) {
        // The material-side rule, assembled once at the end so the
        // stitching, the run-outs and the clip all report through the
        // same walk: rings (or nothing), then the run-out strips, then
        // one test against the metal - if what the selection bounds
        // holds less material than what it does not, the answer is the
        // complement inside the material. A bore rim rings air, and a
        // convex rim's outward side is all air; a Face region that
        // named the void as its own answer would machine nothing,
        // which is gouging by another route.
        let mut region = if loops.is_empty() {
            Region::empty()
        } else {
            Region::new(loops.clone())?
        };
        let extended = strips.len();
        for s in strips {
            region = region.union(&Region::new(vec![s])?)?;
        }
        let mut flipped = false;
        if chain_only && let Some(mat) = &material_sil {
            let keep = region.intersect(mat)?.area().abs();
            let whole = mat.area().abs();
            if keep < OUTSIDE_BIAS * (whole - keep) {
                region = mat.difference(&region)?;
                flipped = true;
            } else {
                region = region.intersect(mat)?;
            }
        }
        if let Some(n) = &mut notes {
            if extended > 0 {
                n.push(format!(
                    "{extended} open run{} extended to the stock boundary",
                    if extended == 1 { "" } else { "s" },
                ));
            }
            if flipped {
                n.push("machining outside the run - the selection bounds air".into());
            }
        }
        loops = region.loops().to_vec();
    }
    let mut auto = machining2d::AutoSide::Outside;
    if matches!(params, OpParams::Contour { side: Side::Auto, .. }) && !loops.is_empty() {
        // #1933: Auto asks which side of the rings holds the material AT
        // THE RINGS' OWN LEVEL - a blind bore's floor fills the XY
        // silhouette, never the section at the mouth. The loop count and
        // winding are untouched either way (M1.5.4 issue 2).
        auto = super::contour_auto_pick::auto_side_of_picks(world, setup, items, &loops, &stitched, frame)?;
        if let (machining2d::AutoSide::Inside, Some(n)) = (&auto, &mut notes) {
            n.push("machining inside the ring - the selection bounds air".into());
        }
    }
    if contour && contour_side != Side::Auto && loops.is_empty() && !chains.is_empty() {
        // An explicit Side on a pure open-run selection changes nothing -
        // the cutter stays on the run's air side, read from the part
        // (#1777) - so say that plainly instead of letting the combo
        // imply an effect.
        // #1928: a run off the stock (any pick on the stock entity) obeys
        // Side instead, so the note does not claim the part rule there.
        let stock_name = crate::setup::stock_entity_name(world, setup);
        let on_stock = items.iter().any(|g| g.entity == stock_name);
        if let Some(n) = &mut notes
            && !on_stock
        {
            n.push("Side does not apply to open runs - the tool stays on the run's air side, read from the part".into());
        }
    }
    Ok((loops, chains, auto))
}

/// The region half of `selection_geometry` in the shape `region_of` and
/// every non-contour generator wants: bounds, or a named refusal - never
/// a silently empty area. A Contour of pure open runs is the case that
/// skips this and reads `selection_geometry` directly.
///
/// The pocket family picked on a part takes the one region law of
/// [`pocket_region_on_part`] (M11.4 A): the area the picks enclose, the
/// part's own material at the ladder's planes as the islands - so a
/// face, its edge loop and its wall resolve to the same region. Every
/// kind's answer then crosses the area-controls seam (`apply_area_controls`,
/// M11.5) at its caller.
pub(super) fn selection_region(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
    area: &AreaControls,
) -> Result<Region, GenError> {
    let (loops, _) = selection_geometry(world, setup, items, params)?;
    if loops.is_empty() {
        return Err(GenError::Empty(
            "nothing in the selection bounds an area".into(),
        ));
    }
    // Containment On / Outside lets the tool past the selection's edge:
    // the enclosed area goes to the boundary law whole, which grows the
    // outline first and takes the part's material out after (user hand
    // test 2026-11-06: a picked underside the part fills is the band
    // around the part, not a refusal).
    let past_edge = matches!(area.containment, Some(Containment::On | Containment::Outside));
    let region = match pocket_ladder(world, setup, items, params)? {
        Some((part, planes)) if !past_edge => super::pocket_memo::pocket_region_memo(loops, &part, &planes, crate::units::of(world))?,
        _ => Region::new(loops)?,
    };
    // The area controls run after this, at the seam's call sites (the
    // job collector, the preview) - they need the tool and the stock.
    Ok(region)
}
