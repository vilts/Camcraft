//! Issue #2121: a clearance plane set inside the stock - the word that names it on raw stock when the operation is made, generate's re-read of a convicted traverse AT the plane against the program ahead (`verify::acquit_traverses_by_prior`), and the note that names the cause; our own reading of the shop rule "never traverse through uncut stock", no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::cl::{MoveKind, Toolpath};
use crate::geometry::Triangle;
use crate::model;
use crate::stock_sim::Columns;
use crate::tools::ToolGeom;
use crate::verify::{self, Mode, Traverse, Warning};

use super::drill_prior::PriorCell;
use super::kinds::Section;
use super::material::Z_TOL;
use super::{OpParams, SrcRef};

/// Leads the path note that names the cause. The post is held by the
/// move's own [`Warning::RapidInStock`], never by the note.
pub const CLEARANCE_STOCK_WARNING: &str = "Clearance in stock:";

/// Whether move `at` is a lateral rapid riding at or above `clearance`.
fn traverse_at(path: &Toolpath, at: usize, clearance: f64) -> Option<f64> {
    let m = path.moves.get(at)?;
    let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
    let low = m.from.z.min(m.to.z);
    (m.kind == MoveKind::Rapid && dx * dx + dy * dy > Z_TOL * Z_TOL && low >= clearance - Z_TOL).then_some(low)
}

/// The convicted traverses at or above the clearance plane, with the
/// height each rides at.
fn convicted(path: &Toolpath, clearance: f64, warns: &[Warning]) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = Vec::new();
    for w in warns {
        if let Warning::RapidInStock { at } = *w
            && let Some(low) = traverse_at(path, at, clearance)
            && !out.iter().any(|(i, _)| *i == at)
        {
            out.push((at, low));
        }
    }
    out
}

/// What a convicted traverse at the clearance plane is re-read against.
pub(super) struct Reread<'a> {
    /// The stock the column law judged (`material_source::judged_stock`),
    /// in the path's frame; `None` when the op's own material is all the
    /// material there is (a stock-sourced op, a document with no stock).
    pub stock: Option<&'a [Triangle]>,
    /// The op's own material, in the path's frame.
    pub part: &'a [Triangle],
    /// Whether the stock's mesh fidelity is known (#1056): an unknown
    /// chord is never proof of air.
    pub fidelity_known: bool,
    pub tool: &'a ToolGeom,
    pub clearance: f64,
    /// The op's model excess (#864/#904).
    pub excess: f64,
}

/// Re-read `warns`' convictions of traverses at the clearance plane (the
/// sim's modules have the laws): first against the solids under the tool
/// (the column law caps every column at the stock's highest point, a
/// 3+2 plane's at the tilted block's highest corner), then, with `prior`,
/// against what the program ahead left standing. The simulation is the
/// costly half: it runs only while such a traverse stays convicted. An
/// unknown prior is the raw stock.
pub(super) fn acquit(path: &Toolpath, mode: Mode, cx: &Reread<'_>, prior: Option<&mut PriorCell>, warns: &mut Vec<Warning>) {
    if convicted(path, cx.clearance, warns).is_empty() {
        return;
    }
    if cx.fidelity_known {
        let solids: Vec<&[Triangle]> = cx.stock.into_iter().chain([cx.part]).collect();
        verify::acquit_traverses_by_solids(path, mode, &solids, cx.tool, (cx.clearance, Z_TOL, cx.excess), warns);
    }
    if convicted(path, cx.clearance, warns).is_empty() {
        return;
    }
    let Some(dex) = prior.and_then(PriorCell::get) else { return };
    let tr = Traverse { part: cx.part, tool_r: cx.tool.radius, clearance_z: cx.clearance, z_tol: Z_TOL, excess: cx.excess };
    verify::acquit_traverses_by_prior(path, mode, &Columns::of_grid(dex), &tr, warns);
}

