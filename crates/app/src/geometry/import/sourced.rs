//! A file import that keeps the SHA-256 of the bytes it parsed (#2689).
//! The file is read once: a buffered format hashes the buffer it parses,
//! and STL hashes its stream as the streaming parser reads it (the STL
//! law: the file never coexists with its triangles). So a file that
//! changes during the import records the digest of the bytes that made
//! the mesh, and the link check imports the newer file next.
//! Native only: the browser has no file to check again.
//! Provenance: our own code over our importers and the kernel's FIPS
//! 180-4 SHA-256.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use camcraft_geometry::solidcraft::Sha256;

use super::{format_from_path, gate, mesh_outward, parse_model_open_in, stl, Format, Graded, ImportError, MAX_FILE_BYTES};

/// A file that hashes every byte it hands out. A rewind to the start
/// hashes from the start again (the ASCII STL path reads the head twice);
/// any other seek is refused, so the digest is always of one pass.
struct Hashing {
    file: File,
    hash: Sha256,
}

impl Read for Hashing {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.file.read(buf)?;
        if let Some(got) = buf.get(..n) {
            self.hash.update(got);
        }
        Ok(n)
    }
}

impl Seek for Hashing {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        if pos != SeekFrom::Start(0) {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "a hashed import reads in one pass"));
        }
        let at = self.file.seek(pos)?;
        self.hash = Sha256::new();
        Ok(at)
    }
}

/// [`super::load_model_open_within`], with the SHA-256 of the bytes the
/// parse used, as lowercase hex. `units` is the caller's length unit for a
/// STEP file that states none (#1874).
pub fn load_model_open_sourced(path: &Path, budget: usize, units: Option<crate::units::Unit>) -> Result<(Graded, String), ImportError> {
    let format = format_from_path(path)?;
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge);
    }
    if format == Format::Stl {
        let mut src = Hashing { file, hash: Sha256::new() };
        let parsed = stl::parse_stream(BufReader::with_capacity(stl::READ_BUF_BYTES, &mut src), Some(size));
        let mut mesh = gate(parsed?)?;
        // Bytes after the last facet, if the file grew: still the one pass.
        io::copy(&mut src, &mut io::sink())?;
        let notes = mesh_outward::turn_outward(&mut mesh)?.into_iter().collect();
        let graded = Graded { mesh, coarse_mm: None, file_tolerance: false, notes, open: None, step_source: None };
        return Ok((graded, src.hash.finish_hex()));
    }
    let mut data = Vec::new();
    file.take(MAX_FILE_BYTES.saturating_add(1)).read_to_end(&mut data)?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge);
    }
    let digest = camcraft_geometry::solidcraft::sha256_hex(&data);
    Ok((parse_model_open_in(&data, format, budget, units)?, digest))
}
