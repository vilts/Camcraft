//! A selected tool's Properties and the shared creation form (#2002).
//! Provenance: our own tool editor extracted from pages.rs, retaining its commands.

use super::fields::{combo, field, num, num_feed, num_plain};
use crate::state::{CommandKind, ErrorScope, State};
use camcraft_app::{model, tools, units};
use egui::{TextEdit, Ui};

/// Tool kind labels, same order as `ToolKind` (mirrors the tool page).
const TOOL_KINDS: [&str; 17] = [
    "End mill",
    "Ball nose",
    "Bull nose",
    "Drill",
    "Tap",
    "Bore",
    "Turning insert",
    "Grooving blade",
    "Threading insert",
    "Form gauge",
    "Chamfer mill",
    "Touch probe",
    // #670: the kerf is its diameter; it turns no spindle.
    "Plasma torch",
    // #541: diameter = X size, width = Y size; it turns no spindle.
    "Sinker electrode",
    // #894: rows of teeth at one pitch / one row over a pitch range.
    "Thread mill",
    "Single-form thread mill",
    "Waterjet nozzle",
];

pub(crate) fn page(ui: &mut Ui, state: &mut State, name: &str) {
    if name.is_empty() {
        ui.label("Open the Tool library from Tools to browse or create tools.");
        return;
    }
    ui.heading(format!("Tool: {name}"));
    if ui.button("Delete").clicked() {
        state
            .pending
            .push_back(CommandKind::DeleteTool(name.to_owned()));
    }
    if state.tool_editor.is_some() {
        ui.label("Finish or close the tool editor to edit these Properties.");
        return;
    }
    state.tool_ui.name = name.to_owned();
    state.sync_tool_ui();
    contents(ui, state, false);
}

