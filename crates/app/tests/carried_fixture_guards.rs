//! #1530 review: the carried-workholding mesh reading only ACQUITS -
//! whatever it cannot read keeps the box's depth. Pinned on hand-built
//! carried bodies, so each guard is reached alone: an unread column line
//! (spent budget), an unread rectangle between the columns (an
//! inside-out shell over another), and a feature between the column
//! lines. The second review: the rectangle read's contact band and trim
//! are the tolerance (literals), and a rectangle too slim to trim keeps
//! the box.
//! Provenance: our own geometry.
//!
//! Gate map (#1530): covers app::fixture_carried (mesh_penetration,
//! penetration), sim::fixture_mesh::column (steel_on_line),
//! sim::fixture_mesh::prism (steel_in_prism).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_app::fixture::FixtureBox;
use camcraft_app::fixture_carried::{MESH_VISIT_BUDGET, mesh_penetration, penetration};
use camcraft_app::geometry::{Point3, Triangle, cube};
use camcraft_app::stock_sim::{Body, Dexel};
use camcraft_sim::fixture_mesh::FixtureMesh;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Facets of `boxes`; the `flipped` ones inside out.
fn facets(boxes: &[(Point3, Point3)], flipped: &[(Point3, Point3)]) -> Vec<Triangle> {
    let mut t: Vec<Triangle> = boxes.iter().flat_map(|(a, b)| cube(*a, *b).facets.iter().copied().collect::<Vec<_>>()).collect();
    for (a, b) in flipped {
        t.extend(cube(*a, *b).facets.iter().map(|f| Triangle { a: f.a, b: f.c, c: f.b }));
    }
    t
}

/// A carried body `n x n` columns of 1 mm, material `floor..top`.
fn body(n: f64, floor: f64, top: f64) -> Body {
    Body::on_floor(Dexel::block(0.0, 0.0, n, n, top, 1.0).expect("grid"), floor)
}

fn judge(body: &Body, tris: &[Triangle], budget: u64) -> (Option<f64>, Option<f64>) {
    let mesh = FixtureMesh::build(tris).expect("closed");
    let (lo, hi) = tris.iter().flat_map(|t| [t.a, t.b, t.c]).fold(
        (p(f64::INFINITY, f64::INFINITY, f64::INFINITY), p(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY)),
        |(lo, hi), q| (p(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z)), p(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z))),
    );
    let b = FixtureBox { min: lo, max: hi };
    let mut budget = budget;
    (mesh_penetration(body, &b, &mesh, &mut budget), penetration(body, &b))
}

#[test]
fn an_unread_column_line_keeps_the_box_depth() {
    // One column (centre 0.5, 0.5), material Z 0..10; two blocks either
    // side of its line, Z 2..8: the mesh clears it, the box does not.
    let b = body(1.0, 0.0, 10.0);
    let t = facets(&[(p(0.0, 0.0, 2.0), p(0.3, 1.0, 8.0)), (p(0.7, 0.0, 2.0), p(1.0, 1.0, 8.0))], &[]);
    let (mesh, boxed) = judge(&b, &t, MESH_VISIT_BUDGET);
    assert_eq!((mesh, boxed), (None, Some(6.0)), "read, the line is clear");
    let (mesh, boxed) = judge(&b, &t, 0);
    assert_eq!(mesh, boxed, "unread, the box speaks");
}

#[test]
fn an_inside_out_shell_between_the_columns_keeps_the_box_depth() {
    // 2 x 2 columns, material Z 5.5..9.5. Between them: A at Z 0..10 and
    // B at Z 5..15 inside out, both 0.505..1.495 - no facet in the
    // material slab of the rectangle between the column centres, and
    // the line through it winds negative. A plate under it (Z -3..-1)
    // puts the four centres inside the box; their lines see contact.
    let b = body(2.0, 5.5, 9.5);
    let s = (p(0.505, 0.505, 0.0), p(1.495, 1.495, 10.0));
    let flipped = (p(0.505, 0.505, 5.0), p(1.495, 1.495, 15.0));
    let plate = (p(-1.0, -1.0, -3.0), p(3.0, 3.0, -1.0));
    let (mesh, _) = judge(&b, &facets(&[s, plate], &[flipped]), MESH_VISIT_BUDGET);
    assert_eq!(mesh, Some(4.0), "the steel at 5.5..9.5 must not read as air");
}

#[test]
fn a_post_between_the_column_lines_is_steel() {
    // 3 x 3 columns, material Z 0..10; a 0.19 mm post Z 0..3 in the
    // middle of the four centres (1.5, 1.5) - no line meets it.
    let b = body(3.0, 0.0, 10.0);
    let r = 0.095;
    let t = facets(&[(p(-1.0, -1.0, -5.0), p(4.0, 4.0, 0.0)), (p(1.0 - r, 1.0 - r, 0.0), p(1.0 + r, 1.0 + r, 3.0))], &[]);
    let (mesh, _) = judge(&b, &t, MESH_VISIT_BUDGET);
    assert_eq!(mesh, Some(3.0));
    let t = facets(&[(p(-1.0, -1.0, -5.0), p(4.0, 4.0, 0.0))], &[]);
    assert_eq!(judge(&b, &t, MESH_VISIT_BUDGET).0, None, "the plate alone is contact");
}

