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
    let entities: Vec<EntityDto> = model::named_entities(world)
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
            } else if crate::sketch::is_sketch(world, &entity) {
                Some(EntityKind::Sketch)
            } else {
                None
            };
            let stock = model::stock_def_of(world, &entity).map(|s| StockDto::from_shape(&s));
            // #2016: a library holder is written as its reference.
            let (holder, body) = crate::tool_body::body_of(world, &entity)
                .map_or((None, None), |b| super::tool_body_wire::body_to_dto(&b));
            let tool = model::tool_of(world, &entity).map(|t| ToolDto {
                number: model::tool_number_of(world, &entity),
                body,
                holder,
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
                        work_offset: d.work_offset.map(crate::project::work_offset_text),
                        part_alignment: d.part_alignment.as_ref().map(AlignmentDto::from_location),
                        machine: d.machine.as_ref().map(SetupMachineDto::from_def),
                        post: d.post.as_ref().map(SetupPostDto::from_ref),
                        relative_to: d.relative_to.clone(),
                        stock_source: d.stock_source.map(|s| s.word().to_string()),
                        post_options: d.post_options.as_ref().map(SetupPostOptionsDto::from_options),
                        post_properties: d.post_properties.as_ref().map(|p| p.by_post.clone()),
                        nose_to_axis: d.nose_to_axis.filter(|v| v.is_finite()),
                        workholding_max_rpm: d.workholding_max_rpm,
                        sheet_job: d.sheet_job,
                        order: d.order,
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
                    profile: d.profile.as_ref().and_then(super::fixture_keys::profile_value),
                });
            let op_meta = model::operation_meta_of(world, &entity);
            let operation = model::operation_of(world, &entity).map(|op| OperationDto {
                label: op_meta.label,
                order: op_meta.order,
                needs_review: op_meta.needs_review,
                tool: op.tool,
                src: match &op.src {
                    SrcRef::Stock => SrcDto::Stock,
                    SrcRef::Entity(name) => SrcDto::Entity { name: name.clone() },
                    SrcRef::Selection(items) => SrcDto::Selection {
                        items: items.iter().map(geo_sel_dto).collect(),
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
                | Some(EntityKind::Sketch)
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
                coarse_mm: crate::model_fidelity::chord_of(world, &entity)
                    .filter(|c| !crate::model_fidelity::is_unknown(*c)),
                fidelity_unknown: crate::model_fidelity::chord_of(world, &entity)
                    .is_some_and(crate::model_fidelity::is_unknown),
                file_tolerance: crate::model_fidelity::file_tolerance_of(world, &entity),
                mesh_open: crate::mesh_open::hole_of(world, &entity),
                solids: kind.is_none().then(|| crate::mesh_solids::to_wire(&crate::mesh_solids::of(world, &entity))).flatten(),
                fidelity_vouched: kind.is_none().then(|| super::VouchDto::of(world, &entity)).flatten(),
                facet_budget: crate::model_fidelity::budget_of(world, &entity),
                step_source: kind.is_none().then(|| crate::step_source::of(world, &entity)).flatten(),
                source: kind
                    .is_none()
                    .then(|| crate::model_source::of(world, &entity))
                    .flatten()
                    .map(|s| super::SourceDto::of(&s)),
                kind,
                raw_stock_only: mark.is_some(),
                raw_stock_print: mark.as_ref().map(crate::stock_print::to_text),
                stock_material: model::is_stock(world, &entity)
                    .then(|| crate::stock_material::of_entity(world, &entity))
                    .flatten()
                    .map(super::StockMaterialWord),
                stock,
                tool,
                operation,
                setup,
                fixture,
                entry: super::program_entry_dto::collect(world, &entity),
                sketch: super::sketch_dto::collect(world, &entity),
                flip: matches!(kind, Some(EntityKind::Wcs))
                    .then(|| crate::setup_wcs_flip::of_entity(world, &entity))
                    .filter(|f| f.iter().any(|v| *v))
                    .map(|[x, y, z]| super::entity::WcsFlipDto { x, y, z }),
            }
        })
        .collect();
    ProjectFile {
        // #3368: a flat or ball taper tip steps the version (file.rs).
        version: super::written_project_version(
            entities.iter().filter_map(|e| e.tool.as_ref()).any(|t| !t.tip_form.is_sharp()),
        ),
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
            let script = crate::setup_post_options::script_document(world).by_post;
            (o.threading != crate::schema::ThreadingOutput::default()
                || o.end_units != crate::schema::EndUnits::default()
                || o.between_setups != crate::schema::BetweenSetups::default()
                || o.wcs_rotation != crate::schema::WcsRotationOutput::default()
                || !script.is_empty())
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
                // #1532: the default pause writes no key.
                between_setups: (o.between_setups != crate::schema::BetweenSetups::default())
                    .then(|| o.between_setups.word().to_owned()),
                // #760: the default bake writes no key.
                wcs_rotation: (o.wcs_rotation != crate::schema::WcsRotationOutput::default())
                    .then(|| o.wcs_rotation.word().to_owned()),
                script,
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
        // Nothing stated writes no key (#258).
        program: {
            let p = crate::posting::program_info(world);
            (p != crate::schema::ProgramInfo::default())
                .then_some(ProgramDto { number: p.number, comment: p.comment })
        },
        // Off writes no key (#616).
        smoothing: crate::setup_post_options::document(world).smoothing,
        // Off writes no key (#1386).
        explain: crate::setup_post_options::document(world).explain,
        entities,
    }
}