/// Name the cause on the path when a traverse at the clearance plane
/// stays convicted: the plane stands inside the stock. `tilted` says
/// the heights are the 3+2 plane's own (along the tool), not world Z.
pub(super) fn note(path: &Toolpath, clearance: f64, stock_top: f64, tilted: bool, warns: &[Warning]) -> Option<String> {
    let hits = convicted(path, clearance, warns);
    let &(at, low) = hits.first()?;
    let more = match hits.len() - 1 {
        0 => String::new(),
        n => format!(" (and {n} more rapid{})", if n == 1 { "" } else { "s" }),
    };
    let frame = if tilted { " (heights along the tilted tool axis)" } else { "" };
    Some(format!(
        "{CLEARANCE_STOCK_WARNING} rapid move {at} travels through stock at Z{low:.3}{more} - the clearance plane \
         Z{clearance:.3} is inside the stock (top Z{stock_top:.3}) and nothing ahead has cut it there{frame}; \
         raise Clearance above the stock top or face first"
    ))
}

/// Whether `kind`'s clearance is a world-Z plane its rapids traverse
/// at over an upright stock: the planar and 3-D milling ladders and the
/// vertical hole kinds. A tilted plane's heights run along the tool, a
/// cross drill's and a lathe's along other axes, and the multi-axis and
/// turning kinds read theirs into stations of their own - the rapid laws
/// judge those at generation, the door does not.
fn upright_plane(world: &World, setup: Option<&str>, params: &OpParams) -> bool {
    if params.tilt().is_some() {
        return false;
    }
    match params.kind().info().section {
        Section::Mill2D | Section::Mill3D => true,
        Section::Drilling => {
            !matches!(params, OpParams::Drill { cross, .. } if !cross.is_empty())
                && crate::machine::kind_of_setup(world, setup) != crate::machine::Kind::Lathe
        }
        _ => false,
    }
}

/// The stock's top when it is one flat face over its whole plan: every
/// facet that faces up stands at the stock's highest Z. A stepped or
/// sloped stock (a casting, a turned or tilted block) has no single top
/// a plane can be read against - `None`, and the rapid law judges the
/// traverse under the tool at generation (#1095).
fn flat_top(stock: &[Triangle]) -> Option<f64> {
    let top = stock.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::NEG_INFINITY, f64::max);
    if !top.is_finite() {
        return None;
    }
    let flat = stock.iter().all(|t| {
        let (u, v) = (t.b - t.a, t.c - t.a);
        let nz = u.x * v.y - u.y * v.x;
        // NaN fails both comparisons: not flat.
        nz <= 1e-12 || [t.a.z, t.b.z, t.c.z].iter().all(|z| (top - z).abs() <= Z_TOL)
    });
    flat.then_some(top)
}

/// Whether an operation that cuts stands ahead of `this` in the program
/// (of a new operation joining `setup` when `this` is `None`): what it
/// removed is not known at the door, so the door stands down for it.
fn cut_ahead(world: &World, setup: Option<&str>, this: Option<&str>) -> bool {
    let order = crate::setup::names(world);
    let rank = crate::setup::rank(&order, setup);
    for (name, entity) in crate::program_entry::sequence(world) {
        if this == Some(name.as_str()) {
            return false;
        }
        let Some(op) = model::operation_of(world, &entity) else { continue };
        if this.is_none() && crate::setup::rank(&order, op.setup.as_deref()) > rank {
            return false; // a new operation is appended to its own setup
        }
        if !op.suppressed {
            return true;
        }
    }
    false
}

