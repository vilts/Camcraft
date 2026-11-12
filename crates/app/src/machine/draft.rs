//! A machine file being edited (issue #383): opened from a catalogue
//! entry, copied from one as a template, or started blank for a kind;
//! edited field by field; saved only when it validates.
//!
//! Provenance: our own editor model, specced from the issue's workflow
//! (new from template, new blank, edit, Save / Save As, validation
//! blocks the save, unknown keys kept) and the `.mch` grammar in
//! `assets/machines/AGENTS.md`.
//!
//! The draft holds the file's whole JSON and edits it in place, so a
//! key the editor does not know - a vendor block, a hand-written note -
//! is written back as it was read. Editing a machine FILE is a save
//! with its own confirmation (the pane asks), not a document edit: it
//! touches no entity and no command history (#287 rules; assigning a
//! machine to a setup stays `LoadMachine`'s job).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::fields::{self, FieldGroup, FieldKind};
use super::library::{text_of, MachineSource};
use super::order::{self, KeyOrder};
use super::store::{self, SaveError};
use super::validate::{validate_text, Refusal};
use super::Kind;

/// One machine file open in the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineDraft {
    /// The file's whole JSON, unknown keys included.
    pub value: Value,
    /// Where Save writes; `None` = never saved (Save As first).
    pub path: Option<PathBuf>,
    /// A shipped machine: Save is refused, Save As writes a copy.
    pub read_only: bool,
    /// Edited since it was opened or last saved.
    pub dirty: bool,
    /// The key order the file was read in; a save keeps it (#480).
    pub order: KeyOrder,
}

fn read(source: &MachineSource) -> Result<(Value, KeyOrder), Refusal> {
    let text = text_of(source).map_err(|e| Refusal { key: "file".to_owned(), rule: e })?;
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => Ok((v, KeyOrder::of_text(&text))),
        Ok(_) => Err(Refusal { key: "file".to_owned(), rule: "must be one machine object { ... }".to_owned() }),
        Err(e) => Err(Refusal { key: "file".to_owned(), rule: format!("not readable JSON ({e})") }),
    }
}

/// A file inside a shipped `assets/machines` folder is read-only too:
/// the editor never writes into the install tree.
fn is_shipped_path(p: &Path) -> bool {
    p.parent().is_some_and(|d| d.ends_with(Path::new("assets").join("machines")))
}

impl MachineDraft {
    /// Open a catalogue entry for editing. A built-in (or a file in the
    /// shipped folder) opens read-only. Refused by name when the file
    /// does not read as one JSON object.
    pub fn open(source: &MachineSource) -> Result<Self, Refusal> {
        let (value, order) = read(source)?;
        let (path, read_only) = match source {
            MachineSource::Builtin(_) => (None, true),
            MachineSource::File(p) => (Some(p.clone()), is_shipped_path(p)),
        };
        Ok(Self { value, path, read_only, dirty: false, order })
    }

    /// A new machine copied from a catalogue entry: every key of the
    /// template, its model marked as a copy, never saved yet.
    pub fn from_template(source: &MachineSource) -> Result<Self, Refusal> {
        let (mut value, order) = read(source)?;
        let model = value.pointer("/general/model").and_then(Value::as_str).unwrap_or("Machine").to_owned();
        // The template read as an object, so this cannot refuse.
        let _ = fields::set(&mut value, "/general/model", FieldKind::Text, &format!("{model} copy"));
        Ok(Self { value, path: None, read_only: false, dirty: true, order })
    }

    /// A new machine with the keys its kind requires: the model, the
    /// kind, the units and the axis tree the kind moves (mill X/Y/Z,
    /// lathe X/Z, mill-turn X/Y/Z and a C rotary, plasma X/Y/Z and a
    /// spindle-less torch head), with one head to
    /// carry the spindle and tool station. Travels, feeds and speeds
    /// are left unstated for the user to fill.
    #[must_use]
    pub fn blank(kind: Kind) -> Self {
        let lin = |n: &str| json!({ "id": n.to_ascii_lowercase(), "name": n, "type": "linear" });
        // #530: an EDM machine carries no spindle and no coolant; its
        // head is the wire guides or the electrode ram.
        let head = if matches!(kind, Kind::WireEdm | Kind::SinkerEdm) {
            json!({ "id": "head", "name": "head", "type": "head" })
        } else {
            json!({ "id": "head", "name": "head", "type": "head",
                "spindle": {}, "tool_station": { "coolants": [ { "name": "Flood", "mCode": "M8" } ] } })
        };
        let mut parts: Vec<Value> = match kind {
            Kind::Lathe => vec![lin("X"), lin("Z")],
            Kind::Mill | Kind::MillTurn | Kind::Plasma | Kind::SinkerEdm => {
                vec![lin("X"), lin("Y"), lin("Z")]
            }
            // #530: lower guide X/Y and upper guide U/V.
            Kind::WireEdm => vec![lin("X"), lin("Y"), lin("U"), lin("V")],
        };
        if kind == Kind::MillTurn {
            parts.push(json!({ "id": "c", "name": "C", "type": "rotary", "axis": [0.0, 0.0, 1.0] }));
        }
        // #669: a plasma table's head is the torch - no spindle, no
        // coolant (the plasma rules refuse both by name).
        parts.push(if kind == Kind::Plasma { json!({ "id": "torch", "name": "torch", "type": "head" }) } else { head });
        let value = json!({
            "general": { "model": format!("New {}", kind.word()), "kind": kind.word() },
            "kinematics": { "default": {
                "units": { "length": "mm", "angle": "degrees" },
                "parts": [ { "id": "base", "name": "base", "type": "table", "parts": parts } ]
            } }
        });
        Self { value, path: None, read_only: false, dirty: true, order: KeyOrder::default() }
    }

