use super::*;

pub(super) fn identity_rotation() -> [f64; 4] {
    IDENTITY_Q
}

pub(super) fn is_unrevised(rev: &u64) -> bool {
    *rev == 0
}

pub(super) fn corrupt(msg: impl Into<String>) -> ProjectError {
    ProjectError::Corrupt(msg.into())
}

pub(super) fn facets_to_dto(tris: &[Triangle]) -> Vec<[[f64; 3]; 3]> {
    tris.iter()
        .map(|t| {
            [
                [t.a.x, t.a.y, t.a.z],
                [t.b.x, t.b.y, t.b.z],
                [t.c.x, t.c.y, t.c.z],
            ]
        })
        .collect()
}

pub(super) fn dto_to_facets(v: Vec<[[f64; 3]; 3]>) -> Vec<Triangle> {
    v.into_iter()
        .map(|t| {
            Triangle::new(
                Point3::new(t[0][0], t[0][1], t[0][2]),
                Point3::new(t[1][0], t[1][1], t[1][2]),
                Point3::new(t[2][0], t[2][1], t[2][2]),
            )
        })
        .collect()
}

pub fn collect(world: &World) -> ProjectFile {
    let entities = model::named_entities(world)
        .into_iter()
        .map(|(name, entity)| {
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            let kind = if model::is_stock(world, &entity) {
                Some(EntityKind::Stock)
            } else if model::is_wcs(world, &entity) {
                Some(EntityKind::Wcs)
            } else if model::is_tool(world, &entity) {
                Some(EntityKind::Tool)
            } else if model::is_operation(world, &entity) {
                Some(EntityKind::Operation)
            } else if model::is_setup(world, &entity) {
                Some(EntityKind::Setup)
            } else if crate::program_entry::is_entry(world, &entity) {
                Some(EntityKind::Entry)
            } else {
                None
            };
            let stock = model::stock_def_of(world, &entity).map(|s| StockDto::from_shape(&s));
            let tool = model::tool_of(world, &entity).map(|t| ToolDto {
                number: model::tool_number_of(world, &entity),
                body: crate::tool_body::body_of(world, &entity)
                    .map(|b| super::tool_body_wire::ToolBodyDto::from_body(&b)),
                cutter: crate::tool_cutter::cutter_of(world, &entity)
                    .map(|c| super::tool_body_wire::spun_to_dto(&c)),
                ..ToolDto::from_tool(&t)
            });
            // The setup payload only ever rides a setup row (the kind test
            // above and the component below are the same SetupDef).
            let setup = if matches!(kind, Some(EntityKind::Setup)) {
                entity
                    .entity_view(world)
                    .try_get::<&SetupDef>(|d| SetupDto {
                        stock: d.stock.clone(),
                        wcs: WcsDto::One(d.wcs.clone()),
                        datum_from: d.datum_from.clone(),
                        work_offset: d
                            .work_offset
                            .and_then(|i| crate::schema::WCS_SLOTS.get(i))
                            .map(|w| (*w).to_string()),
                        part_alignment: d.part_alignment.as_ref().map(AlignmentDto::from_location),
                        machine: d.machine.as_ref().map(SetupMachineDto::from_def),
                        post: d.post.as_ref().map(SetupPostDto::from_ref),
                        relative_to: d.relative_to.clone(),
                        stock_source: d.stock_source.map(|s| s.word().to_string()),
                        post_options: d.post_options.as_ref().map(SetupPostOptionsDto::from_options),
                        workholding_max_rpm: d.workholding_max_rpm,
                    })
            } else {
                None
            };
            // The fixture marker rides whatever model it tags; the kind
            // test above deliberately does NOT look at it (a fixture is
            // a model, not a kind).
            let fixture = entity
                .entity_view(world)
                .try_get::<&FixtureDef>(|d| FixtureDto {
                    setup: d.setup.clone(),
                    fixed: d.fixed,
                    vise: d.vise.as_ref().and_then(super::fixture_keys::vise_value),
                    accessory: d.accessory.as_ref().and_then(super::fixture_keys::accessory_value),
                    plate: super::plate_dto::plate_value(&d.plate),
                    on_plate: super::plate_dto::on_plate_value(&d.plate),
                    mod_jaw: super::modvise_dto::value(d.mod_jaw.as_ref()),
                });
            let operation = model::operation_of(world, &entity).map(|op| OperationDto {
                label: model::operation_meta_of(world, &entity).label,
                order: model::operation_meta_of(world, &entity).order,
                tool: op.tool,
                src: match &op.src {
                    SrcRef::Stock => SrcDto::Stock,
                    SrcRef::Entity(name) => SrcDto::Entity { name: name.clone() },
                    SrcRef::Selection(items) => SrcDto::Selection {
                        items: items
                            .iter()
                            .map(|g| GeoSelDto {
                                entity: g.entity.clone(),
                                feature: match g.feature {
                                    FeatureSel::Face(id) => FeatureSelDto::Face { id },
                                    FeatureSel::Chain(id) => FeatureSelDto::Chain { id },
                                    FeatureSel::Patch(id) => FeatureSelDto::Patch { id },
                                    FeatureSel::Vertex(id) => FeatureSelDto::Vertex { id },
                                },
                            })
                            .collect(),
                    },
                },
                params: ParamsDto::from_params(&op.params),
                stamp: op.stamp.map(|g| g.0),
                cache: model::toolpath_of(world, &entity).map(|c| PathDto::from_cache(&c)),
                area: AreaDto::from_controls(&op.area),
                coolant: op.coolant,
                after: op.after,
                suppressed: op.suppressed,
                setup: op.setup,
            });
            // Parametric stock regenerates its facets on load; offsets, tools,
            // operations and setups never have geometry, so none writes a
            // facet array.
            let facets = match &kind {
                Some(EntityKind::Wcs)
                | Some(EntityKind::Tool)
                | Some(EntityKind::Operation)
                | Some(EntityKind::Setup)
                | Some(EntityKind::Entry) => None,
                Some(EntityKind::Stock)
                    if matches!(
                        stock,
                        Some(StockDto::Box { .. }) | Some(StockDto::Cylinder { .. })
                    ) =>
                {
                    None
                }
                _ => model::facets_of(world, &entity).map(|t| facets_to_dto(&t)),
            };
            // #1094: the mark, as set, with its print (the verdict checks
            // the print against the loaded model, so a stale one stays void).
            let mark = entity
                .entity_view(world)
                .try_get::<&crate::schema::StockSourceOnly>(|f| f.clone())
                .filter(|f| matches!(&stock, Some(StockDto::FromModel { source }) if *source == f.source))
                .map(|f| f.print);
            EntityDto {
                name,
                translation: [loc.pos.x, loc.pos.y, loc.pos.z],
                rotation: loc.rot,
                facets,
                // The counter moves on models and stocks (every facet
                // landing); everywhere else it reads 0 and omits itself.
                rev: model::rev_of(world, &entity),
                coarse_mm: crate::model_fidelity::chord_of(world, &entity),
                facet_budget: crate::model_fidelity::budget_of(world, &entity),
                kind,
                raw_stock_only: mark.is_some(),
                raw_stock_print: mark.as_ref().map(crate::stock_print::to_text),
                stock,
                tool,
                operation,
                setup,
                fixture,
                entry: super::program_entry_dto::collect(world, &entity),
            }
        })
        .collect();
    ProjectFile {
        version: PROJECT_VERSION,
        // mm omits the field (the default); only an inch document pays
        // for saying so.
        units: match units::of(world) {
            Unit::Millimeters => None,
            Unit::Inches => Some("in".into()),
        },
        // The mute singleton (no path, no machine) omits the field -
        // the mm/units law again: only a loaded machine pays for
        // saying so, and an absent key is the mute state verbatim.
        machine: {
            let (path, m) = crate::machine::document_of(world);
            m.map(|machine| MachineDto { path, machine })
        },
        // The raw singleton, not the resolved one: a document where the
        // user never chose a setup writes no key, exactly like the mm
        // units and mute machine laws - and a name left dangling by a
        // deleted setup saves as it reads (the fallback is the law, not
        // a defect to hide).
        active_setup: crate::setup::stored_active(world),
        // The default post matches every pre-v23 document, so it
        // writes no key (the mute-machine law, worn in the post's clothes).
        // The default options write no key (issue #34, the post's law);
        // the #110 / #111 switches ride their own top-level keys below.
        post_options: {
            let o = crate::setup_post_options::document(world);
            (o.threading != crate::schema::ThreadingOutput::default()
                || o.end_units != crate::schema::EndUnits::default())
            .then(|| PostOptionsDto {
                threading: match o.threading {
                    crate::schema::ThreadingOutput::G32Passes => None,
                    crate::schema::ThreadingOutput::G76Cycle => Some("g76".into()),
                },
                // #596: left modal (the default) writes no key.
                end_units: match o.end_units {
                    crate::schema::EndUnits::Leave => None,
                    crate::schema::EndUnits::Inch => Some("inch".into()),
                    crate::schema::EndUnits::Millimeters => Some("mm".into()),
                },
            })
        },
        post: match crate::posting::document_post_of(world) {
            crate::schema::PostRef::Builtin => None,
            crate::schema::PostRef::Named(name) => Some(PostDto::Named(name)),
            crate::schema::PostRef::File(path) => Some(PostDto::File(path)),
        },
        // An unpinned ribbon writes no key (the theme's law, worn in
        // the pin's clothes - and the default is what every pre-M6.2
        // document is).
        pinned: crate::ribbon::pinned_of(world),
        // No stated material writes no key (the theme's law again).
        material: crate::cutting::material_of(world).map(|m| m.word().to_owned()),
        // Off writes no key (the material's law, #110).
        skip_stops: crate::setup_post_options::document(world).skip_stops,
        // One combined program writes no key (#111).
        per_op_files: crate::setup_post_options::document(world).per_op,
        // Off writes no key (#616).
        smoothing: crate::setup_post_options::document(world).smoothing,
        entities,
    }
}

