//! Operation steps on the simulation timeline (#2608): where each
//! operation starts, and the start that a "next operation" or "previous
//! operation" step goes to. Pure lookups over [`Script`], so the
//! transport bar and the stand-alone simulator (#2587) step the same way.
//! Provenance: our own timeline (`sim::Script`); nothing from vendor
//! binaries.
//!
//! An operation starts at its first segment. That is the connector rapid
//! when there is one: the script gives the rapid between two programs to
//! the operation that it arrives for (its tool is in the spindle). At
//! that time the earlier operations are complete, and the operation has
//! cut nothing. An operation with no moves is not in the script, so it is
//! never a stop.
//!
//! [`OpStarts`] walks the segments one time. A caller that asks in each
//! frame (the transport bar) keeps it beside its script; each lookup is
//! then a binary search over one entry for each operation.

use crate::sim::Script;

/// Two starts closer than this are the same position (seconds). A
/// step lands on a start exactly, so the tolerance only absorbs the
/// rounding of a playhead that a slider or a clock set.
const SAME_T: f64 = 1e-9;

/// Each operation's start on one script, in program order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpStarts {
    /// (index into [`Script::ops`], seconds), ascending in time. One
    /// entry for each run of segments of one operation.
    starts: Vec<(usize, f64)>,
    /// The script's total: a non-finite playhead reads as the end.
    total: f64,
}

impl OpStarts {
    /// Walk the segments of `sc` one time.
    #[must_use]
    pub fn of(sc: &Script) -> Self {
        let mut starts: Vec<(usize, f64)> = Vec::new();
        for (i, s) in sc.segs.iter().enumerate() {
            if starts.last().is_some_and(|&(op, _)| op == s.op) {
                continue;
            }
            // Segment `i` starts where segment `i - 1` ends.
            let start = if i == 0 { 0.0 } else { sc.cum.get(i - 1).copied().unwrap_or(0.0) };
            starts.push((s.op, start));
        }
        Self { starts, total: sc.total }
    }

    /// The starts: (index into [`Script::ops`], seconds).
    #[must_use]
    pub fn as_slice(&self) -> &[(usize, f64)] {
        &self.starts
    }

    fn clamp(&self, t: f64) -> f64 {
        if t.is_finite() { t } else { self.total }
    }

    /// The start of the first operation after the playhead `t`, or
    /// `None` in the last operation (and on an empty script).
    #[must_use]
    pub fn next(&self, t: f64) -> Option<f64> {
        let t = self.clamp(t);
        let i = self.starts.partition_point(|&(_, s)| s <= t + SAME_T);
        self.starts.get(i).map(|&(_, s)| s)
    }

    /// The start of the operation under the playhead `t`, or - when the
    /// playhead already stands at that start - the start of the one
    /// before it. `None` at the start of the first operation (and on an
    /// empty script).
    #[must_use]
    pub fn prev(&self, t: f64) -> Option<f64> {
        let t = self.clamp(t);
        let i = self.starts.partition_point(|&(_, s)| s < t - SAME_T);
        i.checked_sub(1).and_then(|k| self.starts.get(k)).map(|&(_, s)| s)
    }

    /// The operation (index into [`Script::ops`]) that starts at `start`,
    /// a value that [`Self::next`] or [`Self::prev`] gave.
    #[must_use]
    pub fn op_at(&self, start: f64) -> Option<usize> {
        let i = self.starts.partition_point(|&(_, s)| s < start - SAME_T);
        self.starts.get(i).filter(|&&(_, s)| (s - start).abs() <= SAME_T).map(|&(op, _)| op)
    }
}

impl Script {
    /// Each operation's start, in program order: (index into
    /// [`Script::ops`], seconds). Walks every segment: a caller that asks
    /// in each frame keeps an [`OpStarts`] instead.
    #[must_use]
    pub fn op_starts(&self) -> Vec<(usize, f64)> {
        OpStarts::of(self).starts
    }

    /// [`OpStarts::next`] on this script (walks every segment).
    #[must_use]
    pub fn next_op_start(&self, t: f64) -> Option<f64> {
        OpStarts::of(self).next(t)
    }

    /// [`OpStarts::prev`] on this script (walks every segment).
    #[must_use]
    pub fn prev_op_start(&self, t: f64) -> Option<f64> {
        OpStarts::of(self).prev(t)
    }

    /// The name of the operation that starts at `start` (a value that
    /// `next_op_start` or `prev_op_start` gave): the step tips name it.
    #[must_use]
    pub fn op_starting_at(&self, start: f64) -> Option<&str> {
        self.op_name(OpStarts::of(self).op_at(start)?)
    }

    /// The name of operation `op` (an index into [`Script::ops`]).
    #[must_use]
    pub fn op_name(&self, op: usize) -> Option<&str> {
        self.ops.get(op).map(String::as_str)
    }
}
