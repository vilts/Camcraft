

use super::*;

/// Inputs an operation depends on: tool (name and numerics), parameters,
/// and the source's placement and shape. `None` when a referenced entity
/// is gone - an unresolvable digest is never equal to a stored one, which
/// is exactly the "Stale" reading a deleted tool should give.
/// The digest of an operation's inputs as they stand right now.
/// `op_name` is the operation's own name: re-machining digests the
/// program AHEAD of it (M3.1 D), and its position can be edited, so
/// the name is part of the question. Callers that build a stamp for an
/// operation that has no entity yet (or none that matters) pass `""`,
/// which reads as "no predecessors" and writes no rest term.
#[must_use]
/// `setup` is the operation's declared setup (M3.2 A): the stock entity,
/// the resolved heights and the re-machining mask all read it, so an
/// operation that switches setups stamps like the world it would
/// machine. `None` is the first setup - bit-for-bit the old stamp in a
/// document without setups (the implicit-setup law).
pub fn current_stamp(
    world: &World,
    tool_name: &str,
    src: &SrcRef,
    params: &OpParams,
    op_name: &str,
    setup: Option<&str>,
) -> Option<GenStamp> {
    let mut h = std::hash::DefaultHasher::new();
    // v2: heights became From+Offset expressions and their resolved
    // sources feed the digest - every op cached under v1 reads Stale
    // once and clears on regenerate (same one-time churn as the
    // b3eaebc stamp move, the mechanism working, not broken).
    // (M4.3: the salt says camcraft - a one-time Stale read for
    // documents saved under the old salt, the same churn the version
    // bumps produce, working as designed.)
    // v3 (#2152): the rapid law began to judge a traverse AT the
    // clearance plane (#2121). The post gate trusts the verdicts a cache
    // carries, so a cache judged under the old law must not read fresh:
    // it reads Stale once and the post refuses it until a regenerate.
    // Bump this salt again whenever a verify law a cache records changes.
    h.write(b"camcraft-op-stamp-v3");
    let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
    super::stamp_tool::write(world, tool_name, params, &mut h)?;
    if let OpParams::WireContour(op) = params {
        super::wire_op::stamp_condition(world, setup, tool_name, op, &mut h);
    }
    // #541: the sinker burn places its tip one spark gap above the floor,
    // read from the setup machine's condition - a re-stated gap (or a
    // condition gone) is a different burn, marked Stale through here.
    // (The electrode itself - kind and face width - is `stamp_tool`'s.)
    // #544: so is a re-stated orbit list or maximum orbit radius.
    if let OpParams::SinkerBurn(p) = params {
        h.write(b"sinker-gap");
        match super::sinker_op::burn_gap(world, setup, p) {
            Ok(g) => w(&mut h, g),
            Err(e) => h.write(e.as_bytes()),
        }
        // #543: a rough or finish reads its condition's undersize, and a
        // finish the rough it follows - a re-stated undersize, or a
        // rough added, removed or re-sized, is a different judgement.
        if let Some(read) = super::sinker_pass::read_pass(world, op_name, setup, p) {
            h.write(b"sinker-pass");
            match read {
                Ok(r) => {
                    w(&mut h, r.undersize);
                    if let Some((name, u)) = r.rough {
                        h.write(name.as_bytes());
                        w(&mut h, u);
                    }
                }
                Err(e) => h.write(e.as_bytes()),
            }
        }
        // #546: and the condition's stated wear rate - a re-stated rate
        // is a different (compensated) burn. Nothing when unstated.
        if let Some(r) = super::sinker_wear_op::condition_wear(world, setup, p) {
            h.write(b"sinker-wear");
            w(&mut h, r);
        }
    }
    // #545: the side burn's gap too, behind its allowance law.
    if let OpParams::SinkerSideBurn(p) = params {
        h.write(b"sinker-side-gap");
        match super::sinker_side_op::side_gap(world, setup, p) {
            Ok(g) => w(&mut h, g),
            Err(e) => h.write(e.as_bytes()),
        }
    }
    params.stamp_with(world, setup, Some(src), &mut h);
    // #326: the setup's part alignment is a moved input for every op of
    // the setup (the parts it machines, verifies against or measures
    // heights from). Written only when one is set, so no stamp of an
    // as-modelled setup changes.
    if let Some(a) = crate::setup_align::alignment(world, setup) {
        h.write(b"part-alignment");
        for v in [a.pos.x, a.pos.y, a.pos.z, a.rot[0], a.rot[1], a.rot[2], a.rot[3]] {
            w(&mut h, v);
        }
    }
    // #320: a picked stock source is a moved input for every op of the
    // setup (the material it machines). Written only once picked, so no
    // stamp of an untouched setup changes.
    if let Some(src) = crate::setup::effective(world, setup).and_then(|s| crate::setup_stock::stored(world, &s)) {
        h.write(b"stock-source");
        h.write(src.word().as_bytes());
    }
    // Issue #62: a kind that turns about the spindle measures every
    // diameter from its work offset's origin, so moving that origin is
    // a moved input - the op goes Stale. Written only off the world
    // origin, so no digest of an op on an untouched offset changes.
    if super::spindle_law::turns(world, setup, params) {
        let (_, o) = crate::posting::wcs_context(world, setup);
        if o != [0.0; 3] {
            h.write(b"spindle-origin");
            for v in o {
                w(&mut h, v);
            }
        }
    }
    if super::wcs_yaw::op_applies(crate::machine::kind_of_setup(world, setup), params)
        && let Some(deg) = crate::posting::wcs_yaw(world, setup)
    {
        let (_, o) = crate::posting::wcs_context(world, setup);
        h.write(b"wcs-yaw");
        w(&mut h, deg);
        w(&mut h, o[0]);
        w(&mut h, o[1]);
    }
    match src {
        SrcRef::Stock => {
            h.write_u8(b'S');
            let stock_name = crate::setup::stock_entity_name(world, setup);
            let entity = model::find_by_name(world, &stock_name)?;
            super::stamp_stock::write_stock_source(world, &entity, &mut h)?;
        }
        SrcRef::Entity(name) => {
            h.write_u8(b'E');
            h.write(name.as_bytes());
            let entity = model::find_by_name(world, name)?;
            // #326: the pose in the setup's frame (as modelled: the
            // document `Location`, the bytes every older stamp read).
            let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
            for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]]
            {
                w(&mut h, v);
            }
            // M3.1 A: the revision signature, not count + bounds. Facet
            // count and AABB are coincidences waiting to happen - a solid
            // that keeps both while moving material (the diamond where the
            // cube was) must read Stale, and a re-tessellation of the same
            // faces must not. Face ids are local-frame content hashes, so
            // the transform hashed above carries all the placement.
            h.write_u64(model::geometry_signature(world, &entity)?);
        }
        SrcRef::Selection(items) => {
            // b'L', not b'S': the Stock arm already owns that byte, and a
            // digest that collides across source kinds is a staleness bug.
            h.write_u8(b'L');
            h.write_usize(items.len());
            let mut digested: Vec<&str> = Vec::new();
            for g in items {
                h.write_u8(match g.feature {
                    FeatureSel::Face(_) => 0,
                    FeatureSel::Chain(_) => 1,
                    FeatureSel::Patch(_) => 2,
                    FeatureSel::Vertex(_) => 3,
                });
                h.write_usize(g.entity.len());
                h.write(g.entity.as_bytes());
                let id = g.feature.id();
                h.write_u64(id);
                // The pick resolves against this entity's LOCAL features
                // (ids are transform-free) and projects through its
                // transform, so both belong in the digest. The face-id
                // signature - M3.1's ids-as-signatures law, the same one
                // the Entity arm reads - keeps a re-import whose faces
                // survive fresh (a re-tessellation cannot move a pick),
                // and any real change moves ids, so it goes stale. Face
                // ids are content hashes of their facet positions, so a
                // different mesh that repeats the facet count cannot hide.
                let entity = model::find_by_name(world, &g.entity)?;
                if !digested.contains(&g.entity.as_str()) {
                    digested.push(g.entity.as_str());
                    let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
                    for v in [
                        loc.pos.x,
                        loc.pos.y,
                        loc.pos.z,
                        loc.rot[0],
                        loc.rot[1],
                        loc.rot[2],
                        loc.rot[3],
                    ] {
                        w(&mut h, v);
                    }
                    h.write_u64(model::geometry_signature(world, &entity)?);
                }
            }
        }
    }
    // Issue #156 (generalising #77/#91's Drill term): every op whose
    // verdict reads the setup's stock beside its source - the rapid-in-
    // stock laws, the prior acquittal, the reach, R-plane and meridian
    // laws - digests that stock, so a stock edit after generation reads
    // Stale and the post refuses until it is regenerated. The set is
    // `stamp_stock::reads_setup_stock`'s.
    if super::stamp_stock::reads_setup_stock(src) {
        super::stamp_stock::write_setup_stock(world, setup, &mut h)?;
    }
    // M3.1 D re-machining: the program AHEAD is an input too - a 3D
    // pocket consumes what earlier operations leave standing, so when
    // their recorded PATHS or their order move, its cached path no
    // longer describes the same cut. With nothing ahead of it nothing
    // is written: first operations' and other kinds' stamps are
    // bit-for-bit what they were before stage D (the inert-data law).
    // Issue #89: an axial Drill's reach, R-plane and holder laws read
    // what the program ahead removed. Issue #109: so does every
    // unposed op's rapid law (prior acquittal, pilot and facing
    // credits) - each carries the digest (one-time Stale, by design).
    // #319: a posed op in a setup entered with a carried body judges the
    // stock that body spans, so it digests the same chain - only then.
    let digest = if super::drill_prior::reads_prior(params) {
        crate::rest::prior_digest(world, op_name, setup)
    } else {
        crate::rest::carried_digest(world, op_name, setup)
    };
    if let Some(digest) = digest {
        h.write_u8(b'R');
        h.write_u64(digest);
    }
    // M3.2 B what-the-program-clamps-to is deliberately NOT here.
    // Workholding - user fixtures and the machine-derived table and
    // spindle bands - rides the post's LIVE verification gate, exactly
    // like the M2.3 machine limits: "load the roomy one and the same
    // program still posts" is the user-facing law, and the gate
    // re-checks the steel here and now, so a moved vise can never post
    // silently without churning a single stamp. Generation still
    // reports fixture warnings early (the verify call below reads the
    // boxes live); staleness from workholding is the failure mode this
    // keeps out (see RECOMMENDED.md, the M2.3-law extension).
    Some(GenStamp(h.finish()))
}