pub fn save(world: &World, path: &Path) -> Result<(), ProjectError> {
    let file = collect(world);
    let text = serde_json::to_string_pretty(&file).map_err(|e| {
        tracing::warn!(file = %crate::logging::file_name(path), "project not saved: {e}");
        corrupt(e.to_string())
    })?;
    if let Err(e) = std::fs::write(path, &text) {
        tracing::warn!(file = %crate::logging::file_name(path), "project not saved: {e}");
        return Err(e.into());
    }
    tracing::info!(file = %crate::logging::file_name(path), bytes = text.len(), entities = file.entities.len(), "project saved");
    Ok(())
}

/// Rebuild world content from a project file. Not a command: opening a
/// project replaces the document and clears the history (M1.2 wires that).
pub fn load(world: &World, path: &Path) -> Result<(), ProjectError> {
    load_noted(world, path).map(|_| ())
}

/// [`load`], returning the load notes ([`open_noted`]).
pub fn load_noted(world: &World, path: &Path) -> Result<Vec<String>, ProjectError> {
    let text = std::fs::read_to_string(path).inspect_err(|e| {
        tracing::warn!(file = %crate::logging::file_name(path), "project not opened: {e}");
    })?;
    match open_noted(world, &text) {
        Ok(notes) => {
            tracing::info!(file = %crate::logging::file_name(path), bytes = text.len(), "project opened");
            Ok(notes)
        }
        Err(e) => {
            tracing::warn!(file = %crate::logging::file_name(path), "project not opened: {e}");
            Err(e)
        }
    }
}

pub fn open(world: &World, text: &str) -> Result<(), ProjectError> {
    open_noted(world, text).map(|_| ())
}

/// [`open`], returning the load notes: what a migration changed that the
/// user should hear about (#342: dropped work offsets no setup posted).
/// Each note is logged as well.
pub fn open_noted(world: &World, text: &str) -> Result<Vec<String>, ProjectError> {
    let file: ProjectFile = serde_json::from_str(text).map_err(|e| corrupt(e.to_string()))?;
    // #321: the carried-body cache is freed with the document it served.
    crate::rest::clear_carried_cache();
    let notes = check_and_apply(world, file)?;
    for n in &notes {
        tracing::warn!("project load note: {n}");
    }
    Ok(notes)
}
