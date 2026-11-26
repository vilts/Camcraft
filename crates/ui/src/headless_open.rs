//! A file named on a headless start (#2648): `camcraft-ui --headless
//! part.sc` imports the model and `--headless job.camproj` opens the
//! project, through the same commands as the window (`launch_file`), in
//! place of the seeded proof document. The run then frames what it opened
//! and draws it. A file that does not open is a failure that names the
//! file; it is never ignored without a word.
//! Provenance: our own code over our own commands.

use std::path::Path;

use camcraft_app::model;

use crate::state::{CommandKind, State};

/// What a headless open landed: the state to draw, whether it has part
/// geometry to see, and the line the run says.
pub(crate) struct Opened {
    pub state: State,
    pub has_geometry: bool,
    pub said: String,
}

fn label(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Run `cmd` (from `launch_file::command`) on a new state, blocking until
/// the import or the open is done, then frame the document.
pub(crate) fn open(cmd: CommandKind) -> Result<Opened, String> {
    let (name, project) = match &cmd {
        CommandKind::ImportFile(p) => (label(p), false),
        CommandKind::OpenProjectFile(p) => (label(p), true),
        _ => return Err(String::from("headless open: not a file command")),
    };
    let mut state = State::new();
    state.pending.push_back(cmd);
    state.settle();
    let models: Vec<_> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| !model::is_stock(&state.world, e))
        .collect();
    let facets: usize = models
        .iter()
        .filter_map(|(_, e)| model::facets_of(&state.world, e))
        .map(|t| t.len())
        .sum();
    let opened = if project { state.status.starts_with("opened ") } else { facets > 0 };
    if !opened {
        return Err(format!("{name} did not open: {}", state.status));
    }
    state.pending.push_back(CommandKind::FrameAll);
    state.drain_actions();
    let said = format!("opened {name}: {} model(s), {facets} facets", models.len());
    Ok(Opened { state, has_geometry: facets > 0, said })
}
