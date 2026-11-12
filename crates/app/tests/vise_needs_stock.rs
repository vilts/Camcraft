//! #1105: fitting a vise, a mod vise pair or parallels to a setup with no
//! stock body is refused in plain English - what is missing, where to set
//! it, no "invalid parameters:" prefix - and the dialog's preview
//! (`vise_fit::preview`) is the command's own derivation, so what the
//! dialog says beforehand is what Place does.
//!
//! Gate map (M4.4): covers app::vise_fit, app::commands::builtin_vise,
//! app::commands::mod_vise, app::commands::accessory.

use camcraft_app::accessory::Accessory;
use camcraft_app::commands::{
    AccessoryPlacement, History, InsertAccessory, InsertPlate, InsertVise, PlaceModVisePair, PlatePlacement, SetStock, VisePlacement,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::modvise;
use camcraft_app::plate;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::vise::{self, ViseParams};
use camcraft_app::vise_fit::{self, VisePreview};
use camcraft_app::world::new_world;

fn six() -> ViseParams {
    vise::size("6 in").map(vise::ViseSize::half_open).expect("6 in")
}

fn msg<T>(r: Result<T, camcraft_app::commands::CommandError>) -> String {
    r.err().map(|e| e.to_string()).unwrap_or_default()
}

#[test]
fn the_three_fits_name_the_missing_stock_and_the_stock_page() {
    let world = new_world();
    let m = msg(InsertVise::new(&world, &six(), Some("Setup1"), VisePlacement::FitStock));
    assert_eq!(
        m,
        "setup 'Setup1' has no stock yet, so there is nothing to fit the vise to - set the stock first (Stock page) - or place the vise by position instead"
    );
    assert!(!m.contains("invalid parameters"), "{m}");

    // A mod vise pair on a plate in a setup with no stock.
    let mut history = History::default();
    let p = plate::preset("770MX table 28 x 15 in").map(plate::PlatePreset::params).expect("preset");
    let cmd = InsertPlate::new(&world, &p, None, PlatePlacement::At(Location { pos: Point3::ZERO, rot: IDENTITY_Q })).expect("plate");
    let plate_name = cmd.name().to_owned();
    history.execute(&world, cmd);
    let row = modvise::preset("1 in grid 3/8-16").expect("preset");
    let m = msg(PlaceModVisePair::new(&world, &plate_name, &row.fixed(), &row.adjustable(), "A1", 0.0));
    assert_eq!(m, "the setup has no stock yet, so there is nothing to fit the mod vise to - set the stock first (Stock page)");

    // Parallels under the stock in a vise that is there.
    let at = VisePlacement::At(Location::default());
    let v = InsertVise::new(&world, &six(), None, at).expect("vise");
    let vname = v.name().to_owned();
    history.execute(&world, v);
    let par = Accessory::default_of("parallels").expect("parallels");
    let fit = AccessoryPlacement::UnderStockInVise { vise: vname, above: 5.0 };
    let m = msg(InsertAccessory::new(&world, &par, None, fit));
    assert_eq!(m, "the setup has no stock yet, so there is nothing to fit the parallels under - set the stock first (Stock page)");
}

#[test]
fn the_preview_is_the_commands_own_fit() {
    let world = new_world();
    let mut history = History::default();
    assert_eq!(vise_fit::preview(&world, &six(), None), VisePreview::NoStock);
    assert!(!vise_fit::has_stock(&world, None));
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Location::default()).expect("stock"));
    let VisePreview::Fits { opening, at, centred } = vise_fit::preview(&world, &six(), None) else { panic!("fits") };
    assert!(!centred);
    assert!((opening - 40.0).abs() < 1e-9, "opens to the stock's Y width: {opening}");
    let placed = InsertVise::new(&world, &six(), None, VisePlacement::FitStock).expect("fits");
    assert_eq!(placed.params().map(|p| p.opening), Some(opening));
    assert!((placed.location().pos.y - at.y).abs() < 1e-9 && (placed.location().pos.x - at.x).abs() < 1e-9);
    assert!((placed.location().pos.z - at.z).abs() < 1e-9);
}

#[test]
fn a_stock_wider_than_the_vise_is_refused_up_front_and_by_the_command() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 400.0, dz: 20.0 }, Location::default()).expect("stock"));
    let VisePreview::Refused(why) = vise_fit::preview(&world, &six(), None) else { panic!("refused") };
    assert!(why.contains("opens") && why.contains("at most"), "{why}");
    assert_eq!(msg(InsertVise::new(&world, &six(), None, VisePlacement::FitStock)), why, "same text, no prefix");
}