#[test]
fn a_fixture_narrower_than_a_cell_off_the_lines_keeps_the_box_depth() {
    // One column, no rectangle between columns to read: a block holding
    // no column centre is judged by the column under it, by the box.
    let b = body(1.0, 0.0, 10.0);
    let t = facets(&[(p(0.6, 0.6, 2.0), p(0.8, 0.8, 8.0))], &[]);
    assert_eq!(judge(&b, &t, MESH_VISIT_BUDGET), (Some(6.0), Some(6.0)));
}

#[test]
fn a_face_on_a_column_centre_where_the_material_ends_is_contact() {
    // Material only under x < 2 (columns 0.5 and 1.5; 2.5 is air). A
    // jaw from x 1.5 outward, Z 2..8, and a plate under all of it: the
    // jaw's face lies on the 1.5 column centres - contact, not steel in
    // the rectangle 0.5..1.5 (whose edge it lies on).
    let dex = Dexel::block(0.0, 0.0, 3.0, 3.0, 10.0, 1.0).expect("grid");
    let b = Body::on_mesh(dex, &cube(p(0.0, 0.0, 0.0), p(2.0, 3.0, 10.0)).facets);
    let jaw = (p(1.5, -0.5, 2.0), p(3.2, 3.5, 8.0));
    let plate = (p(-1.0, -1.0, -3.0), p(4.0, 4.0, -1.0));
    let (mesh, boxed) = judge(&b, &facets(&[jaw, plate], &[]), MESH_VISIT_BUDGET);
    assert_eq!((mesh, boxed), (None, Some(8.0)));
}

/// 3 x 3 columns of 1 mm, material Z 0..10, a plate under it (Z -5..0:
/// contact) and `extra` blocks.
fn on_plate(extra: &[(Point3, Point3)]) -> (Option<f64>, Option<f64>) {
    let mut boxes = vec![(p(-1.0, -1.0, -5.0), p(4.0, 4.0, 0.0))];
    boxes.extend_from_slice(extra);
    judge(&body(3.0, 0.0, 10.0), &facets(&boxes, &[]), MESH_VISIT_BUDGET)
}

#[test]
fn a_rectangle_reads_its_contact_band_at_the_tolerance() {
    // A block mid-cell (no column line meets it) on the plate: 0.03 mm
    // into the material is steel, 0.005 mm is contact. Literals, so a
    // wider band in the rectangle read turns this red (#1530 review P2b).
    let block = |h: f64| (p(0.9, 0.9, 0.0), p(1.1, 1.1, h));
    assert_eq!(on_plate(&[block(0.03)]).0, Some(0.03));
    assert_eq!(on_plate(&[block(0.005)]).0, None);
}

#[test]
fn a_rectangle_is_trimmed_by_the_tolerance_only() {
    // A 0.015 mm post 0.012 mm off a column's centre line (x 1.5): past
    // the line beside it (1.51), inside the rectangle trimmed by one
    // tolerance - a wider trim misses it (#1530 review P2c).
    assert_eq!(on_plate(&[(p(1.512, 0.9, 0.0), p(1.527, 1.1, 3.0))]).0, Some(3.0));
}

#[test]
fn a_pitch_too_fine_to_trim_keeps_the_box_between_the_columns() {
    // At a 0.015 mm pitch the rectangle trimmed by 0.01 mm each side is
    // inverted: it is not read, the box speaks. A 0.003 mm post 3 mm
    // into the material mid-cell (no line meets it) is refused, as the
    // box refuses it (#1530 review request 2).
    let pitch = 0.015;
    let b = Body::on_floor(Dexel::block(0.0, 0.0, 20.0 * pitch, 20.0 * pitch, 10.0, pitch).expect("grid"), 0.0);
    assert!((b.dex.cell - pitch).abs() < 1e-12, "the grid keeps the pitch: {}", b.dex.cell);
    let c = b.dex.center(9, 9);
    let (mx, my) = (c.x + pitch * 0.5, c.y + pitch * 0.5);
    let r = 0.0015;
    let plate = (p(-1.0, -1.0, -5.0), p(1.0, 1.0, 0.0));
    let post = (p(mx - r, my - r, 0.0), p(mx + r, my + r, 3.0));
    let (mesh, boxed) = judge(&b, &facets(&[plate, post], &[]), MESH_VISIT_BUDGET);
    assert_eq!(boxed, Some(3.0), "the box refuses it");
    assert_eq!(mesh, Some(3.0), "the mesh never clears what it did not read");
}
