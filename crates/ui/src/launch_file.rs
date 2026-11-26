//! A file named on the command line (#2462): `camcraft-ui part.sc` imports
//! the model and `camcraft-ui job.camproj` opens the project, through the
//! same commands as the Model and Project dialogs and a drop
//! (`CommandKind::ImportFile`, `CommandKind::OpenProjectFile`). So the
//! worker, the update-or-spawn decision and the refusals stay shared. A
//! second file, an extension that CamCraft does not read, or a path that is
//! not a file is refused by name; the app then starts with an empty project.
//! Provenance: our own code over our own commands and option table; no
//! HSMWorks material.

use std::path::{Path, PathBuf};

use crate::state::{CommandKind, State, MODEL_EXTENSIONS};

/// The extension of a project file, lower case.
const PROJECT_EXTENSION: &str = "camproj";

fn label(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The command for the file arguments: `None` when there is no file, or the
/// refusal that names why the file cannot be opened.
pub fn command(files: &[String]) -> Result<Option<CommandKind>, String> {
    let path = match files {
        [] => return Ok(None),
        [one] => PathBuf::from(one),
        more => {
            return Err(format!(
                "open refused: {} files were given on the command line; give one model or project file",
                more.len()
            ));
        }
    };
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let project = ext == PROJECT_EXTENSION;
    if !project && !MODEL_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "open refused: {} is not a model or project file (read: .{}, .{PROJECT_EXTENSION})",
            label(&path),
            MODEL_EXTENSIONS.join(", .")
        ));
    }
    if !path.is_file() {
        return Err(format!("open refused: {} is not a readable file", label(&path)));
    }
    Ok(Some(if project { CommandKind::OpenProjectFile(path) } else { CommandKind::ImportFile(path) }))
}

/// Queue the command for the file arguments on a new `state`, or put the
/// refusal on its status line. Called once, at start. `read_only`
/// (`--read-only`, #3609) opens a project read-only; a model import makes
/// a new project with no file, so the flag changes nothing there.
pub fn queue(state: &mut State, files: &[String], read_only: bool) {
    match command(files) {
        Ok(Some(CommandKind::OpenProjectFile(path))) if read_only => {
            state.pending.push_back(CommandKind::OpenProjectReadOnly(path));
        }
        Ok(Some(cmd)) => state.pending.push_back(cmd),
        Ok(None) => {}
        Err(msg) => state.refuse_drop(msg),
    }
}
