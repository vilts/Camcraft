//! #31: a model imported from bytes in hand (what the browser's file
//! picker hands over: a name and the file's contents) lands exactly like
//! the same file opened from disk, and every refusal the path import makes
//! is made here too, by name - never a panic.
//!
//! Gate map (M4.4): covers app::commands::import_model, app::geometry.

use std::path::{Path, PathBuf};

use camcraft_app::commands::{CommandError, ImportModel};
use camcraft_app::geometry::import::ImportError;

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(testdata(name)).expect("test model present")
}

#[test]
fn picked_bytes_import_like_the_same_file_from_disk() {
    for file in ["cube.stl", "box.obj", "box.step"] {
        let from_disk = ImportModel::new(&testdata(file)).expect("disk import");
        let picked = ImportModel::from_bytes(file, &read(file), None).expect("bytes import");
        assert_eq!(picked.name, from_disk.name, "{file}: named by its stem");
        assert_eq!(picked.mesh, from_disk.mesh, "{file}: the same mesh");
        assert_eq!(picked.coarse_mm, from_disk.coarse_mm, "{file}: the same fidelity");
        assert_eq!(picked.facet_budget, None);
    }
    let cube = ImportModel::from_bytes("cube.stl", &read("cube.stl"), None).expect("cube");
    assert_eq!(cube.name, "cube");
    assert_eq!(cube.mesh.len(), 12);
}

#[test]
fn picked_bytes_are_refused_by_name_never_a_panic() {
    // IGES: refused by extension, before any parsing.
    let iges = ImportModel::from_bytes("part.igs", b"anything", None).err();
    assert!(matches!(iges, Some(CommandError::UnsupportedType(ref t)) if t == "IGS"), "{iges:?}");
    // An extension no importer reads.
    let txt = ImportModel::from_bytes("notes.txt", b"hello", None).err();
    assert!(matches!(txt, Some(CommandError::Import(ImportError::Unsupported(_)))), "{txt:?}");
    // No extension at all (a browser can hand over any name).
    let bare = ImportModel::from_bytes("model", b"solid x", None).err();
    assert!(matches!(bare, Some(CommandError::Import(ImportError::Unsupported(_)))), "{bare:?}");
    // An empty and a corrupt file of each format.
    for name in ["e.stl", "e.obj", "e.step"] {
        assert!(ImportModel::from_bytes(name, b"", None).is_err(), "empty {name} must be refused");
        assert!(
            ImportModel::from_bytes(name, &[0xff; 97], None).is_err(),
            "corrupt {name} must be refused"
        );
    }
    // A STEP facet budget given for a mesh format is refused, as on disk.
    let budget = ImportModel::from_bytes("cube.stl", &read("cube.stl"), Some(100_000)).err();
    assert!(matches!(budget, Some(CommandError::Invalid(_))), "{budget:?}");
}
