//! M7.3 cost law, measured: the field engine's per-move work on every
//! committed golden program - bricks visited and touched, per-cell
//! evaluations and wall time per move - printed per program, so the
//! claim "an update touches only the sweep's dirty bricks" is a
//! number, not a promise. Heavy lane: it runs the verdict engine at
//! full size. The bound asserted here is the coarse one (every program
//! finishes inside its budget, none refuses); the per-brick law itself
//! is proved in the debug lane by camcraft-sim's `stock_field_cost`.
//! Provenance: our own golden programs and engines - nothing from
//! vendor material.
//!
//! Gate map (M4.4): covers app::stock_sim, sim::stock_sim (the field arm's simulate).

mod common;

use std::time::{Duration, Instant};

use common::engine_fixtures::programs;

use camcraft_app::stock_sim::{Backend, SimOp, Stock, FIELD_BAND};

/// The lattice the heavy lanes judge on.
const CELL: f64 = 0.5;

/// Past this a program's walk is stopped and its rate extrapolated.
const BUDGET: Duration = Duration::from_secs(20);

#[test]
#[ignore = "heavy lane (M7.3 cost law): field engine over every golden program, release ~1 min - run at milestone close with -- --ignored"]
fn field_cost_per_move_on_the_golden_programs() {
    let progs = programs().expect("the goldens parse");
    let mut bad = Vec::new();
    println!("program | moves done/total | ms/move | bricks visited/move | touched/move | skipped/move | cells evaluated/move | build s | note");
    let t_all = Instant::now();
    for pg in &progs {
        let (x0, y0, x1, y1) = pg.stock_box(CELL);
        let t_build = Instant::now();
        let mut stock = match Stock::block(Backend::Field { band: FIELD_BAND }, x0, y0, x1, y1, pg.top, CELL) {
            Ok(s) => s,
            Err(e) => {
                println!("{} | BLOCK REFUSED: {e}", pg.file);
                bad.push(pg.file);
                continue;
            }
        };
        let build = t_build.elapsed();
        let t0 = Instant::now();
        // Chunks double (1, 2, 4, ... moves per `simulate`), so an
        // unaffordable engine still reports a rate inside its budget
        // and an affordable one is measured at the batch size it ships.
        let (mut done, mut chunk, mut note) = (0usize, 1usize, String::new());
        while done < pg.moves.len() {
            let end = (done + chunk).min(pg.moves.len());
            let ops = [SimOp { moves: &pg.moves[done..end], radius: pg.radius, ball: pg.ball, cone: None, corner: pg.corner }];
            if let Err(e) = stock.simulate(&ops) {
                note = format!("REFUSED at move {done}: {e}");
                break;
            }
            done = end;
            chunk = (chunk * 2).min(1024);
            if t0.elapsed() > BUDGET {
                note = "over budget".into();
                break;
            }
        }
        let dt = t0.elapsed();
        let st = stock.field().map(|f| f.cut_stats()).unwrap_or_default();
        let per = |v: u64| v as f64 / done.max(1) as f64;
        println!(
            "{} | {done}/{} | {:.3} | {:.1} | {:.1} | {:.1} | {:.0} | {:.2} | {note}",
            pg.file,
            pg.moves.len(),
            dt.as_secs_f64() * 1e3 / done.max(1) as f64,
            per(st.bricks_visited),
            per(st.bricks_touched),
            per(st.bricks_skipped),
            per(st.cells_evaluated),
            build.as_secs_f64(),
        );
        if !note.is_empty() {
            bad.push(pg.file);
        }
    }
    println!("total {:.1}s", t_all.elapsed().as_secs_f64());
    assert!(bad.is_empty(), "programs the field could not afford: {bad:?}");
}
