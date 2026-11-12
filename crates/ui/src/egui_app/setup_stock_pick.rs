//! The setup page's stock source (#320): "Its own stock" or "What
//! <previous setup> leaves behind", and the incoming stock that second
//! choice brings - the carried body's box and volume. A pick queues the
//! undoable `SetSetupStockSource` (`CommandKind::SetSetupStockSource`);
//! nothing here writes the world. The first setup shows no choice, and a
//! carry the sim refuses shows its reason and cannot be picked.
//!
//! Provenance: our own page law (issue #320, #343's per-node behaviour);
//! no HSMWorks material consulted.

use egui::{RichText, Ui};

use camcraft_app::setup_stock::{self, StockSource};
use camcraft_app::units;

use crate::state::{CommandKind, State};

/// Draw the stock-source choice of setup `name`.
pub(super) fn picker(ui: &mut Ui, state: &mut State, name: &str) {
    ui.label("Stock");
    let (Some(prev), Some(now)) = (setup_stock::previous(&state.world, name), setup_stock::effective(&state.world, name))
    else {
        let _ = ui.radio(true, "Its own stock (the Stock node)");
        ui.label(RichText::new("The first setup has no setup before it to take stock from.").weak());
        return;
    };
    let refusal = setup_stock::carry_refusal(&state.world, name);
    let mut pick = None;
    if ui.radio(now == StockSource::Own, "Its own stock (the Stock node)").clicked() && now != StockSource::Own {
        pick = Some(StockSource::Own);
    }
    let carried = ui
        .add_enabled(
            refusal.is_none(),
            egui::RadioButton::new(now == StockSource::FromPrevious, format!("What {prev} leaves behind")),
        )
        .on_disabled_hover_text(refusal.clone().unwrap_or_default());
    if carried.clicked() && now != StockSource::FromPrevious {
        pick = Some(StockSource::FromPrevious);
    }
    if let Some(source) = pick {
        state.pending.push_back(CommandKind::SetSetupStockSource { setup: name.to_owned(), source });
    }
    match (&refusal, now) {
        (Some(why), StockSource::FromPrevious) => {
            let ink = super::colorvision::text_inks(ui, &state.world).error;
            // #866: what happened, then what to do - in the page's words.
            super::error_text::error_text(
                ui,
                ink,
                &format!(
                    "The stock simulation cannot use what {prev} leaves behind - {why}. To go on, choose \"Its own stock\" \
                     for this setup, or hold the part here the way {prev} holds it (flipped over X or Y, or turned about Z, also works)."
                ),
            );
        }
        (Some(why), StockSource::Own) => {
            ui.label(RichText::new(format!("Cannot take what {prev} leaves behind: {why}")).weak());
        }
        (None, StockSource::FromPrevious) => incoming(ui, state, name, &prev),
        (None, StockSource::Own) => {}
    }
}

/// The incoming stock's box and volume, measured once per document
/// version (the program sim is not run every frame): the viewport
/// sheet's measurement when it stands (#406), else measured here.
fn incoming(ui: &mut Ui, state: &State, name: &str, prev: &str) {
    let id = egui::Id::new(("setup_incoming_stock", name.to_owned(), state.doc_version));
    // #406: the viewport's sheet measured this version already - one sim.
    let cached = crate::incoming_view::measured(state, name).or_else(|| ui.ctx().data(|d| d.get_temp(id)));
    let got = cached.unwrap_or_else(|| {
        let v = setup_stock::incoming(&state.world, name);
        ui.ctx().data_mut(|d| d.insert_temp(id, v.clone()));
        v
    });
    match got {
        Ok(i) => {
            let u = state.units();
            ui.label(format!(
                "Incoming from {prev}: X {} .. {}, Y {} .. {}, Z {} .. {} {}",
                units::len(i.min[0], u, 3),
                units::len(i.max[0], u, 3),
                units::len(i.min[1], u, 3),
                units::len(i.max[1], u, 3),
                units::len(i.min[2], u, 3),
                units::len(i.max[2], u, 3),
                u.suffix()
            ));
            ui.label(format!(
                "Incoming volume: {} {} (this setup's Stock node is not cut from)",
                units::vol(i.volume, u, 1),
                u.vol_suffix()
            ));
        }
        Err(why) => {
            ui.label(RichText::new(format!("Incoming stock not known yet: {why}")).weak());
        }
    }
}
