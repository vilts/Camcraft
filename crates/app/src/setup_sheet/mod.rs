//! Setup sheets: printable setup documentation generated from CAM data
//! (M3.2 stage D).
//!
//! Provenance: the setup-sheet convention - one page per setup carrying
//! the stock, the work offsets, the fixtures, the program-ordered
//! operation table with its resolved depths, and the tool list with its
//! times - is standard machine-shop practice, and the sheet's shape was
//! specced from the SolveCraft setup-sheet reference images cited in
//! ROADMAP.md's M3.2 section. BEHAVIOR ONLY: no vendor text, template,
//! or layout file was copied; every string, column and alignment here is
//! our own.
//!
//! ## The laws here
//!
//! - **Determinism is the law.** The same document renders byte-identical
//!   text, every call. Nothing is iterated in entity order (everything
//!   comes from name-sorted queries or [`crate::setup::operation_order`]),
//!   every float goes through [`mm`], and there is deliberately **no
//!   timestamp** - a printed date would break the law, so the sheet says
//!   "resolved at render time" instead.
//! - **Resolved numbers, expressed references.** The depth columns print
//!   world Zs from [`crate::operation::resolve_height`] - the exact
//!   machinery the generation stamp digests through, so the sheet and a
//!   regenerate can never disagree. A height that will not resolve
//!   renders `-` plus an annotated reason line (via the public
//!   `crate::operation::missing_source`), never a guessed number (Prime
//!   Directive: visibly refused, never silently wrong).
//! - **Membership by the setup law.** The table lists exactly the
//!   operations [`crate::setup::effective`] resolves into this setup -
//!   including implicit members whose declared setup is `None` - in the
//!   one program order the post walks.
//! - **The sim is not free.** [`simulate_verdict`] is the only path that
//!   runs the dexel program simulation (a grid over the stock, not
//!   cheap); [`render`] never calls it.
//! - **No panics.** Every read is an `Option`/`Result` from the public
//!   model/setup/operation APIs; a half-built document degrades to
//!   "(none)" lines, and only a declared setup name that does not exist
//!   is an error.

//!
//! The sheet is split along its sections (the 400-line law): the
//! stock/WCS/datum/fixtures sections in `sections`, the operation
//! table in `operations` with its per-kind depth columns in `heights`,
//! and the tool list in `tools`. Every public path stays here.

mod heights;
mod operations;
mod sections;
mod tools;

use flecs_ecs::prelude::World;

use crate::setup;

use operations::render_operations;
pub use sections::{FIXED_WORDING, TURNING_WORDING};
use sections::{render_datum, render_fixtures, render_machine_axes, render_stock, render_wcs};
use tools::render_tool_list;

/// Format one millimeter number for the sheet: fixed three decimals, no
/// trailing-zero trimming (fixed width keeps the columns honest, and
/// trimming would make alignment depend on values - determinism is the
/// law). A non-finite value prints `n/a`: the resolve machinery refuses
/// NaN/inf heights upstream, so if one ever reached the sheet the `n/a`
/// is the visible refusal, never a silent `NaN`.
#[must_use]
pub fn mm(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.3}")
    } else {
        "n/a".to_string()
    }
}

/// Format one time (minutes) for the sheet's totals: two decimals. Same
/// non-finite refusal as [`mm`].
#[must_use]
pub fn minutes(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.2}")
    } else {
        "n/a".to_string()
    }
}

/// The plain-text setup sheet for one setup context.
///
/// `setup` is a DECLARED setup: `None` means "the first setup by name",
/// which in a document without setups is the legacy implicit document
/// (the implicit-setup law) and titles the sheet `Implicit`. The only
/// error this returns is a declared name that wears no setup - a
/// half-built document renders with "(none)" sections, never an error.
pub fn render(world: &World, setup_name: Option<&str>) -> Result<String, String> {
    if let Some(s) = setup_name
        && !setup::exists(world, s)
    {
        return Err(format!("no setup named '{s}' in this document"));
    }
    // The resolved context every lookup below stands in for. `None`
    // stays `None` - the setup resolvers read that as the first setup
    // (or legacy globals), exactly what the sheet must print.
    let target = setup::effective(world, setup_name);
    let ctx = target.as_deref();
    let title = target.clone().unwrap_or_else(|| "Implicit".to_string());

    let mut out = String::new();
    out.push_str(&format!("SETUP SHEET - {title}\n"));
    out.push_str("LAW: every number is resolved from the live document at render\n");
    out.push_str("     time (no timestamps - the same document renders byte-identical).\n");
    out.push_str("     Operations follow the program law: setup rank, then position.\n");
    out.push_str("PAGE 1 OF 1 (single-page sheet)\n");
    out.push('\n');

    render_stock(world, ctx, &mut out);
    render_wcs(world, ctx, &mut out);
    render_datum(world, ctx, &mut out);
    render_machine_axes(world, ctx, &mut out);
    render_fixtures(world, ctx, &mut out);
    render_operations(world, target.as_ref(), &mut out);
    render_tool_list(world, ctx, &mut out);

    out.push('\n');
    out.push_str("END OF SHEET\n");
    Ok(out)
}

/// The flip chart: one entry per named setup in program order, or the
/// single `Implicit` entry of a setup-less document. Entries are
/// `(title, sheet)`; a setup whose sheet cannot render (it exists by
/// construction here, so it can only fail defensively) is skipped, never
/// panicked on.
#[must_use]
pub fn render_all(world: &World) -> Vec<(String, String)> {
    let names = setup::names(world);
    if names.is_empty() {
        return vec![(
            "Implicit".to_string(),
            render(world, None).unwrap_or_else(|e| format!("(render failed: {e})")),
        )];
    }
    names
        .into_iter()
        .filter_map(|n| render(world, Some(&n)).ok().map(|sheet| (n, sheet)))
        .collect()
}

/// The "program simulation" verdict line a sheet builder may opt into.
/// NOT called by [`render`]: the dexel grid over the stock is real
/// work, and a sheet builder should opt in explicitly. Deterministic
/// like everything else here - the sim reads only the document.
#[must_use]
pub fn simulate_verdict(world: &World) -> String {
    let line = sim_line(world);
    // #1168: a verdict the field engine judged says so, never silently.
    match crate::stock_sim::carry_engine_note() {
        Some(note) => format!("{line} ({note})"),
        None => line,
    }
}

fn sim_line(world: &World) -> String {
    match crate::stock_sim::simulate_document(world) {
        Ok(sim) if sim.dev.is_clean() => "SIM: clean - within deviation tolerance".to_string(),
        Ok(sim) => format!(
            "SIM: {} rest cell(s) (max {} mm), {} gouge cell(s) (max {} mm)",
            sim.dev.rest_cells,
            mm(sim.dev.max_rest),
            sim.dev.gouge_cells,
            mm(sim.dev.max_gouge),
        ),
        Err(e) => format!("SIM: not judged - {e}"),
    }
}

