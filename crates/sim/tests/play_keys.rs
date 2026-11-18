//! #2181 / #2192 review: the playback keyframe store and the
//! calculation before playback, on plain values. Every insertion counts
//! against the budget (the calculation's and the live playback's), the
//! thinning doubles the stride, and every end of the calculation is
//! said by name. Provenance: our own fixtures; nothing from vendor
//! binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::play_keys, camcraft_sim::play_precalc.

use std::cell::{Cell, RefCell};

use camcraft_sim::play_keys::{KeyCost, Keyframes, Precalc};
use camcraft_sim::play_precalc::{drive, PrecalcCut, PrecalcEnd, PrecalcOwner};

/// A keyframe that costs its own number of bytes.
#[derive(Clone, Debug)]
struct Key(usize);

impl KeyCost for Key {
    fn key_bytes(&self) -> usize {
        self.0
    }
}

#[test]
fn without_a_budget_the_count_cap_alone_holds() {
    let mut k = Keyframes::new(Key(0), 4, 2);
    assert!(k.offer(4, &Key(10)) && k.offer(8, &Key(10)));
    assert!(!k.offer(12, &Key(10)), "the cap of 2 keyframes past key 0 is full");
    assert!(!k.offer(8, &Key(10)), "a keyframe at or before the last one is refused");
    assert_eq!((k.len(), k.bytes(), k.held_bytes(), k.stride()), (3, 20, 20, 4));
    assert_eq!((k.floor_at(11), k.floor_at(3), k.next_boundary(5, 7)), (8, 0, 7));
}

#[test]
fn a_budget_thins_every_insertion_from_then_on() {
    let mut k = Keyframes::new(Key(0), 1, 32);
    k.restart(1, 1024, 25);
    // The callers offer on the stride, as the live seek does.
    for at in 1..=40 {
        if at % k.stride() != 0 {
            continue;
        }
        k.offer(at, &Key(10));
        assert_eq!(k.bytes(), k.held_bytes(), "at {at}: each kept keyframe is counted");
        assert!(k.bytes() <= 25, "at {at}: {} bytes over the budget", k.bytes());
    }
    assert!(k.stride() > 1 && k.len() > 1, "thinning doubled the stride: {} keys, stride {}", k.len(), k.stride());
    assert_eq!(k.floor_at(0), 0, "key 0 stays");
}

#[test]
fn a_zero_budget_keeps_only_key_zero() {
    let mut k = Keyframes::new(Key(0), 2, 32);
    k.restart(2, 1024, 0);
    for at in (2..=20).step_by(2) {
        assert!(!k.offer(at, &Key(1)));
    }
    assert_eq!((k.len(), k.bytes()), (1, 0));
}

/// A program of `total` moves cut `stride` at a time; `fail_at` refuses.
struct Prog {
    total: usize,
    at: usize,
    stride: usize,
    fail_at: Option<usize>,
}

impl PrecalcCut for Prog {
    fn total(&self) -> usize {
        self.total
    }
    fn done(&self) -> usize {
        self.at
    }
    fn begin(&mut self) -> Result<(), String> {
        self.at = 0;
        Ok(())
    }
    fn step(&mut self) -> Result<bool, String> {
        if self.fail_at.is_some_and(|f| self.at >= f) {
            return Err("refused".into());
        }
        self.at = (self.at + self.stride).min(self.total);
        Ok(self.at >= self.total)
    }
    fn ready(&self, secs: f64) -> Precalc {
        Precalc::Ready { keys: 1, bytes: 0, stride: self.stride, secs }
    }
}

#[derive(Default)]
struct Owner {
    stop_after: Option<usize>,
    cancel_after: Option<usize>,
    reports: RefCell<Vec<Precalc>>,
    asked: Cell<usize>,
}

impl PrecalcOwner for Owner {
    fn stopped(&self) -> bool {
        self.asked.set(self.asked.get() + 1);
        self.stop_after.is_some_and(|n| self.asked.get() > n)
    }
    fn cancelled(&self) -> bool {
        self.cancel_after.is_some_and(|n| self.asked.get() > n)
    }
    fn elapsed(&self) -> f64 {
        0.5
    }
    fn report(&self, p: Precalc) {
        self.reports.borrow_mut().push(p);
    }
}

fn last(o: &Owner) -> Precalc {
    o.reports.borrow().last().cloned().unwrap_or(Precalc::Off)
}

#[test]
fn the_calculation_says_each_end_by_name() {
    let mut p = Prog { total: 10, at: 0, stride: 3, fail_at: None };
    let o = Owner::default();
    assert_eq!(drive(&mut p, &o), PrecalcEnd::Play);
    assert_eq!(last(&o), Precalc::Ready { keys: 1, bytes: 0, stride: 3, secs: 0.5 });
    assert!(o.reports.borrow().iter().any(|r| matches!(r, Precalc::Running { done: 3, total: 10 })));

    let mut p = Prog { total: 10, at: 0, stride: 3, fail_at: None };
    let o = Owner { cancel_after: Some(1), ..Owner::default() };
    assert_eq!(drive(&mut p, &o), PrecalcEnd::Play, "a cancel keeps playback");
    assert_eq!(last(&o), Precalc::Incomplete { done: 3, total: 10, why: "cancelled".into() });

    let mut p = Prog { total: 10, at: 0, stride: 3, fail_at: None };
    let o = Owner { stop_after: Some(2), ..Owner::default() };
    assert_eq!(drive(&mut p, &o), PrecalcEnd::Stopped);
    assert_eq!(last(&o), Precalc::Incomplete { done: 6, total: 10, why: "stopped".into() });

    let mut p = Prog { total: 10, at: 0, stride: 3, fail_at: Some(3) };
    let o = Owner::default();
    assert_eq!(drive(&mut p, &o), PrecalcEnd::Refused("refused".into()));
    assert_eq!(last(&o), Precalc::Incomplete { done: 3, total: 10, why: "refused".into() });
}
