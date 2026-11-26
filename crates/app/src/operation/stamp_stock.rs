//! The stock term of an operation's stamp (issue #156): which operations
//! read the setup's stock while generating and verifying, and the digest
//! of the stock they read (shape kind, size, placement, mesh).
//!
//! Provenance: our own staleness model (M3.1, `stamp.rs`), specced from
//! the Prime Directive - a verdict judged against a stock that has since
//! moved is a verdict nobody gave, so the op must read Stale and the
//! post refuse it. No vendor material.

use std::hash::Hasher;

use flecs_ecs::prelude::{Entity, World};

use super::SrcRef;
use crate::model;
use crate::stock::StockShape;

/// Whether an operation's generation reads the setup's stock beyond its
/// own source - the explicit set the stock term is written for.
///
/// The stock reaches the generators through ONE seam: `collect_job`
/// hands every kind whose source is not the stock itself the stock's
/// world facets and box. From there it feeds the clearance top
/// (`stock_top_z`), the stock region the reach and entry laws widen
/// into, the rapid-in-stock column (#96) and footprint rings (#106),
/// the prior acquittal, the plunge-solid law of the 3D and multi-axis
/// kinds, the cross-drill standoff (#77), the axial drill's reach and
/// R-plane laws (#91) and the lathe meridian's bar (`revolved_bar`).
/// Every kind passes that seam, so every kind with a non-stock source
/// reads it. A stock-sourced op digests the stock as its source
/// already (the `SrcRef::Stock` arm), so it needs no second term.
#[must_use]
pub fn reads_setup_stock(src: &SrcRef) -> bool {
    !matches!(src, SrcRef::Stock)
}

fn w(h: &mut impl Hasher, v: f64) {
    h.write(&v.to_bits().to_le_bytes());
}

fn write_placement(world: &World, entity: &Entity, h: &mut impl Hasher) {
    let loc = model::transform_of(world, entity).unwrap_or_default();
    for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]] {
        w(h, v);
    }
}

/// The stock as a SOURCE: placement, then the shape's own parameters,
/// and for stock from a model the copied mesh's signature. These bytes
/// are the pre-#156 `SrcRef::Stock` arm's, unchanged, so no stock-
/// sourced op churns. `None` when the entity carries no stock.
pub(super) fn write_stock_source(world: &World, entity: &Entity, h: &mut impl Hasher) -> Option<()> {
    write_placement(world, entity, h);
    match model::stock_def_of(world, entity)? {
        StockShape::Box { dx, dy, dz } => {
            h.write_u8(0);
            for v in [dx, dy, dz] {
                w(h, v);
            }
        }
        StockShape::Cylinder { diameter, height } => {
            h.write_u8(1);
            w(h, diameter);
            w(h, height);
        }
        StockShape::FromModel { source } => {
            h.write_u8(2);
            h.write(source.as_bytes());
            // M3.1 B: the name is provenance; what gets milled is the
            // stock's own copy, so the copy is what the digest reads.
            // UpdateModel re-derives the copy inside the update command
            // (the propagation law), so an update to the source lands
            // here as a moved signature - and a stock whose source was
            // deleted stays honestly Ok, because the material it
            // describes never changed. (M5.3 A: via the per-entity memo.)
            h.write_u64(model::geometry_signature(world, entity)?);
            crate::model_fidelity::write_stamp(world, entity, h);
        }
    }
    Some(())
}

/// The setup's stock as an op READS it beside its source (#77, #91,
/// generalised to every kind by #156): absent or present; placement;
/// the mesh signature, which the command layer mirrors from the shape
/// (so a box's size and a cylinder's diameter move it), and covers a
/// mesh stock's own copy; then the shape's kind and parameters, so the
/// term names the stock's definition, not only its tessellation.
/// `None` when a present stock has no geometry left to digest - an
/// unresolvable stamp, which reads Stale.
pub(crate) fn write_setup_stock(world: &World, setup: Option<&str>, h: &mut impl Hasher) -> Option<()> {
    h.write_u8(b'X');
    match model::find_by_name(world, &crate::setup::stock_entity_name(world, setup)) {
        None => h.write_u8(0),
        Some(e) => {
            h.write_u8(1);
            write_placement(world, &e, h);
            h.write_u64(model::geometry_signature(world, &e)?);
            crate::model_fidelity::write_stamp(world, &e, h);
            match model::stock_def_of(world, &e) {
                None => h.write_u8(0xff),
                Some(StockShape::Box { dx, dy, dz }) => {
                    h.write_u8(0);
                    for v in [dx, dy, dz] {
                        w(h, v);
                    }
                }
                Some(StockShape::Cylinder { diameter, height }) => {
                    h.write_u8(1);
                    w(h, diameter);
                    w(h, height);
                }
                Some(StockShape::FromModel { source }) => {
                    h.write_u8(2);
                    h.write(source.as_bytes());
                }
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_non_stock_source_reads_the_setup_stock() {
        assert!(!reads_setup_stock(&SrcRef::Stock));
        assert!(reads_setup_stock(&SrcRef::Entity("part".into())));
        assert!(reads_setup_stock(&SrcRef::Selection(Vec::new())));
    }
}