/// The word at command creation (#2121, AGENTS.md "validate early"): a
/// clearance plane set under the top of raw stock is named when the
/// operation is made or edited, before any path exists - the MCP create
/// and edit tools answer with it as their `warning`. It speaks only
/// where the reading is plain at the door: an upright plane
/// ([`upright_plane`]), a stock with one flat top ([`flat_top`]), and
/// nothing ahead in the program that could have cut that stock down
/// ([`cut_ahead`]).
///
/// A word, not a refusal: the door cannot see where the rapids will
/// run (a profile beside the block never crosses it), so the verdict is
/// the rapid law's at generation, which reads every rapid against the
/// stock under it and holds the post. The Retract plane is not named:
/// under the stock top it is legal inside the operation's own kerf.
/// `this` names the operation being edited; `None` is a new one joining
/// `setup`.
#[must_use]
pub fn creation_advice(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
    this: Option<&str>,
) -> Option<String> {
    if !upright_plane(world, setup, params) {
        return None;
    }
    let clearance = super::resolve_params(world, setup, Some(src), params).ok()?.clearance;
    let stock = model::find_by_name(world, &crate::setup::stock_entity_name(world, setup)).filter(|e| model::is_stock(world, e))?;
    let top = flat_top(&model::world_facets(world, &stock)?)?;
    if clearance.is_nan() || clearance >= top - Z_TOL || cut_ahead(world, setup, this) {
        return None;
    }
    let unit = crate::units::of(world);
    let len = |v: f64| format!("{} {}", crate::geometry::units::len(v, unit, 3), unit.suffix());
    Some(format!(
        "Clearance Z {} is inside the stock (its top is Z {}) and no operation ahead cuts it down: rapids at the \
         clearance plane over the stock would travel through uncut stock, and the post is held for them - raise \
         Clearance to the stock top or above, or face the stock first",
        len(clearance),
        len(top),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::Move;
    use crate::geometry::Point3;

    fn rapid(x1: f64, z0: f64, z1: f64) -> Move {
        Move {
            rotary: None,
            kind: MoveKind::Rapid,
            from: Point3::new(0.0, 0.0, z0),
            to: Point3::new(x1, 0.0, z1),
            feed: None,
            arc: None,
            axis: None,
        }
    }

    #[test]
    fn the_note_names_the_first_traverse_its_height_and_both_planes() {
        // Move 0 traverses at the plane, move 1 dives, move 2 links
        // under the plane, move 3 traverses again.
        let path = Toolpath::new(vec![rapid(10.0, 8.0, 8.0), rapid(0.0, 8.0, 6.0), rapid(10.0, 6.0, 6.0), rapid(5.0, 8.0, 8.0)]);
        let warns: Vec<Warning> = (0..4).map(|at| Warning::RapidInStock { at }).collect();
        let n = note(&path, 8.0, 10.0, false, &warns).expect("a note");
        for text in [CLEARANCE_STOCK_WARNING, "rapid move 0 travels through stock at Z8.000", "(and 1 more rapid)", "Z10.000", "raise Clearance"] {
            assert!(n.contains(text), "{n}");
        }
    }

    #[test]
    fn only_a_stock_with_one_flat_top_has_a_top_to_read_a_plane_against() {
        use crate::geometry::cube;
        let block = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)).facets.to_vec();
        assert_eq!(flat_top(&block), Some(10.0));
        // A boss on the block: the low top faces up under the highest Z.
        let mut stepped = block.clone();
        stepped.extend(cube(Point3::new(0.0, 0.0, 10.0), Point3::new(5.0, 5.0, 14.0)).facets.iter().copied());
        assert_eq!(flat_top(&stepped), None);
        // A sloped cap, and nothing at all.
        let slope = [Triangle::new(Point3::ZERO, Point3::new(10.0, 0.0, 4.0), Point3::new(0.0, 10.0, 0.0))];
        assert_eq!(flat_top(&slope), None);
        assert_eq!(flat_top(&[]), None);
        let nan = [Triangle::new(Point3::ZERO, Point3::new(10.0, 0.0, f64::NAN), Point3::new(0.0, 10.0, 0.0))];
        assert_eq!(flat_top(&nan), None);
    }

    #[test]
    fn dives_links_under_the_plane_and_other_warnings_add_no_note() {
        let path = Toolpath::new(vec![rapid(0.0, 8.0, 6.0), rapid(10.0, 6.0, 6.0)]);
        let warns = vec![Warning::RapidInStock { at: 0 }, Warning::RapidInStock { at: 1 }, Warning::Gouge { at: 1 }, Warning::RapidInStock { at: 9 }];
        assert_eq!(note(&path, 8.0, 10.0, false, &warns), None);
    }
}
