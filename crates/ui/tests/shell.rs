//! State/contract smoke tests through the queue: no GPU, no window.
//! Provenance: our own harness; behavior specced from the shell itself.
//! The imgui-era `headless::run_frames` probe retired at M-g - the egui
//! headless proof is `headless_cli_exit_code_is_zero` below, which runs the
//! real app through `run()`, and tests/egui_app.rs covers the same entry.
//!
//! Gate map (M4.4): covers ui::run, ui::state, app::geometry, app::model, app::schema, app::stock.


#[test]
fn headless_cli_exit_code_is_zero() {
    assert_eq!(camcraft_ui::run(&[String::from("--headless")]), 0);
}

#[test]
fn state_starts_clean() {
    let state = camcraft_ui::state::State::new();
    assert_eq!(state.status, "ready");
    assert!(!state.has_selection());
    assert!(!state.history.can_undo());
}

#[test]
fn selecting_a_missing_model_is_a_no_op_not_a_crash() {
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(CommandKind::Select(Some("ghost".into())));
    state.drain_actions();
    assert!(!state.has_selection());
}

#[test]
fn camera_frame_all_on_empty_world_is_polite() {
    let mut state = camcraft_ui::state::State::new();
    state.pending.push_back(camcraft_ui::state::CommandKind::FrameAll);
    state.drain_actions();
    assert_eq!(state.status, "nothing to frame");
}

#[test]
fn import_completes_on_the_worker_thread() {
    use camcraft_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle(); // pumps the worker to its result
    assert!(state.status.starts_with("imported cube"), "{:?}", state.status);
    assert!(state.history.can_undo());

    state.pending.push_back(CommandKind::Undo);
    state.settle();
    assert!(!state.history.can_undo());
    assert!(state.history.can_redo());
}

#[test]
fn reimporting_the_same_name_updates_the_model_without_duplicating_or_dropping_it() {
    // M3.1 A's UI law: a same-named import lands as UpdateModel on the one
    // entity the name keys - never a silent no-op, never a second model,
    // and undo restores the previous geometry instead of deleting it.
    use camcraft_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path.clone()));
    state.settle();
    assert!(state.status.starts_with("imported cube"), "{:?}", state.status);

    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle();
    // The same file over the same name: the update lands, and the honest
    // summary is that nothing moved.
    assert_eq!(state.status, "updated cube (geometry unchanged)", "{:?}", state.status);
    let entity = camcraft_app::model::find_by_name(&state.world, "cube").expect("one model");
    assert_eq!(camcraft_app::model::rev_of(&state.world, &entity), 2, "revision two");
    assert_eq!(camcraft_app::model::facets_of(&state.world, &entity).map(|f| f.len()), Some(12));

    // Undo trades back, not away: the entity stands with revision 1.
    state.pending.push_back(CommandKind::Undo);
    state.settle();
    let entity = camcraft_app::model::find_by_name(&state.world, "cube").expect("not deleted");
    assert_eq!(camcraft_app::model::rev_of(&state.world, &entity), 1);
    assert_eq!(
        camcraft_app::model::facets_of(&state.world, &entity).map(|f| f.len()),
        Some(12),
        "geometry restored, not gone"
    );

    state.pending.push_back(CommandKind::Redo);
    state.settle();
    let entity = camcraft_app::model::find_by_name(&state.world, "cube").expect("still one");
    assert_eq!(camcraft_app::model::rev_of(&state.world, &entity), 2, "redo counts again");
    // One entity, ever: the tree has no second "cube" to show.
    let named: Vec<String> = camcraft_app::model::model_entities(&state.world)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(named.iter().filter(|n| n.as_str() == "cube").count(), 1, "{named:?}");
}

#[test]
fn corrupt_import_errors_on_the_worker_never_panics() {
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    // Dispatch refuses by extension before reading; no IGES kernel exists.
    state
        .pending
        .push_back(CommandKind::ImportFile(std::path::PathBuf::from("x.iges")));
    state.settle();
    assert!(state.status.starts_with("import refused"), "{:?}", state.status);
    assert!(!state.history.can_undo());
}

fn box_stock(
    shape: camcraft_app::stock::StockShape,
    pos: [f64; 3],
) -> camcraft_ui::state::CommandKind {
    camcraft_ui::state::CommandKind::SetStock {
        shape,
        pos,
        yaw_deg: 0.0,
    }
}

