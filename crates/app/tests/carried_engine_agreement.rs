//! #637, carried stock on both engines side by side: the documents of
//! the carried-stock suites (`setup_carry`, `rest_across_setups`,
//! `carried_verify`) are generated once, then every carried read is taken
//! on the column engine AND the field engine and must agree - the program
//! verdict column for column within `FIELD_VERDICT_EPS` with the same
//! class, counts and aggregates; the body a setup is entered with (tops,
//! floors, empties, volume, `floor_judged`); the rest grid generation
//! masks with; the bounds and grids verify reads. A refusal on one
//! engine only is a finding. Findings are located and printed whole
//! before the suite fails; nothing is rounded away.
//! Heavy lane: fourteen two-setup documents, each read twice, the field
//! arm marching every column - release, about a minute.
//! Provenance: our own fixtures, engines and agreement law
//! (`stock_engine_agreement`'s, carried across setups).
//!
//! Gate map (#637): covers app::stock_sim::held (Held, the engine
//! switch), app::stock_sim::carry (walk, incoming_body), app::rest
//! (prior_state, prior_plan), app::rest::across (sweep, carried_stock_body,
//! entered_body), sim::stock_sim::carried (CarriedStock on both arms).

#[path = "carried_agreement/fixtures.rs"]
mod fixtures;

use fixtures::{documents, Doc};

use camcraft_app::rest;
use camcraft_app::stock_sim::{
    incoming_body, simulate_document, with_carry_engine, Backend, Body, Deviation, Dexel, FIELD_BAND, FIELD_VERDICT_EPS,
};

const FIELD: Backend = Backend::Field { band: FIELD_BAND };

/// A column's verdict word, as `deviate` draws its line.
fn class(v: f64, tol: f64) -> &'static str {
    if v.is_nan() {
        "no verdict"
    } else if v > tol {
        "rest"
    } else if v < -tol {
        "gouge"
    } else {
        "clean"
    }
}

/// One read on both engines; a refusal on exactly one is a finding, the
/// same refusal on both is agreement (returned as `None`).
fn both<T>(at: &str, out: &mut Vec<String>, read: impl Fn() -> Result<T, String>) -> Option<(T, T)> {
    match (with_carry_engine(Backend::Column, &read), with_carry_engine(FIELD, &read)) {
        (Ok(c), Ok(f)) => Some((c, f)),
        (Err(c), Err(f)) => {
            if c != f {
                out.push(format!("{at}: refused differently - column: {c}; field: {f}"));
            }
            None
        }
        (c, f) => {
            let say = |r: &Result<T, String>| r.as_ref().map_or_else(Clone::clone, |_| "ran".to_owned());
            out.push(format!("{at}: the engines did not both run - column: {}; field: {}", say(&c), say(&f)));
            None
        }
    }
}

fn same_lattice(at: &str, c: &Dexel, f: &Dexel, out: &mut Vec<String>) -> bool {
    let ok = (c.nx, c.ny) == (f.nx, f.ny) && c.cell == f.cell && (c.x0 - f.x0).abs() <= 1e-9 && (c.y0 - f.y0).abs() <= 1e-9;
    if !ok {
        out.push(format!(
            "{at}: lattice {}x{} @ {} from ({}, {}) vs field {}x{} @ {} from ({}, {})",
            c.nx, c.ny, c.cell, c.x0, c.y0, f.nx, f.ny, f.cell, f.x0, f.y0
        ));
    }
    ok
}

/// Two grids of tops: every column within the epsilon, air on both or
/// neither.
fn compare_tops(at: &str, c: &Dexel, f: &Dexel, out: &mut Vec<String>) -> f64 {
    if !same_lattice(at, c, f, out) {
        return f64::NAN;
    }
    let mut worst = 0.0f64;
    for k in 0..c.nx * c.ny {
        let (a, b) = (c.top_at(k), f.top_at(k));
        if a.is_finite() != b.is_finite() || (a.is_finite() && (a - b).abs() > FIELD_VERDICT_EPS) {
            let q = c.center(k % c.nx, k / c.nx);
            out.push(format!("{at}: column ({:.3}, {:.3}): top {a:.6} vs field {b:.6}", q.x, q.y));
        } else if a.is_finite() {
            worst = worst.max((a - b).abs());
        }
    }
    worst
}

