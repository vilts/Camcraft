//! The file mirror of a linked model's source (#2463, project v72): the
//! path a model was imported from and the SHA-256 of the bytes read.
//! Absent is "not linked", what every v71 model was. An old reader must
//! refuse a v72 file: a linked model it saved again would come back
//! unlinked without a word.
//! Provenance: our own DTO, the pattern of the other entity keys.

use serde::{Deserialize, Serialize};

use crate::schema::ModelSource;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceDto {
    pub path: String,
    pub sha256: String,
}

impl SourceDto {
    #[must_use]
    pub fn of(source: &ModelSource) -> Self {
        SourceDto { path: source.path.clone(), sha256: source.sha256.clone() }
    }

    #[must_use]
    pub fn to_source(&self) -> ModelSource {
        ModelSource { path: self.path.clone(), sha256: self.sha256.clone() }
    }

    /// A source row is a model's, names a file, and holds a SHA-256 digest
    /// in lowercase hex. Anything else is a corrupt file.
    pub fn check(&self, owner: &str, is_model: bool) -> Result<(), String> {
        if !is_model {
            return Err(format!("'{owner}' records a source file but is not a model"));
        }
        if self.path.trim().is_empty() {
            return Err(format!("'{owner}' records a source with no file path"));
        }
        if !crate::model_source::is_digest(&self.sha256) {
            return Err(format!("'{owner}' records a source whose sha256 is not 64 lowercase hex digits"));
        }
        Ok(())
    }
}