/// Derived status of one entity: never stored, always recomputed from the
/// cache, the stamp and the live world (AGENTS.md: commands own fields).
#[must_use]
pub fn status_of(world: &World, entity: &flecs_ecs::prelude::Entity) -> OpStatus {
    let Some(op) = model::operation_of(world, entity) else {
        return OpStatus::Never;
    };
    let Some(cache) = model::toolpath_of(world, entity) else {
        return OpStatus::Never;
    };
    if cache.path.is_empty() {
        return OpStatus::Never; // a cache with no moves is no cache
    }
    let name = model::name_of(world, entity).unwrap_or_default();
    match (op.stamp, op_stamp(world, &op, &name)) {
        (Some(stored), Some(now)) if stored == now => OpStatus::Ok,
        _ => OpStatus::Stale,
    }
}

/// Why an operation reads Stale, phrased for a user who can act on it
/// (M3.1 C). The stamp is a single digest, so it cannot itself say what
/// moved; this asks the world the cheap live questions instead - the
/// ones whose answer is true independent of history: is the tool still
/// there, does each referenced entity still exist, does each picked
/// feature id still exist in its entity's live geometry, can every
/// height still resolve. When none of them fires, the honest answer is
/// the general one: something the generation read has changed. None
/// when the operation is not Stale.
#[must_use]
pub fn stale_reason(
    world: &flecs_ecs::prelude::World,
    entity: &flecs_ecs::prelude::Entity,
) -> Option<String> {
    if status_of(world, entity) != OpStatus::Stale {
        return None;
    }
    let op = model::operation_of(world, entity)?;
    match model::find_by_name(world, &op.tool) {
        None => return Some(format!("its tool '{}' no longer exists", op.tool)),
        Some(t) if !model::is_tool(world, &t) => {
            return Some(format!("'{}' is no longer a tool", op.tool));
        }
        _ => {}
    }
    if let OpParams::WireContour(w) = &op.params
        && let Err(why) = super::wire_op::condition_of(world, op.setup.as_deref(), &op.tool, w)
    {
        return Some(why);
    }
    match &op.src {
        SrcRef::Stock => {
            let stock_name = crate::setup::stock_entity_name(world, op.setup.as_deref());
            if model::find_by_name(world, &stock_name).is_none() {
                return Some("the stock no longer exists - set it up again".into());
            }
        }
        SrcRef::Entity(name) => {
            if model::find_by_name(world, name).is_none() {
                return Some(format!("its geometry source '{name}' was deleted"));
            }
        }
        SrcRef::Selection(items) => {
            for g in items {
                let Some(e) = model::find_by_name(world, &g.entity) else {
                    return Some(format!("'{}' was deleted", g.entity));
                };
                let Some(tris) = model::facets_of(world, &e) else {
                    return Some(format!("'{}' no longer carries geometry", g.entity));
                };
                let features = model::features_of_local(&tris);
                let (present, what) = match g.feature {
                    FeatureSel::Face(id) => (
                        revision::face_ids(&features).binary_search(&id).is_ok(),
                        "face",
                    ),
                    FeatureSel::Patch(id) => (
                        revision::patch_ids(&features).binary_search(&id).is_ok(),
                        "welded face",
                    ),
                    FeatureSel::Chain(id) => (
                        revision::chain_ids(&features).binary_search(&id).is_ok(),
                        "edge chain",
                    ),
                    FeatureSel::Vertex(id) => (
                        crate::geometry::feature::vertex_ids(&features).binary_search(&id).is_ok(),
                        "vertex",
                    ),
                };
                if !present {
                    return Some(format!(
                        "the picked {what} of '{}' no longer exists - the geometry moved",
                        g.entity
                    ));
                }
            }
        }
    }
    for h in collect_heights(&op.params) {
        if let Some(missing) =
            missing_source(world, op.setup.as_deref(), Some(&op.src), h.reference)
        {
            return Some(format!("a height is measured from {missing}"));
        }
    }
    if let Some(why) = area_stale_reason(world, &op) {
        return Some(why);
    }
    // M3.1 D: the program ahead is the LAST question, and reaching it
    // forces the answer: every own-input question above came back
    // clean while the stamp still mismatches, so a cutting
    // predecessor's recorded path is what moved - whatever its own
    // status reads.
    if super::drill_prior::reads_prior(&op.params)
        && let Some(name) = model::name_of(world, entity)
        && let Some(why) = crate::rest::prior_stale(world, &name, op.setup.as_deref())
    {
        return Some(why);
    }
    Some("the geometry or tool inputs changed since it was generated".into())
}