/// Two carried bodies: tops and floors of every standing column, the
/// empties, the volume and whether the floor is judged.
fn compare_bodies(at: &str, c: &Body, f: &Body, out: &mut Vec<String>) -> f64 {
    if !same_lattice(at, &c.dex, &f.dex, out) {
        return f64::NAN;
    }
    if c.floor_judged() != f.floor_judged() {
        out.push(format!("{at}: floor judged {} vs field {}", c.floor_judged(), f.floor_judged()));
    }
    let d = &c.dex;
    let mut worst = 0.0f64;
    for k in 0..d.nx * d.ny {
        let q = d.center(k % d.nx, k / d.nx);
        if c.empty_at(k) != f.empty_at(k) {
            out.push(format!("{at}: column ({:.3}, {:.3}): empty {} vs field {}", q.x, q.y, c.empty_at(k), f.empty_at(k)));
            continue;
        }
        if c.empty_at(k) {
            continue;
        }
        let (dt, df) = ((d.top_at(k) - f.dex.top_at(k)).abs(), (c.floor_at(k) - f.floor_at(k)).abs());
        if dt.is_nan() || df.is_nan() || dt > FIELD_VERDICT_EPS || df > FIELD_VERDICT_EPS {
            out.push(format!(
                "{at}: column ({:.3}, {:.3}): top {:.6} / floor {:.6} vs field top {:.6} / floor {:.6}",
                q.x, q.y, d.top_at(k), c.floor_at(k), f.dex.top_at(k), f.floor_at(k)
            ));
        } else {
            worst = worst.max(dt).max(df);
        }
    }
    let vol_eps = FIELD_VERDICT_EPS * d.cell * d.cell * (d.nx * d.ny) as f64;
    let dv = (c.volume() - f.volume()).abs();
    if dv.is_nan() || dv > vol_eps {
        out.push(format!("{at}: volume {:.6} vs field {:.6}", c.volume(), f.volume()));
    }
    worst
}

/// Every way two program verdicts can differ, said with its location.
fn compare_verdicts(at: &str, c: &Deviation, f: &Deviation, out: &mut Vec<String>) -> f64 {
    if (c.cells_x, c.cells_y) != (f.cells_x, f.cells_y) || c.cell != f.cell || c.tol != f.tol {
        out.push(format!("{at}: verdict lattice {}x{} vs field {}x{}", c.cells_x, c.cells_y, f.cells_x, f.cells_y));
        return f64::NAN;
    }
    let mut worst = 0.0f64;
    for (k, (a, b)) in c.dev.iter().zip(&f.dev).enumerate() {
        let d = (a - b).abs();
        if a.is_finite() && b.is_finite() {
            worst = worst.max(d);
        }
        let (ca, cb) = (class(*a, c.tol), class(*b, f.tol));
        if a.is_nan() != b.is_nan() || (a.is_finite() && d > FIELD_VERDICT_EPS) || ca != cb {
            let (ix, iy) = (k % c.cells_x, k / c.cells_x);
            out.push(format!("{at}: column [{ix}, {iy}]: dev {a:.6} ({ca}) vs field {b:.6} ({cb}), |delta| {d:.3e}"));
        }
    }
    let vol_eps = FIELD_VERDICT_EPS * c.cell * c.cell * c.dev.len() as f64;
    for (name, a, b, e) in [
        ("max_rest", c.max_rest, f.max_rest, FIELD_VERDICT_EPS),
        ("max_gouge", c.max_gouge, f.max_gouge, FIELD_VERDICT_EPS),
        ("rest_volume", c.rest_volume, f.rest_volume, vol_eps),
        ("gouge_volume", c.gouge_volume, f.gouge_volume, vol_eps),
    ] {
        // NaN on either side is a disagreement too, never a pass.
        if (a - b).abs().is_nan() || (a - b).abs() > e {
            out.push(format!("{at}: {name} {a:.6} vs field {b:.6}"));
        }
    }
    if (c.rest_cells, c.gouge_cells, c.is_clean()) != (f.rest_cells, f.gouge_cells, f.is_clean()) {
        out.push(format!(
            "{at}: counts rest {} / gouge {} / clean {} vs field rest {} / gouge {} / clean {}",
            c.rest_cells, c.gouge_cells, c.is_clean(), f.rest_cells, f.gouge_cells, f.is_clean()
        ));
    }
    for (name, a, b) in [("max_rest_at", c.max_rest_at, f.max_rest_at), ("max_gouge_at", c.max_gouge_at, f.max_gouge_at)] {
        let near = match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => (a.x - b.x).abs().max((a.y - b.y).abs()).max((a.z - b.z).abs()) <= FIELD_VERDICT_EPS,
            _ => false,
        };
        if !near {
            out.push(format!("{at}: {name} {a:?} vs field {b:?}"));
        }
    }
    worst
}

