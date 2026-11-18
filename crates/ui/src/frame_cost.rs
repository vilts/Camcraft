//! Named costs of one frame for the headless frame probe (#2238): a span
//! is a no-op flag check unless a probe records, then it adds its wall
//! time and one count under its name. Display only - nothing reads a span
//! back to decide what to draw. Our own design notes; no vendor material
//! consulted.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

thread_local! {
    /// True while [`record`] runs on this thread.
    static ON: Cell<bool> = const { Cell::new(false) };
    /// Name -> (calls, total time), in first-seen order.
    static COSTS: RefCell<Vec<Cost>> = const { RefCell::new(Vec::new()) };
}

/// One named piece of a frame: how often it ran and its total time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cost {
    /// `phase/piece`, for example `scene/parts.models`.
    pub name: &'static str,
    /// Calls while recording.
    pub count: u32,
    /// Summed wall time of those calls.
    pub total: Duration,
}

/// A running span; its time is added when it drops.
#[must_use = "a span measures until it drops"]
pub struct Span(Option<(&'static str, Instant)>);

/// Open a span named `name` (a `'static` `phase/piece` string).
pub fn span(name: &'static str) -> Span {
    if ON.with(Cell::get) { Span(Some((name, Instant::now()))) } else { Span(None) }
}

/// Run `f` inside a span named `name`.
pub fn time<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    let _span = span(name);
    f()
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((name, start)) = self.0.take() else { return };
        let took = start.elapsed();
        COSTS.with(|c| {
            let mut c = c.borrow_mut();
            if let Some(cost) = c.iter_mut().find(|k| k.name == name) {
                cost.count = cost.count.saturating_add(1);
                cost.total = cost.total.saturating_add(took);
            } else {
                c.push(Cost { name, count: 1, total: took });
            }
        });
    }
}

/// Run `f` with the spans of this thread recording; return its result and
/// the named costs. A nested call keeps the outer record.
pub fn record<R>(f: impl FnOnce() -> R) -> (R, Vec<Cost>) {
    if ON.with(Cell::get) {
        return (f(), Vec::new());
    }
    struct Off;
    impl Drop for Off {
        fn drop(&mut self) {
            ON.with(|o| o.set(false));
        }
    }
    COSTS.with(|c| c.borrow_mut().clear());
    ON.with(|o| o.set(true));
    let off = Off;
    let out = f();
    drop(off);
    (out, COSTS.with(|c| std::mem::take(&mut *c.borrow_mut())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_count_only_while_recording() {
        drop(span("ui/outside"));
        let ((), costs) = record(|| {
            for _ in 0..3 {
                time("ui/inside", || ());
            }
        });
        assert_eq!(costs.len(), 1, "{costs:?}");
        assert_eq!((costs[0].name, costs[0].count), ("ui/inside", 3));
        drop(span("ui/after"));
        let ((), again) = record(|| ());
        assert!(again.is_empty(), "{again:?}");
    }
}