#[test]
fn set_stock_creates_the_entity_and_undo_removes_it() {
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 30.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.drain_actions();
    assert_eq!(state.status, "stock set");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("Stock entity");
    assert!(model::is_stock(&state.world, &stock));
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 30.0
        })
    );
    assert_eq!(model::facets_of(&state.world, &stock).map(|t| t.len()), Some(12));

    // Yaw enters through the same queue path and lands as a unit quat.
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Cylinder {
            diameter: 40.0,
            height: 20.0,
        },
        pos: [1.0, 2.0, -3.0],
        yaw_deg: 30.0,
    });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still there");
    let loc = model::transform_of(&state.world, &stock).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (1.0, 2.0, -3.0));
    let (s, c) = 15.0f64.to_radians().sin_cos();
    assert!(
        (loc.rot[2] - s).abs() < 1e-12 && (loc.rot[3] - c).abs() < 1e-12,
        "{:?}",
        loc.rot
    );

    // Undo the replace: the original box parameters come back exactly.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("box stock restored");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 30.0
        })
    );

    // Undo the creation: the entity is gone.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
    assert!(state.history.can_redo());
}

#[test]
fn invalid_stock_parameters_are_reported_not_applied() {
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::State;

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Box {
            dx: f64::NAN,
            dy: 10.0,
            dz: 10.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.drain_actions();
    assert!(state.status.starts_with("stock not set"), "{:?}", state.status);
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
    assert!(!state.history.can_undo(), "a refused command never enters history");
}

#[test]
fn stock_fit_wraps_models_and_double_fit_does_not_grow() {
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle();
    assert!(state.status.starts_with("imported"), "{:?}", state.status);

    let bounds = model::solid_world_aabb(&state.world).expect("model extent");
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 0,
        });
    state.drain_actions();
    assert_eq!(state.status, "stock fitted to models");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock");
    let Some(StockShape::Box { dx, dy, dz }) = model::stock_def_of(&state.world, &stock) else {
        panic!("fit must produce box stock");
    };
    let eps = 1e-9;
    assert!(dx >= (bounds.max.x - bounds.min.x) + 4.0 - eps, "{dx}");
    assert!(dy >= (bounds.max.y - bounds.min.y) + 4.0 - eps, "{dy}");
    assert!(dz >= (bounds.max.z - bounds.min.z) + 4.0 - eps, "{dz}");

    // The stock is not a model: fitting twice must not inflate it.
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 0,
        });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still stock");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box { dx, dy, dz }),
        "double fit must be idempotent"
    );
}

#[test]
fn cylinder_fit_keeps_the_cylinder_choice() {
    // Feedback 4: fit ignored the selected shape and rebuilt the stock as a
    // box, after which the pane snapped the radio back to Box - the cylinder
    // looked broken in both Apply-then-Fit and plain Fit flows.
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle();
    assert!(state.status.starts_with("imported"), "{:?}", state.status);

    state.stock_ui.shape = 1; // the user picks "Cylinder" in the pane
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 1,
        });
    state.drain_actions();
    assert_eq!(state.status, "stock fitted to models");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock");
    let Some(StockShape::Cylinder { diameter, height }) = model::stock_def_of(&state.world, &stock)
    else {
        panic!("cylinder fit must produce cylinder stock");
    };
    let bounds = model::solid_world_aabb(&state.world).expect("model extent");
    let eps = 1e-9;
    // #230: snug - the models' largest radius from the fit's axis (a cube's
    // half-diagonal) plus the 2 mm margin, which still contains every corner.
    let c = [(bounds.min.x + bounds.max.x) * 0.5, (bounds.min.y + bounds.max.y) * 0.5];
    let r_max = model::solid_world_radius_about(&state.world, c).expect("model radius");
    let half_diag = 0.5 * (bounds.max.x - bounds.min.x).hypot(bounds.max.y - bounds.min.y);
    assert!((r_max - half_diag).abs() < 1e-6, "a cube's farthest corner: {r_max} vs {half_diag}");
    assert!((diameter - 2.0 * (r_max + 2.0)).abs() < eps, "{diameter} is the radius plus the margin, twice");
    assert!(
        height >= (bounds.max.z - bounds.min.z) + 4.0 - eps,
        "{height}"
    );

    // And the draft must agree with the world after the resync, so the
    // radio never silently reverts.
    state.sync_setup_ui();
    assert_eq!(state.stock_ui.shape, 1, "radio stays on Cylinder");
    assert!((state.stock_ui.diameter - diameter).abs() < eps);

    // Double cylinder fit stays idempotent (stock excluded from the bounds).
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 1,
        });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still stock");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Cylinder { diameter, height }),
        "double cylinder fit must be idempotent"
    );

    // Undo unwinds one fit at a time, restoring the exact parameters: the
    // first fit created the stock, so the second undo removes it.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock after one undo");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Cylinder { diameter, height }),
        "undo of an idempotent re-fit leaves the same cylinder"
    );
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(
        model::find_by_name(&state.world, STOCK_NAME).is_none(),
        "undoing the creating fit removes the stock"
    );
}

