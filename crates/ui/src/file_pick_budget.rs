//! Bounded browser batch reads, from our own memory budget (#3160).

use super::too_big;

/// Raw file bytes retained while one folder or drop is read. Leave room
/// for decoded text, script parsing, meshes and the rest of the wasm heap.
pub(crate) const BATCH_READ_LIMIT: u64 = 32 << 20;

#[derive(Default)]
pub(crate) struct ReadBudget {
    used: u64,
}

impl ReadBudget {
    /// Reserve before starting the read. An invalid or excess size does
    /// not consume the budget, so later smaller files can still be read.
    pub(crate) fn reserve(&mut self, name: &str, size: f64) -> Result<(), String> {
        if let Some(why) = too_big(name, size) {
            return Err(why);
        }
        if size < 0.0 || size.fract() != 0.0 {
            return Err(format!("{name}: the browser file size is invalid"));
        }
        let used = self
            .used
            .checked_add(size as u64)
            .filter(|n| *n <= BATCH_READ_LIMIT)
            .ok_or_else(|| {
                format!(
                    "{name}: the batch exceeds the {} MiB browser read limit",
                    BATCH_READ_LIMIT >> 20
                )
            })?;
        self.used = used;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn individually_allowed_files_cannot_exceed_the_batch_budget() {
        let mut budget = ReadBudget::default();
        let size = (BATCH_READ_LIMIT / 2) as f64;
        for name in ["first.mch", "second.cps", "excess.cps"] {
            assert!(too_big(name, size).is_none(), "each file alone is allowed");
        }
        budget.reserve("first.mch", size).expect("first half");
        budget.reserve("second.cps", size).expect("second half");
        let why = budget
            .reserve("excess.cps", size)
            .expect_err("aggregate refused before a read");
        assert!(why.contains("excess.cps") && why.contains("batch"), "{why}");
        assert_eq!(budget.used, BATCH_READ_LIMIT);
        assert!(budget.reserve("negative.mch", -1.0).is_err());
        assert!(budget.reserve("overflow.cps", f64::MAX).is_err());
    }
}
