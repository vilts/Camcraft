//! Platform key names for the shortcut hints (tips, hover text, status
//! lines). Provenance: egui's `Modifiers::command` (Cmd on macOS, Ctrl
//! elsewhere) is the key the keyboard layer binds, so the hints name the
//! same key; clean-room, no third-party text.

/// The key the platform's command shortcuts use: `Cmd` on macOS, `Ctrl`
/// elsewhere. This is the one place that decides the word.
pub const fn command_key() -> &'static str {
    command_key_for(cfg!(target_os = "macos"))
}

/// The command key's name for a platform, pure so both answers are
/// testable on any host: `Cmd` when `macos`, `Ctrl` otherwise.
pub const fn command_key_for(macos: bool) -> &'static str {
    if macos { "Cmd" } else { "Ctrl" }
}

/// A command shortcut written for this platform: `chord("Shift+O")` is
/// `Cmd+Shift+O` on macOS and `Ctrl+Shift+O` elsewhere.
pub fn chord(rest: &str) -> String {
    format!("{}+{rest}", command_key())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_key_names_the_platform_key() {
        #[cfg(target_os = "macos")]
        assert_eq!(command_key(), "Cmd");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(command_key(), "Ctrl");
    }

    #[test]
    fn command_key_for_names_both_platforms() {
        assert_eq!(command_key_for(true), "Cmd");
        assert_eq!(command_key_for(false), "Ctrl");
    }

    #[test]
    fn chord_prefixes_the_platform_key() {
        assert_eq!(chord("Z"), format!("{}+Z", command_key()));
        assert_eq!(chord("Shift+O"), format!("{}+Shift+O", command_key()));
    }

    /// #1293 review: the pick hints name the key the picks read
    /// (`mods.command`), so they follow `command_key()` and never carry a
    /// literal Ctrl on macOS.
    #[test]
    fn pick_hints_name_the_command_key() {
        let key = command_key();
        assert!(crate::egui_app::edge_hint().starts_with(&format!("{key}+click adds")));
        assert!(crate::egui_app::click_selects_hint().contains(&format!("{key} adds")));
        assert!(crate::egui_app::boundary_hints()[2].contains(&format!("{key}-click to add")));
    }
}
