//! The GPU compute off switch and the once-only fallback log (#2620).
//! `CAMCRAFT_GPU_COMPUTE=off` keeps every kernel on the CPU path: for
//! tests, and for machines with bad drivers (#2424, #74). It does not
//! touch the window's adapter choice (`CAMCRAFT_GPU`, `CAMCRAFT_GPU_BACKEND`).
//! Provenance: our own rule.

use std::sync::Mutex;

/// The env var of the switch.
pub const COMPUTE_ENV: &str = "CAMCRAFT_GPU_COMPUTE";

/// The switch for one value of `CAMCRAFT_GPU_COMPUTE`: `Err` (the reason)
/// for `off`, `0`, `false` or `no` in any case; `Ok` when unset or any
/// other value.
pub fn compute_switch(value: Option<&str>) -> Result<(), String> {
    let off = value.map(|v| v.trim().to_ascii_lowercase()).is_some_and(|v| {
        matches!(v.as_str(), "off" | "0" | "false" | "no")
    });
    if off {
        Err(format!("GPU compute is off ({COMPUTE_ENV}=off)"))
    } else {
        Ok(())
    }
}

/// The switch as the process environment sets it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn env_switch() -> Result<(), String> {
    compute_switch(std::env::var(COMPUTE_ENV).ok().as_deref())
}

/// Reasons already logged; bounded, so a stream of distinct reasons cannot grow it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
static LOGGED: Mutex<Vec<String>> = Mutex::new(Vec::new());
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const LOG_CAP: usize = 32;

/// Log a fallback reason one time for each process. A poisoned lock still
/// logs: a lost diagnostic is worse than a repeated one.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn log_fallback_once(reason: &str) {
    let first = match LOGGED.lock() {
        Ok(mut seen) => {
            if seen.iter().any(|s| s == reason) {
                false
            } else {
                if seen.len() < LOG_CAP {
                    seen.push(reason.to_string());
                }
                true
            }
        }
        Err(_) => true,
    };
    if first {
        tracing::warn!(target: "camcraft::gpu", "GPU compute not used, the CPU path runs: {reason}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_values_turn_compute_off_by_name() {
        for v in ["off", "OFF", " 0 ", "false", "No"] {
            let err = compute_switch(Some(v)).err().unwrap_or_default();
            assert!(err.contains(COMPUTE_ENV), "{v}: {err}");
        }
        for v in [None, Some("on"), Some("1"), Some("")] {
            assert_eq!(compute_switch(v), Ok(()), "{v:?}");
        }
    }
}
