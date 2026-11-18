//! #2192 review, finding 4: the begin/step orchestration, the seek law
//! and the progress and cancel mailbox are the sim crate's, and hold on
//! plain values with no app type. Provenance: our own fixtures; nothing
//! from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::play_precalc, camcraft_sim::play_precalc_mailbox, camcraft_sim::play_keys.

use std::cell::Cell;
use std::sync::Mutex;

use camcraft_sim::play_keys::{KeyCost, Keyframes, Precalc};
use camcraft_sim::play_precalc::{begin, drive, step, Calc, PrecalcEnd, Playback};
use camcraft_sim::play_precalc_mailbox::{Mailbox, MailboxOwner};

/// The value after `n` moves: `n` itself, at a fixed cost per keyframe.
#[derive(Clone, Debug, PartialEq)]
struct Cut(usize);

impl KeyCost for Cut {
    fn key_bytes(&self) -> usize {
        10
    }
}

/// A playback of `total` moves whose value is the moves cut; it counts
/// what it cuts and can refuse past `fail_at`.
struct Line {
    total: usize,
    at: usize,
    cur: Cut,
    keys: Keyframes<Cut>,
    cut: usize,
    rewound: bool,
    fail_at: Option<usize>,
}

impl Line {
    fn new(total: usize) -> Self {
        Line { total, at: 0, cur: Cut(0), keys: Keyframes::new(Cut(0), 1, 32), cut: 0, rewound: false, fail_at: None }
    }
}

impl Playback for Line {
    type Value = Cut;
    fn total(&self) -> usize {
        self.total
    }
    fn at(&self) -> usize {
        self.at
    }
    fn keys(&self) -> &Keyframes<Cut> {
        &self.keys
    }
    fn keys_and_value(&mut self) -> (&mut Keyframes<Cut>, &Cut) {
        (&mut self.keys, &self.cur)
    }
    fn rewind(&mut self, uncut: Cut) {
        self.cur = uncut;
        self.at = 0;
        self.rewound = true;
    }
    fn cut_to(&mut self, to: usize) -> Result<(), String> {
        if self.fail_at.is_some_and(|f| to > f) {
            return Err("the cut refused".into());
        }
        self.cut += to - self.at;
        self.at = to;
        self.cur = Cut(to);
        Ok(())
    }
}

#[test]
fn domain_calculation_resets_cuts_and_reports_the_bounded_store() {
    let mut p = Line::new(100);
    p.at = 37;
    p.cur = Cut(37);
    begin(&mut p, 10, 4, 1000).expect("begin");
    assert!(p.rewound && p.at == 0 && p.cur == Cut(0), "begin stands at the uncut value");
    assert_eq!((p.keys.stride(), p.keys.budget(), p.keys.len()), (10, Some(1000), 1));
    while !step(&mut p).expect("a stride cuts") {}
    assert_eq!(p.cut, 100, "the program is cut one time");
    assert_eq!(p.keys.len(), 11, "a keyframe at each stride boundary, key 0 included");
    assert!(p.keys.floor(55).is_some_and(|k| *k == (50, Cut(50))), "each keyframe is the exact prefix");
    assert!(step(&mut p).expect("the end"), "a step past the end cuts nothing");
    assert_eq!(p.cut, 100);

    // The minimum stride wins over a small program, the budget thins.
    let mut p = Line::new(100);
    begin(&mut p, 10, 30, 25).expect("begin");
    assert_eq!(p.keys.stride(), 30);
    while !step(&mut p).expect("a stride cuts") {}
    assert!(p.keys.bytes() <= 25 && p.keys.bytes() == p.keys.held_bytes(), "{} bytes", p.keys.bytes());

    // `Calc` reports the store it left.
    let mut p = Line::new(40);
    let o = MailboxOwner { mailbox: &Mailbox::default(), stopped: || false, elapsed: || 2.0 };
    let mut c = Calc { play: &mut p, keys: 4, min_stride: 1, budget: 1 << 20 };
    assert_eq!(drive(&mut c, &o), PrecalcEnd::Play);
    assert_eq!(o.mailbox.read(40), Precalc::Ready { keys: 5, bytes: 40, stride: 10, secs: 2.0 });
}