#[test]
fn from_model_stock_refuses_fit_with_a_clear_status() {
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 2,
        });
    state.drain_actions();
    assert!(
        state.status.contains("follows its source"),
        "{:?}",
        state.status
    );
    assert!(!state.history.can_undo());
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
}

#[test]
fn stock_fit_without_models_is_a_polite_status() {
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 0,
        });
    state.drain_actions();
    assert_eq!(state.status, "nothing to fit: no models in the document");
    assert!(!state.history.can_undo());
}

/// #1538: the Stock page's Fit to Models (box and cylinder) wraps the
/// 50 x 50 x 6 part, not the 5 in vise closed on the stock.
#[test]
fn stock_fit_with_a_vise_wraps_the_part_not_the_vise() {
    use camcraft_app::commands::{ImportModel, InsertVise, SetStock, VisePlacement};
    use camcraft_app::geometry::{Point3, cube};
    use camcraft_app::model;
    use camcraft_app::schema::{Location, STOCK_NAME};
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    let block = ImportModel::from_mesh("block", cube(Point3::new(2.5, 2.5, 4.0), Point3::new(52.5, 52.5, 10.0)));
    state.history.execute(&state.world, block);
    let stock = SetStock::new(&state.world, StockShape::Box { dx: 55.0, dy: 55.0, dz: 11.0 }, Location::default());
    state.history.execute(&state.world, stock.expect("stock"));
    let p = camcraft_app::vise::size("5 in").expect("5 in").half_open();
    let vise = InsertVise::new(&state.world, &p, None, VisePlacement::FitStock).expect("vise");
    state.history.execute(&state.world, vise);

    state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
    state.drain_actions();
    assert_eq!(state.status, "stock fitted to models");
    let b = model::stock_world_aabb(&state.world).expect("stock box");
    let got = [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z];
    let want = [0.5, 0.5, 2.0, 54.5, 54.5, 12.0];
    assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6), "box fit {got:?}, want {want:?}");

    state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 1 });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).and_then(|e| model::stock_def_of(&state.world, &e));
    let Some(StockShape::Cylinder { diameter, height }) = stock else { panic!("a cylinder: {stock:?}") };
    let d = 2.0 * (25.0 * 2f64.sqrt() + 2.0);
    assert!((diameter - d).abs() < 1e-6 && (height - 10.0).abs() < 1e-6, "d {diameter} h {height}, want d {d} h 10");
}

#[test]
fn set_wcs_creates_the_wcs_undo_removes_and_a_bad_rotation_refuses() {
    use camcraft_app::model;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(CommandKind::SetWcs {
        pos: [10.0, 5.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 90.0,
    });
    state.drain_actions();
    assert_eq!(state.status, "WCS (G54) origin set");
    // #342: one WCS - a document without setups keeps the legacy name.
    let g55 = model::find_by_name(&state.world, "G54").expect("the WCS entity");
    assert!(model::is_wcs(&state.world, &g55));
    assert_eq!(model::facets_of(&state.world, &g55), None, "offsets are not solids");
    let loc = model::transform_of(&state.world, &g55).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (10.0, 5.0, 0.0));

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, "G54").is_none());

    // A rotation that is not a number: refused, and nothing was created.
    state.pending.push_back(CommandKind::SetWcs {
        pos: [0.0, 0.0, 0.0],
        roll_deg: f64::INFINITY,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(state.status.starts_with("WCS not set"), "{:?}", state.status);
    assert!(model::find_by_name(&state.world, "G54").is_none());
}

#[test]
fn delete_by_name_round_trips_stock() {
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Cylinder {
            diameter: 30.0,
            height: 10.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.drain_actions();
    state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
    state.drain_actions();
    assert_eq!(state.status, "deleted Stock");
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("restored");
    assert!(model::is_stock(&state.world, &stock), "kind survives delete+undo");
}

#[test]
fn anchor_grid_places_origin_on_the_stock_box() {
    // A grid click in the pane boils down to these two helpers feeding the
    // existing SetWcs command; pin their contract end to end.
    use camcraft_app::model;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Box {
            dx: 60.0,
            dy: 40.0,
            dz: 20.0,
        },
        [10.0, -5.0, 0.0],
    ));
    state.drain_actions();
    let b = model::stock_world_aabb(&state.world).expect("stock box");
    assert_eq!((b.min.x, b.min.y, b.min.z), (-20.0, -25.0, 0.0));
    assert_eq!((b.max.x, b.max.y, b.max.z), (40.0, 15.0, 20.0));

    // "NE / Top": max X, max Y, max Z of the stock box.
    let p = model::anchor_point(&b, [2, 2, 2]);
    state.pending.push_back(CommandKind::SetWcs {
        pos: p,
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let g54 = model::find_by_name(&state.world, "G54").expect("origin");
    let loc = model::transform_of(&state.world, &g54).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (40.0, 15.0, 20.0));

    // Undo removes the origin; undoing once more removes the stock, and
    // then there is no anchor box at all.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, "G54").is_none());
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::stock_world_aabb(&state.world).is_none());
}

