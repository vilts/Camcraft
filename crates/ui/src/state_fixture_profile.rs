//! Dialog and row draft state for spun and extruded fixture profiles (#744).
//! Provenance: clean-room UI state binding `camcraft_app::fixture_profile` commands.

use std::path::PathBuf;

use camcraft_app::commands::{DeriveFixtureProfile, InsertProfileFixture, SetFixtureProfile};
use camcraft_app::fixture;
use camcraft_app::fixture_profile::{ExtrudeAxis, FixtureProfile, ProfilePoint, SpunPoint};
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::model;
use camcraft_app::schema::Location;

use super::{ErrorScope, FixtureLibDialog, State};
use crate::file_dialog::FileDialog;

/// Variant chosen inside the "Add profile / DXF fixture…" dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKindChoice {
    Spun,
    Extruded,
}

/// Modal draft for creating or importing a spun or extruded fixture profile.
#[derive(Clone, Debug, PartialEq)]
pub struct FixtureProfileDraft {
    pub setup: String,
    pub name: String,
    pub kind: ProfileKindChoice,
    pub spun_points: Vec<SpunPoint>,
    pub extrude_points: Vec<ProfilePoint>,
    pub axis: ExtrudeAxis,
    pub depth: f64,
    pub override_dxf_depth: bool,
    pub pos: [f64; 3],
    pub yaw_deg: f64,
    pub source_label: Option<String>,
    pub error: Option<String>,
}

impl FixtureProfileDraft {
    /// Starter profile draft targeting `setup` (`""` means the implicit setup).
    #[must_use]
    pub fn new(setup: String) -> Self {
        Self {
            setup,
            name: "Fixture profile".to_owned(),
            kind: ProfileKindChoice::Extruded,
            spun_points: vec![
                SpunPoint::line(0.0, 0.0),
                SpunPoint::line(0.0, 40.0),
                SpunPoint::line(30.0, 40.0),
                SpunPoint::line(30.0, 0.0),
            ],
            extrude_points: vec![
                ProfilePoint::line(0.0, 0.0),
                ProfilePoint::line(40.0, 0.0),
                ProfilePoint::line(40.0, 20.0),
                ProfilePoint::line(0.0, 20.0),
            ],
            axis: ExtrudeAxis::Z,
            depth: 25.0,
            override_dxf_depth: false,
            pos: [0.0; 3],
            yaw_deg: 0.0,
            source_label: None,
            error: None,
        }
    }

    /// Build the active `FixtureProfile` from the selected tab's fields.
    #[must_use]
    pub fn active_profile(&self) -> FixtureProfile {
        match self.kind {
            ProfileKindChoice::Spun => FixtureProfile::Spun { points: self.spun_points.clone() },
            ProfileKindChoice::Extruded => FixtureProfile::Extruded {
                points: self.extrude_points.clone(),
                axis: self.axis,
                depth: self.depth,
            },
        }
    }
}

/// Per-fixture draft for editing a placed profile fixture or deriving a reference from STEP.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileEditDraft {
    pub profile: FixtureProfile,
    pub synced_profile: Option<FixtureProfile>,
    pub synced_rev: u64,
    pub derive_axis: ExtrudeAxis,
    pub error: Option<String>,
}

impl ProfileEditDraft {
    #[must_use]
    fn from_world(world_profile: Option<FixtureProfile>, rev: u64) -> Self {
        let profile = world_profile.clone().unwrap_or(FixtureProfile::Extruded {
            points: Vec::new(),
            axis: ExtrudeAxis::Z,
            depth: 25.0,
        });
        Self {
            profile,
            synced_profile: world_profile,
            synced_rev: rev,
            derive_axis: ExtrudeAxis::Z,
            error: None,
        }
    }
}

impl State {
    /// Open "Add profile / DXF fixture…" over `setup`'s Fixtures node.
    pub fn open_profile_fixture_dialog(&mut self, setup: String) {
        self.fixture_lib = Some(FixtureLibDialog::Profile(FixtureProfileDraft::new(setup)));
    }

