//! Provenance: explicit tap handedness and safe refusal, from issue #1453.
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHand {
    #[default]
    Right,
    Left,
}

impl ThreadHand {
    pub fn is_right(&self) -> bool {
        *self == Self::Right
    }
}

impl<'de> Deserialize<'de> for ThreadHand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)?.as_str() {
            Some("right") => Ok(Self::Right),
            Some("left") => Ok(Self::Left),
            _ => Err(serde::de::Error::custom("thread hand must be right or left")),
        }
    }
}