    /// The model the file states (empty when none).
    #[must_use]
    pub fn model(&self) -> String {
        self.value.pointer("/general/model").and_then(Value::as_str).unwrap_or("").to_owned()
    }

    /// A Save As file name from the model: letters and digits, the rest
    /// as `-`.
    #[must_use]
    pub fn suggested_name(&self) -> String {
        let mut s: String = self
            .model()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect();
        while s.contains("--") {
            s = s.replace("--", "-");
        }
        let s = s.trim_matches('-');
        if s.is_empty() { "machine".to_owned() } else { s.to_owned() }
    }

    /// The property grid for the file as it stands.
    #[must_use]
    pub fn groups(&self) -> Vec<FieldGroup> {
        fields::groups(&self.value)
    }

    /// Write one field's typed text into the file (empty = unstated).
    pub fn set(&mut self, pointer: &str, kind: FieldKind, text: &str) -> Result<(), String> {
        fields::set(&mut self.value, pointer, kind, text)?;
        self.dirty = true;
        Ok(())
    }

    /// Add a named coolant (no code yet) to the first head's tool
    /// station, the list the reader takes first.
    pub fn add_coolant(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("coolant name is empty".to_owned());
        }
        let head = fields::heads(&self.value).into_iter().next().ok_or("this machine has no head to carry a coolant")?;
        let list = format!("{head}/tool_station/coolants");
        if !self.value.pointer(&list).is_some_and(Value::is_array) {
            // Creates the station on the way; the placeholder is replaced at once.
            fields::set(&mut self.value, &list, FieldKind::Text, "-")?;
        }
        match self.value.pointer_mut(&list) {
            Some(Value::Array(a)) => a.push(json!({ "name": name })),
            Some(slot) => *slot = json!([ { "name": name } ]),
            None => return Err(format!("{list}: not a coolant list")),
        }
        self.dirty = true;
        Ok(())
    }

    /// The file as it will be written: pretty JSON in the order it was
    /// read (new keys last in their object), one trailing newline.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = order::to_pretty(&self.value, &self.order);
        s.push('\n');
        s
    }

    /// The validator's refusals for the file as it stands.
    #[must_use]
    pub fn refusals(&self) -> Vec<Refusal> {
        validate_text(&self.text())
    }

    /// Save over the file it came from. Refused for a shipped or
    /// never-saved machine (Save As), and when it does not validate.
    pub fn save(&mut self) -> Result<PathBuf, SaveError> {
        let path = match (&self.path, self.read_only) {
            (Some(p), false) => p.clone(),
            _ => return Err(SaveError::NeedsSaveAs),
        };
        store::save_text(&path, &self.text())?;
        self.dirty = false;
        Ok(path)
    }

    /// Save as `name` in the user machines folder `dir`. An existing
    /// file is replaced only when `replace` says the user confirmed.
    pub fn save_as(&mut self, dir: Option<&Path>, name: &str, replace: bool) -> Result<PathBuf, SaveError> {
        let dir = dir.ok_or(SaveError::NoFolder)?;
        let path = store::file_in(dir, name)?;
        let refusals = self.refusals();
        if !refusals.is_empty() {
            return Err(SaveError::Refused(refusals));
        }
        if path.exists() && !replace && self.path.as_deref() != Some(path.as_path()) {
            return Err(SaveError::Exists(path));
        }
        store::save_text(&path, &self.text())?;
        self.path = Some(path.clone());
        self.read_only = false;
        self.dirty = false;
        Ok(path)
    }
}
