//! API keys (#1945): a key lives in the OS keychain or in an environment
//! variable, never in `prefs.json` or a project file. The `ApiKey` type
//! has no `Display` and a redacted `Debug`, so a key cannot reach a log,
//! a status line or an error message by formatting.
//!
//! Provenance: our own design; the variable names are the ones each
//! provider's public SDK documentation reads (`ANTHROPIC_API_KEY`,
//! `OPENAI_API_KEY`, `GEMINI_API_KEY`).

use std::fmt;

/// Which provider account a key belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyName {
    Anthropic,
    OpenAi,
    Gemini,
}

impl KeyName {
    pub const ALL: [Self; 3] = [Self::Anthropic, Self::OpenAi, Self::Gemini];

    /// The environment variable read before the keychain.
    #[must_use]
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
        }
    }

    /// The keychain account name (under the service `KEYCHAIN_SERVICE`).
    #[must_use]
    pub fn account(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    /// The provider's name for messages.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::OpenAi => "OpenAI",
            Self::Gemini => "Gemini",
        }
    }
}

/// The keychain service every CamCraft AI key is filed under.
pub const KEYCHAIN_SERVICE: &str = "camcraft-ai";

/// Where a key was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    Environment,
    Keychain,
}

/// A provider API key. Opaque: no `Display`, `Debug` prints `ApiKey(***)`.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Accept a key. Refuses an empty one and any control character (a
    /// newline in a header value would split the request).
    pub fn new(text: &str) -> Result<Self, String> {
        let t = text.trim();
        if t.is_empty() {
            return Err("the API key is empty".into());
        }
        if t.chars().any(char::is_control) {
            return Err("the API key holds a control character".into());
        }
        Ok(Self(t.to_owned()))
    }

    /// The key text, for the auth header only.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(***)")
    }
}

/// Find the key for `name`: the environment variable first, then the
/// keychain. `env` and `keychain` are passed in so the order is testable
/// without a real keychain (the native one is `keychain::load`).
pub fn lookup(
    name: KeyName,
    env: impl Fn(&str) -> Option<String>,
    keychain: impl Fn(KeyName) -> Result<Option<ApiKey>, String>,
) -> Result<Option<(ApiKey, KeySource)>, String> {
    if let Some(v) = env(name.env_var()).filter(|v| !v.trim().is_empty()) {
        let key = ApiKey::new(&v).map_err(|e| format!("{}: {e}", name.env_var()))?;
        return Ok(Some((key, KeySource::Environment)));
    }
    Ok(keychain(name)?.map(|k| (k, KeySource::Keychain)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_key() {
        let k = ApiKey::new("  placeholder-not-a-key ").expect("accepted");
        assert_eq!(format!("{k:?}"), "ApiKey(***)");
        assert_eq!(k.expose(), "placeholder-not-a-key");
        assert!(ApiKey::new("a\r\nX-Evil: 1").is_err());
        assert!(ApiKey::new("   ").is_err());
    }

    #[test]
    fn environment_wins_over_the_keychain() {
        let env = |v: &str| (v == "OPENAI_API_KEY").then(|| "from-env".to_owned());
        let chain = |_| Ok(Some(ApiKey::new("from-chain").expect("ok")));
        let (k, src) = lookup(KeyName::OpenAi, env, chain).expect("ok").expect("found");
        assert_eq!((k.expose(), src), ("from-env", KeySource::Environment));
        let (k, src) = lookup(KeyName::Gemini, env, chain).expect("ok").expect("found");
        assert_eq!((k.expose(), src), ("from-chain", KeySource::Keychain));
        assert!(lookup(KeyName::Anthropic, |_| None, |_| Ok(None)).expect("ok").is_none());
    }
}
