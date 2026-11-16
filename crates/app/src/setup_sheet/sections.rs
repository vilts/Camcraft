//! Setup-sheet sections: stock, WCS offsets, datum, fixtures (M3.2 stage
//! D). Provenance: our own text; machine-shop setup-sheet convention,
//! behavior only - see the module doc in `setup_sheet/mod.rs`.

use flecs_ecs::prelude::World;

use super::mm;
use crate::fixture;
use crate::model;
use crate::setup;
use crate::stock::StockShape;

pub(super) fn render_stock(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("STOCK\n");
    render_source(world, ctx, out);
    let name = setup::stock_entity_name(world, ctx);
    let Some(entity) = setup::stock_entity(world, ctx) else {
        out.push_str(&format!("  {name}: (none)\n\n"));
        return;
    };
    out.push_str(&format!("  entity   {name}\n"));
    match model::stock_def_of(world, &entity) {
        Some(StockShape::Box { dx, dy, dz }) => {
            out.push_str(&format!(
                "  box      X {}  Y {}  Z {}\n",
                mm(dx),
                mm(dy),
                mm(dz)
            ));
        }
        Some(StockShape::Cylinder { diameter, height }) => {
            out.push_str(&format!(
                "  cylinder DIA {}  HEIGHT {}\n",
                mm(diameter),
                mm(height)
            ));
        }
        Some(StockShape::FromModel { source }) => {
            let facets = model::facets_of(world, &entity).map_or(0, |t| t.len());
            out.push_str(&format!(
                "  from model '{source}' - {facets} facet(s)\n"
            ));
            if let Some(b) = model::entity_world_aabb(world, &entity) {
                out.push_str(&format!(
                    "  world box  min ({}, {}, {})  max ({}, {}, {})\n",
                    mm(b.min.x),
                    mm(b.min.y),
                    mm(b.min.z),
                    mm(b.max.x),
                    mm(b.max.y),
                    mm(b.max.z),
                ));
            }
        }
        None => out.push_str("  (stock entity with no shape definition)\n"),
    }
    let loc = model::transform_of(world, &entity).unwrap_or_default();
    out.push_str(&format!(
        "  placed at  X {}  Y {}  Z {}\n\n",
        mm(loc.pos.x),
        mm(loc.pos.y),
        mm(loc.pos.z)
    ));
}

/// #320: where a setup after the first takes its stock from, and what
/// reaches it when it takes what the setup before leaves. The first
/// setup prints nothing (sheets of one-setup documents unchanged).
fn render_source(world: &World, ctx: Option<&str>, out: &mut String) {
    use crate::setup_stock::{self, StockSource};
    let Some(name) = setup::effective(world, ctx) else { return };
    let Some(words) = setup_stock::describe(world, &name) else { return };
    out.push_str(&format!("  source   {words}\n"));
    if setup_stock::effective(world, &name) != Some(StockSource::FromPrevious) {
        return;
    }
    match setup_stock::incoming(world, &name) {
        Ok(i) => out.push_str(&format!(
            "  incoming min ({}, {}, {})  max ({}, {}, {})  volume {} mm3 - the block below is not cut from\n",
            mm(i.min[0]),
            mm(i.min[1]),
            mm(i.min[2]),
            mm(i.max[0]),
            mm(i.max[1]),
            mm(i.max[2]),
            mm(i.volume),
        )),
        Err(why) => out.push_str(&format!("  incoming not known: {why}\n")),
    }
}

pub(super) fn render_wcs(world: &World, ctx: Option<&str>, out: &mut String) {
    // #342: one WCS per setup, labelled with the word the post writes.
    let label = setup::wcs_label(world, ctx);
    out.push_str("WCS\n");
    match setup::wcs_entity(world, ctx) {
        Some(entity) => {
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            let pos = loc.pos;
            let entity_name = model::name_of(world, &entity).unwrap_or_default();
            out.push_str(&format!(
                "  {label}  X {}  Y {}  Z {}   (entity {entity_name})\n",
                mm(pos.x),
                mm(pos.y),
                mm(pos.z)
            ));
            // #327: the rotation the translation line cannot show.
            match crate::posting::offset_frame(&loc) {
                crate::posting::OffsetFrame::Square => {}
                crate::posting::OffsetFrame::Rotated { deg, axis } => out.push_str(&format!(
                    "       rotated {deg:.3} deg about {axis} - the post refuses a rotated offset\n"
                )),
                crate::posting::OffsetFrame::NonFinite => out.push_str(
                    "       placement is not finite - the post refuses this offset\n",
                ),
            }
        }
        None => out.push_str(&format!("  {label}  (not placed - at the origin)\n")),
    }
    // #196, #329, #931: the one offset the setup's program runs in: the
    // setup's label (offset 1 when unset), named by the setup's post -
    // never another offset's name.
    out.push_str(&format!("  program offset: {}\n", setup::wcs_name(world, ctx)));
    out.push('\n');
}

