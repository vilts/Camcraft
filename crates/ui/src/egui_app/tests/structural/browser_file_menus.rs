//! Browser file-menu structural checks for #1431.
//! Provenance: our own tests over our own shell; no vendor material.
//! Gate map (M4.4): covers ui::egui_app::theme_drop, ui::egui_app::fixture_menu.

use super::*;

/// #1431: unsupported browser menu rows expose their disabled state and reason.
#[test]
fn browser_theme_browse_is_disabled_with_a_reason() {
    let _g = snapshot_guard();
    for browser in [true, false] {
        let mut h = shell_harness();
        h.state_mut().state.browser_build = browser;
        h.state_mut().state.ribbon_theme_open = true;
        h.run_steps(3);
        let browse = h.get_by_label("Browse...");
        assert_eq!(browse.accesskit_node().is_disabled(), browser);
        let center = browse.rect().center();
        if browser {
            h.ctx.all_styles_mut(|s| { s.interaction.tooltip_delay = 0.0; s.interaction.show_tooltips_only_when_still = false; });
            h.hover_at(center);
            h.run_steps(4);
            assert!(painted(&h).iter().any(|(t, _)| t == crate::browser_files::NEEDS_PICKER), "painted: {:?}; center: {center:?}", painted(&h));
            button_at(&mut h, egui::PointerButton::Primary, center, true);
            button_at(&mut h, egui::PointerButton::Primary, center, false);
            assert!(h.state().state.ribbon_theme_open, "disabled Browse cannot select a row");
        }
    }
}

#[test]
fn browser_fixture_file_rows_are_disabled_with_reasons() {
    let _g = snapshot_guard();
    for browser in [true, false] {
        let mut h = shell_harness();
        h.state_mut().state.browser_build = browser;
        h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        h.run_steps(2);
        h.state_mut().state.tree_menu = Some(TreeMenuTarget::Fixtures("Setup1".into()));
        h.run_steps(3);
        for (label, reason) in [
            ("Import fixture…", crate::browser_files::NEEDS_PICKER),
            ("Save fixture to library…", crate::browser_files::NO_LIBRARY),
            ("Insert from library…", crate::browser_files::NO_LIBRARY),
        ] {
            let row = h.get_by_label(label);
            assert_eq!(row.accesskit_node().is_disabled(), browser, "{label}");
            let center = row.rect().center();
            if browser {
                h.ctx.all_styles_mut(|s| { s.interaction.tooltip_delay = 0.0; s.interaction.show_tooltips_only_when_still = false; });
                h.hover_at(center);
                h.run_steps(4);
                assert!(painted(&h).iter().any(|(t, _)| t == reason), "{label} explains its refusal");
                button_at(&mut h, egui::PointerButton::Primary, center, true);
                button_at(&mut h, egui::PointerButton::Primary, center, false);
                assert!(h.state().state.tree_menu.is_some(), "disabled row cannot choose a command");
            }
        }
        assert!(!h.get_by_label("Add built-in vise…").accesskit_node().is_disabled());
    }
}
