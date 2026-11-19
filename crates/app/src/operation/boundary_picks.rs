//! Boundary picks, read and edited (M11.5 UX, the user's hand-test: "it
//! is very unclear how the open boundary selection works ... if i select
//! an edge it will close the loop ... no way of breaking up a closed
//! selection"). Three things live here, all on the world's features:
//!
//! - what an entry's picks SPELL ([`boundary_pick_shape`]): one open
//!   chain (several picked runs joined end to end), separate chains
//!   that stay apart (#1727), or closed rings - the
//!   same walk generation takes (`picks_geom`), so the page's summary and
//!   the generator can never disagree;
//! - the door check ([`area_pick_issue`]): an include / exclude whose
//!   picks spell an open chain is refused at Apply, by name, with the
//!   two ways out - the generate-time refusal stays only as a backstop;
//! - the pick edits the viewport makes: the ONE run under the cursor
//!   ([`run_pick`]), toggling a run in or out of an entry - breaking a
//!   picked loop open ([`toggle_run`]) - and closing an open pick around
//!   its face loop ([`extend_to_loop`]).
//!
//! Provenance: our own design notes; the run is `feature::edge_runs`'
//! law, the loop is the planar face's own boundary ring. No vendor
//! material consulted.

use super::boundary::OpenGeom;
use super::boundary_chains::{group_runs, touching_refusal};
use super::*;

/// What a boundary entry's picks spell.
#[derive(Clone, Debug, PartialEq)]
pub enum PickShape {
    /// One open chain, world XY in walk order.
    Open { picks: usize, chain: Vec<Point2> },
    /// Two or more separate open chains (#1727): the area lies between them.
    Chains { picks: usize, chains: Vec<Vec<Point2>> },
    /// Closed rings (`faces` of the picks were faces).
    Closed { picks: usize, faces: usize, rings: usize },
    /// The picks spell no boundary; the reason, named.
    Invalid(String),
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl PickShape {
    #[must_use]
    pub fn is_open(&self) -> bool {
        matches!(self, PickShape::Open { .. } | PickShape::Chains { .. })
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        matches!(self, PickShape::Closed { .. })
    }

    /// One line for the page: "3 edges, open chain", "1 face loop, closed".
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            PickShape::Open { picks, .. } => format!("{}, open chain", plural(*picks, "edge", "edges")),
            PickShape::Chains { picks, chains } => {
                format!("{}, {} open chains", plural(*picks, "edge", "edges"), chains.len())
            }
            PickShape::Closed { picks, faces, rings } => {
                let mut parts = Vec::new();
                if *faces > 0 {
                    parts.push(plural(*faces, "face loop", "face loops"));
                }
                if picks > faces {
                    parts.push(plural(picks - faces, "edge", "edges"));
                }
                let ring = if *rings > 1 { format!("closed, {rings} rings") } else { "closed".to_owned() };
                format!("{}, {ring}", parts.join(" + "))
            }
            PickShape::Invalid(why) => format!("not a boundary: {why}"),
        }
    }
}