    /// Open a native `.dxf` picker and load it into the open profile fixture dialog.
    pub fn load_profile_dxf_dialog(&mut self) {
        if let Some(why) = self.in_browser(crate::browser_files::NEEDS_PICKER) {
            self.status = why.to_owned();
            if let Some(FixtureLibDialog::Profile(d)) = self.fixture_lib.as_mut() {
                d.error = Some(why.to_owned());
            }
            return;
        }
        let Some(path) = FileDialog::new()
            .set_title("Load fixture DXF profile")
            .add_filter("DXF", &["dxf"])
            .pick_file()
        else {
            return;
        };
        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("DXF fixture")
            .to_owned();
        match std::fs::read(&path) {
            Ok(bytes) => self.load_profile_dxf_bytes(&label, &bytes),
            Err(err) => {
                let msg = format!("DXF read failed for '{}': {err}", path.display());
                if let Some(FixtureLibDialog::Profile(d)) = self.fixture_lib.as_mut() {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// Parse `bytes` into the open `FixtureLibDialog::Profile` draft (also used by tests).
    pub fn load_profile_dxf_bytes(&mut self, file_label: &str, bytes: &[u8]) {
        let Some(FixtureLibDialog::Profile(d)) = self.fixture_lib.as_mut() else {
            return;
        };
        let parsed = match d.kind {
            ProfileKindChoice::Spun => FixtureProfile::spun_from_dxf(bytes),
            ProfileKindChoice::Extruded => {
                let depth_opt = d.override_dxf_depth.then_some(d.depth);
                FixtureProfile::extruded_from_dxf(bytes, d.axis, depth_opt)
            }
        };
        match parsed {
            Ok(profile) => {
                let kind = profile.kind_label().to_ascii_lowercase();
                match profile {
                    FixtureProfile::Spun { points } => d.spun_points = points,
                    FixtureProfile::Extruded { points, axis, depth } => {
                        d.extrude_points = points;
                        d.axis = axis;
                        d.depth = depth;
                    }
                }
                if !file_label.trim().is_empty()
                    && (d.name == "Fixture profile" || d.source_label.is_some())
                {
                    d.name = file_label.trim().to_owned();
                }
                d.source_label = Some(file_label.to_owned());
                d.error = None;
                self.status = format!("loaded {kind} DXF profile from {file_label}");
            }
            Err(err) => {
                let msg = format!("DXF profile not loaded: {err}");
                d.error = Some(msg.clone());
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// Ensure fixture `name` has an edit/derive draft synchronized to the current world state.
    pub(crate) fn profile_edit_draft(&mut self, name: &str) -> Option<&mut ProfileEditDraft> {
        let entity = model::find_by_name(&self.world, name)?;
        let def = fixture::def_of(&self.world, &entity)?;
        let step_src = camcraft_app::step_source::of(&self.world, &entity);
        if def.profile.is_none() && step_src.is_none() {
            return None;
        }
        let rev = model::rev_of(&self.world, &entity);
        let entry = self
            .profile_edits
            .entry(name.to_owned())
            .or_insert_with(|| ProfileEditDraft::from_world(def.profile.clone(), rev));
        if entry.synced_rev != rev || entry.synced_profile != def.profile {
            if let Some(p) = &def.profile {
                entry.profile = p.clone();
            }
            entry.synced_profile = def.profile;
            entry.synced_rev = rev;
            entry.error = None;
        }
        Some(entry)
    }

    /// Drop profile drafts whose fixture is gone.
    pub fn retire_profile_edit_drafts(&mut self) {
        let world = &self.world;
        self.profile_edits.retain(|name, _| model::find_by_name(world, name).is_some());
    }

    /// `CommandKind::InsertProfileFixture`: insert validated spun/extruded fixture.
    pub(super) fn insert_profile_fixture(
        &mut self,
        setup: Option<String>,
        name: String,
        profile: FixtureProfile,
        pos: [f64; 3],
        yaw_deg: f64,
    ) {
        let location = Location {
            pos: Point3::new(pos[0], pos[1], pos[2]),
            rot: quat_from_euler(0.0, 0.0, yaw_deg.to_radians()),
        };
        match InsertProfileFixture::new(&self.world, &name, profile, setup.as_deref(), location) {
            Ok(cmd) => {
                let landed = cmd.name().to_owned();
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                self.status = format!("placed profile fixture {landed}");
                self.fixture_lib = None;
            }
            Err(err) => {
                let msg = format!("profile fixture not placed: {err}");
                if let Some(FixtureLibDialog::Profile(d)) = self.fixture_lib.as_mut() {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// `CommandKind::SetFixtureProfile`: edit a profile fixture and regenerate its mesh.
    pub(super) fn set_fixture_profile(&mut self, name: &str, profile: FixtureProfile) {
        match SetFixtureProfile::new(&self.world, name, profile) {
            Ok(cmd) => {
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                if let Some(d) = self.profile_edits.get_mut(name) {
                    d.error = None;
                }
                self.status = format!("{name}: fixture profile updated");
            }
            Err(err) => {
                let msg = format!("{name}: {err}");
                if let Some(d) = self.profile_edits.get_mut(name) {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// `CommandKind::DeriveFixtureProfile`: derive reference spun/extruded profile from STEP.
    pub(super) fn derive_fixture_profile(&mut self, name: &str, axis: Option<ExtrudeAxis>) {
        let planned = match axis {
            None => DeriveFixtureProfile::spun(&self.world, name),
            Some(ax) => DeriveFixtureProfile::extruded(&self.world, name, ax),
        };
        match planned {
            Ok(cmd) => {
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                if let Some(d) = self.profile_edits.get_mut(name) {
                    d.error = None;
                }
                let kind = match axis {
                    None => "spun".to_owned(),
                    Some(ax) => format!("extruded ({})", ax.label()),
                };
                self.status = format!("{name}: derived {kind} reference profile");
            }
            Err(err) => {
                let msg = format!("{name}: {err}");
                if let Some(d) = self.profile_edits.get_mut(name) {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// Open save picker and export fixture `name`'s profile as ASCII DXF.
    pub fn export_fixture_dxf_dialog(&mut self, name: &str) {
        if let Some(why) = self.in_browser(crate::browser_files::NO_EXPORT) {
            self.status = why.to_owned();
            return;
        }
        let dxf = match self.fixture_dxf_text(name) {
            Ok(text) => text,
            Err(msg) => {
                self.refuse(ErrorScope::Setup, msg);
                return;
            }
        };
        let Some(path) = FileDialog::new()
            .set_title("Export fixture profile as DXF")
            .add_filter("DXF", &["dxf"])
            .set_file_name(format!("{name}.dxf"))
            .save_file()
        else {
            return;
        };
        self.write_fixture_dxf(name, &path, &dxf);
    }

    /// `CommandKind::ExportFixtureDxf`: export fixture `name`'s profile to `path`.
    pub(super) fn export_fixture_dxf_path(&mut self, name: &str, path: PathBuf) {
        match self.fixture_dxf_text(name) {
            Ok(dxf) => self.write_fixture_dxf(name, &path, &dxf),
            Err(msg) => self.refuse(ErrorScope::Setup, msg),
        }
    }

    fn fixture_dxf_text(&self, name: &str) -> Result<String, String> {
        let entity = model::find_by_name(&self.world, name)
            .ok_or_else(|| format!("fixture '{name}' not found"))?;
        let def = fixture::def_of(&self.world, &entity)
            .ok_or_else(|| format!("'{name}' is not a fixture"))?;
        let profile = def
            .profile
            .ok_or_else(|| format!("'{name}' has no profile to export as DXF"))?;
        profile.to_dxf()
    }

    fn write_fixture_dxf(&mut self, name: &str, path: &std::path::Path, dxf: &str) {
        match std::fs::write(path, dxf) {
            Ok(()) => self.status = format!("exported {name} profile DXF to {}", path.display()),
            Err(err) => self.refuse(
                ErrorScope::Setup,
                format!("DXF export failed for '{}': {err}", path.display()),
            ),
        }
    }
}
