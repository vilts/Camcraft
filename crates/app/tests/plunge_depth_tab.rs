//! Issue #3679: the verifier names a feed plunge that reaches deeper
//! below the floor the levels above left under the full tool disc than
//! one stepdown. #3678 made the 2D Contour peck its tab window dives
//! below a ring's first level, so the catalog case `2d/contour-tabs`
//! (the reporter's tabbed contour of #3453) now pecks each later tab
//! window of its last level one stepdown at a time and carries no
//! finding (#3925). The law keeps its end-to-end pin on the same case
//! with its top lowered 4 mm under the stock top (carried stock): the
//! dives that land on Z 2 bite 4 mm against the 2 mm stepdown and are
//! named, advisory by the #3525 pattern - nothing gouges and nothing
//! collides, the risk is the tool load, so the post is not held.
//!
//! Gate map (#3679, #3925): covers app::operation (generate, cutting_rows),
//! app::model, app::project, app::commands (EditOperationParams), sim::verify,
//! motion::machining2d (contour_peck).
//! Provenance: clean-room test over the catalog case of #3453.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{EditOperationParams, GenerateOperation, History};
use camcraft_app::operation::{OpParams, OpStatus};
use camcraft_app::verify::Warning;
use camcraft_app::{model, operation, project, setup, world};
use camcraft_app::schema::Toolpath as CachedToolpath;

/// The case's stock top and the operation's stepdown.
const STOCK_TOP: f64 = 6.0;
const STEP: f64 = 2.0;

fn repo(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

/// Load the catalog case, let `edit` change the contour's params (if any),
/// regenerate and return its cached path.
fn tabbed_contour(edit: impl FnOnce(&mut OpParams)) -> CachedToolpath {
    let w = world::new_world();
    let proj = repo("examples/catalog/2d/contour-tabs/contour-tabs.camproj");
    let notes = project::load_noted(&w, &proj).unwrap_or_else(|e| panic!("load {}: {e}", proj.display()));
    assert!(notes.is_empty(), "load notes: {notes:?}");
    let ops = setup::operation_order(&w);
    assert_eq!(ops.len(), 1, "the case holds one contour: {ops:?}");
    let (op_name, entity) = ops.into_iter().next().expect("one op");
    let mut history = History::default();
    let mut params = model::operation_of(&w, &entity).expect("the op reads back").params;
    let before = params.clone();
    edit(&mut params);
    if params != before {
        let cmd = EditOperationParams::new(&w, &op_name, params).unwrap_or_else(|e| panic!("edit {op_name}: {e}"));
        history.execute(&w, cmd);
    }
    let cmd = GenerateOperation::new(&w, &op_name).unwrap_or_else(|e| panic!("generate {op_name}: {e}"));
    history.execute(&w, cmd);
    assert_eq!(operation::status_of(&w, &entity), OpStatus::Ok, "status after regenerate");
    model::toolpath_of(&w, &entity).expect("cached path")
}

fn plunge_findings(tp: &CachedToolpath) -> Vec<(usize, f64, f64)> {
    tp.warnings
        .iter()
        .filter_map(|warn| match warn {
            Warning::PlungeTooDeep { at, mm, limit_mm } => Some((*at, *mm, *limit_mm)),
            _ => None,
        })
        .collect()
}

/// The depths each XY column of feed plunges that reaches the last level
/// (Z 0) stops at, in path order, one entry per column.
fn last_level_columns(path: &Toolpath) -> Vec<((f64, f64), Vec<f64>)> {
    let mut cols: Vec<((f64, f64), Vec<f64>)> = Vec::new();
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Plunge && m.to.z < m.from.z) {
        let xy = (m.to.x, m.to.y);
        match cols.iter_mut().find(|(at, _)| *at == xy) {
            Some((_, depths)) => depths.push(m.to.z),
            None => cols.push((xy, vec![m.to.z])),
        }
    }
    cols.retain(|(_, depths)| depths.iter().any(|z| z.abs() < 1e-9));
    cols
}

#[test]
fn the_tabbed_contour_catalog_case_pecks_its_tab_window_dives() {
    let tp = tabbed_contour(|_| {});

    // #3678 pecks each tab window dive, so the law names none.
    let dives = plunge_findings(&tp);
    assert!(dives.is_empty(), "the pecked tab window dives carry no finding: {:?}", tp.warnings);

    // Window 0 at the ring start dives once per level (Z 4, 2, 0); the
    // three later windows of the last level each peck 4, 2, 0 in a row.
    let cols = last_level_columns(&tp.path);
    assert_eq!(cols.len(), 4, "four tab window dive columns reach the last level: {cols:?}");
    for (xy, depths) in &cols {
        assert_eq!(depths.len(), 3, "the column at {xy:?} stops at three depths: {depths:?}");
        let mut floor = STOCK_TOP;
        for z in depths {
            assert!(
                floor - z <= STEP + 1e-9,
                "the column at {xy:?} bites {} mm below Z {floor} (stepdown {STEP}): {depths:?}",
                floor - z
            );
            floor = *z;
        }
        assert!(floor.abs() < 1e-9, "the column at {xy:?} ends on the last level: {depths:?}");
    }
}

#[test]
fn a_tabbed_contour_under_carried_stock_names_its_deep_dives() {
    // Top 4 mm under the stock top, levels Z 2 and Z 0. The ring start
    // dives to its first level (Z 2), which never pecks (#3678). Each
    // later window of the last level pecks from the floor one stepdown
    // over the first level (Z 4), so its first peck also lands on Z 2.
    // The stock top is Z 6: each of these four dives bites 4 mm of stock
    // against the 2 mm stepdown.
    let tp = tabbed_contour(|p| match p {
        OpParams::Contour { top_z, .. } => top_z.offset = -4.0,
        other => panic!("the case holds a contour, not {other:?}"),
    });
    let dives = plunge_findings(&tp);
    assert_eq!(dives.len(), 4, "the ring start and the three later windows are named: {:?}", tp.warnings);
    for (at, mm, limit_mm) in &dives {
        let m = tp.path.moves.get(*at)
            .unwrap_or_else(|| panic!("finding at move {at} names no move of a {}-move path", tp.path.moves.len()));
        assert_eq!(m.kind, MoveKind::Plunge, "the finding at move {at} names a feed plunge");
        assert!((m.to.z - 2.0).abs() < 1e-9, "the dive at move {at} lands on Z 2, not Z {}", m.to.z);
        assert!((mm - 4.0).abs() < 1e-9, "the dive at move {at} reads 4 mm below the stock top, not {mm}");
        assert!((limit_mm - STEP).abs() < 1e-12, "the op's stepdown is 2 mm, not {limit_mm}");
        assert!(
            !Warning::PlungeTooDeep { at: *at, mm: *mm, limit_mm: *limit_mm }.blocks_post(),
            "a tool-load note does not hold the post"
        );
    }
    let mut ats: Vec<usize> = dives.iter().map(|d| d.0).collect();
    ats.dedup();
    assert_eq!(ats.len(), 4, "each dive is named once: {dives:?}");
}
