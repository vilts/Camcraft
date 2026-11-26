//! Drop machine (`.mch`) and post (`.cps`) files on the window (#2950).
//! A drop that holds one of them is taken here, not by the model drop
//! (`file_drop`). In the browser the files arrive as a bare name and bytes:
//! one file is kept for the session and mounted (a machine) or chosen (a
//! post), through the same commands Load machine and Load post run; several
//! files only join the session's machine and post lists, and each one that
//! does not parse is refused by name on one status line (`state_picked`).
//! On the desktop a dropped file is a path on disk and goes straight to
//! `LoadMachineFile` / `SelectPostFile`, one file at a time. A bad file
//! never changes the project.
//! Provenance: our own routing over egui's public `RawInput` drop fields
//! and the app's machine and post laws; no HSMWorks material.

use std::path::Path;

use egui::Ui;

use crate::file_pick::{self, PickFor, PickedFile};
use crate::state::State;

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// What one dropped file is for: its own kind, mounted or chosen.
fn purpose_of(name: &str) -> PickFor {
    let machine = Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("mch"));
    if machine { PickFor::Machine } else { PickFor::Post }
}

/// Once a frame, before the model drop: take a drop that holds a machine
/// or post file. `false` leaves the drop to `file_drop`.
pub(super) fn take(ui: &Ui, state: &mut State) -> bool {
    let files = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if !files.iter().any(|f| file_pick::is_setup_file(&name_of(f.path()))) {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if files.iter().all(|f| f.path().is_file()) {
        let [one] = files.as_slice() else {
            state.refuse_drop(format!(
                "drop refused: {} files were dropped; on the desktop drop one machine or post file at a time",
                files.len()
            ));
            return true;
        };
        let path = one.path().to_path_buf();
        let action = match purpose_of(&name_of(&path)) {
            PickFor::Machine => crate::state::CommandKind::LoadMachineFile(path),
            _ => crate::state::CommandKind::SelectPostFile(path),
        };
        state.pending.push_back(action);
        return true;
    }
    let purpose = match files.as_slice() {
        [one] => purpose_of(&name_of(one.path())),
        _ => PickFor::Folder,
    };
    state.status = format!("reading {} dropped file{}…", files.len(), if files.len() == 1 { "" } else { "s" });
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut budget = file_pick::ReadBudget::default();
        for file in &files {
            let name = name_of(file.path());
            let bytes = within_limit(&name, file.bytes()).and_then(|data| {
                budget.reserve(&name, data.len() as f64)?;
                Ok(data)
            });
            file_pick::deliver(PickedFile { purpose, name, bytes });
        }
        ui.ctx().request_repaint();
    }
    #[cfg(target_arch = "wasm32")]
    {
        let ctx = ui.ctx().clone();
        wasm_bindgen_futures::spawn_local(async move {
            // Read in turn, hand over together: one frame lands them all.
            let mut budget = file_pick::ReadBudget::default();
            let plan: Vec<_> = files.iter().map(|file| {
                let name = name_of(file.path());
                let allowed = file.web_file()
                    .ok_or_else(|| format!("{name}: the browser did not supply a file size"))
                    .and_then(|wf| budget.reserve(&name, wf.size()));
                (file, name, allowed)
            }).collect();
            let mut read = Vec::with_capacity(plan.len());
            for (file, name, allowed) in plan {
                let bytes = match allowed {
                    Err(why) => Err(why),
                    Ok(()) => within_limit(&name, file.bytes_async().await),
                };
                if let Err(why) = &bytes {
                    diag_warn!("file drop: {name}: {why}");
                }
                read.push(PickedFile { purpose, name, bytes });
            }
            for file in read {
                file_pick::deliver(file);
            }
            ctx.request_repaint();
        });
    }
    true
}

fn within_limit(name: &str, bytes: Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
    bytes.and_then(|data| match file_pick::too_big(name, data.len() as f64) {
        Some(why) => Err(why),
        None => Ok(data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_file_is_for_its_own_kind_in_any_case() {
        assert_eq!(purpose_of("a.MCH"), PickFor::Machine);
        assert_eq!(purpose_of("b.cps"), PickFor::Post);
        assert!(file_pick::is_setup_file("C.Cps") && file_pick::is_setup_file("d.mch"));
        assert!(!file_pick::is_setup_file("e.stl") && !file_pick::is_setup_file("mch"));
    }
}