/// Picked boundary features in the operation's plane - world XY, or the
/// 3+2 index's tilted plane through `frame` - through the selection
/// walk every region runs (stitching, rim dedupe and every named
/// refusal included): one open chain - its runs joined end to end -,
/// separate chains that stay apart (#1727), or closed rings, never a mix.
pub(super) fn picks_geom(
    world: &World,
    setup: Option<&str>,
    picks: &[GeoSel],
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<OpenGeom, GenError> {
    // The contour walk is the one that hands open runs back instead of
    // refusing or running them out - the reading a boundary wants.
    let probe = retype_params(params, OpType::Contour as usize);
    let (loops, runs, _) = selection_geometry_in(world, setup, picks, &probe, None, frame)?;
    match (loops.is_empty(), runs.len()) {
        (false, 0) => Ok(OpenGeom::Ring(Region::new(loops)?)),
        (true, 0) => Err(GenError::Empty("the boundary picks resolve to nothing".into())),
        (true, _) => {
            // #1727: separate chains bound the area between them, unless
            // they touch or cross (named, never guessed).
            let mut chains = group_runs(runs);
            if chains.len() == 1 {
                return chains.pop().map(OpenGeom::Chain).ok_or_else(|| GenError::Empty("no open chain".into()));
            }
            match touching_refusal(&chains) {
                Some(e) => Err(super::boundary_chain_error(e)),
                None => Ok(OpenGeom::Chains(chains)),
            }
        }
        (false, n) => Err(GenError::Invalid(format!(
            "the boundary picks spell {} closed ring(s) and {n} open run(s) - a boundary is one open chain or closed rings; remove the open edges or close them",
            loops.len()
        ))),
    }
}

/// What `picks` spell as a boundary of an operation of `params`' kind.
#[must_use]
pub fn boundary_pick_shape(world: &World, setup: Option<&str>, picks: &[GeoSel], params: &OpParams) -> PickShape {
    if picks.is_empty() {
        return PickShape::Invalid("nothing picked yet".into());
    }
    let faces = picks.iter().filter(|g| !matches!(g.feature, FeatureSel::Chain(_))).count();
    // A 3+2-indexed op reads its picks in the tilted plane (M11.5 owed).
    let frame = match params.tilt() {
        None => None,
        Some(t) => match t.frame() {
            Some(f) => Some(f),
            None => return PickShape::Invalid(TILT_NO_DIRECTION.into()),
        },
    };
    match picks_geom(world, setup, picks, params, frame.as_ref()) {
        Ok(OpenGeom::Chain(chain)) => PickShape::Open { picks: picks.len(), chain },
        Ok(OpenGeom::Chains(chains)) => PickShape::Chains { picks: picks.len(), chains },
        Ok(OpenGeom::Ring(r)) => PickShape::Closed { picks: picks.len(), faces, rings: r.loops().len() },
        Err(e) => PickShape::Invalid(e.to_string()),
    }
}

/// The Apply door for the Boundary group: an include / exclude whose
/// picks spell an open chain, named with its entry number and the two
/// ways out. `None` when every picked include / exclude closes (or does
/// not resolve at all - generation names that, as before).
#[must_use]
pub fn area_pick_issue(world: &World, setup: Option<&str>, area: &AreaControls, params: &OpParams) -> Option<String> {
    area.extra.iter().enumerate().find_map(|(i, b)| {
        let BoundaryShape::Picks(p) = &b.shape else {
            return None;
        };
        let shape = boundary_pick_shape(world, setup, p, params);
        shape.is_open().then(|| {
            let role = if b.mode == BoundaryMode::Include { "Include" } else { "Exclude" };
            format!(
                "boundary {} ({role}) is an open chain ({}) - an include / exclude must enclose an area: Shift-click or 'Extend to loop' to close it, or set its role to Open",
                i + 1,
                shape.summary()
            )
        })
    })
}

/// Why the operation's area controls refuse as the world stands (the
/// named door refusals generation would raise first - no stock for an
/// open boundary, a Contour with one, a 3+2 index ...), or `None`.
#[must_use]
pub fn area_refusal(world: &World, op: &Operation) -> Option<String> {
    match area_ctx_of(world, op) {
        Some(Err(e)) => Some(e.to_string()),
        _ => None,
    }
}

/// Feature-edge index by vertex pair (low first).
fn edge_map(f: &feature::Features) -> HashMap<(u32, u32), u32> {
    f.edges.iter().enumerate().map(|(i, e)| ((e.a.min(e.b), e.a.max(e.b)), i as u32)).collect()
}

/// A closed vertex ring as feature edges, in ring order.
fn ring_edges(map: &HashMap<(u32, u32), u32>, verts: &[u32]) -> Option<Vec<u32>> {
    let n = verts.len();
    if n < 3 {
        return None;
    }
    (0..n)
        .map(|k| {
            let (a, b) = (verts[k], verts[(k + 1) % n]);
            map.get(&(a.min(b), a.max(b))).copied()
        })
        .collect()
}

/// The feature edges a pick covers: a chain's edges, a face's closed
/// rings; a patch is not broken up (`None`).
fn pick_edges(f: &feature::Features, sel: FeatureSel) -> Option<Vec<Vec<u32>>> {
    match sel {
        FeatureSel::Chain(id) => feature::resolve_chain(f, id).map(|c| vec![c.edges]),
        FeatureSel::Face(id) => {
            let face = f.faces.iter().find(|x| x.id == id)?;
            let map = edge_map(f);
            face.loops
                .iter()
                .filter(|l| !matches!(l.kind, LoopKind::Open))
                .map(|l| ring_edges(&map, &l.verts))
                .collect()
        }
        FeatureSel::Patch(_) | FeatureSel::Vertex(_) => None,
    }
}

/// `edges` (one ring or walk, in order) as run picks with the edges each
/// covers: each edge's run when the run lies wholly inside `edges`, else
/// the edge on its own.
fn as_runs(f: &feature::Features, runs: &[feature::Chain], edges: &[u32], entity: &str) -> Vec<(GeoSel, Vec<u32>)> {
    let mut out: Vec<(GeoSel, Vec<u32>)> = Vec::new();
    for &e in edges {
        let run = match runs.iter().find(|r| r.edges.contains(&e)) {
            Some(r) if r.edges.iter().all(|x| edges.contains(x)) => Some(r.clone()),
            _ => feature::run_of_edges(f, &[e]),
        };
        if let Some(r) = run {
            let g = GeoSel { entity: entity.to_owned(), feature: FeatureSel::Chain(r.id) };
            if !out.iter().any(|(x, _)| *x == g) {
                out.push((g, r.edges));
            }
        }
    }
    out
}

/// The run under a viewport Edge pick: one geometric edge (a straight
/// stretch, or one arc), never the whole tangent loop around a fillet.
#[must_use]
pub fn run_pick(world: &World, entity: &str, edge: u32) -> Option<GeoSel> {
    let e = model::find_by_name(world, entity)?;
    let f = model::features_of(world, &e)?;
    let r = feature::run_containing(&f, edge)?;
    Some(GeoSel { entity: entity.to_owned(), feature: FeatureSel::Chain(r.id) })
}

/// Ctrl-click on a boundary entry: a run already in it comes out; a run
/// inside a larger pick (a closed chain, a face loop) breaks that pick
/// into its runs and takes the clicked one out - the way a picked loop
/// is opened; anything else is added. The second value says whether the
/// click removed.
#[must_use]
pub fn toggle_run(world: &World, picks: &[GeoSel], hit: &GeoSel) -> (Vec<GeoSel>, bool) {
    if let Some(i) = picks.iter().position(|g| g == hit) {
        let mut out = picks.to_vec();
        out.remove(i);
        return (out, true);
    }
    let found = model::find_by_name(world, &hit.entity).and_then(|e| model::features_of(world, &e));
    let (Some(f), FeatureSel::Chain(hid)) = (found, hit.feature) else {
        return (with(picks, hit), false);
    };
    let Some(hit_edges) = crate::model::resolve_chain(&f, hid).map(|c| c.edges) else {
        return (with(picks, hit), false);
    };
    let runs = feature::edge_runs(&f);
    let mut out = Vec::with_capacity(picks.len() + 8);
    let mut removed = false;
    for g in picks {
        let rings = (g.entity == hit.entity && !removed).then(|| pick_edges(&f, g.feature)).flatten();
        match rings {
            Some(rings) if rings.iter().flatten().any(|e| hit_edges.contains(e)) => {
                for ring in &rings {
                    for (r, edges) in as_runs(&f, &runs, ring, &g.entity) {
                        let covered = edges.iter().any(|e| hit_edges.contains(e));
                        if !covered && !out.contains(&r) {
                            out.push(r);
                        }
                    }
                }
                removed = true;
            }
            _ => out.push(g.clone()),
        }
    }
    if removed { (out, true) } else { (with(picks, hit), false) }
}

fn with(picks: &[GeoSel], hit: &GeoSel) -> Vec<GeoSel> {
    let mut out = picks.to_vec();
    if !out.contains(hit) {
        out.push(hit.clone());
    }
    out
}

/// Close an entry's edge picks around the face loop through them in the
/// active setup's frame (or document frame when none exists): the most
/// horizontal planar face whose boundary ring holds every picked edge,
/// smallest first on a tie. Face picks ride along unchanged; the loop
/// comes back as its runs, so a later Ctrl-click can open it again.
pub fn extend_to_loop(world: &World, picks: &[GeoSel]) -> Result<Vec<GeoSel>, String> {
    extend_to_loop_in(world, crate::setup::active(world).as_deref(), picks)
}

/// [`extend_to_loop`] in `setup`'s part-alignment frame (`None` resolves
/// to the first setup, or the document frame when none exists - the
/// same transform the generators read for picks).
pub fn extend_to_loop_in(world: &World, setup: Option<&str>, picks: &[GeoSel]) -> Result<Vec<GeoSel>, String> {
    let chains: Vec<&GeoSel> = picks.iter().filter(|g| matches!(g.feature, FeatureSel::Chain(_))).collect();
    let Some(first) = chains.first() else {
        return Err("pick an edge first - Extend to loop closes picked edges around their face".into());
    };
    let entity = first.entity.clone();
    if chains.iter().any(|g| g.entity != entity) {
        return Err("the picked edges lie on different models - one loop runs on one model".into());
    }
    let e = model::find_by_name(world, &entity).ok_or_else(|| format!("'{entity}' is gone"))?;
    let f = model::features_of(world, &e).ok_or_else(|| format!("'{entity}' has no edges"))?;
    let loc = crate::setup_align::pose_in(world, &e, setup).unwrap_or_default();
    let mut want: Vec<u32> = Vec::new();
    for g in &chains {
        let FeatureSel::Chain(id) = g.feature else { continue };
        let c = crate::model::resolve_chain(&f, id)
            .ok_or_else(|| format!("{} is no longer in the model - re-pick it", crate::pick::geo_label(world, g)))?;
        want.extend(c.edges);
    }
    let map = edge_map(&f);
    let mut best: Option<(f64, f64, Vec<u32>)> = None;
    for face in &f.faces {
        let n = transform_point(Point3::new(face.normal[0], face.normal[1], face.normal[2]), Point3::ZERO, loc.rot);
        let flat = n.z.abs();
        for l in face.loops.iter().filter(|l| !matches!(l.kind, LoopKind::Open)) {
            let Some(ring) = ring_edges(&map, &l.verts) else { continue };
            if !want.iter().all(|w| ring.contains(w)) {
                continue;
            }
            let better = best.as_ref().is_none_or(|(bf, ba, _)| flat > bf + 1e-9 || ((flat - bf).abs() <= 1e-9 && face.area < *ba));
            if better {
                best = Some((flat, face.area, ring));
            }
        }
    }
    let Some((_, _, ring)) = best else {
        return Err("no face loop runs through every picked edge - pick edges of one face's boundary, or pick the face".into());
    };
    let runs = feature::edge_runs(&f);
    let mut out: Vec<GeoSel> = picks.iter().filter(|g| !matches!(g.feature, FeatureSel::Chain(_))).cloned().collect();
    for (r, _) in as_runs(&f, &runs, &ring, &entity) {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod law {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn runs_join_end_to_end_in_either_direction() {
        let a = vec![p(0.0, 0.0), p(10.0, 0.0)];
        let b = vec![p(10.0, 10.0), p(10.0, 0.0)];
        let c = vec![p(0.0, 10.0), p(0.0, 0.0)];
        let mut one = group_runs(vec![a, b, c]);
        assert_eq!(one.len(), 1, "one chain");
        let j = one.pop().expect("one chain");
        assert_eq!(j.len(), 4);
        assert_eq!(qkey(&j[0]), qkey(&p(0.0, 10.0)));
        assert_eq!(qkey(&j[3]), qkey(&p(10.0, 10.0)));
        let far = vec![p(50.0, 50.0), p(60.0, 50.0)];
        assert_eq!(group_runs(vec![vec![p(0.0, 0.0), p(1.0, 0.0)], far]).len(), 2, "two separate runs");
        assert!(group_runs(Vec::new()).is_empty());
    }

    #[test]
    fn the_summary_names_the_shape() {
        assert_eq!(PickShape::Open { picks: 3, chain: Vec::new() }.summary(), "3 edges, open chain");
        assert_eq!(PickShape::Chains { picks: 2, chains: vec![Vec::new(), Vec::new()] }.summary(), "2 edges, 2 open chains");
        assert_eq!(PickShape::Closed { picks: 1, faces: 1, rings: 1 }.summary(), "1 face loop, closed");
        assert_eq!(PickShape::Closed { picks: 4, faces: 0, rings: 1 }.summary(), "4 edges, closed");
    }
}
