//! The fixture marker's optional parameter keys read leniently (#812):
//! a `vise` or `accessory` key this build can't parse (a kind a later
//! release added, a missing or mistyped field) or one that breaks its
//! law is dropped on its own - the fixture and its saved steel stay, as
//! plain steel - with a load note the user sees, never a failed load.
//! The keys are held as raw JSON on the wire so serde never refuses the
//! whole file over one of them (the shape #819's plate keys took).
//!
//! Provenance: our own wire handling over `vise::ViseParams` and
//! `accessory::Accessory`.

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::vise_dto::ViseDto;
use super::FixtureDto;
use crate::accessory::Accessory;
use crate::schema::FixtureDef;
use crate::vise::ViseParams;

/// Key `key` of fixture `model` as a `T`, or `None` with a note naming
/// the fixture, the key and why.
fn read<T: DeserializeOwned>(v: Option<&Value>, model: &str, key: &str, notes: &mut Vec<String>) -> Option<T> {
    match serde_json::from_value::<T>(v?.clone()) {
        Ok(t) => Some(t),
        Err(why) => {
            notes.push(format!("fixture '{model}': {key} key not understood, dropped on load, kept as plain steel: {why}"));
            None
        }
    }
}

/// A law-checked value, or `None` with a note.
fn lawful<T>(t: Option<T>, check: impl Fn(&T) -> Result<(), String>, model: &str, what: &str, notes: &mut Vec<String>) -> Option<T> {
    let t = t?;
    match check(&t) {
        Ok(()) => Some(t),
        Err(why) => {
            notes.push(format!("fixture '{model}': {what} parameters dropped on load, kept as plain steel: {why}"));
            None
        }
    }
}

/// The `vise` key of `p`.
#[must_use]
pub fn vise_value(p: &ViseParams) -> Option<Value> {
    serde_json::to_value(ViseDto::from_params(p))
        .inspect_err(|why| tracing::warn!("vise key not saved, it would read back as plain steel: {why}"))
        .ok()
}

/// The `accessory` key of `a`.
#[must_use]
pub fn accessory_value(a: &Accessory) -> Option<Value> {
    serde_json::to_value(a)
        .inspect_err(|why| tracing::warn!("accessory key not saved, it would read back as plain steel: {why}"))
        .ok()
}

/// The `profile` key of `p` (#744).
#[must_use]
pub fn profile_value(p: &crate::fixture_profile::FixtureProfile) -> Option<Value> {
    serde_json::to_value(p)
        .inspect_err(|why| tracing::warn!("fixture profile key not saved, it would read back as plain steel: {why}"))
        .ok()
}

/// The fixture marker of model `model` from its file form; every key
/// that does not read or keep its law is dropped with a note.
#[must_use]
pub fn fixture_def(f: &FixtureDto, model: &str, notes: &mut Vec<String>) -> FixtureDef {
    let vise = read::<ViseDto>(f.vise.as_ref(), model, "vise", notes)
        .and_then(|d| lawful(Some(d.to_params()), |p| p.check(), model, "vise", notes));
    let accessory = lawful(read::<Accessory>(f.accessory.as_ref(), model, "accessory", notes), Accessory::check, model, "accessory", notes);
    let profile = lawful(
        read::<crate::fixture_profile::FixtureProfile>(f.profile.as_ref(), model, "profile", notes),
        crate::fixture_profile::FixtureProfile::check,
        model,
        "profile",
        notes,
    );
    // #819: the plate keys keep their own drop-and-log lenient read.
    let plate = super::plate_dto::link_from(f.plate.as_ref(), f.on_plate.as_ref(), model, notes);
    // #820: the mod-vise jaw key keeps its own drop-and-log read.
    let mod_jaw = super::modvise_dto::params_from(f.mod_jaw.as_ref(), model, notes);
    FixtureDef { setup: f.setup.clone(), fixed: f.fixed, vise, plate, mod_jaw, accessory, profile }
}
