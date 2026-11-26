//! Native Classic Cut 2S job export, from our own implementation of the native-import format.

use std::path::{Path, PathBuf};

mod contour;
mod cutoff;
mod group;
mod round_profile;
mod spacing;
mod tangent_profile;
pub use crate::agie_setup::{MAX_PUNCH_WORKS, Quality};
pub use crate::agie_setup::valid_stem as valid_name;
pub use group::export_group_with_review;
pub use round_profile::export_round_with_review;

/// The retained side of a closed contour, or the kerf side of a directed open
/// cut. The control applies the wire offset toward the waste: outside a punch,
/// inside a die opening, left or right of an open line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgieSide {
    Punch,
    Die,
    Left,
    Right,
}

impl AgieSide {
    /// Whether the side is that of an open cut with two air endpoints.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// Nominal geometry in setup coordinates (millimetres, stock top at zero).
pub struct AgieJob {
    pub side: AgieSide,
    pub contour: Vec<[f64; 2]>,
    /// The predrilled start hole of a closed contour. `None` for an open cut.
    pub start: Option<[f64; 2]>,
    pub stock_min: [f64; 3],
    pub stock_max: [f64; 3],
}

/// Published setup files and the checks to perform on the imported work.
#[derive(Debug)]
pub struct ExportedJob {
    pub files: Vec<PathBuf>,
    pub controller_review: String,
}

/// Write a complete controller job to a new directory.
///
/// # Errors
/// Refuses unsupported geometry, settings and destinations without publishing a partial job.
pub fn export(
    job: &AgieJob,
    destination: &Path,
    program: &str,
    separation_length_mm: f64,
) -> Result<Vec<PathBuf>, String> {
    export_with_review(job, destination, program, separation_length_mm).map(|result| result.files)
}

/// Publish a setup-only punch, die opening or open cut and return the required operator checks.
///
/// # Errors
/// Returns the same validation and publication errors as [`export`].
pub fn export_with_review(
    job: &AgieJob,
    destination: &Path,
    program: &str,
    separation_length_mm: f64,
) -> Result<ExportedJob, String> {
    // An open cut has no separation section (#2962). A closed contour keeps
    // the 1 mm separation rule of #2984 unchanged.
    if job.side.is_open() {
        if separation_length_mm != 0.0 {
            return Err(
                "Set the separation cut length (separationLengthMm) to zero for an open cutoff"
                    .into(),
            );
        }
    } else if separation_length_mm != 1.0 {
        return Err("Set the separation cut length (separationLengthMm) to 1.0 mm for this closed contour. A zero or unspecified separation cannot pass the control formal check".into());
    }
    if !valid_name(program) {
        return Err("The job program name must be a non-reserved 1–8 character DOS name starting with a letter and using uppercase letters or digits".into());
    }
    // A closed contour returns to its first point. An open cut ends in air.
    let (points, setup) = if job.side.is_open() {
        let (cut, setup) = cutoff::prepare(job, program)?;
        (cut.line.to_vec(), setup.encode())
    } else {
        let (mut points, setup) = contour::prepare(job, program)?;
        let seam = *points.first().ok_or("The job requires a closed contour")?;
        points.push(seam);
        let setup = match job.side {
            AgieSide::Die => setup.encode_die(),
            _ => setup.encode(),
        };
        (points, setup)
    };
    let setup = setup.map_err(|error| error.to_string())?;
    let iso_name = format!("{program}.ISO");
    let files = [
        (iso_name.clone(), iso(&iso_name, &points)),
        (format!("{program}.SBR"), setup.sbr),
        (format!("{program}.SBL"), setup.sbl),
    ];
    Ok(ExportedJob {
        files: publish(destination, &files)?,
        controller_review: setup.controller_review,
    })
}

/// Publish one closed punch with the requested work quality (#2966).
/// [`Quality::Ccb030`] writes the bytes of [`export_with_review`].
/// [`Quality::Ccb035`] is qualified for one polygon punch on a 20 mm plate only:
/// a die, an open cut, a group and a DXF arc punch keep CCB030.
///
/// # Errors
/// The errors of [`export_with_review`], a side other than a punch, and a
/// thickness that the quality has no measured offsets for.
pub fn export_punch_with_review(
    job: &AgieJob,
    quality: Quality,
    destination: &Path,
    program: &str,
    separation_length_mm: f64,
) -> Result<ExportedJob, String> {
    if quality == Quality::Ccb030 {
        return export_with_review(job, destination, program, separation_length_mm);
    }
    if job.side != AgieSide::Punch {
        return Err(format!(
            "Quality {} is qualified for one closed punch only. Select AGIE_QN2 for a die opening or an open cutoff",
            quality.name()
        ));
    }
    if separation_length_mm != 1.0 {
        return Err("Set the separation cut length (separationLengthMm) to 1.0 mm for this closed contour. A zero or unspecified separation cannot pass the control formal check".into());
    }
    if !valid_name(program) {
        return Err("The job program name must be a non-reserved 1–8 character DOS name starting with a letter and using uppercase letters or digits".into());
    }
    let (mut points, setup, _) = contour::prepare_with_reach(job, program, quality)?;
    let seam = *points.first().ok_or("The job requires a closed contour")?;
    points.push(seam);
    let setup = setup
        .encode_quality(quality)
        .map_err(|error| error.to_string())?;
    let iso_name = format!("{program}.ISO");
    let files = [
        (iso_name.clone(), iso(&iso_name, &points)),
        (format!("{program}.SBR"), setup.sbr),
        (format!("{program}.SBL"), setup.sbl),
    ];
    Ok(ExportedJob {
        files: publish(destination, &files)?,
        controller_review: setup.controller_review,
    })
}

/// The nominal ISO geometry of one work, with CRLF line ends.
fn iso(iso_name: &str, points: &[[f64; 2]]) -> String {
    let mut iso = format!("( {iso_name} )\r\n");
    for (i, point) in points.iter().enumerate() {
        let motion = if i == 0 { "G00" } else { "G01" };
        iso.push_str(&format!(
            "N{:04} {motion} X{:.5} Y{:.5} ;\r\n",
            i + 1,
            point[0],
            point[1]
        ));
    }
    iso.push_str(&format!("N{:04} M02 ;\r\n", points.len() + 1));
    iso
}

#[cfg(not(target_arch = "wasm32"))]
fn publish(destination: &Path, files: &[(String, String)]) -> Result<Vec<PathBuf>, String> {
    use std::{fs, io::Write};
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if destination.file_name().is_none() {
        return Err("Choose a new directory name for the job bundle".into());
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(format!(
                "Job destination {} already exists; choose a new directory",
                destination.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "Cannot inspect job destination {}: {e}",
                destination.display()
            ));
        }
    }
    // The staging directory and the final name share a filesystem. The rename commits all
    // files together. Its no-replace mode also protects a destination created since the check.
    let stage = tempfile::Builder::new()
        .prefix(".camcraft-agie-")
        .tempdir_in(parent)
        .map_err(|e| format!("Cannot stage the job in {}: {e}", parent.display()))?;
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
        .map_err(|e| format!("Cannot publish the job to {}: {e}", destination.display()))?;
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
        "atomic job bundle publication is unavailable on this platform",
    ))
}

#[cfg(target_arch = "wasm32")]
fn publish(_: &Path, _: &[(String, String)]) -> Result<Vec<PathBuf>, String> {
    Err("Job bundle output requires a local filesystem".into())
}

#[cfg(test)]
mod cutoff_tests;
#[cfg(test)]
mod die_group_tests;
#[cfg(test)]
mod die_tests;
#[cfg(test)]
mod group_count_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod quality_tests;
#[cfg(test)]
mod reentrant_tests;
#[cfg(test)]
mod round_profile_tests;
#[cfg(test)]
mod tangent_profile_tests;
#[cfg(test)]
mod tests;
