//! Pins for `rho_expanded` over the undercut heads (#1444 x #2055).
//! Provenance: our own brute-force Minkowski check; no vendor material.

use super::rho_expanded;
use crate::verify::probe::{Atom, Prof};

/// The grown radius at `w` of `head` stretched up by `lift`, by brute
/// force over the stretch and the height.
fn brute(head: &Atom, lift: f64, w: f64, m: f64) -> f64 {
    let mut best: f64 = 0.0;
    for i in 0..=200 {
        let t = lift * i as f64 / 200.0;
        // The height level with `w` itself (the only one when m = 0).
        if (head.z0..=head.z1).contains(&(w - t)) {
            best = best.max(head.rho(w - t) + m);
        }
        for j in 0..=400 {
            let z = head.z0 + (head.z1 - head.z0) * j as f64 / 400.0;
            let dz = w - (z + t);
            if dz.abs() <= m {
                best = best.max(head.rho(z) + (m * m - dz * dz).sqrt());
            }
        }
    }
    best
}

fn never_below_the_solid(head: Atom) -> f64 {
    let mut worst_over: f64 = 0.0;
    for lift in [0.0, 1.5, 6.0] {
        for m in [0.0, 0.5, 2.0] {
            let swept = Atom { z0: head.z0, z1: head.z1 + lift, ..head };
            for k in 0..=80 {
                let w = head.z0 - m - 0.5 + (head.z1 + lift - head.z0 + 2.0 * m + 1.0) * k as f64 / 80.0;
                let got = rho_expanded(&swept, w, m);
                let want = brute(&head, lift, w, m);
                assert!(got >= want - 1e-3, "{:?} lift {lift} m {m} w {w}: {got} < {want}", head.prof);
                worst_over = worst_over.max(got - want);
            }
        }
    }
    worst_over
}

#[test]
fn a_lifted_dovetail_head_is_never_grown_smaller_than_its_solid() {
    let head = Atom { z0: 0.0, z1: 4.0, prof: Prof::Dove { r: 8.0, k: 0.577 }, cap: true };
    never_below_the_solid(head);
}

#[test]
fn a_lifted_lollipop_ball_is_never_grown_smaller_than_its_solid() {
    // Main's law reads the ball as its circumscribed cylinder: it may
    // over-report the keep-out, never under-report it.
    let head = Atom { z0: 0.0, z1: 6.0, prof: Prof::Ball(3.0), cap: false };
    never_below_the_solid(head);
}
