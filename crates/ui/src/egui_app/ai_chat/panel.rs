//! Native egui layout for the `Tab::AiChat` dock pane (#1946).
//! Provenance: our own egui layout using CamCraft tokens and colorvision text inks; no vendor material involved.

use camcraft_app::mcp::provider::FallbackMode;
use egui::{RichText, ScrollArea, Ui};

use crate::state::State;

use super::super::{colorvision, theme};
use super::turn::{ChatTurn, TurnStatus};
use super::worker::{ConfirmRequest, ToolCardStatus};

/// Render the AI Chat panel inside its dock leaf.
pub(crate) fn draw(ui: &mut Ui, state: &mut State) {
    let pal = theme::current();
    let inks = colorvision::text_inks(ui, &state.world);
    let mut actions = PanelActions::default();

    egui::Frame::NONE
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            draw_header(ui, state, &pal);
            draw_banners(ui, state, inks);
            ui.add_space(4.0);

            if let Some(req) = state.ai_chat.pending_confirm().cloned() {
                draw_confirm_banner(ui, &req, inks, &pal, &mut actions);
                ui.add_space(4.0);
            }

            let history = &state.history;
            let avail_h = (ui.available_height() - 88.0).max(80.0);
            ScrollArea::vertical()
                .id_salt("ai_chat_transcript")
                .max_height(avail_h)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    draw_transcript(ui, state.ai_chat.turns(), history, inks, &pal, &mut actions);
                });

            ui.add_space(6.0);
            draw_composer(ui, state, &mut actions);
        });

    apply_panel_actions(state, actions);
}

#[derive(Default)]
struct PanelActions {
    submit: bool,
    cancel: bool,
    confirm_choice: Option<bool>,
    undo_turn_id: Option<u64>,
}

fn draw_header(ui: &mut Ui, state: &mut State, pal: &theme::Tokens) {
    let selected_idx = state.ai_chat.selected_preset_index();
    let current_label = state.ai_chat.selected_preset().label.clone();
    let mut next_preset = None;
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Provider")
            .selected_text(current_label)
            .width(185.0)
            .show_ui(ui, |ui| {
                for (i, preset) in state.ai_chat.presets().iter().enumerate() {
                    if ui.selectable_label(i == selected_idx, &preset.label).clicked() {
                        next_preset = Some(i);
                    }
                }
            });
    });
    if let Some(idx) = next_preset {
        state.ai_chat.select_preset(idx);
    }

    let model_hint = match state.ai_chat.selected_preset().endpoint_kind {
        super::EndpointKind::Local => "Automatic: installed tool-capable model",
        super::EndpointKind::Cloud => camcraft_app::ai::presets::default_model(
            state.ai_chat.selected_preset().provider,
        ),
    };
    ui.horizontal(|ui| {
        let model_lbl = ui.label("Model");
        ui.add(
            egui::TextEdit::singleline(&mut state.ai_chat.model_input)
                .hint_text(model_hint)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(model_lbl.id);
    });
    if state.ai_chat.selected_preset().endpoint_kind == super::EndpointKind::Local {
        ui.hyperlink_to("Get Ollama models", "https://ollama.com/library");
    }

    ui.horizontal_wrapped(|ui| {
        // #1945: the switch is the person's policy - kept in prefs.json.
        if ui.checkbox(&mut state.ai_chat.offline_only, "Offline only").changed() {
            state.persist_prefs();
        }
        ui.checkbox(&mut state.ai_chat.attach_selection, "Attach selection");
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut state.ai_chat.attach_screenshot, "Attach screenshot"),
        );
    });

    let preset = state.ai_chat.selected_preset();
    let preset_id = preset.id.clone();
    // Test reads the Model field like Send: trimmed, and empty uses the default.
    let model =
        super::settings_text::resolve_model(&state.ai_chat.model_input, &preset.default_model);
    super::settings_panel::draw(
        ui,
        &mut state.ai_chat.settings,
        &preset_id,
        &model,
        state.ai_chat.offline_only,
    );

    ui.label("Viewport screenshot capture is unavailable.");

    let preset = state.ai_chat.selected_preset();
    ui.label(
        RichText::new(format!(
            "Mode: {} · Endpoint: {}",
            mode_label(preset.fallback_mode),
            match preset.endpoint_kind {
                super::EndpointKind::Cloud => "Cloud",
                super::EndpointKind::Local => "Local",
            }
        ))
        .small()
        .color(pal.text_dim),
    );
}

fn mode_label(mode: FallbackMode) -> &'static str {
    match mode {
        FallbackMode::Native => "Native tools",
        FallbackMode::PromptJson => "Prompt JSON (confirm every edit)",
        FallbackMode::ReadOnly => "Read-only Q&A",
    }
}