/// Every carried read of one document, on both engines.
fn judge(doc: &Doc, out: &mut Vec<String>) {
    let (w, name) = (&doc.world, &doc.name);
    let before = out.len();
    let mut worst = 0.0f64;
    let mut reads = 0;
    let mut took = |v: f64| {
        worst = worst.max(v);
        reads += 1;
    };
    if let Some((c, f)) = both(&format!("{name} / program verdict"), out, || simulate_document(w).map(|s| s.dev)) {
        took(compare_verdicts(&format!("{name} / program verdict"), &c, &f, out));
    }
    if let Some((c, f)) = both(&format!("{name} / incoming body"), out, || incoming_body(w, "Setup2")) {
        took(compare_bodies(&format!("{name} / incoming body"), &c, &f, out));
    }
    if let Some((op, r)) = doc.consumer {
        let setup = Some("Setup2");
        let at = format!("{name} / rest grid of {op}");
        let read = || rest::prior_state(w, op, setup, r).map(|p| (p.grid, p.digest)).map_err(|e| e.to_string());
        if let Some(((cg, cd), (fg, fd))) = both(&at, out, read) {
            if cd != fd {
                out.push(format!("{at}: the stamp digest differs between engines"));
            }
            match (cg, fg) {
                (Some(c), Some(f)) => took(compare_tops(&at, &c, &f, out)),
                (None, None) => {}
                (c, f) => out.push(format!("{at}: a mask on one engine only (column {}, field {})", c.is_some(), f.is_some())),
            }
        }
        let at = format!("{name} / verify prior of {op}");
        let read = || rest::prior_plan(w, op, setup, r)?.map(rest::PriorPlan::simulate).transpose();
        match both(&at, out, read) {
            Some((Some(c), Some(f))) => took(compare_tops(&at, &c, &f, out)),
            Some((None, None)) | None => {}
            Some((c, f)) => out.push(format!("{at}: a prior on one engine only (column {}, field {})", c.is_some(), f.is_some())),
        }
        let at = format!("{name} / entered body of {op}");
        let read = || rest::entered_body(w, op, setup, r).map(|b| b.map(|(origin, body)| (origin, (*body).clone())));
        match both(&at, out, read) {
            Some((Some((co, c)), Some((fo, f)))) => {
                if co != fo {
                    out.push(format!("{at}: origin '{co}' vs field '{fo}'"));
                }
                took(compare_bodies(&at, &c, &f, out));
            }
            Some((None, None)) | None => {}
            Some((c, f)) => out.push(format!("{at}: carried on one engine only (column {}, field {})", c.is_some(), f.is_some())),
        }
        let at = format!("{name} / verify bounds of {op}");
        if let Ok(Some((_, block))) = rest::carried_stock_block(w, op, setup) {
            match both(&at, out, || rest::carried_stock_body(w, op, setup, r, block)) {
                Some((Some((cb, c)), Some((fb, f)))) => {
                    let far = |a: camcraft_app::geometry::Point3, b: camcraft_app::geometry::Point3| {
                        (a.x - b.x).abs().max((a.y - b.y).abs()).max((a.z - b.z).abs()) > FIELD_VERDICT_EPS
                    };
                    if far(cb.0, fb.0) || far(cb.1, fb.1) {
                        out.push(format!("{at}: bounds {cb:?} vs field {fb:?}"));
                    }
                    took(compare_tops(&at, &c, &f, out));
                }
                Some((None, None)) | None => {}
                Some((c, f)) => out.push(format!("{at}: bounds on one engine only (column {}, field {})", c.is_some(), f.is_some())),
            }
        }
    }
    println!("{name} | reads {reads} | max |delta| {worst:.3e} | findings {}", out.len() - before);
}

#[test]
#[ignore = "heavy lane (#637 carried-stock agreement): the carried suites' documents read on both stock engines, release ~1 min - run at milestone close with -- --ignored"]
fn both_engines_read_every_carried_document_alike() {
    let mut findings: Vec<String> = Vec::new();
    println!("document | reads compared | max |delta| | findings");
    let docs = documents();
    for doc in &docs {
        judge(doc, &mut findings);
    }
    for f in &findings {
        println!("FINDING {f}");
    }
    assert!(findings.is_empty(), "{} finding(s) across {} documents - see the list above", findings.len(), docs.len());
}

#[test]
fn the_engine_switch_is_scoped_and_defaults_to_the_column_engine() {
    use camcraft_app::stock_sim::carry_engine;
    // The process default (no CAMCRAFT_CARRY_ENGINE in a gate run) is
    // what ships; a scope sets it and puts it back, nested and on panic.
    let outer = carry_engine();
    if std::env::var(camcraft_app::stock_sim::CARRY_ENGINE_ENV).is_err() {
        assert_eq!(outer, Backend::Column, "the column engine is the default");
    }
    with_carry_engine(FIELD, || {
        assert_eq!(carry_engine(), FIELD);
        with_carry_engine(Backend::Column, || assert_eq!(carry_engine(), Backend::Column));
        assert_eq!(carry_engine(), FIELD);
    });
    assert_eq!(carry_engine(), outer);
    let caught = std::panic::catch_unwind(|| with_carry_engine(FIELD, || panic!("unwound")));
    assert!(caught.is_err());
    assert_eq!(carry_engine(), outer, "restored after an unwind");
}
