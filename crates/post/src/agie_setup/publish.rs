//! Atomic publication of native inspection files, from CamCraft’s no-replace Agie bundle writer.

use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn publish(
    destination: &Path,
    files: &[(String, String)],
) -> Result<Vec<PathBuf>, String> {
    use std::{fs, io::Write};
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if destination.file_name().is_none() {
        return Err("Choose a new directory name for the Agie bundle".into());
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(format!(
                "Agie destination {} already exists; choose a new directory",
                destination.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "Cannot inspect Agie destination {}: {e}",
                destination.display()
            ));
        }
    }
    // The staging directory and the final name share a filesystem. The rename commits all
    // files together. Its no-replace mode also protects a destination created since the check.
    let stage = tempfile::Builder::new()
        .prefix(".camcraft-agie-")
        .tempdir_in(parent)
        .map_err(|e| format!("Cannot stage Agie job in {}: {e}", parent.display()))?;
    let mut paths = Vec::new();
    for (name, text) in files {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.path().join(name))
            .map_err(|e| format!("Cannot stage {name}: {e}"))?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Cannot write {name}: {e}"))?;
        paths.push(destination.join(name));
    }
    rename_new(stage.path(), destination)
        .map_err(|e| format!("Cannot publish Agie job to {}: {e}", destination.display()))?;
    // The temporary name ceased to exist at the rename. Disarm its path-based cleanup.
    let _old_name = stage.keep();
    Ok(paths)
}

#[cfg(any(target_vendor = "apple", target_os = "linux"))]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(Into::into)
}

#[cfg(windows)]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    fn wide(path: &Path) -> std::io::Result<Vec<u16>> {
        let mut chars: Vec<_> = path.as_os_str().encode_wide().collect();
        if chars.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "NUL in output path",
            ));
        }
        chars.push(0);
        Ok(chars)
    }
    let from = wide(from)?;
    let to = wide(to)?;
    // SAFETY: both buffers contain terminated UTF-16 and remain live for the call.
    // MoveFileW on the same volume fails if the destination already exists.
    if unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileW(from.as_ptr(), to.as_ptr()) }
        == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(
    target_arch = "wasm32",
    target_vendor = "apple",
    target_os = "linux",
    windows
)))]
fn rename_new(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic Agie bundle publication is unavailable on this platform",
    ))
}

#[cfg(target_arch = "wasm32")]
pub(super) fn publish(_: &Path, _: &[(String, String)]) -> Result<Vec<PathBuf>, String> {
    Err("Agie bundle output requires a local filesystem".into())
}