#[test]
fn domain_mailbox_cancel_preserves_partial_checkpoints_and_names_the_end() {
    let mb = Mailbox::default();
    assert_eq!(mb.read(9), Precalc::Off, "nothing asked reads Off");
    let mut p = Line::new(100);
    let asked = Cell::new(0usize);
    let o = MailboxOwner {
        mailbox: &mb,
        stopped: || {
            asked.set(asked.get() + 1);
            if asked.get() > 3 {
                mb.cancel();
            }
            false
        },
        elapsed: || 0.0,
    };
    assert_eq!(drive(&mut Calc { play: &mut p, keys: 10, min_stride: 1, budget: 1 << 20 }, &o), PrecalcEnd::Play);
    assert_eq!(mb.read(100), Precalc::Incomplete { done: 30, total: 100, why: "cancelled".into() });
    assert_eq!(p.keys.len(), 4, "the keyframes made before the cancel stay");
    assert!(p.keys.floor(99).is_some_and(|k| *k == (30, Cut(30))));

    // A refused cut ends by name too.
    let mb = Mailbox::default();
    let mut p = Line::new(100);
    p.fail_at = Some(25);
    let o = MailboxOwner { mailbox: &mb, stopped: || false, elapsed: || 0.0 };
    let end = drive(&mut Calc { play: &mut p, keys: 10, min_stride: 1, budget: 1 << 20 }, &o);
    assert_eq!(end, PrecalcEnd::Refused("the cut refused".into()));
    assert_eq!(mb.read(100), Precalc::Incomplete { done: 20, total: 100, why: "the cut refused".into() });
}

#[test]
fn domain_mailbox_reports_progress_and_uses_the_owners_stop_and_clock() {
    let mb = Mailbox::default();
    let seen = Mutex::new(Vec::new());
    let mut p = Line::new(30);
    let o = MailboxOwner {
        mailbox: &mb,
        stopped: || {
            seen.lock().map(|mut s| s.push(mb.read(30))).ok();
            mb.read(30) == Precalc::Running { done: 20, total: 30 }
        },
        elapsed: || 7.5,
    };
    assert_eq!(drive(&mut Calc { play: &mut p, keys: 3, min_stride: 1, budget: 1 << 20 }, &o), PrecalcEnd::Stopped);
    assert_eq!(mb.read(30), Precalc::Incomplete { done: 20, total: 30, why: "stopped".into() });
    let seen = seen.into_inner().unwrap_or_default();
    assert!(seen.contains(&Precalc::Running { done: 10, total: 30 }), "progress is reported: {seen:?}");

    let mb = Mailbox::default();
    let mut p = Line::new(30);
    let o = MailboxOwner { mailbox: &mb, stopped: || false, elapsed: || 7.5 };
    drive(&mut Calc { play: &mut p, keys: 3, min_stride: 1, budget: 1 << 20 }, &o);
    assert!(matches!(mb.read(30), Precalc::Ready { secs, .. } if secs == 7.5), "the owner's clock is read");
}

#[test]
fn a_seek_restores_behind_or_more_than_a_stride_ahead() {
    let mut k = Keyframes::new(Cut(0), 10, 32);
    for at in [10, 20, 30] {
        k.offer(at, &Cut(at));
    }
    assert!(k.restores(25, 5), "behind");
    assert!(k.restores(0, 25), "a keyframe at 20 is more than a stride ahead of 0");
    assert!(!k.restores(15, 22), "within one stride: cut on");
    assert!(!k.restores(30, 45), "no keyframe ahead");
    assert!(!k.restores(12, 12), "no move");
}
