use super::*;

/// Generate the path for an operation as the world stands now, and verify
/// it. Reads only; the caller stores whatever comes back. The error class
/// is `GenError` for both setup problems (missing tool/stock, unusable
/// geometry) and generator refusals - commands surface them at
/// construction, so nothing half-generated reaches the world.
/// `op_name` is the operation's own name, needed by re-machining to find
/// the program ahead of it (M3.1 D); callers generating a params preview
/// for an operation that has no entity (or none that matters) pass `""`,
/// which reads as "nothing ahead" and generates against the bare stock.
pub fn generate(
    world: &World,
    op: &Operation,
    op_name: &str,
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let job = collect_job(world, op, op_name).inspect_err(|e| {
        tracing::warn!(op = op_name, kind = %op.params.kind(), "generation refused before it ran: {e}");
    })?;
    run_job(job)
}

/// Run a fully-collected job: every world read happened in
/// [`collect_job`], on the caller's thread, and nothing here names a
/// `World`. Split from [`generate`] at the seam M5.3 B needed - a
/// batch's jobs run on worker threads, where a `&World` cannot exist -
/// and it is exactly the old body below the old head, so a solo
/// generation is byte-identical to the file's whole history of them.
pub fn run_job(mut job: GenJob) -> Result<(Toolpath, Vec<Warning>), GenError> {
    // The spindle law's set-aside note (issue #71 review) rides the
    // path whichever arm cut it: a note, never a warning - the post
    // is not held for features the op honestly leaves standing.
    let note = job.spindle_note.take();
    let carried = job.carried_overhang.take();
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
    let body = job.body.clone().filter(|p| super::body_judged::falls_back(&judged_tool, p));
    let _span = tracing::info_span!("generate", op = %job.op_name).entered();
    let started = crate::clock::Instant::now();
    tracing::debug!(kind = %kind, "generation started");
    let (mut path, warns) = run_job_arms(job)
        .map_err(|e| super::body_judged::name_enclosure(e, &judged_tool, body.as_ref()))
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
    for n in note.into_iter().chain(carried) {
        if !path.notes.contains(&n) {
            path.notes.push(n);
        }
    }
    Ok((path, warns))
}