pub(super) fn render_datum(world: &World, ctx: Option<&str>, out: &mut String) {
    if let Some(from) = ctx.and_then(|s| setup::def_of(world, s))
        .map(|d| d.datum_from)
        .filter(|s| !s.is_empty())
    {
        out.push_str("DATUM\n");
        out.push_str(&format!(
            "  transferred from '{from}' - its stock and placed offsets were cloned\n\n"
        ));
    }
    // #326: the operator must clamp the part the way the program assumes.
    // As modelled prints nothing (identity is absence - sheets unchanged).
    if let Some(a) = setup::effective(world, ctx).and_then(|s| crate::setup_align::stored(world, &s)) {
        out.push_str("PART ALIGNMENT\n");
        out.push_str(&format!(
            "  {} - clamp the part this way up; stock, offsets and fixtures above are in this frame\n\n",
            crate::setup_align::describe(Some(a))
        ));
    }
}

/// #375: one line per inverted linear axis of the machine this setup
/// mounts - the operator must know the slide runs the other way and the
/// program is mirrored for it. A standard machine prints nothing (sheets
/// unchanged).
pub(super) fn render_machine_axes(world: &World, ctx: Option<&str>, out: &mut String) {
    let (_, machine) = crate::machine::of_setup(world, ctx);
    let Some(m) = machine else {
        return;
    };
    let letters = m.inverted_axes();
    if letters.is_empty() {
        return;
    }
    out.push_str("MACHINE AXES\n");
    for l in letters {
        out.push_str(&format!(
            "  {} - the program is mirrored in {l} for this machine\n",
            crate::machine::inverted_axis_line(l)
        ));
    }
    out.push('\n');
}

pub(super) fn render_fixtures(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("FIXTURES\n");
    let lines = fixture_lines(world, ctx);
    if lines.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for l in lines {
            out.push_str(&format!("  {l}\n"));
        }
    }
    out.push('\n');
}

/// One line per fixture of the setup context, name-sorted by
/// `fixture::tagged` (its own determinism law). A fixture is now a solid
/// the user loaded, so the sheet prints its placement and the footprint
/// its geometry occupies in the world (the transformed AABB) - there
/// are no defining parameters to name any more, and the machine reads
/// the sheet against real hardware it can measure. The derived machine
/// table, if the loaded `.mch` states one, is a final line.
///
/// Every line ends with the workholding law the verifier judged the
/// steel by (M13.8): [`FIXED_WORDING`] for a fixture declared fixed -
/// it stays in the machine frame while a rotary turns the part - and
/// [`TURNING_WORDING`] for the default that rides the rotary with it.
/// The machine table is always fixed: a table never turns. Printed so
/// the operator reads exactly the assumption the collision verdict
/// stood on, never a guess at it.
fn fixture_lines(world: &World, setup: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = fixture::tagged(world, setup)
        .into_iter()
        .filter_map(|(name, entity)| {
            let loc = model::transform_of(world, &entity)?;
            let box_ = model::entity_world_aabb(world, &entity)?;
            let fixed = fixture::def_of(world, &entity).is_some_and(|d| d.fixed);
            Some(format!(
                "{name}  at X {}  Y {}  Z {}  footprint {} x {}  {}",
                mm(loc.pos.x),
                mm(loc.pos.y),
                mm(loc.pos.z),
                mm(box_.max.x - box_.min.x),
                mm(box_.max.y - box_.min.y),
                frame_wording(fixed),
            ))
        })
        .collect();
    if let Some(t) = fixture::table_box(world, setup) {
        out.push(format!(
            "{}  top Z {}  span {} x {}  {}",
            fixture::MACHINE_TABLE,
            mm(t.max.z),
            mm(t.max.x - t.min.x),
            mm(t.max.y - t.min.y),
            frame_wording(true),
        ));
    }
    out
}

/// The sheet's words for a fixture judged in the machine frame.
pub const FIXED_WORDING: &str = "fixed (machine frame)";

/// The sheet's words for a fixture judged in the part's turning frame.
pub const TURNING_WORDING: &str = "turns with the part";

/// The trailing column of a fixture line: which frame the verifier
/// judged the steel in, in the operator's words.
fn frame_wording(fixed: bool) -> &'static str {
    if fixed { FIXED_WORDING } else { TURNING_WORDING }
}