/// The project file's text, exactly what [`save`] writes (#1431): the
/// browser build hands it to a download instead of a path.
pub fn save_text(world: &World) -> Result<String, ProjectError> {
    serde_json::to_string_pretty(&collect(world)).map_err(|e| {
        tracing::warn!("project not saved: {e}");
        corrupt(e.to_string())
    })
}

/// Write the project to `path` through a temp file in the same folder and
/// a rename, so a save that fails keeps the old file whole and no other
/// program reads half a project. A missing folder and a project file that
/// this process cannot write are refused, as the plain write that this
/// replaces refused them.
pub fn save(world: &World, path: &Path) -> Result<(), ProjectError> {
    let text = save_text(world)?;
    if let Err(e) = write_project(path, &text) {
        tracing::warn!(file = %crate::logging::file_name(path), "project not saved: {e}");
        return Err(e.into());
    }
    tracing::info!(file = %crate::logging::file_name(path), bytes = text.len(), "project saved");
    Ok(())
}

/// The checks and the atomic write of [`save`]. The shared writer creates
/// a missing folder and, on Linux and macOS, its rename replaces any file
/// that the folder allows, a read-only one included: a project save does
/// neither, so both are checked first.
fn write_project(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let folder = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if !folder.is_dir() {
        return Err(Error::new(ErrorKind::NotFound, format!("folder {} does not exist", folder.display())));
    }
    if std::fs::metadata(path).is_ok_and(|m| m.permissions().readonly()) {
        return Err(Error::new(ErrorKind::PermissionDenied, format!("{} is read-only", path.display())));
    }
    // The mode alone does not say that this process may write the file.
    // On Unix a file that another user owns can have mode 0644 and still
    // refuse a write with EACCES; `rename(2)` would replace it, because
    // the rename needs write permission on the folder only. Open the file
    // for writing, as the plain write that this save replaces did, with
    // no truncate and no create. A path that does not exist is made by
    // the temp file and the rename.
    let unwritable = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .is_err_and(|e| e.kind() != ErrorKind::NotFound);
    if unwritable {
        return Err(Error::new(ErrorKind::PermissionDenied, format!("{} is read-only", path.display())));
    }
    crate::machine::store::write_atomic(path, text).map_err(|e| match e {
        crate::machine::store::SaveError::Io(why) => Error::other(why),
        other => Error::other(other.to_string()),
    })
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