pub(super) fn contents(ui: &mut Ui, state: &mut State, wide: bool) {
    let u = state.units();
    let exists = state.tool_ui.exists;
    // One snapshot of the draft drives the picture, so fields and
    // preview cannot disagree mid-frame; the DragValues below still
    // edit the live draft, and the next frame's snapshot follows.
    let preview_tool = state.tool_ui.to_tool();
    let columns = |ui: &mut Ui| {
        ui.vertical(|ui| {
            ui.set_width(if wide { 220.0 } else { ui.available_width().min(220.0) });
            if !exists {
                // Typing the name is not an event; it rides the
                // Add command at the bottom, same law as ever.
                field(ui, "Name");
                ui.add(
                    TextEdit::singleline(&mut state.tool_ui.name)
                        .desired_width(f32::INFINITY),
                );
            }
            field(ui, "Type");
            let before = state.tool_ui.kind;
            let mut k = state.tool_ui.kind.min(TOOL_KINDS.len() - 1);
            combo(ui, "tool_kind", &mut k, &TOOL_KINDS);
            state.tool_ui.kind = k.min(TOOL_KINDS.len() - 1);
            if state.tool_ui.kind != before {
                // A draft inherited from a mill carries every
                // specialty field inert (0.0); moving to a kind
                // that READS them seeds the standard value, so
                // flipping the combo alone never leaves the Add
                // button refusing on a half-built tool.
                if state.tool_ui.kind == 3
                    && (state.tool_ui.tip_angle.is_nan() || state.tool_ui.tip_angle <= 60.0)
                {
                    state.tool_ui.tip_angle = 118.0;
                }
                if state.tool_ui.kind == 4
                    && (state.tool_ui.pitch.is_nan() || state.tool_ui.pitch <= 0.0)
                {
                    state.tool_ui.pitch = 1.0;
                }
                if state.tool_ui.kind == 6
                    && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
                {
                    state.tool_ui.nose_radius = 0.8;
                }
                // The boring bar's nose (M5.2 F): the same
                // corner circle, read by the turning kind now -
                // seed it the way the insert's row is, so a
                // combo flip alone never leaves the bore op
                // refusing at the kernel's point-tip door.
                if state.tool_ui.kind == 5
                    && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
                {
                    state.tool_ui.nose_radius = 0.4;
                }
                if state.tool_ui.kind == 7
                    && (state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0)
                {
                    state.tool_ui.width = 2.0;
                }
                // The threading V (M5.2 D): crest width 0.866 x
                // the seed op's pitch 2.0, and the 60-degree
                // metric/unified flank angle - the two numbers
                // the generator fits against the thread it cuts.
                if state.tool_ui.kind == 8 {
                    if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
                        state.tool_ui.width = 1.732;
                    }
                    if state.tool_ui.tip_angle.is_nan()
                        || !(0.0..180.0).contains(&state.tool_ui.tip_angle)
                    {
                        state.tool_ui.tip_angle = 60.0;
                    }
                }
                // The form gauge (M5.2 E2): the house row's
                // 4-wide band on a 6-radius crown - wide
                // enough to state a valley, narrow enough to
                // stand inside twice its own arc.
                if state.tool_ui.kind == 9 {
                    if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
                        state.tool_ui.width = 4.0;
                    }
                    if state.tool_ui.nose_radius.is_nan()
                        || state.tool_ui.nose_radius <= 0.0
                    {
                        state.tool_ui.nose_radius = 6.0;
                    }
                }
                // The chamfer mill (M2.4 B): the shop's 90 -
                // the supplement of the 45-degree face the
                // catalog's chamfer seed stands at.
                if state.tool_ui.kind == 10
                    && (state.tool_ui.tip_angle.is_nan()
                        || !(0.0..180.0).contains(&state.tool_ui.tip_angle))
                {
                    state.tool_ui.tip_angle = 90.0;
                }
                // The thread mills (#894): a form and a pitch.
                super::tool_thread_mill_row::seed(&mut state.tool_ui);
                // The torch (#670): no spindle, no corner - a
                // kerf. Leaving it, a spinning kind needs an rpm.
                // The sinker electrode (#541) turns none either.
                if matches!(state.tool_ui.tool_kind(), tools::ToolKind::Torch | tools::ToolKind::Waterjet | tools::ToolKind::Electrode) {
                    state.tool_ui.rpm = 0.0;
                    state.tool_ui.corner_r = 0.0;
                } else if matches!(before, 12 | 13 | 16) && (state.tool_ui.rpm.is_nan() || state.tool_ui.rpm <= 0.0) {
                    state.tool_ui.rpm = 10000.0;
                }
            }
            let torch = matches!(state.tool_ui.tool_kind(), tools::ToolKind::Torch | tools::ToolKind::Waterjet);
            let electrode = state.tool_ui.tool_kind() == tools::ToolKind::Electrode;
            if electrode {
                // #541: the burning face's two sides.
                field(ui, "Electrode X size");
                num(ui, u, &mut state.tool_ui.diameter);
                field(ui, "Electrode Y size");
                num(ui, u, &mut state.tool_ui.width);
                ui.label("flute length = the electrode's length below the holder; shank length = its pickup offset");
            } else if torch {
                // The kerf width IS the torch's diameter (#669 /
                // #670): every offset reads radius = kerf / 2.
                field(ui, "Kerf width");
                num(ui, u, &mut state.tool_ui.diameter);
                ui.label("the cut's width; confirm it with a test cut");
            } else {
                field(ui, "Diameter");
                num(ui, u, &mut state.tool_ui.diameter);
            }
            // Kind-read fields (the inert-unless-the-kind-reads-them
            // law): a point angle means nothing on an end mill, a
            // pitch means nothing on a drill - show each only where
            // it feeds a generator or the collision profile.
            if state.tool_ui.kind == 3 {
                field(ui, "Point angle (deg)");
                num_plain(ui, &mut state.tool_ui.tip_angle);
                ui.label("cone tip, 60 to 180; standard twist drill is 118");
            }
            if state.tool_ui.kind == 4 {
                field(ui, "Thread pitch");
                num(ui, u, &mut state.tool_ui.pitch);
                let mut left = state.tool_ui.thread_hand == camcraft_app::tools::ThreadHand::Left;
                super::fields::check_row(ui, "Left-hand tap (posting unavailable)", &mut left);
                state.tool_ui.thread_hand = if left {
                    camcraft_app::tools::ThreadHand::Left
                } else {
                    camcraft_app::tools::ThreadHand::Right
                };
                ui.label("distance advanced per turn - M6 is 1.0, M8 is 1.25");
            }
            if state.tool_ui.kind == 6 {
                field(ui, "Insert nose radius");
                num(ui, u, &mut state.tool_ui.nose_radius);
                ui.label("the corner circle ground into the tip - it IS the cutting ball");
            }
            if state.tool_ui.kind == 5 {
                field(ui, "Bar nose radius");
                num(ui, u, &mut state.tool_ui.nose_radius);
                ui.label("the ground tip circle the bar rides the hole wall on (M5.2 F) - inert on drilling cycles, which size by the slide");
            }
            // #630: which point of the nose the lathe posts write.
            super::tool_orientation_row::orientation_row(ui, state);
            // #894: a thread mill's pitch (range) and form angle.
            super::tool_thread_mill_row::thread_mill_rows(ui, state);
            // #891: a lathe steel's surface speed, feed per rev, clamp.
            super::tool_turning_row::turning_rows(ui, state);
            if state.tool_ui.kind == 7 {
                field(ui, "Blade width");
                num(ui, u, &mut state.tool_ui.width);
                ui.label("the cutting edge along the axis - the kerf's width IS the tool");
            }
            if state.tool_ui.kind == 8 {
                field(ui, "Blade width");
                num(ui, u, &mut state.tool_ui.width);
                ui.label("the V's crest width across the axis - 0.866 x the pitch it threads, and it must fit inside that pitch");
                field(ui, "Form angle (deg)");
                num_plain(ui, &mut state.tool_ui.tip_angle);
                ui.label("the included angle of the V's flanks - 60 for the metric and unified forms, 55 for Whitworth");
            }
            if state.tool_ui.kind == 9 {
                field(ui, "Band width");
                num(ui, u, &mut state.tool_ui.width);
                ui.label("the crown's ground edge across the axis - it must stand inside twice the crown radius");
                field(ui, "Crown radius");
                num(ui, u, &mut state.tool_ui.nose_radius);
                ui.label("the arc of the ground edge in the work's meridian - the sagitta the verdict bends by");
            }
            if state.tool_ui.kind == 10 {
                field(ui, "Cone angle (deg)");
                num_plain(ui, &mut state.tool_ui.tip_angle);
                ui.label("the included angle of the taper - 180 minus twice the chamfer's slope, so 90 cuts a 45-degree face");
            }
            if state.tool_ui.tool_kind() == tools::ToolKind::BallNose {
                // #263: a ball's corner IS its ball, half the
                // diameter (`Tool::tip_corner_r`) - shown, never
                // edited, since no reader takes any other number.
                field(ui, "Corner radius");
                ui.label(format!("{} {}", units::len(state.tool_ui.diameter * 0.5, u, 3), u.suffix()));
                ui.label("half the diameter - a ball nose's whole tip is its ball");
            } else if !matches!(
                state.tool_ui.tool_kind(),
                tools::ToolKind::Tap | tools::ToolKind::Torch | tools::ToolKind::Waterjet | tools::ToolKind::Electrode | tools::ToolKind::ThreadMill | tools::ToolKind::ThreadMillSingle
            ) {
                field(ui, "Corner radius");
                num(ui, u, &mut state.tool_ui.corner_r);
            }
            field(ui, "Flute length");
            num(ui, u, &mut state.tool_ui.flute_len);
            field(ui, "Shank diameter");
            num(ui, u, &mut state.tool_ui.shank_d);
            field(ui, "Shank length");
            num(ui, u, &mut state.tool_ui.shank_len);
            field(ui, "Holder diameter");
            num(ui, u, &mut state.tool_ui.holder_d);
            field(ui, "Holder length");
            num(ui, u, &mut state.tool_ui.holder_len);
            field(ui, "Material group");
            ui.add(
                TextEdit::singleline(&mut state.tool_ui.material_group)
                    .desired_width(140.0),
            );
            super::tool_catalog_rows::catalog_rows(ui, &mut state.tool_ui);
            if !torch && !electrode {
                field(ui, "Spindle rpm");
                num_plain(ui, &mut state.tool_ui.rpm);
            }
            field(ui, &format!("Feed ({})", u.feed_suffix()));
            num_feed(ui, u, &mut state.tool_ui.feed);
            super::tool_cutting_rows::cutting_rows(ui, &mut state.tool_ui, u, torch);
            // M11.3: an entry may plunge into solid only with a
            // centre-cutting tool; otherwise it helixes, ramps,
            // or the operation is refused by name.
            ui.checkbox(&mut state.tool_ui.center_cutting, "Centre cutting (may plunge into solid)");
            // #221: the machine's tool-table number - the post's
            // T and H words. 0 leaves it to the list position.
            field(ui, "Tool number (0 = list position)");
            ui.add(egui::DragValue::new(&mut state.tool_ui.number).range(0..=camcraft_app::tool_numbers::MAX));
        });
        ui.vertical(|ui| {
            // #371: the placed body's envelope rides the profile.
            let body = if exists { super::tool_body_page::envelope(state) } else { None };
            super::tool_preview::draw(ui, &preview_tool, u, body.as_deref(), wide);
        });
        ui.vertical(|ui| {
            ui.set_width(if wide { 260.0 } else { ui.available_width().min(260.0) });
            // #93: Body / Profile tabs for a milling tool.
            super::tool_profile_page::third_column(ui, state, exists, u);
        });
    };
    if wide {
        ui.horizontal_top(columns);
    } else {
        ui.vertical(columns);
    }
    ui.separator();
    let name_now = state.tool_ui.name.clone();
    let block = if name_now.trim().is_empty() {
        "Name the tool first.".to_owned()
    } else if !exists && model::find_by_name(&state.world, &name_now).is_some() {
        "That name is already in the document.".to_owned()
    } else {
        String::new()
    };
    if exists {
        let recipe_refusal = (state.tool_ui.tool_kind() == tools::ToolKind::Waterjet)
            .then(|| state.tool_ui.to_tool().torch_refusal())
            .flatten();
        if ui.add_enabled(recipe_refusal.is_none(), egui::Button::new("Apply"))
            .on_disabled_hover_text(recipe_refusal.as_deref().unwrap_or_default())
            .clicked()
        {
            // #93 slice 3: a landed drawn cutter rides the edit.
            let apply = state.apply_tool_numbers(&name_now);
            state.pending.push_back(apply);
            push_number_if_changed(state, &name_now);
        }
    } else {
        let add = ui.add_enabled(block.is_empty(), egui::Button::new("Add tool"));
        if add.clicked() {
            state.pending.push_back(CommandKind::AddTool {
                name: name_now.trim().to_owned(),
                tool: state.tool_ui.to_tool(),
            });
            push_number_if_changed(state, name_now.trim());
        }
        if !block.is_empty() {
            ui.label(block);
        }
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
        super::error_text::error_text(
            ui,
            super::colorvision::text_inks(ui, &state.world).error,
            &msg,
        );
    }
}

/// #221: queue a `SetToolNumber` when the editor's number differs from the
/// tool's own (a new tool has none). Rides the same Apply / Add click as
/// the geometry, but as its own undoable command.
fn push_number_if_changed(state: &mut State, name: &str) {
    let want = (state.tool_ui.number > 0).then_some(state.tool_ui.number);
    let have = model::find_by_name(&state.world, name)
        .and_then(|e| model::tool_number_of(&state.world, &e));
    if want != have {
        state.pending.push_back(CommandKind::SetToolNumber {
            name: name.to_owned(),
            number: want,
        });
    }
}
