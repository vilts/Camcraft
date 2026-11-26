//! Healing and import bench of the kernel on trial (M12.1 stage 3): the
//! seam's `heal` on solidcore against closed forms (truck has no B-rep
//! healing - recorded as Unsupported, never as agreement), the port's native
//! B-rep text reader (`.smb`) round-tripping the bench fixtures, and a
//! vertex-gap fixture the healer must close. Every port call runs under a
//! time bound (smb_util). STEP is benched in step_agreement.rs and
//! step_import.rs, not here; the corrupt-file corpus is import_corpus.rs.
//! Provenance: our own bench design; closed forms are elementary solid
//! geometry (box, cylinder, box minus a through-cylinder, the quarter-round
//! section r^2 (1 - pi/4)).
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_import and the
//! `heal` arm of camcraft_kernel::{solidcore_impl, truck_impl}.

mod common;
mod smb_util;

use std::f64::consts::PI;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_import::{
    HEAL_GAP_LIMIT, VertexGaps, import_smb, read_smb, vertex_gaps, write_smb,
};
use camcraft_kernel::truck_impl::TruckKernel;
use camcraft_kernel::{Blend, Kernel, KernelError, Measure};
use common::{REL_FLOOR, Want, verdict};
use smb_util::{bounded, gapped_box, port_verdict};
use solidcore::topology::Brep;

/// The bench fixtures, built through the seam on solidcore.
fn fixture(k: &mut SolidcoreKernel, name: &str) -> Result<Brep, KernelError> {
    match name {
        "box 10" => k.make_box([0.0; 3], [10.0; 3]),
        "cylinder r3 h10" => k.make_cylinder([0.0; 3], 3.0, 10.0),
        "box - through cylinder r3" => {
            let b = k.make_box([0.0; 3], [10.0; 3])?;
            let c = k.make_cylinder([5.0, 5.0, -1.0], 3.0, 12.0)?;
            k.boolean_subtract(b, c)
        }
        "box | box (L)" => {
            let a = k.make_box([0.0; 3], [10.0, 5.0, 10.0])?;
            let b = k.make_box([0.0; 3], [5.0, 10.0, 10.0])?;
            k.boolean_union(a, b)
        }
        _ => {
            let b = k.make_box([0.0; 3], [10.0; 3])?;
            k.blend_edges(b, &[[5.0, 0.0, 0.0]], 2.0, Blend::Round)
        }
    }
}

fn wants() -> Vec<Want> {
    let cube = ([0.0; 3], [10.0; 3]);
    vec![
        Want {
            name: "box 10",
            volume: 1000.0,
            bbox: cube,
        },
        Want {
            name: "cylinder r3 h10",
            volume: 90.0 * PI,
            bbox: ([-3.0, -3.0, 0.0], [3.0, 3.0, 10.0]),
        },
        Want {
            name: "box - through cylinder r3",
            volume: 1000.0 - 90.0 * PI,
            bbox: cube,
        },
        Want {
            name: "box | box (L)",
            volume: 750.0,
            bbox: cube,
        },
        Want {
            name: "box, one edge round r2",
            volume: 1000.0 - 40.0 * (1.0 - PI / 4.0),
            bbox: cube,
        },
    ]
}

fn check(w: &Want, engine: &str, m: Result<Measure, KernelError>) -> Option<String> {
    match m {
        Ok(m) => verdict(engine, &m, w, REL_FLOOR * w.volume, 1e-6).err(),
        Err(e) => Some(format!("{}: {engine} {e}", w.name)),
    }
}

#[test]
fn heal_keeps_every_fixture_at_its_closed_form() {
    let mut fails = Vec::new();
    for w in wants() {
        let name = w.name;
        let m = bounded(name, move || {
            let mut k = SolidcoreKernel::default();
            let s = fixture(&mut k, name)?;
            k.heal(s).and_then(|mut h| k.measure(&mut h))
        });
        fails.extend(check(&w, "solidcore", m));
    }
    assert!(
        fails.is_empty(),
        "heal left the closed form:\n{}",
        fails.join("\n")
    );
}

