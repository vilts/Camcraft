## #1105 - the vise dialog says a setup needs its stock before a fit

**Decided.** (1) One app derivation, `vise_fit::fit`, feeds both `InsertVise` and the dialog's `vise_fit::preview`, so what the dialog says beforehand is what Place does. (2) The "invalid parameters:" prefix comes from `CommandError::Invalid`'s display; instead of changing every command's text, a new variant `CommandError::Needs(String)` (displayed as is) carries the three no-stock refusals and the vise's stock-too-wide refusal. The other commands keep their text. (3) With no stock body the checkbox is greyed and off, the position rows show, and Place sends a by-position insert; the draft's own `fit_stock` flag is untouched, so Fit returns once a stock exists. (4) "Open the Stock page" reuses the browser's Stock-row click (activates the dialog's setup, targets the Stock page). (5) The dialog says the vise grips the stock, not the imported model (maintainer's note on the issue). (6) Browser Stock row suffix reads `not set` while the stock has no body.

**Why.** The refusal was right but late and cryptic; one derivation keeps preview and command from drifting.

**Considered and dropped.** Rewording `CommandError::Invalid` globally (touches every command's text and tests); a UI-side stock check (layering law forbids world calls there, and it would be a second derivation); disabling Place on a too-wide stock (refusals stay refusals, the command still names it).

**Cost to change.** Small: the variant is one line; texts live in `vise_fit.rs` and `fixture_vise_dialog.rs`.
