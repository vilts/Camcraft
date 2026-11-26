//! How the first window appears: the default shows and focuses it; `--no-activate`
//! shows it without taking the focus; `--minimized` opens it minimized, so it
//! covers no other window. Provenance: our own code over the public winit
//! `WindowAttributes::with_active` / `with_visible` and `Window::set_minimized`
//! docs; no HSMWorks material.

use winit::window::{Window, WindowAttributes};

use crate::cli_options::StartMode;

/// The window attributes for `mode`. `--no-activate` makes winit show the
/// window with `SW_SHOWNOACTIVATE` on Windows.
pub(super) fn attributes(mode: StartMode, attrs: WindowAttributes) -> WindowAttributes {
    attrs.with_active(!mode.no_activate)
}

/// What happens to a window just created with [`attributes`].
pub(super) fn show(mode: StartMode, window: &Window) {
    if mode.minimized {
        window.set_minimized(true);
    } else if !mode.no_activate {
        window.focus_window();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_start_activates_and_shows() {
        let a = attributes(StartMode::default(), WindowAttributes::default());
        assert!(a.active && a.visible);
    }

    #[test]
    fn no_activate_shows_without_the_focus() {
        let mode = StartMode { minimized: false, no_activate: true };
        let a = attributes(mode, WindowAttributes::default());
        assert!(!a.active && a.visible);
    }

    #[test]
    fn minimized_creates_an_inactive_window() {
        let mode = StartMode { minimized: true, no_activate: true };
        let a = attributes(mode, WindowAttributes::default());
        assert!(!a.active);
    }
}