fn draw_banners(ui: &mut Ui, state: &State, inks: colorvision::TextInks) {
    if state.ai_chat.selected_preset().fallback_mode == FallbackMode::ReadOnly {
        ui.add_space(4.0);
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.0, inks.warn))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Read-only Q&A mode: tool execution is disabled.")
                        .color(inks.warn)
                        .strong(),
                );
            });
    }
    if let Some(err) = state.ai_chat.last_error() {
        ui.add_space(4.0);
        ui.label(RichText::new(err).color(inks.error));
    }
}

fn draw_confirm_banner(
    ui: &mut Ui,
    req: &ConfirmRequest,
    inks: colorvision::TextInks,
    pal: &theme::Tokens,
    actions: &mut PanelActions,
) {
    egui::Frame::group(ui.style())
        .fill(pal.well)
        .stroke(egui::Stroke::new(1.5, inks.warn))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Confirmation required")
                    .color(inks.warn)
                    .strong(),
            );
            ui.label(format!("Tool: {}", req.tool_name));
            if let Ok(args) = serde_json::to_string_pretty(&req.arguments) {
                ui.label(RichText::new(args).monospace().small().color(pal.text_dim));
            }
            ui.horizontal(|ui| {
                if ui.button("Allow").clicked() {
                    actions.confirm_choice = Some(true);
                }
                if ui.button("Deny").clicked() {
                    actions.confirm_choice = Some(false);
                }
            });
        });
}

fn draw_transcript(
    ui: &mut Ui,
    turns: &[ChatTurn],
    history: &camcraft_app::commands::History,
    inks: colorvision::TextInks,
    pal: &theme::Tokens,
    actions: &mut PanelActions,
) {
    if turns.is_empty() {
        ui.label(
            RichText::new("Ask a question or propose CAM edits.")
                .color(pal.text_dim)
                .small(),
        );
        return;
    }
    for turn in turns {
        egui::Frame::group(ui.style())
            .fill(pal.ink_alt)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.label(RichText::new("You").strong().small().color(pal.text_dim));
                ui.label(&turn.prompt);

                ui.add_space(4.0);
                for card in &turn.tool_cards {
                    let status_color = match card.status {
                        ToolCardStatus::Succeeded => pal.accent,
                        ToolCardStatus::AwaitingConfirmation => inks.warn,
                        ToolCardStatus::Failed | ToolCardStatus::Denied => inks.error,
                        ToolCardStatus::Pending | ToolCardStatus::Running => pal.text_dim,
                    };
                    egui::Frame::group(ui.style())
                        .fill(pal.well)
                        .inner_margin(egui::Margin::same(4))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&card.name).strong());
                                ui.label(RichText::new(card.status.label()).color(status_color));
                            });
                            if !card.summary.is_empty() {
                                ui.label(RichText::new(&card.summary).small().color(pal.text_dim));
                            }
                        });
                }

                if !turn.assistant_text.is_empty() {
                    ui.add_space(3.0);
                    ui.label(&turn.assistant_text);
                }

                match &turn.status {
                    TurnStatus::Running => {
                        ui.label(RichText::new("Working...").small().color(pal.accent));
                    }
                    TurnStatus::Cancelled => {
                        ui.label(RichText::new("Cancelled").small().color(inks.warn));
                    }
                    TurnStatus::Failed(err) => {
                        ui.label(RichText::new(format!("Failed: {err}")).small().color(inks.error));
                    }
                    TurnStatus::Completed => {}
                }

                if !matches!(turn.status, TurnStatus::Running) && turn.change_count > 0 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let note = if turn.undone {
                            format!("Undone ({} changes reverted)", turn.change_count)
                        } else {
                            format!("{} changes in this turn", turn.change_count)
                        };
                        ui.label(RichText::new(note).small().color(pal.text_dim));
                        if ui
                            .add_enabled(turn.can_undo(history), egui::Button::new("Undo Turn"))
                            .clicked()
                        {
                            actions.undo_turn_id = Some(turn.id);
                        }
                    });
                }
            });
        ui.add_space(4.0);
    }
}

fn draw_composer(ui: &mut Ui, state: &mut State, actions: &mut PanelActions) {
    let prompt_lbl = ui.label("Prompt");
    let resp = ui
        .add(
            egui::TextEdit::singleline(&mut state.ai_chat.prompt_draft)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(prompt_lbl.id);
    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        actions.submit = true;
    }

    let running = state.ai_chat.is_running();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!running, egui::Button::new("Send"))
            .clicked()
        {
            actions.submit = true;
        }
        if ui
            .add_enabled(running, egui::Button::new("Cancel"))
            .clicked()
        {
            actions.cancel = true;
        }
    });
}

fn apply_panel_actions(state: &mut State, actions: PanelActions) {
    if let Some(allow) = actions.confirm_choice {
        state.ai_chat.resolve_confirmation(allow);
    }
    if actions.cancel {
        state.ai_chat.cancel();
    }
    if let Some(turn_id) = actions.undo_turn_id {
        state.ai_chat.undo_turn(
            turn_id,
            &state.world,
            &mut state.history,
            &mut state.doc_version,
            &mut state.status,
        );
    }
    if actions.submit {
        state.submit_ai_prompt();
    }
}
