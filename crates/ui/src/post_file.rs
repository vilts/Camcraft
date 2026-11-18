//! #258 (part 1): where the Post dialog opens and the file it proposes.
//!
//! The dialog used to open with no folder and `program.nc` every time,
//! so each post meant walking back to the job's folder and renaming.
//! It now remembers this document's last post - the G-code viewer
//! keeps that path, and a document switch retires it - and otherwise
//! starts beside the saved project, named after it.
//! Provenance: our own UI law; no vendor material consulted.

use std::path::{Path, PathBuf};

/// The name Post proposes when the document has neither posted nor been
/// saved.
pub const DEFAULT_PROGRAM: &str = "program.nc";
/// The extension every shipped post writes (each `.cps` states `nc`).
const PROGRAM_EXT: &str = "nc";

/// Where the Post dialog starts, and the file name it fills in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostTarget {
    /// The folder the dialog opens in; `None` leaves it to the OS.
    pub dir: Option<PathBuf>,
    pub file_name: String,
}

/// The dialog's start for a document whose last successful post wrote
/// `last_post` and whose project file is `project` (either may be
/// unknown). The last post wins both folder and name, so posting again
/// proposes the same file; otherwise the project's folder and
/// `<project name>.nc`; otherwise no folder and [`DEFAULT_PROGRAM`].
#[must_use]
pub fn post_target(last_post: Option<&Path>, project: Option<&Path>) -> PostTarget {
    let dir = last_post
        .and_then(Path::parent)
        .or_else(|| project.and_then(Path::parent))
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf);
    let file_name = last_post
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .or_else(|| {
            project
                .and_then(Path::file_stem)
                .and_then(|s| s.to_str())
                .map(|stem| format!("{stem}.{PROGRAM_EXT}"))
        })
        .unwrap_or_else(|| DEFAULT_PROGRAM.to_owned());
    PostTarget { dir, file_name }
}

/// [`post_target`] for `state`'s document: its last successful post
/// (the G-code viewer's file) and its saved project file.
#[must_use]
pub fn for_document(state: &crate::state::State) -> PostTarget {
    post_target(
        state.post_view.as_ref().map(|v| v.path.as_path()),
        state.project_path.as_deref(),
    )
}

/// The active post's preferred file extension without a leading dot (#258).
#[must_use]
pub fn post_extension(state: &crate::state::State) -> String {
    camcraft_app::posting::script_source(&state.world)
        .ok()
        .and_then(|(_, cps)| camcraft_app::post::engine::extension_of(&cps).ok())
        .map(|ext| ext.trim_start_matches('.').to_owned())
        .filter(|ext| !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| String::from("nc"))
}

/// Initial folder and file name for the in-app Post dialog (#258):
/// prefers remembered dialog folder/name, then `for_document` dir and project/dialect extension.
#[must_use]
pub fn dialog_target(state: &crate::state::State) -> PostTarget {
    let base = for_document(state);
    let ext = post_extension(state);
    let dir = state.post_last_dir.clone().or(base.dir);
    let file_name = if let Some(name) = state.post_last_name.clone().filter(|n| !n.trim().is_empty()) {
        name
    } else if state.post_view.is_some() {
        base.file_name
    } else if let Some(stem) = state
        .project_path
        .as_deref()
        .and_then(Path::file_stem)
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
    {
        format!("{stem}.{ext}")
    } else {
        format!("program.{ext}")
    };
    PostTarget { dir, file_name }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_new_unsaved_document_proposes_program_nc_and_no_folder() {
        assert_eq!(
            post_target(None, None),
            PostTarget {
                dir: None,
                file_name: DEFAULT_PROGRAM.to_owned()
            }
        );
    }

    #[test]
    fn a_saved_project_proposes_its_own_name_in_its_folder() {
        let project = p("jobs/bracket/bracket-v2.camproj");
        assert_eq!(
            post_target(None, Some(&project)),
            PostTarget {
                dir: Some(p("jobs/bracket")),
                file_name: "bracket-v2.nc".to_owned()
            }
        );
    }

    #[test]
    fn the_last_post_sets_both_the_folder_and_the_name() {
        let project = p("jobs/bracket/bracket-v2.camproj");
        let last = p("shop/out/op10.nc");
        assert_eq!(
            post_target(Some(&last), Some(&project)),
            PostTarget {
                dir: Some(p("shop/out")),
                file_name: "op10.nc".to_owned()
            }
        );
        assert_eq!(
            post_target(Some(&last), None).dir,
            Some(p("shop/out")),
            "an unsaved document still remembers where it posted"
        );
    }

    #[test]
    fn a_documents_project_file_and_last_post_feed_the_dialog() {
        let mut state = crate::state::State::new();
        assert_eq!(
            for_document(&state).file_name,
            DEFAULT_PROGRAM,
            "a fresh document"
        );
        state.project_path = Some(p("jobs/bracket/bracket-v2.camproj"));
        assert_eq!(
            for_document(&state).file_name,
            "bracket-v2.nc",
            "the saved project names it"
        );
        state.post_view = Some(crate::state::PostView {
            path: p("shop/out/op10.nc"),
            program: String::new(),
            line_pos: Vec::new(),
            open: false,
            line: 0,
            blocks: Vec::new(),
            trace: false,
            not_judged: None,
        });
        assert_eq!(
            for_document(&state),
            PostTarget {
                dir: Some(p("shop/out")),
                file_name: "op10.nc".to_owned()
            },
            "the viewer's file is the last post, even with its window closed"
        );
    }

    #[test]
    fn a_bare_file_name_opens_no_folder() {
        // A relative path with no parent (a test's `PostProgramFile`)
        // has an empty parent: no folder rather than "".
        assert_eq!(post_target(Some(&p("p.nc")), None).dir, None);
        assert_eq!(post_target(None, Some(&p("job.camproj"))).dir, None);
    }
}
