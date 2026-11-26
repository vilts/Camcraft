// Provenance: clean-room checks of the catalog case 2d/slots (#2861),
// over CamCraft's own posted program. Pulled into example_catalog.rs with
// `include!` so that each test keeps its name (the case contract test
// lists `catalog_2d_slots::regenerates` and `catalog_2d_slots::rebuilds`).

// #2861: about 40 s each in debug alone, over the 60 s budget beside the
// other cases. `slots_case_posts_a_slot_and_a_trochoidal_slot_to_the_same_floor`
// keeps the committed program pinned in the default suite.
mod catalog_2d_slots {
    use super::*;
    #[test]
    #[ignore = "heavy lane (#2861 2d/slots): ~40 s in debug alone, over its 60 s budget under the parallel suite - run with --release -- --ignored"]
    fn regenerates() {
        run_case("2d", "slots", "regenerates", regenerate);
    }
    #[test]
    #[ignore = "heavy lane (#2861 2d/slots): ~40 s in debug alone, over its 60 s budget under the parallel suite - run with --release -- --ignored"]
    fn rebuilds() {
        run_case("2d", "slots", "rebuilds", rebuild);
    }
}

/// One operation of a posted program: the `Z` levels of its cut blocks
/// (`G01`/`G02`/`G03`), and the counts of its arc blocks and motion blocks.
struct PostedOp {
    cut_levels: Vec<f64>,
    arcs: usize,
    blocks: usize,
}

/// Reads the blocks between the comment `(OPERATION: name - ...)` and the
/// next operation comment.
fn posted_op(nc: &str, name: &str) -> PostedOp {
    let header = format!("(OPERATION: {name} - ");
    let mut inside = false;
    let mut op = PostedOp { cut_levels: Vec::new(), arcs: 0, blocks: 0 };
    let mut z = f64::NAN;
    for line in nc.lines() {
        if line.starts_with("(OPERATION: ") {
            inside = line.starts_with(&header);
            continue;
        }
        if !inside {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(g) = words.next().filter(|w| matches!(*w, "G00" | "G01" | "G02" | "G03")) else {
            continue;
        };
        if let Some(v) = words.find_map(|w| w.strip_prefix('Z')).and_then(|w| w.parse::<f64>().ok()) {
            z = v;
        }
        op.blocks += 1;
        if g != "G00" {
            if g != "G01" {
                op.arcs += 1;
            }
            if !op.cut_levels.iter().any(|l| (l - z).abs() < 1e-9) {
                op.cut_levels.push(z);
            }
        }
    }
    op
}

/// #2861: the case 2d/slots posts what its README says. The Slot and the
/// Trochoidal slot cut the same width to the same floor (Z6). The
/// Trochoidal slot is a chain of loops, so its program has many more arc
/// blocks and is longer.
#[test]
fn slots_case_posts_a_slot_and_a_trochoidal_slot_to_the_same_floor() {
    let nc_path = repo("examples/catalog/2d/slots/slots.nc");
    let nc = fs::read_to_string(&nc_path)
        .unwrap_or_else(|e| panic!("case 2d/slots must hold {}: {e}", nc_path.display()));
    let slot = posted_op(&nc, "Slot1");
    let trochoid = posted_op(&nc, "Trochoidal slot1");
    assert!(slot.blocks > 0, "the program has the tool path Slot1");
    assert!(trochoid.blocks > 0, "the program has the tool path Trochoidal slot1");

    let floor = |op: &PostedOp| op.cut_levels.iter().copied().fold(f64::INFINITY, f64::min);
    assert_eq!(floor(&slot), 6.0, "the Slot cuts to the slot floor (Z6)");
    assert_eq!(floor(&trochoid), 6.0, "the Trochoidal slot cuts to the slot floor (Z6)");
    assert_eq!(slot.cut_levels, [9.0, 6.0], "the Slot cuts two levels with a 3 mm stepdown");
    // #2906: rows of straight moves, then a wall lap at each level whose
    // two round ends are four arcs (split at the row ends -2, 0 and 2).
    assert_eq!(slot.arcs, 8, "the Slot is rows of straight moves and a wall lap");
    assert!(
        trochoid.cut_levels.contains(&6.3),
        "each loop of the Trochoidal slot comes back 0.3 mm above the floor: {:?}",
        trochoid.cut_levels
    );
    assert!(
        trochoid.arcs > 10 * slot.arcs.max(1),
        "the Trochoidal slot is a chain of loops: {} arc blocks against {}",
        trochoid.arcs,
        slot.arcs
    );
    assert!(
        trochoid.blocks > slot.blocks,
        "the Trochoidal slot program is longer: {} blocks against {}",
        trochoid.blocks,
        slot.blocks
    );
}
