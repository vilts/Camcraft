//! #2181 / #2192 review: the playback keyframe store and the
//! calculation before playback, on plain values. Every insertion counts
//! against the budget (the calculation's and the live playback's), the
//! thinning doubles the stride, and every end of the calculation is
//! said by name. #2194: a keyframe pays only for what the keyframe
//! before it does not share, and a shared cut brick is copied before a
//! write. Provenance: our own fixtures; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::play_keys, camcraft_sim::play_precalc, camcraft_sim::stock_sim::field.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::play_keys::{KeyCost, Keyframes, Precalc};
use camcraft_sim::stock_sim::{SdfField, SdfTool, Stock, FIELD_BAND};
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

/// A keyframe of shared parts, 10 bytes each: a part costs only where
/// the keyframe before does not hold the same allocation.
#[derive(Clone, Debug)]
struct Parts(Vec<Arc<u8>>);

impl KeyCost for Parts {
    fn key_bytes(&self) -> usize {
        self.0.len() * 10
    }
    fn key_bytes_after(&self, prev: &Self) -> usize {
        self.0.iter().filter(|p| !prev.0.iter().any(|q| Arc::ptr_eq(p, q))).count() * 10
    }
}

#[test]
fn a_keyframe_costs_what_the_one_before_does_not_share() {
    let mut k = Keyframes::new(Parts(Vec::new()), 1, 32);
    k.restart(1, 1024, 45);
    let mut live = Parts(Vec::new());
    for at in 1..=4 {
        live.0.push(Arc::new(0));
        assert!(k.offer(at, &live), "key {at} fits: one new part");
    }
    // Whole copies cost 10 + 20 + 30 + 40 and would thin at key 3.
    assert_eq!((k.len(), k.stride(), k.bytes(), k.held_bytes()), (5, 1, 40, 40));
    // The budget still thins, and the count follows the keys that stay:
    // key 2 pays for the parts of keys 1 and 2, key 4 for those of 3 and 4.
    live.0.push(Arc::new(0));
    assert!(!k.offer(5, &live), "key 5 is off the doubled stride");
    assert_eq!((k.len(), k.stride(), k.bytes(), k.held_bytes()), (3, 2, 40, 40));
}

fn slot(f: &mut SdfField, y: f64) {
    let mv = Move::new(MoveKind::Feed, Point3::new(-1.0, y, 3.3), Point3::new(13.0, y, 3.3), Some(500.0));
    f.subtract(&mv, &SdfTool::Flat { r: 0.7, flank: 10.0 }, 0.01).expect("cut");
}

/// Every cell centre's distance, as bits.
fn read(f: &SdfField) -> Vec<u64> {
    let c = |i: i32| f64::from(i) * 0.25 + 0.125;
    let mut out = Vec::new();
    for i in -4..52 {
        for j in -4..52 {
            for k in 0..20 {
                out.push(f.distance(Point3::new(c(i), c(j), c(k))).to_bits());
            }
        }
    }
    out
}

#[test]
fn a_clone_shares_its_cut_bricks_and_a_write_copies_first() {
    let (lo, hi) = (Point3::new(0.0, 0.0, 0.0), Point3::new(12.0, 12.0, 4.0));
    let mut f = SdfField::from_box(lo, hi, 0.25, FIELD_BAND).expect("field");
    slot(&mut f, 2.1);
    let key = f.clone();
    let (whole, shared) = (f.cut_layer_bytes(), f.cut_bytes_after(&key));
    assert!(whole > 0 && shared * 16 < whole, "a clone holds its map entries only: {shared} of {whole} bytes");
    let before = read(&key);
    // A second slot far from the first: new bricks, and none of the old.
    slot(&mut f, 9.9);
    assert_eq!(read(&key), before, "the keyframe is not written through");
    let own = f.cut_bytes_after(&key);
    assert!(own > shared && own * 3 < f.cut_layer_bytes() * 2, "only the new slot's bricks are the field's own: {own} of {}", f.cut_layer_bytes());
    // A write INTO a shared brick copies it: the keyframe keeps its value.
    slot(&mut f, 2.4);
    assert_eq!(read(&key), before, "a shared brick is copied before the write");
    let mut fresh = SdfField::from_box(lo, hi, 0.25, FIELD_BAND).expect("field");
    for y in [2.1, 9.9, 2.4] {
        slot(&mut fresh, y);
    }
    assert_eq!(read(&f), read(&fresh), "the cut over shared bricks is the plain cut, bit for bit");
    let (a, b) = (Stock::Field(f), Stock::Field(key));
    assert_eq!(a.key_bytes_after(&b), a.field().map_or(0, |f| f.cut_bytes_after(b.field().expect("field"))));
    assert!(a.key_bytes_after(&b) < a.key_bytes() + a.key_bytes() / 16 && b.key_bytes() > 0);
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
