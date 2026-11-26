//! v55 (#343): a setup's own machine and post as files hold them. Both
//! keys are absent when the setup follows the document's machine or post
//! (every older file, and every setup never given its own), so the
//! absent key is the whole migration. A v55 file is refused by an older
//! reader (the version step), which would otherwise post every setup on
//! the document's machine without a word.
//!
//! Provenance: our own file grammar (the `MachineDto` / `PostDto` shapes
//! restated for one setup); no vendor material consulted.

use super::*;
use crate::schema::{MachineDef, PostRef};

/// A setup's own machine: the `.mch` path it was loaded from (display
/// only) and the parsed definition; `machine` absent is a setup that
/// deliberately mounts NO machine while the document has one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupMachineDto {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<crate::machine::Machine>,
}

/// A setup's own post. Unlike the document's `PostDto` the shipped
/// default must be spellable (`"builtin"`): absent already means
/// "follows the document".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SetupPostDto {
    Builtin,
    Named(String),
    File(String),
}

impl SetupMachineDto {
    #[must_use]
    pub fn from_def(d: &MachineDef) -> Self {
        Self { path: d.path.clone(), machine: d.machine.clone() }
    }

    /// The live value; a machine stating an impossible limit is a corrupt
    /// file, refused by the setup's name.
    pub fn to_def(&self, setup: &str) -> Result<MachineDef, String> {
        if let Some(r) = self.machine.as_ref().and_then(crate::machine::tooling_refusal) {
            return Err(format!("setup '{setup}' mounts a machine whose {r}"));
        }
        if let Some(m) = &self.machine
            && !m.is_sane()
        {
            return Err(format!("setup '{setup}' mounts a machine that states a limit that cannot be meant"));
        }
        Ok(MachineDef { path: self.path.clone(), machine: self.machine.clone() })
    }
}

impl SetupPostDto {
    #[must_use]
    pub fn from_ref(p: &PostRef) -> Self {
        match p {
            PostRef::Builtin => SetupPostDto::Builtin,
            PostRef::Named(n) => SetupPostDto::Named(n.clone()),
            PostRef::File(f) => SetupPostDto::File(f.clone()),
        }
    }

    #[must_use]
    pub fn to_ref(&self) -> PostRef {
        match self {
            SetupPostDto::Builtin => PostRef::Builtin,
            SetupPostDto::Named(n) => crate::posting::agie::current_ref(PostRef::Named(n.clone())),
            SetupPostDto::File(f) => PostRef::File(f.clone()),
        }
    }
}
