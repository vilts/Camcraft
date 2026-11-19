//! camcraft-post - the post engine seam of the M4.1 crate split (R3,
//! docs/M4.1-CRATE-SPLIT.md). Scripts are plain JS (boa_engine, the
//! sandbox of `post::js`), the machine is a Value, every hook has a
//! PRELUDE default - the whole product contract of M1.7, minus any
//! knowledge of documents, entities or UI. The `.mch` TYPES and reader
//! (`machine::def`/`machine::parse`) joined here because the mirror in
//! `post::machine` restates them: one grammar, one home. The product
//! binary `post.exe` (src/bin) is this crate's, not the app's.
//!
//! The aliases below exist so the moved files' `crate::cl::` and
//! `crate::geometry::` paths held verbatim across the boundary; the
//! re-exported names are part of the seam (like the app's own
//! geometry/cl/tools re-exports), not an accident of the move.
//!
//! Provenance: RS-274 word layout, ISO 6983 tool addressing and the
//! observable shape of published post-processor documentation -
//! behavior only; every hook name, template and grammar key is ours
//! (assets/posts/AGENTS.md). No flecs, no schema, no world.

pub mod data_dir;
pub mod data_scan;
pub mod kinematics;
pub mod machine;
pub mod post;

pub use camcraft_cl::cl;
pub use camcraft_geometry::{geo2d, geometry};
