//! The keep law with the stock as the source (#1554, #1603): the region
//! is the stock minus the setup's parts, and an area offset into the
//! part is still convicted. One test for each (kind, stock) case (#1715).

use super::*;

/// The feed points whose tool centre stands over the part's own section
/// (the block, or the boss above it) closer than the cutter radius, below
/// that section's top. A path that keeps the part has none.
fn over_the_part(t: &Toolpath) -> Vec<Point3> {
    let near = |p: &Point3, half: f64| (p.x.abs() - half).max(0.0).hypot((p.y.abs() - half).max(0.0)) < R - 0.05;
    feeds(t)
        .into_iter()
        .filter(|p| (near(p, 15.0) && p.z < 10.0 - EPS) || (near(p, 6.0) && p.z < 20.0 - EPS))
        .collect()
}

/// #1554: the stock as the source cleared its whole footprint, the part's
/// own section included; generate said ok and only the keep law (#1603)
/// or verify stopped it. Now the region is the stock minus the setup's
/// parts (the #12 island law): no move over the part, nothing on the op,
/// and the program posts on both stocks.
fn stock_source_goes_around((label, params): Kind, rotated: bool) {
    let (world, mut h) = job(Some(block_with_boss()), rotated);
    let warns = generated(&world, &mut h, SrcRef::Stock, params, AreaControls::default());
    let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{label} rotated {rotated}: {blocking:?}");
    let e = model::find_by_name(&world, "P1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let over = over_the_part(&tp.path);
    assert!(over.is_empty(), "{label} rotated {rotated}: {} feed point(s) over the part, first {:?}", over.len(), over.first());
    assert_posts(&world, &format!("stock-around-{label}-{rotated}"));
}

mod a_stock_source_clearing_goes_around_the_part_and_posts {
    use super::*;

    #[test]
    fn pocket_aligned() {
        stock_source_goes_around(pocket_k(20.0, 5.0), false);
    }

    #[test]
    fn pocket_turned() {
        stock_source_goes_around(pocket_k(20.0, 5.0), true);
    }

    #[test]
    fn adaptive_aligned() {
        stock_source_goes_around(adaptive_k(20.0, 10.0), false);
    }

    #[test]
    fn adaptive_turned() {
        stock_source_goes_around(adaptive_k(20.0, 10.0), true);
    }
}

/// The keep law stays the backstop for a stock source: an offset of +3
/// grows the region 3 mm into the part's own section again.
fn stock_source_offset_refused((label, params): Kind, rotated: bool) {
    let (world, mut h) = job(Some(block_with_boss()), rotated);
    let warns = generated(&world, &mut h, SrcRef::Stock, params, offset_3());
    assert!(gouges(&warns) > 0, "{label} rotated {rotated}: the part under the stock is convicted: {warns:?}");
    assert_post_refused(&world, &format!("stock-offset-{label}-{rotated}"), rotated);
}

mod a_stock_source_area_offset_into_the_part_gouges_and_never_posts {
    use super::*;

    #[test]
    fn pocket_aligned() {
        stock_source_offset_refused(pocket_k(20.0, 5.0), false);
    }

    #[test]
    fn pocket_turned() {
        stock_source_offset_refused(pocket_k(20.0, 5.0), true);
    }

    #[test]
    fn adaptive_aligned() {
        stock_source_offset_refused(adaptive_k(20.0, 5.0), false);
    }

    #[test]
    fn adaptive_turned() {
        stock_source_offset_refused(adaptive_k(20.0, 5.0), true);
    }
}

/// A plain 30 x 30 x 10 block under 10 mm of stock: clearing the stock
/// down to the block's top touches it and no more.
fn stock_cut_stops_on_the_part((label, params): Kind, rotated: bool) {
    let block = Mesh::from_facets(cube(Point3::new(-15.0, -15.0, 0.0), Point3::new(15.0, 15.0, 10.0)).facets.to_vec());
    let (world, mut h) = job(Some(block), rotated);
    let warns = generated(&world, &mut h, SrcRef::Stock, params, AreaControls::default());
    let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{label} rotated {rotated}: {blocking:?}");
    assert_posts(&world, &format!("stock-legal-{label}-{rotated}"));
}

mod a_stock_source_cut_that_stops_on_the_part_stays_clean_and_posts {
    use super::*;

    #[test]
    fn pocket_aligned() {
        stock_cut_stops_on_the_part(pocket_k(20.0, 10.0), false);
    }

    #[test]
    fn pocket_turned() {
        stock_cut_stops_on_the_part(pocket_k(20.0, 10.0), true);
    }

    #[test]
    fn adaptive_aligned() {
        stock_cut_stops_on_the_part(adaptive_k(20.0, 10.0), false);
    }

    #[test]
    fn adaptive_turned() {
        stock_cut_stops_on_the_part(adaptive_k(20.0, 10.0), true);
    }
}

#[test]
fn a_stock_source_cut_with_no_part_in_the_setup_judges_nothing() {
    for (label, params) in kinds(20.0, 5.0) {
        let (world, mut h) = job(None, false);
        let warns = generated(&world, &mut h, SrcRef::Stock, params, AreaControls::default());
        assert_eq!(gouges(&warns), 0, "{label}: no part, no part law: {warns:?}");
    }
}
