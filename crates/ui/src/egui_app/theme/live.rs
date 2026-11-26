//! Live theme storage: process-wide at runtime, isolated per unit-test thread.
//! Provenance: original theme storage extracted and isolated for issue #2373.

use super::Tokens;

// The application keeps one live table. Unit tests render independent
// documents on separate harness threads; their theme choices must not leak
// into another test's overlay reads or its default dark frame.
#[cfg(not(test))]
static LIVE: std::sync::OnceLock<std::sync::RwLock<Tokens>> = std::sync::OnceLock::new();

#[cfg(not(test))]
fn live() -> &'static std::sync::RwLock<Tokens> {
    LIVE.get_or_init(|| std::sync::RwLock::new(Tokens::DARK))
}

#[cfg(test)]
thread_local! {
    static LIVE: std::cell::Cell<Tokens> = const { std::cell::Cell::new(Tokens::DARK) };
}

/// The theme in force. A poisoned runtime cell still answers: Tokens is
/// Copy, so a panicked writer left either the old or the new table.
#[must_use]
pub fn current() -> Tokens {
    #[cfg(not(test))]
    {
        *live().read().unwrap_or_else(|p| p.into_inner())
    }
    #[cfg(test)]
    {
        LIVE.with(std::cell::Cell::get)
    }
}

/// Install the live theme without touching a context. The shell's theme
/// sync is the runtime writer; a refused theme keeps the previous table.
pub fn set_current(t: &Tokens) {
    #[cfg(not(test))]
    {
        *live().write().unwrap_or_else(|p| p.into_inner()) = *t;
    }
    #[cfg(test)]
    {
        LIVE.with(|live| live.set(*t));
    }
}
