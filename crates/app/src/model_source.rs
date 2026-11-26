//! A linked model's source (#2463): the file a model was imported from and
//! the SHA-256 of the bytes that were read. The UI checks the file while
//! the project is open; a new hash imports it again under the same name
//! (`UpdateModel`, one undo step, operations only Stale). Nothing here
//! regenerates or posts.
//! Provenance: our own ECS helper, the `step_source` pattern; the digest is
//! the kernel's FIPS 180-4 SHA-256.

use std::path::Path;

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::schema::ModelSource;

/// The largest SHA-256 hex text a file may hold for one source (64 chars).
const DIGEST_LEN: usize = 64;

/// The source recorded on `entity`, if the model is linked.
#[must_use]
pub fn of(world: &World, entity: &Entity) -> Option<ModelSource> {
    entity
        .entity_view(world)
        .try_get::<&ModelSource>(|s| s.clone())
        .filter(|s| !s.path.is_empty())
}

/// Link (`Some`) or unlink (`None`) `entity`.
pub fn set(world: &World, entity: &Entity, src: Option<&ModelSource>) {
    let view = entity.entity_view(world);
    match src.filter(|s| !s.path.is_empty()) {
        Some(s) => {
            view.set(s.clone());
        }
        None => {
            view.remove(world.component::<ModelSource>());
        }
    }
}

/// The source record for the bytes `data` read from `path`.
#[must_use]
pub fn of_bytes(path: &Path, data: &[u8]) -> Option<ModelSource> {
    digest(data).map(|sha256| ModelSource { path: path.to_string_lossy().into_owned(), sha256 })
}

/// Read `path` and give its source record. An unreadable file is an error
/// that names it. The file is hashed as it is read (#2689), never held
/// whole: a linked STL may be gigabytes.
#[cfg(not(target_arch = "wasm32"))]
pub fn read(path: &Path) -> Result<ModelSource, String> {
    use std::io::Read;
    let named = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut file = std::fs::File::open(path).map_err(named)?;
    let mut hash = camcraft_geometry::solidcraft::Sha256::new();
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(named)?;
        match buf.get(..n) {
            Some([]) | None => break,
            Some(got) => hash.update(got),
        }
    }
    Ok(ModelSource { path: path.to_string_lossy().into_owned(), sha256: hash.finish_hex() })
}

/// A browser has no file to read again.
#[cfg(target_arch = "wasm32")]
pub fn read(path: &Path) -> Result<ModelSource, String> {
    Err(format!("{}: no SHA-256 in this build", path.display()))
}

/// How much of a source file is read at a time for its digest.
#[cfg(not(target_arch = "wasm32"))]
const READ_CHUNK: usize = 1 << 20;

/// A recorded digest is lowercase hex of the right length; anything else in
/// a project file is corrupt.
#[must_use]
pub fn is_digest(text: &str) -> bool {
    text.len() == DIGEST_LEN && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(not(target_arch = "wasm32"))]
fn digest(data: &[u8]) -> Option<String> {
    Some(camcraft_geometry::solidcraft::sha256_hex(data))
}

/// A browser has no file to check again, so it records no link.
#[cfg(target_arch = "wasm32")]
fn digest(_data: &[u8]) -> Option<String> {
    None
}