/// truck has no B-rep healing: the seam says so by name. Recorded as a
/// "not yet" on the truck side, never counted as agreement.
#[test]
fn truck_offers_no_heal() {
    let mut t = TruckKernel::default();
    let r = t.make_box([0.0; 3], [10.0; 3]).and_then(|b| t.heal(b));
    println!("truck heal -> {:?}", r.as_ref().map(|_| "Ok"));
    assert!(
        matches!(
            r,
            Err(KernelError::Unsupported {
                kernel: "truck",
                op: "heal"
            })
        ),
        "truck heal: {:?}",
        r.map(|_| "Ok")
    );
}

#[test]
fn native_round_trip_matches_closed_form() {
    let mut fails = Vec::new();
    for w in wants() {
        let name = w.name;
        let m = bounded(name, move || {
            let mut k = SolidcoreKernel::default();
            let s = fixture(&mut k, name)?;
            let bytes = write_smb(&k, &s)?;
            let imp = import_smb(&mut k, &bytes)?;
            assert!(imp.notes.is_empty(), "{name}: notes {:?}", imp.notes);
            let mut b = imp.brep;
            k.measure(&mut b)
        });
        fails.extend(check(&w, "solidcore .smb", m));
    }
    assert!(
        fails.is_empty(),
        "round trip left the closed form:\n{}",
        fails.join("\n")
    );
}

/// Raw reader and importer on a vertex gap. The port's healer covers a gap
/// by growing the vertex tolerance, not by moving the vertex, so the tight
/// bounding box keeps the moved vertex. Under [`HEAL_GAP_LIMIT`] the import
/// is a solid within the gap's reach of the closed form (|dV| <= gap x the
/// box's surface area, bbox within the gap); past it, a clean refusal.
#[test]
fn vertex_gap_is_closed_by_the_importer() {
    let mut fails = Vec::new();
    let cube = Want {
        name: "gapped box 10",
        volume: 1000.0,
        bbox: ([0.0; 3], [10.0; 3]),
    };
    for gap in [1e-6, 1e-4, 5e-4, 2e-3, 1e-2] {
        let text = gapped_box(gap);
        let r = bounded("vertex gap", move || {
            let mut k = SolidcoreKernel::default();
            let raw = read_smb(&mut k, text.as_bytes());
            let raw = port_verdict(raw, &mut k);
            let read = read_smb(&mut k, text.as_bytes())?;
            let healed = k.heal(read);
            let gaps = healed
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|h| vertex_gaps(&k, h));
            let imp = import_smb(&mut k, text.as_bytes()).and_then(|i| {
                let mut b = i.brep;
                k.measure(&mut b)
            });
            Ok::<_, KernelError>((raw, gaps, imp))
        });
        let Ok((raw, gaps, imported)) = r else {
            fails.push(format!("gap {gap:e}: read {r:?}"));
            continue;
        };
        println!(
            "gap {gap:e}: read -> {raw}; healed (gap, vertex tol) {gaps:?}; import -> {imported:?}"
        );
        // within the limit: the healed vertex tolerance covers the gap
        if gap <= HEAL_GAP_LIMIT {
            match &gaps {
                Ok(VertexGaps {
                    gap: g,
                    max_vertex_tolerance: tol,
                    unjudged_edges: 0,
                }) if *tol >= *g * (1.0 - 1e-9) && *g <= HEAL_GAP_LIMIT => {}
                other => fails.push(format!("gap {gap:e}: healed gaps {other:?}")),
            }
            let tol = gap * 600.0 + REL_FLOOR * 1000.0;
            match imported
                .map_err(|e| e.to_string())
                .and_then(|m| verdict("solidcore", &m, &cube, tol, gap * 1.01))
            {
                Ok(()) => {}
                Err(e) => fails.push(format!("gap {gap:e}: {e}")),
            }
        } else if !matches!(imported, Err(KernelError::InvalidResult { op: "heal", .. })) {
            fails.push(format!("gap {gap:e} past the limit: import {imported:?}"));
        }
    }
    assert!(
        fails.is_empty(),
        "vertex gaps not healed:\n{}",
        fails.join("\n")
    );
}