#[test]
fn origin_undo_leaves_no_stale_readout() {
    use camcraft_app::model;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(CommandKind::SetWcs {
        pos: [12.0, 3.0, -1.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    state.sync_setup_ui();
    assert_eq!(state.wcs_ui.x, 12.0);
    assert_eq!(state.wcs_ui.y, 3.0);
    assert_eq!(state.wcs_ui.z, -1.0);

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    state.sync_setup_ui();
    assert!(model::find_by_name(&state.world, "G54").is_none());
    assert_eq!(
        (state.wcs_ui.x, state.wcs_ui.y, state.wcs_ui.z),
        (0.0, 0.0, 0.0),
        "the readout follows reality after an undo"
    );
}

#[test]
fn wcs_orientation_tilts_the_offset_and_undo_restores_it() {
    use camcraft_app::geometry::{Point3, transform_point};
    use camcraft_app::model;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    // A spindle that points along world -Y is a +90 roll about X.
    state.pending.push_back(CommandKind::SetWcs {
        pos: [10.0, 0.0, 0.0],
        roll_deg: 90.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert_eq!(state.status, "WCS (G54) origin set");
    let g54 = model::find_by_name(&state.world, "G54").expect("origin");
    let loc = model::transform_of(&state.world, &g54).expect("location");
    let tip = transform_point(Point3::new(0.0, 0.0, 1.0), loc.pos, loc.rot);
    assert!(
        (tip.x - 10.0).abs() < 1e-9 && (tip.y + 1.0).abs() < 1e-9 && tip.z.abs() < 1e-9,
        "local +Z must point -Y: {tip:?}"
    );

    // The pane's draft decodes back to the same angles (round trip).
    state.sync_setup_ui();
    assert!((state.wcs_ui.roll_deg - 90.0).abs() < 1e-9, "{:?}", state.wcs_ui.roll_deg);
    assert!(state.wcs_ui.pitch_deg.abs() < 1e-9 && state.wcs_ui.yaw_deg.abs() < 1e-9);

    // A second tilt replaces the first; undo returns exactly to it.
    state.pending.push_back(CommandKind::SetWcs {
        pos: [10.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 90.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let loc = model::transform_of(&state.world, &g54).expect("first tilt back");
    let tip = transform_point(Point3::new(0.0, 0.0, 1.0), loc.pos, loc.rot);
    assert!((tip.y + 1.0).abs() < 1e-9 && tip.z.abs() < 1e-9, "{tip:?}");

    // A NaN angle is refused visibly and creates nothing (Prime Directive).
    state.pending.push_back(CommandKind::SetWcs {
        pos: [0.0, 0.0, 0.0],
        roll_deg: f64::NAN,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(state.status.starts_with("WCS not set"), "{:?}", state.status);
    assert_eq!(model::transform_of(&state.world, &g54), Some(loc), "the WCS did not move");
}

#[test]
fn setup_furniture_stays_out_of_the_model_list() {
    use camcraft_app::model;
    use camcraft_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        camcraft_app::stock::StockShape::Box {
            dx: 10.0,
            dy: 10.0,
            dz: 10.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.pending.push_back(CommandKind::SetWcs {
        pos: [0.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    // The exact filter tree.rs renders through.
    let listed: Vec<String> = model::model_entities(&state.world)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(listed.is_empty(), "{listed:?} must not list Stock or G54");
    assert_eq!(model::named_entities(&state.world).len(), 2, "both still in the world");
}

/// #230: a round part's fitted bar is R + the allowance - its revolved max
/// radius about the fit's axis - not the footprint's diagonal (1.41 R).
#[test]
fn a_cylinder_fit_round_a_turned_part_is_snug() {
    use camcraft_app::model;
    use camcraft_app::schema::STOCK_NAME;
    use camcraft_app::stock::StockShape;
    use camcraft_ui::state::{CommandKind, State};
    let mut state = State::new();
    let bar = camcraft_app::geometry::cylinder(20.0, 40.0, 64);
    state.history.execute(&state.world, camcraft_app::commands::ImportModel::from_mesh("bar", bar));
    state.doc_version += 1;
    state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 1 });
    state.drain_actions();
    assert_eq!(state.status, "stock fitted to models");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock");
    let Some(StockShape::Cylinder { diameter, height }) = model::stock_def_of(&state.world, &stock) else {
        panic!("cylinder fit must produce cylinder stock");
    };
    assert!((diameter - 24.0).abs() < 1e-9, "R 10 + 2 allowance = 24 mm, got {diameter}");
    assert!((height - 44.0).abs() < 1e-9, "{height}");
}