/// [`run_job`]'s body: every arm, unchanged.
fn run_job_arms(job: GenJob) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let GenJob {
        tool,
        body,
        neck,
        lathe_body,
        tool_name,
        params,
        src,
        material,
        frame,
        hs,
        stock_facets,
        stock_aabb,
        analytic_stock,
        stock_fidelity_known,
        mut section_z,
        mut picked_region,
        mut contour_sel,
        mut drive_chains,
        mut prior,
        verify_prior,
        drills_ahead,
        turned_ahead,
        is_mill,
        machine_kind,
        machine_rotaries,
        spindle,
        fixtures,
        fixture_names,
        z_ceiling,
        workholding,
        fixture_meshes,
        rotary_axis,
        part_solid,
        area,
        wcs_origin,
        spindle_note: _,
        carried_overhang: _,
        op_name: _,
    } = job;
    let geom = tool.geom();
    // #368: what the fixture laws sweep above the shank.
    // #852: and a necked tool's drawn shank beside it.
    let holder = crate::tool_body::stack(&geom, body.as_ref()).with_neck(neck);
    // #835: the cutter every stack law reads - its tip (M13.7) and,
    // with a body, the same bands above the shank, never the cylinder
    // enclosing them (#842: drill reach and the cross drill too).
    let cutter = verify::Cutter::of(&tool).with_body(&geom, &holder);
    // The probe (#108) cuts nothing: its own protected-move law judges
    // the stylus against the model, and no cutting law or region applies.
    if matches!(params, OpParams::Probe { .. }) {
        // #842: its stylus reads the body's bands above the stem; only a
        // body that does not fit it is judged by the enclosing band, and
        // the path says so.
        let stack = body.as_ref().map(|_| &holder);
        let enclosed = stack.is_some_and(|s| super::probe_op::stylus_cutter(&tool, Some(s)).body.is_none());
        let note = super::body_judged::probe_note(&tool, body.as_ref().filter(|_| enclosed));
        return super::probe_op::generate_probe(&params, &tool, stack, hs.clearance, &material.facets).map(|(mut p, w)| {
            p.notes.extend(note);
            (p, w)
        });
    }
    // The plasma cut (#670) is its own law: picked contours (or the
    // model's through-silhouette) cut with the torch's kerf; no milling
    // law has a torch to judge, the generator certifies its kerf law.
    if let OpParams::PlasmaProfile(op) = &params {
        let picks = contour_sel.take().transpose()?.map(|(loops, chains, _)| (loops, chains));
        return super::plasma_op::generate_plasma(
            op,
            &tool,
            &hs,
            picks,
            || region_for_job(&mut picked_region, &mut section_z, &src, &params, &material, None),
            &material.facets,
        );
    }
    // The program ahead, cut only when a law asks (#89, #96 review).
    let mut verify_prior = super::drill_prior::PriorCell::new(verify_prior);
    // #339: the mesh law reads the tool as an upright rotating stack -
    // a mill's cutter. A lathe's insert, blade or gauge keeps the box
    // verdict (its body is not that stack).
    use crate::tools::ToolKind as K;
    let lathe_tool = matches!(tool.kind, K::Turning | K::Groove | K::Thread | K::Form);
    let mesh_steel: &[crate::fixture::MeshedFixture] = if is_mill && !lathe_tool { &fixture_meshes } else { &[] };
    // Tool: the diameter drives the path, the body drives the reach law.
    let d = tool.diameter;

    // Source stock, when it is not the source itself: the swarf column
    // around a smaller model. The world reads that chose it rode into
    // the job (M5.3 B); the projection below is pure, so it stays here.
    // Island-vs-swarf in the reach law keys on `region.contains`, so
    // this only widens what counts as material - and it is THIS
    // operation's setup's stock (M3.2 A), never a neighbor setup's
    // block, which is what the collect read of the setup's stock
    // entity guarantees.
    // #1066: a hole-list kind on the stock itself reads the unpunched stock.
    let judged_stock = super::material_source::judged_stock(stock_facets.as_deref(), &src, &params, &material);
    let stock_region = judged_stock
        .and_then(|f| slice::project(f, FOOTPRINT_SAMPLES).ok())
        .filter(|r| !r.is_empty());
    let stock_top_z =
        material.top.max(stock_aabb.map(|(_, b)| b.z).unwrap_or(f64::NEG_INFINITY));

    // Which verify laws the path answers to is registry data (M5.0.2):
    // the row's comment explains each choice. One seam-side override:
    // a drill op carrying radial holes runs the live-tooling kernel,
    // whose chain is horizontal in world XY - `Mode::Radial` stands
    // the vertical-axis laws down for exactly that geometry (the
    // sim-side comment is the doctrine; the registry row stays the
    // Z-hole truth).
    // The lathe's axial hole (M13.5 A) is the second seam-side
    // override: a Z drill on a turned bar is a meridian cut no
    // height field can judge, so `Mode::Revolve` stands the column
    // laws down and `drilling::axial_warnings` judges it below.
    // Issue #80: a lathe drill with cross holes is radial work - its
    // laws live in `cross_spindle` (spindle, raw-stock standoffs,
    // reach) - so the axial verdict judges the Z holes only. A list
    // mixing both kinds is refused by name in the drill arm.
    let lathe_axial = matches!(&params, OpParams::Drill { cross, .. } if cross.is_empty())
        && machine_kind == crate::machine::Kind::Lathe;
    let mode = match &params {
        OpParams::Drill { cross, .. } if !cross.is_empty() => {
            crate::verify::Mode::Radial
        }
        OpParams::Drill { .. } if lathe_axial => crate::verify::Mode::Revolve,
        _ => params.kind().info().mode,
    };

    // The height seam crossed in the collect: the params' From+offset
    // expressions became the generators' plain f64s there, every
    // unresolved reference refused by name before a single move was
    // computed - never a guessed z. The 3+2 wrap's frame (M5.1 B)
    // rode along too; the shift below is pure arithmetic -
    // `h - origin.z`, the identity-exact law: an upright frame at the
    // world origin shifts nothing, so every prismatic file and golden
    // rides on untouched. The generators themselves never see a frame
    // - rotation invariance is the whole argument that this is sound
    // - and the laws that verify the path below run on the local one
    // for exactly the same reason.
    let oz = frame.map_or(0.0, |f| f.origin.z);
    let hl = ResolvedHeights {
        clearance: hs.clearance - oz,
        top: hs.top.map(|v| v - oz),
        bottom: hs.bottom.map(|v| v - oz),
        face_top: hs.face_top.map(|v| v - oz),
        retract: hs.retract.map(|v| v - oz),
    };
    let material_local = frame.as_ref().map(|f| Material {
        // A 1:1 map of the facets: the body ranges ride unchanged.
        facets: f.local_facets(&material.facets),
        top: material.top - oz,
        bodies: material.bodies.clone(),
        excess: material.excess,
    });
    let m_gen = material_local.as_ref().unwrap_or(&material);

    // The area controls (M11.5): the region every arm takes is the
    // controlled one, resolved here once into the slot `region_for_job`
    // reads first. The Contour selection arm resolves its own (its side
    // is only known there). Face keeps the band constraints and turn.
    let contour_sel_arm = matches!((&params, &src), (OpParams::Contour { .. }, SrcRef::Selection(_)));
    let mut area = area.transpose()?;
    // A Contour under an open boundary rides the chain itself (its arm
    // below, `contour_open`): no area law runs for it.
    let contour_open = matches!(params, OpParams::Contour { .. }) && area.as_ref().is_some_and(|c| c.open.is_some());
    let mut face_band: Option<(Vec<(&'static str, Region)>, f64)> = None;
    if !contour_sel_arm && !contour_open && let Some(ctx) = area.take() {
        let base = region_for_job(&mut picked_region, &mut section_z, &src, &params, m_gen, frame.as_ref())?;
        let applied = apply_area_full(base, Some(&ctx))?;
        face_band = Some((applied.forbidden, applied.turn));
        picked_region = Some(Ok(applied.region));
    }


    // Region + path per kind. The level transitions and links (M11.2)
    // are read once: every ladder kind hands them to its generator.
    let mut lk = params.links().copied().unwrap_or_else(|| params.default_links());
    // M11.3: the tool says whether an entry may plunge into solid - laid
    // over the generation's copy only, never stored in the op.
    lk.approach.center_cutting = tool.center_cutting;
    // The bar the turning kinds approach from (#130), read once.
    let (turn_stock, turn_note) = super::turning_stock::read(&params, &material.facets, stock_facets.as_deref());
    let (region, path) = match &params {
        OpParams::Probe { .. } | OpParams::PlasmaProfile(_) => {
            return Err(GenError::Invalid("a probe or plasma cut is generated by its own law".into()));
        }
        OpParams::Face {
            stepover,
            stepdown,
            feed,
            plunge_feed,
            climb,
            blend_turns,
            holes,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let (turn, band) = match face_band.take() {
                Some((forbidden, turn)) => (turn, Some(forbidden)),
                None => (0.0, None),
            };
            let laid = boundary_turn::turn_region(&region, turn)?;
            if let Some(forbidden) = &band {
                boundary_turn::face_band_sane(&laid, forbidden, turn, d, *stepover)?;
            }
            let fparams = FaceParams {
                tool_diameter: d,
                stepover: *stepover,
                clearance_z: hl.clearance,
                cut_z: req_z(hl.bottom, "Cut Z")?,
                top_z: hl.face_top,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                blend_turns: *blend_turns,
                holes: *holes,
            };
            // M13.11: under the cut-across law the facer needs to know
            // which holes of its region hold material standing above
            // the face (an island, lifted over) and which are air (cut
            // across) - the outline of the material above the cut
            // plane, in the facer's own turned frame.
            let solid = match holes {
                machining2d::FaceHoles::CutAir => {
                    super::material_source::standing_above(m_gen, fparams.cut_z)
                        .map(|s| boundary_turn::turn_region(&s, turn))
                        .transpose()?
                }
                _ => None,
            };
            let path = boundary_turn::face_over_back(&laid, &fparams, 0.0, solid.as_ref(), turn)?;
            // M11.3 owed: the facer enters a radius off the footprint -
            // from air over the stock's edge; over the stock (an inner
            // patch) the entry runs on along its row out to air, or the
            // op is refused by name. A centre-cutting tool reads nothing.
            let path = if tool.center_cutting {
                path
            } else {
                super::plunge_solid::face_air_entry(
                    path,
                    (stock_facets.as_deref(), m_gen, frame.as_ref()),
                    band.as_deref().unwrap_or(&[]),
                    |lead| boundary_turn::face_over_back(&laid, &fparams, lead, solid.as_ref(), turn),
                    d * 0.5,
                )?
            };
            (region, path)
        }
        OpParams::Contour {
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            side,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
            entry,
            helix_radius,
            blend_turns,
            ..
        } => {
            // Backstop for a committed file that carries Inside on a
            // whole-model source (commands refuse it at creation): the
            // mirror-gouge law must never reach a path.
            if let Some(msg) = side_sane(&params, &src) {
                return Err(GenError::Invalid(msg));
            }
            // Control-side compensation (M13.11) rides the closed-ring
            // contour only: an open boundary, an open run and a 3+2
            // tilt are refused by name, never posted as a centre path
            // under a Control op.
            // Wear (#38) rides the same doors under its own name.
            let control_comp = lk.approach.compensation.in_control();
            let comp_name = lk.approach.compensation.label();
            if control_comp && contour_open {
                return Err(GenError::Invalid(format!(
                    "{comp_name} compensation does not ride an open contour - choose In computer"
                )));
            }
            if control_comp && matches!(params, OpParams::Contour { tilt: Some(_), .. }) {
                return Err(GenError::Invalid(format!(
                    "{comp_name} compensation does not ride a 3+2 tilt - choose In computer"
                )));
            }
            let cp = ContourParams {
                tool_diameter: d,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                retract_z: req_z(hl.retract, "Retract Z")?,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                side: *side,
                lead: *lead,
                tolerance: *tolerance,
                tab_count: *tab_count,
                tab_width: *tab_width,
                tab_height: *tab_height,
                entry: *entry,
                helix_radius: *helix_radius,
                blend_turns: *blend_turns,
            links: lk,
            };
            let (region, path) = if contour_open {
                // The open boundary replaces the selection's own path:
                // the selection's area only says which side is Inside,
                // and is what the verify keeps the tool out of.
                let base = match &src {
                    SrcRef::Selection(_) => {
                        let (loops, _, _) = contour_sel
                            .take()
                            .ok_or_else(|| unreached("contour selection geometry"))??;
                        if loops.is_empty() { Region::empty() } else { Region::new(loops)? }
                    }
                    _ => region_for_job(&mut picked_region, &mut section_z, &src, &params, m_gen, frame.as_ref())?,
                };
                let ctx = area.take().ok_or_else(|| unreached("contour open boundary"))?;
                let path = super::contour_open::open_pass(&base, &ctx, &cp)?;
                (base, path)
            } else { match &src {
                SrcRef::Selection(_) => {
                    // The boundary path cuts the loops the picks bound;
                    // each open run in the picks adds a centerline pass
                    // along its own points. Either half alone is enough;
                    // together they machine the whole pick list. The
                    // walk resolved them on the caller's thread (M5.3 B);
                    // this arm only takes the answer.
                    let (loops, runs, auto_inside) = contour_sel
                        .take()
                        .ok_or_else(|| unreached("contour selection geometry"))??;
                    if loops.is_empty() && runs.is_empty() {
                        return Err(GenError::Empty(
                            "nothing in the selection bounds an area or a run".into(),
                        ));
                    }
                    let mut cp = cp;
                    cp.side = match side {
                        Side::Auto if auto_inside => Side::Inside,
                        Side::Auto => Side::Outside,
                        s => *s,
                    };
                    if loops.is_empty() && area.is_some() {
                        return Err(GenError::Invalid(
                            "the Boundary settings shape an area, and these picks are open runs the cutter rides - reset them, or pick closed rings".into(),
                        ));
                    }
                    let region = if loops.is_empty() {
                        Region::empty()
                    } else {
                        // The one area-controls seam (M11.4 A / M11.5),
                        // with the side this arm just resolved.
                        let mut ctx = area.take();
                        if let Some(c) = &mut ctx {
                            c.contour_side(cp.side == Side::Inside);
                        }
                        apply_area_controls(Region::new(loops)?, ctx.as_ref())?
                    };
                    let mut path = if region.is_empty() {
                        Toolpath::new(Vec::new())
                    } else if part_solid {
                        // Picked rings of a part: each walked only down
                        // to the floor of the wall it trims - the part
                        // read as the union of its touching bodies
                        // (M13.11) when the picks span several models.
                        machining2d::contour_on_bodies(&region, &m_gen.facets, &m_gen.bodies, &cp)?
                    } else {
                        machining2d::contour(&region, &cp)?
                    };
                    if control_comp && !runs.is_empty() {
                        return Err(GenError::Invalid(format!(
                            "{comp_name} compensation does not ride open runs - pick closed rings, or choose In computer"
                        )));
                    }
                    for run in &runs {
                        // #1015: a run refused shows the rings and runs before it.
                        let next = machining2d::contour_along_chain(run, &cp).map_err(|e| e.after_moves(&path.moves))?;
                        path = stitch(path, next);
                    }
                    (region, path)
                }
                _ => {
                    let region = region_for_job(
                            &mut picked_region,
                            &mut section_z,
                            &src,
                            &params,
                        m_gen,
                        frame.as_ref(),
                    )?;
                    // A whole part's section is ONE plane's rings: each
                    // is walked only where the part's wall it trims
                    // still exists (a blind bore's ring ends at its
                    // floor) - never the top plane's ring at every level.
                    let path = if part_solid {
                        machining2d::contour_on_bodies(&region, &m_gen.facets, &m_gen.bodies, &cp)?
                    } else {
                        machining2d::contour(&region, &cp)?
                    };
                    (region, path)
                }
            } };
            (region, path)
        }
        OpParams::Pocket {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            optimal_load,
            blend_turns,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    links: lk,
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    strategy: *strategy,
                    entry: *entry,
                    helix_radius: *helix_radius,
                    optimal_load: *optimal_load,
                    blend_turns: *blend_turns,
                },
            )?;
            (region, path)
        }
        // The roughing twins (M2.4 A) are the pocket's own machinery
        // with the strategy pinned out of the params: Adaptive spaces
        // its rings by the engagement door, Offset runs the
        // concentric tiling at the stepover (its engagement field is
        // inert in the generator, so the legal constant stands in).
        OpParams::Adaptive2D {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            entry,
            helix_radius,
            optimal_load,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    links: lk,
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    strategy: PocketStrategy::Adaptive,
                    entry: *entry,
                    helix_radius: *helix_radius,
                    optimal_load: *optimal_load,
                    blend_turns: false,
                },
            )?;
            (region, path)
        }
        OpParams::Offset2D {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            entry,
            helix_radius,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    links: lk,
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    strategy: PocketStrategy::Waterline,
                    entry: *entry,
                    helix_radius: *helix_radius,
                    optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
                    blend_turns: false,
                },
            )?;
            (region, path)
        }
        OpParams::Pocket3D {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            rest_rough,
            optimal_load,
            feed_opt,
            shell,
            ..
        } => {
            // The region is the model's own silhouette (see
            // `region_for`); the level law sections the facets
            // themselves at every plane, so there is nothing here for a
            // picked loop to add - and nothing to erode by hand either:
            // one erosion per level, from that level's target.
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let pp = Pocket3DParams {
                links: lk,
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hl.retract,
                entry: *entry,
                helix_radius: *helix_radius,
                optimal_load: *optimal_load,
            };
            // Re-machining (M3.1 D): what the program ahead of this
            // pocket left standing bounds the coarse run, so it cuts
            // the shelves its predecessors already cleared only if the
            // mask says material still stands there. Nothing ahead (or
            // nothing cut) is `None`, and the path is what it was. The
            // mask itself was resolved on the caller's thread (M5.3 B)
            // - an Arc, because a height field this size is never
            // cloned just to move it between threads.
            let prior = prior
                .take()
                .ok_or_else(|| unreached("re-machining mask"))??;
            // Read through the stock seam (M7.3 4/6): motion takes the
            // column reader, never the grid itself.
            let mask_cols = prior.grid.as_ref().map(crate::stock_sim::Columns::of_grid);
            let mask = mask_cols.as_ref();
            // The rest pass's verdict rides the path: what the sim
            // still reports after the closing ladder is the closing
            // report's business (the simulation view), not a silent
            // extra here.
            let generated = match (*shell, *rest_rough) {
                // The level ladder, and its rest-closing sibling:
                // planes all the way down (M2.1b).
                (false, false) => {
                    machining3d::pocket3d_prev(&material.facets, &region, &pp, mask)
                }
                (false, true) => {
                    machining3d::pocket3d_with_rest_prev(&material.facets, &region, &pp, mask)
                        .map(|(p, _)| p)
                }
                // The offset shells (M2.5 F): the same XY plan, the
                // same superset laws, the rings riding the model's
                // relief between the planes - and on steep flanks
                // answering at the waterline, exactly where the plain
                // ladder's rings already stood.
                (true, false) => {
                    machining3d::pocket3d_offset_prev(&material.facets, &region, &pp, mask)
                }
                (true, true) => {
                    machining3d::pocket3d_offset_with_rest_prev(
                        &material.facets,
                        &region,
                        &pp,
                        mask,
                    )
                    .map(|(p, _)| p)
                }
            };
            let path = match generated {
                Ok(path) => path,
                // The ladder's words are "the model fills the pocket"
                // - true of an unmasked nothing-to-cut. With a mask
                // the truer sentence is this one: the program already
                // machined here, and the mask law only ever removes
                // cuts, so the whole path is air.
                Err(GenError::Empty(_)) if mask.is_some() => {
                    return Err(GenError::Empty(
                        "nothing left to cut - the operations ahead in the program already \
                         machined this area"
                            .into(),
                    ));
                }
                Err(e) => return Err(e),
            };
            // The feed modulation is a re-read of the finished path
            // against the sim (M2.1d): same moves, new F words - it
            // runs after both ladder variants, never instead of one.
            let path = if *feed_opt {
                machining3d::optimize_feed(&path, &material.facets, &region, &pp)?
            } else {
                path
            };
            (region, path)
        }
        OpParams::Finish3D {
            pattern,
            stepover,
            tolerance,
            feed,
            plunge_feed,
            climb,
            ramp,
            min_step,
            ramp_angle,
            entry_ramp,
            steep_stepover,
            pencil_reference,
            ..
        } => {
            // Ball jurisdiction: the rings are the tangent locus of a
            // sphere. A flat-bottomed or torus-bottomed cutter's
            // forbidden set is not this facetwise support scan (the
            // Minkowski sum differs), so an end mill here refuses by
            // name - never a ball path dressed as something else.
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "3D Finish needs a ball-nose tool, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let fp = Finish3DParams {
                pattern: *pattern,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: *ramp,
                min_step: *min_step,
                ramp_angle: *ramp_angle,
                entry_ramp: *entry_ramp,
                steep_stepover: *steep_stepover,
                pencil_reference: *pencil_reference,
            };
            // The pattern picks the generator; Waterline never
            // reaches the raster code at all. SteepShallow is the
            // parallel kernel's twin (M2.5 A): the same raster, the
            // same ladder, and the border law inside `emit_rings`
            // that keeps the rings off shallow tangency - which is
            // the whole difference, and it lives in the pattern.
            // M11.3 owed: the centre-cutting law judges the finished
            // path (read only for a tool that is not centre-cutting).
            let (solid_top, solid_fp) = if tool.center_cutting {
                (f64::INFINITY, None)
            } else {
                super::plunge_solid::plunge_solid(stock_facets.as_deref(), m_gen, frame.as_ref())
            };
            let solid = camcraft_motion::plunge_law::Solid { top: solid_top, footprint: solid_fp.as_ref() };
            // The area controls (M11.5 owed): the whole-model path is
            // clipped to the tool-centre area the law resolved - the
            // precompute above put the controlled region in `region`
            // and its forbidden zones in the band slot Face reads.
            let clip = match face_band.take() {
                Some((forbidden, _)) => Some(super::finish_clip::centre_area(&region, &forbidden, d * 0.5)?),
                None => None,
            };
            let run = |fp: &Finish3DParams| -> Result<Toolpath, GenError> {
                let whole = match fp.pattern {
                finishing3d::FinishPattern::Waterline => {
                    finishing3d::waterline(&material.facets, fp)?
                }
                finishing3d::FinishPattern::Parallel
                | finishing3d::FinishPattern::ParallelMulti
                | finishing3d::FinishPattern::Scallop
                | finishing3d::FinishPattern::SteepShallow => {
                    finishing3d::parallel(&material.facets, fp)?
                }
                finishing3d::FinishPattern::Radial => finishing3d::radial(&material.facets, fp)?,
                finishing3d::FinishPattern::Spiral => finishing3d::spiral(&material.facets, fp)?,
                finishing3d::FinishPattern::Contour => finishing3d::contour(&material.facets, fp)?,
                finishing3d::FinishPattern::Project => finishing3d::project(&material.facets, fp)?,
                finishing3d::FinishPattern::Flat => finishing3d::flat(&material.facets, fp)?,
                // The crease pair (M2.5 B): the same locus walk in
                // pencil.rs, and the band filter's reference is the
                // only thing that differs - Corner asks its own
                // ball's band, Pencil the stated reference's.
                finishing3d::FinishPattern::Pencil => finishing3d::pencil(&material.facets, fp)?,
                finishing3d::FinishPattern::Corner => finishing3d::corner(&material.facets, fp)?,
                // M2.5 C. Ramp is Parallel's twin with the sloped
                // laws always on - the diagonal chains (M2.2e) and
                // the ramped ring entries (M8.2 B) are the two
                // certified ways a cut engages without a vertical
                // plunge; the pattern only forces what the pane
                // otherwise asks. MorphSpiral rides the spiral's
                // march with the terrain's LOCAL scallop spacing;
                // Geodesic marches the iso contours of the open
                // boundary's distance field.
                finishing3d::FinishPattern::Ramp => {
                    let mut q = *fp;
                    q.pattern = finishing3d::FinishPattern::Parallel;
                    q.ramp = true;
                    q.entry_ramp = true;
                    finishing3d::parallel(&material.facets, &q)?
                }
                finishing3d::FinishPattern::MorphSpiral => {
                    finishing3d::morphed_spiral(&material.facets, fp)?
                }
                finishing3d::FinishPattern::Geodesic => {
                    finishing3d::geodesic(&material.facets, fp)?
                }
                // M2.5 D. Blend is Parallel's raster plus the ring
                // ladder reading the border at the BAND line: the
                // 30-45° transition gets both families' lines and
                // the seam's staircase is shaved, per `blend`'s own
                // note - the difference lives in `emit_rings`.
                finishing3d::FinishPattern::Blend => {
                    finishing3d::blend(&material.facets, fp)?
                }
                // M2.5 D. Morph is the waterline ladder PLUS the
                // ruled lofts between its levels: the ring part is
                // emit_rings' ordinary walk (Morph is neither steep
                // nor band there), and `morph` adds one column-
                // grounded interpolated guide per paired level gap.
                finishing3d::FinishPattern::Morph => {
                    finishing3d::morph(&material.facets, fp)?
                }
                // M2.5 E. Flow is the 5-axis kind's field law asked
                // of a 3-axis machine: the host lives in
                // `multiaxis::flow3d` - the SAME `build_field` the
                // Flow5D operation runs, no `stamp_axes` - so the
                // grain lives once and the two entries can never
                // drift.
                finishing3d::FinishPattern::Flow => {
                    crate::multiaxis::flow3d(&material.facets, fp)?
                }
                };
                // M13.10: the clip's re-entries ramp along the kept
                // chain under the guard's `entry_ramp` retry, from the
                // solid's top (infinite for a centre-cutting tool, so
                // its re-entry stays the plunge it may make).
                match &clip {
                    Some(c) => super::finish_clip::clip_path(whole, c, fp, solid_top),
                    None => Ok(whole),
                }
            };
            let path = finishing3d::center_cut_guard(tool.center_cutting, &solid, &fp, run)?;
            (region, path)
        }
        // Deburr (M2.5 E B): the cone stands OVER each exposed edge -
        // rim or convex crease - and breaks it. Tool jurisdiction is
        // the chamfer mill's own cone, and the angle is the TOOL's
        // word, never a second promise on the operation: the op
        // authors only the break width, and `drop` (the kernel's
        // single formula) follows. The congruence test with the 2D
        // chamfer needs no shared number because there is none to
        // disagree with.
        OpParams::Deburr {
            width,
            feed,
            plunge_feed,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Taper {
                return Err(GenError::Invalid(format!(
                    "Deburr rides its cone over the edge - it needs a chamfer mill, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            if !(0.0..180.0).contains(&tool.tip_angle) || !tool.tip_angle.is_finite() {
                return Err(GenError::Invalid(format!(
                    "Deburr needs a cone tool: '{}' has an included angle of {}, which rules no chamfer",
                    tool_name, tool.tip_angle
                )));
            }
            // The cone's flank must physically reach the contact
            // circle: at the tip's drop the flank stands `width` out
            // from the axis, that far along the flank from the apex -
            // a short-fluted V stands on its shank there, and the
            // ride would dress a cylinder as a cone (the derived
            // float quotes to a tenth, chamfer's reading law).
            let slant = width / (0.5 * tool.tip_angle.to_radians()).sin();
            if tool.flute_len < slant - 1e-9 {
                return Err(GenError::Invalid(format!(
                    "the break reaches {:.1} mm up the cone's flank; '{}' has {} mm of flute",
                    slant, tool_name, tool.flute_len
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = finishing3d::deburr(
                &material.facets,
                &DeburrParams {
                    width: *width,
                    tip_angle_deg: tool.tip_angle,
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Window Top Z")?,
                    bottom_z: req_z(hl.bottom, "Window Bottom Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                },
            )?;
            (region, path)
        }
        // The simultaneous-axis kinds (M5.1 stage C). The ball-center
        // path is the finish's own - a sphere's envelope is rotation-
        // invariant, so leaning the spindle changes what the SHANK
        // sees, not what the ball leaves - and the kind adds the POSE
        // to every cut move. No ramp, no min_step: the plunge owns
        // the first contact, and the scallop law alone authors the
        // ladder. The two arms are the same generator pair the finish's
        // pattern combo runs, with the axis stamp bolted on.
        OpParams::Contour5D {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        }
        | OpParams::Parallel5D {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        } => {
            let is_contour = matches!(&params, OpParams::Contour5D { .. });
            // The contour has no stepover field to bind (the or-pattern
            // binds one name on one side and E0642s); read it here,
            // where the parallel's own arm answers.
            let stepover = match &params {
                OpParams::Parallel5D { stepover, .. } => *stepover,
                _ => None,
            };
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "{} needs a ball-nose tool, '{}' is a {}",
                    if is_contour {
                        "5-Axis Contour"
                    } else {
                        "5-Axis Parallel"
                    },
                    tool_name,
                    tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let fp = Finish3DParams {
                pattern: if is_contour {
                    finishing3d::FinishPattern::Waterline
                } else {
                    finishing3d::FinishPattern::Parallel
                },
                stepover: if is_contour { None } else { stepover },
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
                entry_ramp: false,
                steep_stepover: None,
                pencil_reference: None,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            // Stage F's dodge is opt-in and adds nothing when off:
            // None means the stamp never measures the shank, so the
            // paths of every v28 document regenerate byte-identical.
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            // M11.3 owed: the centre-cutting law, read along the pose.
            let run = |q: &Finish3DParams| if is_contour {
                crate::multiaxis::contour5d(&material.facets, q, &axis, shank.as_ref())
            } else {
                crate::multiaxis::parallel5d(&material.facets, q, &axis, shank.as_ref())
            };
            let kind = if is_contour { "5-Axis Contour" } else { "5-Axis Parallel" };
            let path = super::plunge_solid::guard_posed(
                kind, tool.center_cutting, tool.kind == crate::tools::ToolKind::BallNose, stock_facets.as_deref(), m_gen, frame.as_ref(), &fp, run,
            )?;
            (region, path)
        }
        // M5.1 stage D: the swarf family. The path is the raster run
        // along the ruling the model states, so the numbers ride the
        // parallel's law; what the kind ADDS is where the ruling comes
        // from - the generator reads it off the facets, and refuses by
        // name (a dome, a Z-wall) when the surface has none. The
        // ladder's count is the one field the plain kind has no name
        // for, so it reads back below, the twins' stepover trick.
        OpParams::Swarf {
            tolerance,
            stepover,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        }
        | OpParams::AdvancedSwarf {
            tolerance,
            stepover,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            let is_ladder = matches!(&params, OpParams::AdvancedSwarf { .. });
            let passes = match &params {
                OpParams::AdvancedSwarf { passes, .. } => *passes,
                _ => 1,
            };
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "{} needs a ball-nose tool, '{}' is a {}",
                    if is_ladder { "Advanced Swarf" } else { "Swarf" },
                    tool_name,
                    tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let fp = Finish3DParams {
                // The ruling replaces the pattern's choice of lines;
                // the geometry that remains is the parallel's exactly.
                pattern: finishing3d::FinishPattern::Parallel,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
                entry_ramp: false,
                steep_stepover: None,
                pencil_reference: None,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let run = |q: &Finish3DParams| if is_ladder {
                crate::multiaxis::swarf5d_advanced(&material.facets, q, &axis, passes)
            } else {
                crate::multiaxis::swarf5d(&material.facets, q, &axis)
            };
            let kind = if is_ladder { "Advanced Swarf" } else { "Swarf" };
            let path = super::plunge_solid::guard_posed(
                kind, tool.center_cutting, tool.kind == crate::tools::ToolKind::BallNose, stock_facets.as_deref(), m_gen, frame.as_ref(), &fp, run,
            )?;
            (region, path)
        }
        // M5.1 stage E: the rotary twins. The path is the finish's
        // own ladder seen through a frame the FITTED axis defines, so
        // the numbers ride the same law; what the kinds add is the
        // axis - model data, refused by name when the model says
        // nothing of turning (and by the other twin when the part's
        // stance is its sibling's) - and the radial pose in place of
        // the surface normal's.
        OpParams::RotaryContour {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        }
        | OpParams::RotaryParallel {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            let is_contour = matches!(&params, OpParams::RotaryContour { .. });
            let stepover = match &params {
                OpParams::RotaryParallel { stepover, .. } => *stepover,
                _ => None,
            };
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "{} needs a ball-nose tool, '{}' is a {}",
                    if is_contour {
                        "Rotary Contour"
                    } else {
                        "Rotary Parallel"
                    },
                    tool_name,
                    tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let fp = Finish3DParams {
                // The kind names the ladder - waterline rings about
                // the axis, or passes along it - so the pattern byte
                // answers the kernel's own reading.
                pattern: finishing3d::FinishPattern::Waterline,
                stepover: if is_contour { None } else { stepover },
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
                entry_ramp: false,
                steep_stepover: None,
                pencil_reference: None,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let run = |q: &Finish3DParams| if is_contour {
                crate::multiaxis::rotary_contour(&material.facets, q, &axis)
            } else {
                crate::multiaxis::rotary_parallel(&material.facets, q, &axis)
            };
            let kind = if is_contour { "Rotary Contour" } else { "Rotary Parallel" };
            // M13.5 B: the letter is the machine's. The fitted line
            // the kernel will turn about must be a rotary the mounted
            // machine states (A/B on a table, or the spindle's C -
            // issue #33 - whose angle the path then states).
            let spindle_c = rotary_letter_gate(machine_rotaries.as_ref(), &material.facets, kind)?;
            let path = super::plunge_solid::guard_posed(
                kind, tool.center_cutting, tool.kind == crate::tools::ToolKind::BallNose, stock_facets.as_deref(), m_gen, frame.as_ref(), &fp, run,
            )?;
            // Issue #33: on the spindle's C the path states its angle.
            let path = super::rotary_gate::stamp_spindle_c(path, spindle_c, (machine_rotaries.as_ref(), is_contour), *tolerance, kind)?;
            (region, path)
        }
        // M5.1 stage E2: the rotary rough. The pocket ladder runs in
        // WRAP SPACE about the fitted axis, so the numbers ride
        // Pocket3D's shape - but three of them are read the wrap's
        // way: `stepdown` infeeds RADII, `top_z`/`bottom_z` window
        // the STATIONS (the kernel's own `(h - o.z)/u.z` law), and
        // the rapids plane is the kernel's, one infeed and a tool
        // radius off the stock's skin (M13.8) - what resolves here only stands when
        // the axis stands up. The refusal for a model that says
        // nothing of turning comes from the same axis fit the twins
        // invoke, by name.
        OpParams::RotaryOffset {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            optimal_load,
            tolerance,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "Rotary Offset needs a ball-nose tool, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let pp = Pocket3DParams {
                links: lk,
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hl.retract,
                entry: *entry,
                helix_radius: *helix_radius,
                optimal_load: *optimal_load,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            // `tolerance` is the VERIFY budget (the envelope law's),
            // stated here so the field's double life is on record:
            // the kernel chords by its own arc-fit tolerance.
            let _ = tolerance;
            // M13.5 B: the machine's letter for the line the ladder
            // wraps about, or the refusal by name.
            let spindle_c = rotary_letter_gate(machine_rotaries.as_ref(), &material.facets, "Rotary Offset")?;
            // M13.8: the ladder peels from the setup STOCK's outer
            // radius about the fitted axis (the part's skin when no
            // separate stock is set, or when the stock sits inside it).
            let path = crate::multiaxis::rotary_offset_in(
                &material.facets,
                stock_facets.as_deref(),
                &pp,
                &axis,
            )?;
            // Issue #33: on the spindle's C the path states its angle,
            // split to the verify budget so the machine's C-linear
            // motion is the chord the laws judge.
            let path = super::rotary_gate::stamp_spindle_c(path, spindle_c, (machine_rotaries.as_ref(), false), *tolerance, "Rotary Offset")?;
            // M11.3 owed (2): the wrap-space entry guard judged the
            // ladder before the wrap posed it; the law judges the FINAL
            // posed path. Solid is the true bar (#16): the stock
            // revolved about the same fitted axis, so a move in the air
            // outside its radius is clean and one into it is judged.
            super::plunge_solid::audit_bar(
                "Rotary Offset", tool.center_cutting, &material.facets, stock_facets.as_deref(), &path, d * 0.5,
            )?;
            (region, path)
        }
        // M5.1 stage E3: the stamped ladder. The positions are the
        // pocket's - so is this whole arm, down to the re-machining
        // mask and the Empty remap - with the stamp's pose law
        // written over the finished path and the ball-nose gate the
        // tangency search needs (the stamp reads contact off a
        // sphere; a flat end's flank is a cylinder the law does not
        // own). No rest pass, no feed modulation: the stamp has
        // never been shown to commute with either, and silence
        // about a flag is a lie waiting to be filed (RECOMMENDED).
        OpParams::MultiAxisRough {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            optimal_load,
            tolerance,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "Multi-Axis Rough needs a ball-nose tool, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let pp = Pocket3DParams {
                links: lk,
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hl.retract,
                entry: *entry,
                helix_radius: *helix_radius,
                optimal_load: *optimal_load,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let _ = tolerance;
            let prior = prior
                .take()
                .ok_or_else(|| unreached("re-machining mask"))??;
            let mask_cols = prior.grid.as_ref().map(crate::stock_sim::Columns::of_grid);
            let mask = mask_cols.as_ref();
            // Same opt-in law as the twins' arm above: off, the ladder
            // stamps exactly as stage E3 shipped it.
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let path = match crate::multiaxis::multiaxis_rough_prev(
                &material.facets,
                &region,
                &pp,
                &axis,
                mask,
                shank.as_ref(),
            ) {
                Ok(path) => path,
                // The pocket arm's Empty remap, verbatim: with a mask
                // the truer sentence names the program, not the model.
                Err(GenError::Empty(_)) if mask.is_some() => {
                    return Err(GenError::Empty(
                        "nothing left to cut - the operations ahead in the program already \
                         machined this area"
                            .into(),
                    ));
                }
                Err(e) => return Err(e),
            };
            // M11.3 owed (2): the pocket's own entry guard judged the
            // ladder before the stamp posed it; the law judges the
            // FINAL posed path along each move's axis.
            super::plunge_solid::audit_posed(
                "Multi-Axis Rough", tool.center_cutting, stock_facets.as_deref(), m_gen, frame.as_ref(), &path, d * 0.5,
            )?;
            (region, path)
        }
        // M5.1 tail, stage I: flowline finishing. The pass is the
        // parallel's law run on lines that TURN - the direction read
        // off the model's own least-curving field, with the fallback
        // law degrading plain ground to straight raster passes so
        // flat interiors are never left uncut. The pose is stage C's
        // lean law with stage F's dodge, and the numbers are the
        // twins', field for field.
        OpParams::Flow5D {
            tolerance,
            stepover,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "5-Axis Flow needs a ball-nose tool, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let fp = Finish3DParams {
                // The field replaces the pattern's choice of lines;
                // the geometry that remains is the raster's exactly.
                pattern: finishing3d::FinishPattern::Parallel,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
                entry_ramp: false,
                steep_stepover: None,
                pencil_reference: None,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            // Stage F's law again, opt-in: None stamps the flow lines
            // with the plain lean, as every v30 path regenerates.
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let run = |q: &Finish3DParams| crate::multiaxis::flow5d(&material.facets, q, &axis, shank.as_ref());
            let path = super::plunge_solid::guard_posed(
                "5-Axis Flow", tool.center_cutting, tool.kind == crate::tools::ToolKind::BallNose, stock_facets.as_deref(), m_gen, frame.as_ref(), &fp, run,
            )?;
            (region, path)
        }
        // The drive-curve contour (M5.1 J): the picked chains, in
        // world 3D, machined WHERE THEY STAND. No pattern is asked of
        // the model - the curve is the path - so there is no stepover
        // to derive, no ladder to author, and the tolerance is not
        // even read here: it belongs to the verify pass below, the
        // envelope budget. The facets are the union `material_of`
        // collected from the selected entities, whole: the ball rests
        // on the model while the chain tells it where to go.
        OpParams::MultiAxisContour {
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "Multi-Axis Contour needs a ball-nose tool, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let top = req_z(hl.top, "Top of Part Z")?;
            let bottom = req_z(hl.bottom, "Bottom of Part Z")?;
            let chains = drive_chains
                .take()
                .ok_or_else(|| unreached("drive chains"))??;
            // The window applies to the curve itself: a picked chain
            // standing outside the operation's top/bottom is a
            // disagreement between the pick and the heights that no
            // pose can fix - refused by vertex, never quietly
            // clipped (the Prime Directive's kind of wrong).
            for (ci, ch) in chains.iter().enumerate() {
                if let Some(v) = ch
                    .iter()
                    .find(|v| v.z < bottom - Z_TOL || v.z > top + Z_TOL)
                {
                    return Err(GenError::Invalid(format!(
                        "picked chain {} stands at z {:.3}, outside the window [{:.3}, {:.3}] - widen Top/Bottom of Part Z or re-pick",
                        ci + 1,
                        v.z,
                        bottom,
                        top
                    )));
                }
            }
            let cp = crate::multiaxis::CurveParams {
                tool_diameter: d,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let path = crate::multiaxis::contour_curve(
                &chains,
                &material.facets,
                &cp,
                &axis,
                shank.as_ref(),
            )?;
            super::plunge_solid::audit_posed(
                "Multi-Axis Contour", tool.center_cutting, stock_facets.as_deref(), m_gen, frame.as_ref(), &path, d * 0.5,
            )?;
            (region, path)
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
            entry,
            helix_radius,
            ..
        } => {
            let path = machining2d::slot(
                *a,
                *b,
                &SlotParams {
                    links: lk,
                    tool_diameter: d,
                    entry: *entry,
                    helix_radius: *helix_radius,
                    width: *width,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stepover: *stepover,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            (region, path)
        }
        OpParams::TrochoidalSlot { a, b, .. } => {
            let tp = super::trochoid_op::trochoid_params(
                &params,
                d,
                hl.clearance,
                hl.retract,
                req_z(hl.top, "Top of Part Z")?,
                req_z(hl.bottom, "Bottom of Part Z")?,
            )?;
            let path = machining2d::trochoidal_slot(&[*a, *b], &tp)?;
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            (region, path)
        }
        OpParams::Trace {
            stepdown,
            feed,
            plunge_feed,
            climb,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::trace(
                &region,
                &TraceParams {
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        // The engrave twin (M2.4 B): the trace's own machinery,
        // verbatim - same zero-width-kerf ride, same no-entry-knob
        // theorem, the difference being only the registry's name for
        // the pick (text on a face, not a boundary line). A V-bit
        // would cut it wider than the kerf's claim; that tooling
        // question is M10.1's, and until then the disc law says so
        // with the sim, honestly.
        OpParams::Engrave {
            stepdown,
            feed,
            plunge_feed,
            climb,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::trace(
                &region,
                &TraceParams {
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        // The 2D chamfer (M2.4 B): a cone ride on the picked edge.
        // The congruence doors live HERE, where the Tool is visible:
        // the kernel states the law on the face alone. The tool must
        // be a chamfer mill whose cone is the supplement of the
        // authored angle (a 45-degree chamfer takes the shop's
        // 90-degree tool), and its diameter must reach the face's
        // full width - `width / tan(alpha)` of cone height is exactly
        // the corner depth, so the reach door is the whole flute law
        // for a well-made cone. `side` cannot change the ride - the
        // cone meets its wall the same circle whether the material
        // stands inside it (bore) or outside (boss) - it says which
        // wall the region picks, and the sim judges.
        OpParams::Chamfer2D {
            width,
            angle_deg,
            feed,
            plunge_feed,
            climb,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Taper {
                return Err(GenError::Invalid(format!(
                    "2D Chamfer rides its face on a cone - it needs a chamfer mill, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let want = 180.0 - 2.0 * angle_deg;
            if (tool.tip_angle - want).abs() > 1e-6 {
                return Err(GenError::Invalid(format!(
                    "a {angle_deg}-degree chamfer takes a {want:.0}-degree cone; '{}' has {} degrees",
                    tool_name, tool.tip_angle
                )));
            }
            if tool.diameter < 2.0 * width - 1e-9 {
                return Err(GenError::Invalid(format!(
                    "a {width} mm chamfer reaches {} mm from the wall; '{}' is only {} mm across",
                    width, tool_name, tool.diameter
                )));
            }
            if tool.flute_len < width * angle_deg.to_radians().tan() - 1e-9 {
                // The depth is DERIVED, and a derived float must not
                // reach the user as its bits: tan(45 degrees) is
                // 0.9999999999999999, and a refusal that quotes
                // "3.9999999999999996 mm" reads as a bug, not a law.
                // A tenth is the shop's reading precision.
                return Err(GenError::Invalid(format!(
                    "the cone of '{}' is {} mm of flute, short of the {:.1} mm depth this face stands at",
                    tool_name,
                    tool.flute_len,
                    width * angle_deg.to_radians().tan()
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::chamfer(
                &region,
                &ChamferParams {
                    width: *width,
                    angle_deg: *angle_deg,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        // Circular interpolation (M2.4 B): concentric rings about an
        // authored center - the pocket family's heights and doors
        // with the region authored like the slot's, and the kernel
        // needs no tool-kind gate: the rings are the disc law, and a
        // ball on them would just be the 3D finish's job (which
        // refuses non-balls at its own door).
        OpParams::Circular {
            center,
            radius,
            vault,
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            entry,
            helix_radius,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            // Picks author the circle (user report, 0.8.0): a picked
            // rim, round floor or holed face hands its own centre and
            // radius; the typed ones rule only without a selection.
            let (center, radius) = match &src {
                SrcRef::Selection(s) if !s.is_empty() => circle_of_region(&region)?,
                _ => (*center, *radius),
            };
            let path = machining2d::circular(
                &region,
                &CircularParams {
                    links: lk,
                    center,
                    radius,
                    vault: *vault,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    tool_diameter: d,
                    stock_to_leave: *stock_to_leave,
                    stepover: *stepover,
                    stepdown: *stepdown,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    entry: *entry,
                    helix_radius: *helix_radius,
                },
            )?;
            (region, path)
        }
        OpParams::Drill {
            cycle,
            peck,
            dwell,
            feed,
            plunge_feed,
            holes,
            cross,
            ..
        } => {
            // No region feeds the generator - the hole list is the
            // geometry - but the verify pass below needs one, and it is
            // built the same shared way as every other kind's.
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            // M5.2 F2: the radial branch. (Radial first, Z otherwise -
            // both arms end in the shared verify tail below.)
            let path = if !cross.is_empty() {
                // M5.2 F2: the radial branch. The Z laws above are the
                // mill's; these holes are plunged by live tooling from
                // a turret station, so three refusals come first - a
                // document that mixes the two lists (one program, two
                // stories about where the tool stands), a plain mill
                // with nothing to plunge them with, and a bottom
                // plane, which is a Z coordinate no radial hole owns.
                // The heights ride as STANDOFFS: the bar's face is
                // curved, so no world plane says "clear of it" - the
                // kernel adds `stock_r` itself.
                if !holes.is_empty() {
                    return Err(GenError::Invalid(
                        "this op lists Z holes AND cross holes - drill either the mill's holes or the turret's, not both (M5.2 v1)".into(),
                    ));
                }
                if is_mill {
                    return Err(GenError::Invalid(
                        "cross holes are live-tooling work - the mounted machine is a mill with no turret".into(),
                    ));
                }
                if hl.bottom.is_some() {
                    return Err(GenError::Invalid(
                        "a Bottom Z plane cannot drive radial holes - each cross hole's depth rides the hole itself (M5.2 v1)".into(),
                    ));
                }
                let top = req_z(hl.top, "Top of Part Z")?;
                // Issue #67: the lathe reads its bar as turned or
                // refuses it by name, and every standoff is measured
                // about the spindle the post programs - world Z
                // through the work offset - not the world origin.
                if machine_kind == crate::machine::Kind::Lathe {
                    super::cross_spindle::lathe_reads(&material.facets, wcs_origin)?;
                }
                let radial = drilling::CrossParams {
                    // The clearance/retract planes projected onto the
                    // plunge axis: their Z height above R is exactly
                    // the standoff the radial law wants (a curved
                    // face has no one plane - the kernel owns the
                    // stock_r part, measured about the spindle).
                    clearance_d: (hl.clearance - top).max(1.0),
                    r_d: (hl.clearance - top).max(1.0),
                    hop_d: hl.retract.map(|r| (r - top).max(0.0)),
                    bottom_d: None,
                    cycle: *cycle,
                    peck: *peck,
                    dwell: *dwell,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    // Both measured about the spindle by the call below.
                    stock_r: 1.0,
                    stock_z_max: None,
                    tool: &tool,
                    holes: cross,
                };
                super::cross_spindle::cross_drill(radial, &material.facets, stock_facets.as_deref(), wcs_origin, &cutter, &tool_name)?
            } else {
                // M13.5 A: on a plain lathe the Drill kind is the
                // turret's axial hole - the doors refuse by name a
                // hole off the spindle axis, one past the bar's far
                // end, and a drill wider than the modeled bore,
                // before the mill's cycle kernel makes the chain.
                if lathe_axial {
                    drilling::axial_doors(&material.facets, holes, d, hl.bottom)?;
                }
                drilling::drill(&drilling::CycleParams {
                    clearance_z: hl.clearance,
                    r_z: req_z(hl.top, "Top of Part Z")?,
                    hop_z: hl.retract,
                    bottom_z: hl.bottom,
                    cycle: *cycle,
                    peck: *peck,
                    dwell: *dwell,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    tool: &tool,
                    holes,
                })
                .and_then(|t| {
                    // Issue #84: flute reach and holder from the stock;
                    // #89: a raw-stock refusal is re-read against what
                    // the setup's earlier operations left standing.
                    let reach = |prior| {
                        super::drill_reach::axial_reach(
                            &t,
                            holes,
                            &material.facets,
                            stock_facets.as_deref(),
                            prior,
                            (&tool, cutter.body.as_ref()),
                            &tool_name,
                        )
                    };
                    if let Err(raw) = reach(None) {
                        match verify_prior.get() {
                            Some(dex) => reach(Some(super::drill_prior::PriorRead { dex, revolved: lathe_axial }))?,
                            None => return Err(raw),
                        }
                    }
                    Ok(t)
                })?
            };
            (region, path)
        }
        // M2.4 C: standalone boring - circular interpolation, the
        // ride M1.6's Bore cycle declines to be. The per-hole entry
        // and floor are the model's (the drill's law); `bottom_z`
        // overrides every floor at once when authored. A hole no
        // wider than the cutter, and a counterbore fold, are the
        // kernel's refusals - named at its doors, not guessed past.
        OpParams::Bore {
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tolerance,
            holes,
            ..
        } => {
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::bore(&machining2d::BoreParams {
                links: lk,
                holes: holes
                    .iter()
                    .map(|h| machining2d::BoreHole {
                        xy: h.xy,
                        radius: h.radius,
                        top_z: h.entry_z,
                        floor_z: hl.bottom.unwrap_or(h.floor_z),
                        through: h.through,
                        stepped: h.stepped,
                    })
                    .collect(),
                tool_diameter: d,
                stock_to_leave: *stock_to_leave,
                stepdown: *stepdown,
                // The cycles' exit gift, the drilling crate's own
                // number - one law, not a second spelling.
                through_allowance: drilling::THROUGH_CLEAR,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                tolerance: *tolerance,
                entry: Entry3D::Plunge,
                helix_radius: 1.0,
            })?;
            (region, path)
        }
        // M2.4 C: helical thread milling. The pitch is the TOOL's,
        // read the way the tap cycle reads it; a thread mill that
        // cannot state one is refused here, by name. Every move
        // posted is plain 3-axis interpolation - the spindle-sync
        // word stays M3.3's machine-definition question, and this
        // kind asks nothing of it.
        OpParams::Thread {
            infeed,
            feed,
            plunge_feed,
            climb,
            holes,
            ..
        } => {
            if !(tool.pitch.is_finite() && tool.pitch > 0.0) {
                return Err(GenError::Invalid(
                    "a thread mill needs a positive pitch on the tool - the tap cycle's law, \
                     read the same way".into(),
                ));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = machining2d::thread_mill(&machining2d::ThreadParams {
                holes: holes
                    .iter()
                    .map(|h| machining2d::ThreadHole {
                        xy: h.xy,
                        radius: h.radius,
                        top_z: h.entry_z,
                        floor_z: hl.bottom.unwrap_or(h.floor_z),
                        stepped: h.stepped,
                    })
                    .collect(),
                pitch: tool.pitch,
                tool_diameter: d,
                infeed: *infeed,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            })?;
            (region, path)
        }
        // M5.2 stage A: the first turning kind. Which documents may
        // HOLD a Facing is the family door in CreateOperation; the
        // door in this arm is the TOOL - the kernel's no-gouge
        // construction IS the nose ball, so a mill cutter named
        // here is refused by name before a move is computed. The
        // facets are the world's (no tilt: the height laws below
        // already read the planes the axis fit walks).
        OpParams::Facing {
            stepdown,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Facing needs a turning insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            // `tolerance` is the VERIFY budget (the envelope law's,
            // as in RotaryOffset) - the kernel itself chords the
            // profile by that law's own tol/4 promise.
            let _ = tolerance;
            // #122: the setup's stock rides along, so an allowance
            // above a part at its finished length is faced off.
            let path = crate::turning::facing(
                &material.facets,
                stock_facets.as_deref(),
                &crate::turning::FacingParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    facing_z: req_z(hl.bottom, "Facing Plane Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                },
            )?;
            (region, path)
        }
        // M5.2 stage B: the profile kinds. The same tool door as
        // facing's - the no-gouge construction IS the nose ball
        // rolled along the silhouette's offset - and the same height
        // law: world Z planes read as stations on the fitted axis.
        // `tolerance` is NOT ignored here: the kernel chords the
        // silhouette and the ride at it (quarter budgets each), so
        // it shapes the path exactly as the verdict measures it.
        OpParams::ProfileRough {
            stepdown,
            stock_to_leave,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Profile Rough needs a turning insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::profile_rough_in(
                &material.facets,
                &crate::turning::ProfileRoughParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stock_to_leave: *stock_to_leave,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        OpParams::ProfileFinish {
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Profile Finish needs a turning insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::profile_finish_in(
                &material.facets,
                &crate::turning::ProfileFinishParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        // M5.2 stage C: the blade kinds. The tool door names the
        // grooving blade - the kerf's width IS the tool, so a
        // turning insert could no more groove than an end mill
        // part off a bar. The heights read the axis stations the
        // twins ride; `tolerance` is load-bearing twice, the
        // profile kinds' law - the silhouette chords at it and the
        // verdict gates at it.
        OpParams::GrooveRough {
            stepdown,
            stock_to_leave,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Groove {
                return Err(GenError::Invalid(format!(
                    "Groove Rough needs a grooving blade, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::groove_rough_in(
                &material.facets,
                &crate::turning::GrooveRoughParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stock_to_leave: *stock_to_leave,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    blade_width: tool.width,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        OpParams::GrooveFinish {
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Groove {
                return Err(GenError::Invalid(format!(
                    "Groove Finish needs a grooving blade, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::groove_finish_in(
                &material.facets,
                &crate::turning::GrooveFinishParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    blade_width: tool.width,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        OpParams::Part {
            stepdown,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Groove {
                return Err(GenError::Invalid(format!(
                    "Parting needs a grooving blade, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::part_in(
                &material.facets,
                &crate::turning::PartParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    parting_z: req_z(hl.bottom, "Parting Plane Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    blade_width: tool.width,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        // M5.2 stage D: threading. The tool door names the V-form
        // insert - the wave's profile IS the tool, so a grooving
        // blade (rectangular footprint, no flanks) could no more
        // cut a thread than an end mill could. The kernel's own
        // doors read the pair against the pitch (fit, full-form
        // reach) and refuse by name; the heights frame the window
        // of turns the lattice fills.
        OpParams::Threading {
            pitch,
            depth,
            infeed,
            feed,
            plunge_feed,
            tolerance,
            hand,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Thread {
                return Err(GenError::Invalid(format!(
                    "Threading needs a threading insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::thread_in(
                &material.facets,
                &crate::turning::ThreadParams {
                    tolerance: *tolerance,
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    pitch: *pitch,
                    depth: *depth,
                    infeed: *infeed,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    insert_width: tool.width,
                    form_angle: tool.tip_angle,
                    hand: *hand,
                },
                turn_stock,
            )?;
            (region, path)
        }
        // M5.2 stage E: the contour rides. The tool door names the
        // turning insert again - the ride is the nose's, and the
        // profile kinds' ball-of-the-nose law is what stands a
        // tolerance's promise up. The kernel's own door reads the
        // authored face against the model and refuses the lie by
        // name; these arms only spend the window and the numbers.
        OpParams::Chamfering {
            angle_deg,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Chamfering needs a turning insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::chamfer_in(
                &material.facets,
                &crate::turning::ChamferParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    angle_deg: *angle_deg,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        OpParams::TaperTurning {
            bottom_dia,
            top_dia,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Taper Turning needs a turning insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::taper_in(
                &material.facets,
                &crate::turning::TaperParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    bottom_dia: *bottom_dia,
                    top_dia: *top_dia,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        // Multi-pass spends stage B's ladder law verbatim: the same
        // rung numbers into the same door, plus the ride count the
        // spring law shares the leave over.
        OpParams::MultiPassTurning {
            stepdown,
            stock_to_leave,
            passes,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Multi-Pass Turning needs a turning insert, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::multi_pass_in(
                &material.facets,
                &crate::turning::MultiPassParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stock_to_leave: *stock_to_leave,
                    passes: *passes,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
                turn_stock,
            )?;
            (region, path)
        }
        // M5.2 stage E2: form turning. The tool door names the form
        // gauge - the crown IS ground steel, and a nose circle could
        // no more stamp a formed surface than a flat blade can cut a
        // thread. The kernel's own doors read the crown, the band and
        // the modeled valley against each other and refuse a smooth
        // bar's promise by name; the depths are the model's, never a
        // number's, and these arms spend only the window it stands in.
        OpParams::FormTurning {
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Form {
                return Err(GenError::Invalid(format!(
                    "Form Turning needs a form gauge, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::form_in(
                &material.facets,
                &crate::turning::FormParams {
                    tolerance: *tolerance,
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    band_width: tool.width,
                    crown_radius: tool.nose_radius,
                },
                turn_stock,
            )?;
            (region, path)
        }
        // M5.2 stage F: turning boring. The tool door names the boring
        // bar - the ground nose IS the size the hole is held to, and
        // the kernel's doors read the modeled hole against the window
        // and refuse a solid bar, a blocked axis and an unreachable
        // floor by name. The ride is stage B's offset law through the
        // mirror: in on the axis, out to the wall, up the hole.
        OpParams::BoringTurning {
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Bore {
                return Err(GenError::Invalid(format!(
                    "Boring needs a boring bar, '{}' is a {}",
                    tool_name, tool.kind
                )));
            }
            let region = region_for_job(
                    &mut picked_region,
                    &mut section_z,
                    &src,
                    &params,
                m_gen,
                frame.as_ref(),
            )?;
            let path = crate::turning::bore(
                &material.facets,
                &crate::turning::BoreParams {
                    tolerance: *tolerance,
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    // The bar's body, centred on the spindle in its
                    // approach and retract (#103).
                    head_d: tool.diameter,
                    shank_d: tool.shank_d,
                },
            )?;
            (region, path)
        }
    };
    let path = super::turning_stock::noted(path, turn_note);
    // #369: a turning steel with no lathe body says its holder went unjudged.
    let path = super::turning_holder::noted(path, &params, &tool, lathe_body.is_some());

    // M3.2 stage B: what the program clamps to. The op's setup context
    // decides - user fixtures bound to it (tagged models, implicit-
    // setup resolution on both sides) plus the machine table when the
    // .mch states a travel the table can sit in. Both reads rode in
    // with the job (M5.3 B); the machine's own steel too: the
    // unstated-limit law means a file that names no spindle body adds
    // no band, exactly as it adds no feed or travel check.
    let fixture_boxes = fixtures;
    // #339 slice 5: an upright mill's links are planned over the steel -
    // a struck link rises over it, an unclearable one refuses by name.
    // Posed and rotary paths keep the report-only verdict below.
    let path = if frame.is_none() && rotary_axis.is_none() && is_mill && !lathe_tool {
        let named: Vec<_> = fixture_names.into_iter().zip(fixture_boxes.iter().copied()).collect();
        let stack = verify::steel_stack(&geom, &holder, spindle);
        let steel = super::fixture_route::LinkSteel { boxes: &named, stack: &stack, ceiling: z_ceiling };
        // The re-judge is the post gate's own per-op law over the same
        // per-setup inputs (#482's `fixture_struck`), so a routed link
        // posts exactly when the gate would pass it.
        let wh = verify::Workholding { turning: &workholding.0, fixed: &workholding.1, axis: rotary_axis };
        super::fixture_route::plan_links(path, &steel, |p| {
            !crate::fixture_struck::path_hits(p, (&geom, &holder), &wh, spindle, mesh_steel).is_empty()
        })?
    } else {
        path
    };
    let stl = match &params {
        OpParams::Face { .. }
        | OpParams::Probe { .. }
        | OpParams::PlasmaProfile(_)
        | OpParams::Trace { .. }
        // The engrave twin leaves no kerf shelf (M2.4 B), and the
        // chamfer cone owns its face outright - the ride IS the
        // finished surface, a leave number could only move the rest
        // verdict (the contour-rides' answer, spoken on a mill).
        | OpParams::Engrave { .. }
        | OpParams::Chamfer2D { .. }
        // The deburr cone owns its break outright, same grammar as
        // its 2D twin one row up: the ride IS the finished edge, and
        // a leave number could only move the rest verdict (M2.5 E B).
        | OpParams::Deburr { .. }
        | OpParams::Drill { .. }
        | OpParams::Finish3D { .. }
        // The 5-axis kinds leave no stock - the ball path is the
        // finish's path, and the pose adds no material law. The swarf
        // family included: lying WITH the flank changes what the
        // shank sees, not what the ball removes.
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        // Rotary leaves no stock either: the posed path is the
        // finish's path, the axis fit adds no material law.
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. }
        // Flow bends the lines, not the material law. The drive-curve
        // contour removes what the chain says, nothing more - the
        // ball's envelope at the curve's stations is the whole law.
        | OpParams::Flow5D { .. }
        | OpParams::MultiAxisContour { .. }
        // Facing leaves no shelf: the ladder steps to the facing
        // plane and stops - the tolerance measures the finish, a
        // stock-to-leave number would only move the verdict (M5.2 A).
        // The finish ride is that answer on the skin: the tolerance
        // IS its chord budget, and a leave would only move the
        // verdict (M5.2 B).
        | OpParams::Facing { .. }
        // The finish plunge is the floor itself - the leave would
        // only move the rest verdict, and the verdict already reads
        // it (M5.2 C). Parting leaves nothing: the bar that stands
        // past the kerf is the scrap, and it goes.
        | OpParams::ProfileFinish { .. }
        | OpParams::GrooveFinish { .. }
        | OpParams::Part { .. }
        // Threading leaves no shelf either: the full-form pass owns
        // the whole wave, and the shallower rungs before it only
        // widen it - what stands is what the wave scan measures,
        // with nothing added to the floor (M5.2 D). The contour
        // rides own their skin outright: a chamfer and a taper are
        // finished by definition, and a multi-pass program ends ON
        // the profile - its leave is spent inside the ladder, not
        // standing outside the verdict (M5.2 E).
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        | OpParams::MultiPassTurning { .. }
        // A crown plunge leaves no shelf either: the valley IS the
        // promise, and the deepest legal sink stands in it exactly -
        // a leave number could only move the rest verdict (M5.2 E2).
        // The bore ride owns its wall outright: the nose stands ON
        // the skin it offsets, and a leave would only move the
        // envelope's pass band (M5.2 F).
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. }
        // The thread mill owns the wave outright: the helix ends ON
        // the major circle, and the infeed rungs before it only
        // widen it (M2.4 C) - the threading answer on a mill.
        | OpParams::Thread { .. } => 0.0,
        OpParams::Contour { stock_to_leave, .. }
        | OpParams::Pocket { stock_to_leave, .. }
        // The twins leave the pocket's shelf, same law (M2.4 A).
        | OpParams::Adaptive2D { stock_to_leave, .. }
        | OpParams::Offset2D { stock_to_leave, .. }
        | OpParams::Slot { stock_to_leave, .. }
        | OpParams::TrochoidalSlot { stock_to_leave, .. }
        // The circular ladder leaves the pocket's shelf, ring 0 eroded
        // off the wall by it (M2.4 B).
        | OpParams::Circular { stock_to_leave, .. }
        // The boring ride leaves the wall's own shelf: the rim
        // stands this far short of the model's circle (M2.4 C).
        | OpParams::Bore { stock_to_leave, .. }
        | OpParams::Pocket3D { stock_to_leave, .. }
        // The ladder is the one rotary that LEAVES a shelf on purpose:
        // the finishing kinds own the ledge under a column's last cut,
        // so the wrap reads this number as a radial wall (E2).
        | OpParams::RotaryOffset { stock_to_leave, .. }
        // The stamped ladder leaves the pocket's shelf, same law.
        | OpParams::MultiAxisRough { stock_to_leave, .. }
        // The rung ladder leaves a radial wall read as a distance to
        // the part - which is exactly what the scan measures
        // (M5.2 B): every feed point must stand nose+leave off.
        // The blade ladder leaves the same skin on the groove floor,
        // and the meridian verdict gates REST by exactly tol+leave
        // (M5.2 C).
        | OpParams::ProfileRough { stock_to_leave, .. }
        | OpParams::GrooveRough { stock_to_leave, .. } => *stock_to_leave,
    };
    // The 2D kinds' depth-aware verify (the silent-gouge fix): a flat
    // region says where the walls stand at ONE plane, and a path walked
    // at every level of the ladder must also answer to the part solid
    // at each level it cuts - a blind bore's ring below its floor is in
    // steel the flat region never shows.
    let depth_law = depth_law_of(&params, &src, part_solid);
    match &frame {
        None => {
            let mut warns = verify::verify_bodies(
                &path,
                &VerifyContext {
                    region: &region,
                    stock: stock_region.as_ref(),
                    // Only the mesh-driven kinds' laws are depth-dependent;
                    // every other op verifies against its static region,
                    // untouched.
                    model: matches!(
                        &params,
                        OpParams::Pocket3D { .. }
                            | OpParams::Finish3D { .. }
                            // The deburr loci ARE the model's welded
                            // edges: its boundary law reads the mesh
                            // at the tip's plane, never the flat
                            // footprint's silhouette wall (M2.5 E B).
                            | OpParams::Deburr { .. }
                            | OpParams::Contour5D { .. }
                            | OpParams::Parallel5D { .. }
                            | OpParams::Swarf { .. }
                            | OpParams::AdvancedSwarf { .. }
                            | OpParams::RotaryContour { .. }
                            | OpParams::RotaryParallel { .. }
                            | OpParams::RotaryOffset { .. }
                            | OpParams::MultiAxisRough { .. }
                            | OpParams::Flow5D { .. }
                            // The chain rides the model: the scan
                            // measures the rest against these facets,
                            // the same depth-dependent law (M5.1 J).
                            | OpParams::MultiAxisContour { .. }
                            // Facing rides the fitted axis and its
                            // stations stop at the profile's own
                            // radius: the rest is what the facets
                            // say, measured of revolution (M5.2 A).
                            | OpParams::Facing { .. }
                            // The profile rungs and the ride read
                            // the same revolution's rest - the
                            // silhouette IS the model's own envelope
                            // (M5.2 B).
                            | OpParams::ProfileRough { .. }
                            | OpParams::ProfileFinish { .. }
                            // The blades judge against the same
                            // facets - the meridian scan reads this
                            // very silhouette as E(s), the finished
                            // part's own chord polyline (M5.2 C).
                            | OpParams::GrooveRough { .. }
                            | OpParams::GrooveFinish { .. }
                            | OpParams::Part { .. }
                            // The wave rides the same facets: the
                            // modeled thread's silhouette IS what the
                            // lattice must land in (M5.2 D). The
                            // contour rides promise the very skin
                            // this scan measures - door and verdict
                            // read the same polyline (M5.2 E).
                            | OpParams::Threading { .. }
                            | OpParams::Chamfering { .. }
                            | OpParams::TaperTurning { .. }
                            | OpParams::MultiPassTurning { .. }
                            // The crown sinks on the same silhouette:
                            // the modeled formed surface IS what the
                            // plunge must land in (M5.2 E2). The
                            // boring bar rides the same facets from
                            // the inside: the wall's own envelope,
                            // rolled about the centers, IS what the
                            // ride must stand on (M5.2 F).
                            | OpParams::FormTurning { .. }
                            | OpParams::BoringTurning { .. }
                    )
                    .then_some(&material.facets[..])
                    .or((depth_law == DepthLaw::Section).then_some(&material.facets[..])),
                    tool: &geom,
                    // The op's real tip for the solid law (M13.7).
                    cutter: Some(cutter),
                    mode,
                    stock_top_z,
                    clearance_z: hl.clearance,
                    stock_to_leave: stl,
                    gouge_tol: GOUGE_TOL,
                    z_tol: Z_TOL,
                    // The finish op's envelope budget is its own tolerance;
                    // no other kind runs the envelope law - and the
                    // 5-axis kinds ARE the envelope law with a leaning
                    // shank, so they carry their tolerance the same way
                    // (the ball's path is identical; the envelope scan
                    // never sees the difference).
                    finish_tol: super::fidelity_law::finish_tol_of(&params),
                    // Steel runs against the path below, under the
                    // post gate's workholding laws (M13.8 owed).
                    fixtures: &[],
                    spindle: None,
                    // #864: a source imported coarser than promised.
                    model_excess: material.excess,
                },
                // The picks' touching bodies, read as their union
                // (M13.11); empty for one body.
                &material.bodies,
            );
            // #1091: a dive convicted against the stock's highest point is re-read under the tool.
            super::stock_top_law::acquit(&path, mode, judged_stock.filter(|_| stock_fidelity_known), &material.facets, &geom, material.excess, &mut warns);
            // The program ahead cut too (#96 review): a dive the column
            // convicted against the raw block is re-read against what
            // the setup's earlier operations left standing.
            // Cut only when a rapid was convicted: the simulation is
            // the costly half, and a clean path needs no second word.
            if warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. }))
                && let Some(dex) = verify_prior.get()
            {
                let prior = crate::stock_sim::Columns::of_grid(dex);
                verify::acquit_by_prior(&path, &prior, geom.radius, Z_TOL, &mut warns);
            }
            // A turned path's axial rapids are judged in the meridian
            // plane, not by the column (#96, #103): the revolved stock
            // less what the program ahead faced and this path cut.
            let meridian = super::turning_rapids::Meridian {
                part: &material.facets,
                stock: stock_facets.as_deref(),
                // Only an insert's moves carry a nose CENTRE; a
                // blade, thread or form gauge's carry its tip, the
                // body standing outward (a form's `nose_radius` is
                // its crown arc, not a ball round the tip).
                nose: if matches!(tool.kind, crate::tools::ToolKind::Turning | crate::tools::ToolKind::Bore) {
                    tool.nose_radius
                } else {
                    0.0
                },
                // #904: tightened by a coarse model's excess, like the
                // gouge laws - every reading the margin excuses shrinks.
                margin: super::fidelity_law::tightened(GOUGE_TOL, material.excess),
                z_tol: Z_TOL,
                drills: &drills_ahead,
                ahead: &turned_ahead,
                outward: match tool.kind {
                    crate::tools::ToolKind::Groove => Some(f64::INFINITY),
                    crate::tools::ToolKind::Thread => Some((tool.tip_angle.to_radians() * 0.5).tan()).filter(|k| k.is_finite() && *k > 0.0),
                    _ => None,
                },
                // A blade's move is its band's centre (a thread's,
                // its crest's): the edge spans half the width.
                band: if matches!(
                    tool.kind,
                    crate::tools::ToolKind::Groove | crate::tools::ToolKind::Thread | crate::tools::ToolKind::Form
                ) {
                    tool.width * 0.5
                } else {
                    0.0
                },
                // A boring bar's body round its nose, up its reach.
                body: (tool.kind == crate::tools::ToolKind::Bore).then_some(super::turning_rapids::BarBody {
                    head_d: tool.diameter,
                    shank_d: tool.shank_d,
                    flute_len: tool.flute_len,
                    shank_len: tool.shank_len,
                }),
            };
            super::turning_rapids::judge_axial_rapids(&params, &path, &meridian, &mut verify_prior, &mut warns);
            // ...and a boring bar's head and shank on its FEEDS (#146),
            // against the same revolved material at each instant.
            super::turning_feeds::judge_bar_feeds(&params, &path, &meridian, &mut verify_prior, &mut warns);
            // ...and a modelled lathe holder on every move (#369), against
            // the revolved stock standing at each instant.
            super::turning_holder::judge(&params, &path, &meridian, lathe_body.as_deref(), &tool, &mut verify_prior, &mut warns)
                .map_err(GenError::Invalid)?;
            warns.extend(workholding_warnings(&path, (&geom, &holder), &fixture_boxes, &workholding, rotary_axis, spindle, mesh_steel));
            // The revolved-blade verdict (M5.2 C): the meridian
            // scan of what the bands leave. No sphere can speak it
            // - a grooved part is undercut BY DESIGN, material
            // standing beside the kerf out of every ball's reach
            // along Z - so the envelope law stands the family down
            // and the truth is measured here, against these very
            // facets, move-attributed into the same vocabulary.
            if let Some(tol) = match &params {
                OpParams::GrooveRough { tolerance, .. }
                | OpParams::GrooveFinish { tolerance, .. }
                | OpParams::Part { tolerance, .. } => Some(*tolerance),
                _ => None,
            } {
                warns.extend(crate::turning::blade_warnings_excess(
                    &material.facets,
                    &path,
                    tool.width,
                    tol,
                    stl,
                    // #904: the gouge clause gives a coarse model's
                    // excess back, as `verify`'s laws do (#864).
                    material.excess,
                ));
            }
            // The inspection ask (M5.3 D) is NOT judged here: it
            // measures the PART, and the part is what the program's
            // blades cut TOGETHER - a rough's left floor is silent
            // once a later finish removes it, and no single op's
            // scan can know that. The verdict lives at the post,
            // composing every blade leg into the final cut
            // (posting::verification_gate), where the conviction
            // names the op that left the steel. What is judged
            // here is the machining promise alone: this path under
            // its own tolerance, above. An ask that no probe
            // budgets costs nothing extra anywhere.
            // The threading verdict (M5.2 D): the path's own helix
            // states the lattice - every second arc endpoint is a
            // meridian crossing, where the V's floor belongs - and
            // the same meridian scan reads it with the WAVE clause:
            // the tool's V, not a boxcar, says what stands between
            // the turns. The insert's crest width and included
            // angle ride from the tool, the blade's law again.
            if let OpParams::Threading { tolerance, hand, .. } = &params {
                warns.extend(crate::turning::thread_warnings_excess(
                    &material.facets,
                    &path,
                    tool.width,
                    tool.tip_angle,
                    *tolerance,
                    material.excess,
                ));
                // The hand verdict (#629): every half-turn winds the
                // way the operation asked, or the post is refused.
                warns.extend(crate::turning::thread_hand_warnings(&path, *hand));
            }
            // The crown verdict (M5.2 E2): the same meridian scan the
            // blade and the wave ride, read with the CROWN floor - the
            // gauge's arc, not a boxcar or a flank, says what stands
            // between the band's edges - and with the crest law the
            // thread already keeps: what a crown floor stands above
            // the blank belongs to the shell, not to this cut.
            if let OpParams::FormTurning { tolerance, .. } = &params {
                warns.extend(crate::turning::form_warnings_excess(
                    &material.facets,
                    &path,
                    tool.width,
                    tool.nose_radius,
                    *tolerance,
                    material.excess,
                ));
            }
            // The axial-hole verdict (M13.5 A): `Mode::Revolve` stood
            // the column laws down for a drill that runs down the
            // spindle, so the chain is judged here against the part's
            // own recognized bore - width, floor and existence, by
            // name at the cycle's first move.
            if lathe_axial {
                let tol = super::fidelity_law::tightened(GOUGE_TOL, material.excess);
                warns.extend(drilling::axial_warnings(&material.facets, &path, d, tol));
            }
            // The containment kinds' solid law (their verify runs without
            // the model - exact rims): the tool's own tip and stack swept
            // along every move against the part solid. The Section kinds
            // answer the same law inside `verify` (b2), real tip (M13.7).
            if depth_law == DepthLaw::Contain {
                contain_into(&mut warns, &path, &material.facets, &cutter, depth_law, stl, material.excess, &params);
            }
            // The rotary kinds' bands and rapids (M13): `Mode::Rotary`
            // stood the vertical-column laws down; the radial field
            // reads rest material in the frame the work turns in.
            if let Some(tol) = super::rotary_law::rotary_tolerance(&params) {
                super::rotary_law::rotary_into(
                    &mut warns,
                    &path,
                    &material.facets,
                    stock_facets.as_deref(),
                    &cutter,
                    (tol, material.excess),
                );
            }
            // The all-air verdict (M13.8): a finish that provably cuts
            // nothing says so by name instead of passing silent.
            let mut path = super::air_law::air_noted(path, mode, &material.facets, stock_top_z, &geom);
            // #104: explain the height mismatch, without replacing any move-level
            // verdict. Tilted, rotary and turning heights are not world-Z tops.
            if stock_aabb.is_some()
                && matches!(params.kind().info().section, Section::Mill2D | Section::Mill3D)
            {
                super::top_stock::note(
                    &mut path, hl.face_top.or(hl.top), &region,
                    stock_facets.as_deref().unwrap_or(&material.facets), analytic_stock.then_some(&mut verify_prior),
                );
            }
            Ok((path, warns))
        }
        Some(f) => {
            // The split verdict (M5.1 B): the planar laws walk the
            // LOCAL path, because they are rigid-invariant - XY
            // distances, containments and half-planes measured in the
            // cutting plane ARE the machine-truth question (a tilted
            // rapid never "dips" in world Z; it travels the plane).
            // The stock column's cap takes the frame-image of the
            // world stock box's corners: a world-vertical prism is a
            // tilted slab seen from the plane, and the corner max is
            // never below the slab over any sampled column - an over-
            // report, never a hide. The hardware laws walk the WORLD
            // path: the steel of the machine does not move with the
            // index, and stage A's lean enclosures read the pose off
            // the moves' stamped axes. One path is posted; both are
            // measured.
            let path_world = f.world_path(&path, machining2d::ARC_FIT_TOL);
            let mut st_local = stock_top_z - oz;
            let mut boxes: Vec<(crate::geometry::Point3, crate::geometry::Point3)> = Vec::new();
            if let Some(b) = stock_aabb {
                boxes.push(b);
            }
            // The model's own corners too: `stock_top_z` already holds
            // its top, but the tilted slab rides over its XY span, and
            // the affine local-z map takes its max at a corner.
            let mut lo = crate::geometry::Point3::new(f64::MAX, f64::MAX, f64::MAX);
            let mut hi = crate::geometry::Point3::new(f64::MIN, f64::MIN, f64::MIN);
            for t in &material.facets {
                for p in [t.a, t.b, t.c] {
                    lo.x = lo.x.min(p.x);
                    lo.y = lo.y.min(p.y);
                    lo.z = lo.z.min(p.z);
                    hi.x = hi.x.max(p.x);
                    hi.y = hi.y.max(p.y);
                    hi.z = hi.z.max(p.z);
                }
            }
            boxes.push((lo, hi));
            for (b0, b1) in boxes {
                for i in 0..8 {
                    let p = crate::geometry::Point3::new(
                        [b0.x, b1.x][(i >> 2) & 1],
                        [b0.y, b1.y][(i >> 1) & 1],
                        [b0.z, b1.z][i & 1],
                    );
                    st_local = st_local.max(f.z_local(p));
                }
            }
            let stock_local = judged_stock.map(|t| f.local_facets(t));
            let stock_region_local = stock_local
                .as_deref()
                .and_then(|t| slice::project(t, FOOTPRINT_SAMPLES).ok())
                .filter(|r| !r.is_empty());
            let mut warns = verify::verify_bodies(
                &path,
                &VerifyContext {
                    region: &region,
                    stock: stock_region_local.as_ref(),
                    // The local facets: the path is local here, and
                    // the depth law reads the part in the same frame.
                    model: (depth_law == DepthLaw::Section).then_some(&m_gen.facets[..]),
                    tool: &geom,
                    // The op's real tip for the solid law (M13.7).
                    cutter: Some(cutter),
                    mode,
                    stock_top_z: st_local,
                    clearance_z: hl.clearance,
                    stock_to_leave: stl,
                    gouge_tol: GOUGE_TOL,
                    z_tol: Z_TOL,
                    finish_tol: None,
                    // Steel runs against the world path below.
                    fixtures: &[],
                    spindle: None,
                    // #864: a source imported coarser than promised.
                    model_excess: material.excess,
                },
                &m_gen.bodies,
            );
            // #1091: the dive re-read under the tool, in the plane's frame.
            super::stock_top_law::acquit(&path, mode, stock_local.as_deref().filter(|_| stock_fidelity_known), &m_gen.facets, &geom, m_gen.excess, &mut warns);
            // The containment kinds' solid law (their verify runs without
            // the model - exact rims): the tool's own tip and stack swept
            // along every move against the part solid. The Section kinds
            // answer the same law inside `verify` (b2), real tip (M13.7).
            if depth_law == DepthLaw::Contain {
                contain_into(&mut warns, &path, &m_gen.facets, &cutter, depth_law, stl, material.excess, &params);
            }
            warns.extend(workholding_warnings(
                &path_world,
                (&geom, &holder),
                &fixture_boxes,
                &workholding,
                rotary_axis,
                spindle,
                mesh_steel,
            ));
            Ok((path_world, warns))
        }
    }
}

/// The rotary kinds' letter door (M13.5 B): the line the kernel will
/// turn the part about, fitted from the same facets it reads, held
/// against the mounted machine's rotary inventory. A fit that fails
/// here is left to the kernel, which refuses it by its own name; a
/// line no stated rotary holds refuses here. The spindle's C answers
/// the fitted line (issue #33) - the path is then stamped with its C.
fn rotary_letter_gate(
    machine: Option<&(String, Vec<crate::machine::AxisLimits>)>,
    facets: &[Triangle],
    kind: &str,
) -> Result<Option<crate::multiaxis::axis::Axis>, GenError> {
    let Ok(ax) = crate::multiaxis::axis::fit(facets) else {
        return Ok(None);
    };
    let letter = super::rotary_gate::rotary_letter(machine, [ax.dir.x, ax.dir.y, ax.dir.z], kind)?;
    Ok((letter == Some('C')).then_some(ax))
}

/// The fixture laws (c2)/(d2) at generation, exactly as the post gate
/// runs them (`posting::fixture_problems`, M13.8 owed): the steel split
/// by whether it turns with the part, the spindle housing riding posed
/// rapids too. `all` is the unsplit list - empty means no steel this op
/// could strike at all (the machine-derived table included), and the
/// spindle band is never tested on its own. A fixed box loses a hit only
/// where the machine frame proves the move clear; the turning boxes keep
/// the path-frame verdict untouched. #339: `meshes` (the tagged models'
/// closed meshes, empty for a lathe's tool) then acquit the moves
/// (upright, posed or turned) a box flagged and its real steel does not meet
/// (`verify::mesh_refined` - the mesh never adds a hit).
#[allow(clippy::too_many_arguments)] // the gate's inputs + the meshes
fn workholding_warnings(
    path: &Toolpath,
    (geom, holder): (&crate::tools::ToolGeom, &camcraft_tools::holder_profile::HolderStack),
    all: &[crate::fixture::FixtureBox],
    (turning, fixed): &(Vec<crate::fixture::FixtureBox>, Vec<crate::fixture::FixtureBox>),
    axis: Option<verify::RotaryAxis>,
    spindle: Option<(f64, f64)>,
    meshes: &[crate::fixture::MeshedFixture],
) -> Vec<Warning> {
    if all.is_empty() {
        return Vec::new();
    }
    let wh = verify::Workholding { turning, fixed, axis };
    // The post gate's own per-op law (#339 slice 5): one law, one verdict.
    let warns = crate::fixture_struck::path_hits(path, (geom, holder), &wh, spindle, meshes);
    crate::fixture::log_mesh_refusals(meshes);
    warns
}

/// Which depth-aware law a 2D kind's path answers to beyond its flat
/// region (the silent-gouge fix, Prime Directive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DepthLaw {
    /// Nothing beyond the kind's own laws: the 3D kinds (they pass the
    /// model already), stock sources, and the kinds whose source IS the
    /// material they remove or whose cut enters the part by design.
    None,
    /// The full model law: the gouge tangency and containment checked
    /// against the part's section at every move's deepest plane, and
    /// the reach laws reading those sections as islands - plus the
    /// solid law (b2): the tool stack swept along every move against
    /// the part's solid, which no plane section can stand in for.
    Section,
    /// The solid law only (drill, bore, thread mill): their regions
    /// carry exact rims for the mesh's tessellated hole walls, so
    /// tangency to the facets would convict a matched tool - the solid
    /// law excuses exactly those rims, and a stack entering the part
    /// anywhere else (a plunge past or through a blind floor, a lip
    /// over the hole) is a gouge.
    Contain,
}

/// The depth law per kind and source. Only a finished-part source (see
/// `GenJob::part_solid`) can be gouged:
/// - Contour and Face trim/skim the part itself: `Section`.
/// - The pocket family cuts picked part features (`Section`); a whole-
///   model source there is the volume to REMOVE (the region is the
///   material, its interior is cut by contract), so no part law applies.
/// - Slot and Circular cut authored geometry into their source - the
///   source is the material to remove, not a part to keep.
/// - Trace, Engrave and 2D Chamfer enter the part by design (the
///   engraving, the broken edge); their own modes stand the gouge law
///   down for exactly that reason.
/// - Drill (Z holes), Bore, Thread: `Contain`.
pub(super) fn depth_law_of(params: &OpParams, src: &SrcRef, part_solid: bool) -> DepthLaw {
    if !part_solid {
        return DepthLaw::None;
    }
    match params {
        OpParams::Contour { .. } | OpParams::Face { .. } => DepthLaw::Section,
        OpParams::Pocket { .. } | OpParams::Adaptive2D { .. } | OpParams::Offset2D { .. }
            if matches!(src, SrcRef::Selection(_)) =>
        {
            DepthLaw::Section
        }
        OpParams::Drill { cross, .. } if cross.is_empty() => DepthLaw::Contain,
        OpParams::Bore { .. } | OpParams::Thread { .. } => DepthLaw::Contain,
        _ => DepthLaw::None,
    }
}

/// Sample spacing of the solid law along a vertical-plane arc, mm (every
/// other move is swept exactly): finer than any drill the catalog
/// carries.
const CONTAIN_STEP: f64 = 0.25;

/// Append the solid law's warnings that the region laws did not
/// already report, keeping the list's move order: the tool's own tip
/// and stack against the part solid at every sampled position, the
/// containment kinds' tessellated hole rims excused (`SolidLaw`) - the
/// round walls of the holes the op declares, measured about each hole's
/// own axis (#1205), never a flat.
#[allow(clippy::too_many_arguments)]
fn contain_into(
    warns: &mut Vec<Warning>,
    path: &Toolpath,
    facets: &[Triangle],
    cutter: &verify::Cutter,
    law: DepthLaw,
    stl: f64,
    excess: f64,
    params: &OpParams,
) {
    let law = verify::SolidLaw {
        step: CONTAIN_STEP,
        // #864: less a coarse model's excess, while that is honest.
        allow: verify::excess_floor(GOUGE_TOL, excess).unwrap_or(GOUGE_TOL) + (-stl).max(0.0),
        hole_walls: law == DepthLaw::Contain,
    };
    let holes = match params {
        OpParams::Drill { holes, .. } | OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => holes.as_slice(),
        _ => &[],
    };
    let walls: Vec<verify::HoleAxis> = holes.iter().map(|h| verify::HoleAxis::vertical(h.xy.x, h.xy.y)).collect();
    for w in verify::solid_hits_walled(path, facets, cutter, &law, &walls) {
        if !warns.contains(&w) {
            warns.push(w);
        }
    }
}

/// The tool above the ball, as stage F's dodge reads it: the same
/// stack `verify`'s law (c) measures the posted path against, built
/// here at the seam where the ToolDef's diameters halve to radii.
/// One builder for all three axis-law kinds, so the generator and the
/// post-time verifier can never disagree about what the shank is.
fn shank_of(tool: &crate::tools::Tool) -> crate::multiaxis::Shank {
    crate::multiaxis::Shank {
        flute_len: tool.flute_len,
        shank_r: tool.shank_d * 0.5,
        shank_len: tool.shank_len,
        holder_r: tool.holder_d * 0.5,
        holder_len: tool.holder_len,
    }
}
