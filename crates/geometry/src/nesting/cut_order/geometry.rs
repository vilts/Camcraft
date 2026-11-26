//! Provenance: integer polygon predicates (Jordan parity) and circular-tool tangent geometry; CamCraft plasma nesting rules.

use crate::geo2d::{COORD_LIMIT, SCALE};

use super::{CornerClearance, CutFeature, CutGeometry, CutSide, GeoError, invalid};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridPoint {
    x: i64,
    y: i64,
}

pub(super) struct Node {
    points: Vec<GridPoint>,
    pub closed: bool,
    pub area2: i128,
    pub parent: Option<usize>,
    pub depth: usize,
}

// |coordinate| < 1e18, so each difference is below 2e18 and the two
// products and their difference fit i128. Area accumulation is checked.
fn cross(a: GridPoint, b: GridPoint, c: GridPoint) -> i128 {
    (i128::from(b.x) - i128::from(a.x)) * (i128::from(c.y) - i128::from(a.y))
        - (i128::from(b.y) - i128::from(a.y)) * (i128::from(c.x) - i128::from(a.x))
}

fn dot(a: GridPoint, b: GridPoint, c: GridPoint) -> i128 {
    (i128::from(a.x) - i128::from(b.x)) * (i128::from(c.x) - i128::from(b.x))
        + (i128::from(a.y) - i128::from(b.y)) * (i128::from(c.y) - i128::from(b.y))
}

fn on(a: GridPoint, b: GridPoint, p: GridPoint) -> bool {
    cross(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

fn intersects(a: GridPoint, b: GridPoint, c: GridPoint, d: GridPoint) -> bool {
    if a.x.max(b.x) < c.x.min(d.x)
        || c.x.max(d.x) < a.x.min(b.x)
        || a.y.max(b.y) < c.y.min(d.y)
        || c.y.max(d.y) < a.y.min(b.y)
    {
        return false;
    }
    let (ab_c, ab_d, cd_a, cd_b) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (ab_c.signum() != ab_d.signum()
        && cd_a.signum() != cd_b.signum()
        && ab_c != 0
        && ab_d != 0
        && cd_a != 0
        && cd_b != 0)
        || on(a, b, c)
        || on(a, b, d)
        || on(c, d, a)
        || on(c, d, b)
}

impl Node {
    fn segments(&self) -> impl Iterator<Item = (GridPoint, GridPoint)> + '_ {
        let count = if self.closed {
            self.points.len()
        } else {
            self.points.len() - 1
        };
        (0..count).map(|i| (self.points[i], self.points[(i + 1) % self.points.len()]))
    }

    fn contains(&self, p: GridPoint) -> bool {
        let mut inside = false;
        for (a, b) in self.segments() {
            if (a.y > p.y) != (b.y > p.y) {
                let turn = cross(a, b, p);
                if (b.y > a.y && turn > 0) || (b.y < a.y && turn < 0) {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

fn node(feature: &CutFeature) -> Result<Node, GeoError> {
    let (raw, closed) = match &feature.geometry {
        CutGeometry::Closed(ring) => (ring.points(), true),
        CutGeometry::Open(points) => (points.as_slice(), false),
    };
    let fail = |why: &str| invalid(format!("feature {} {why}", feature.id));
    if raw.len() < if closed { 3 } else { 2 } {
        return Err(fail("has too few vertices"));
    }
    let mut points = Vec::with_capacity(raw.len());
    for p in raw {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(fail("has a non-finite vertex"));
        }
        if p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT {
            return Err(fail("has a coordinate outside the machinable range"));
        }
        points.push(GridPoint {
            x: (p.x * SCALE).round() as i64,
            y: (p.y * SCALE).round() as i64,
        });
    }
    let mut node = Node {
        points,
        closed,
        area2: 0,
        parent: None,
        depth: 0,
    };
    for (a, b) in node.segments() {
        if a == b {
            return Err(fail("has a zero-length segment on the geometry grid"));
        }
    }
    if !closed && node.points.first() == node.points.last() {
        return Err(fail("is marked open but closes on itself"));
    }
    let n = node.points.len();
    for i in 0..n {
        if !closed && (i == 0 || i == n - 1) {
            continue;
        }
        let (a, b, c) = (
            node.points[(i + n - 1) % n],
            node.points[i],
            node.points[(i + 1) % n],
        );
        if cross(a, b, c) == 0 && dot(a, b, c) > 0 {
            return Err(fail("retraces an adjacent segment (a slit or spike)"));
        }
    }
    for (i, (a, b)) in node.segments().enumerate() {
        for (j, (c, d)) in node.segments().enumerate().skip(i + 2) {
            if closed && i == 0 && j == n - 1 {
                continue;
            }
            if intersects(a, b, c, d) {
                return Err(fail("crosses or touches itself"));
            }
        }
    }
    if closed {
        let mut area2 = 0i128;
        for (a, b) in node.segments() {
            area2 = area2
                .checked_add(cross(node.points[0], a, b))
                .ok_or_else(|| fail("area exceeds the geometry range"))?;
        }
        // The simple, non-retracing closed ring already has nonzero area.
        node.area2 = area2;
    }
    Ok(node)
}

pub(super) fn forest(features: &[CutFeature]) -> Result<Vec<Node>, GeoError> {
    let mut nodes: Vec<_> = features.iter().map(node).collect::<Result<_, _>>()?;
    // Intersections must be checked before testing a containment probe. Even
    // two endpoints inside a concave contour can have an outside middle.
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            // Open features may meet each other; only a closed boundary
            // separates material and determines containment.
            if !nodes[i].closed && !nodes[j].closed {
                continue;
            }
            if nodes[i]
                .segments()
                .any(|(a, b)| nodes[j].segments().any(|(c, d)| intersects(a, b, c, d)))
            {
                return Err(invalid(format!(
                    "features {} and {} cross or touch",
                    features[i].id, features[j].id
                )));
            }
        }
    }
    for i in 0..nodes.len() {
        nodes[i].parent = (0..nodes.len())
            .filter(|&j| {
                j != i
                    && nodes[j].closed
                    && nodes[j].area2.unsigned_abs() > nodes[i].area2.unsigned_abs()
                    && nodes[j].contains(nodes[i].points[0])
            })
            .min_by_key(|&j| nodes[j].area2.unsigned_abs());
    }
    for i in 0..nodes.len() {
        let mut at = nodes[i].parent;
        let mut depth = 0;
        while let Some(parent) = at {
            // Every parent has strictly larger area, so no cycle is possible.
            if features[parent].release_phase.is_some() {
                depth += 1;
            }
            at = nodes[parent].parent;
        }
        nodes[i].depth = depth;
    }
    Ok(nodes)
}

pub(super) fn corners(id: u64, node: &Node, side: CutSide, radius: f64) -> Vec<CornerClearance> {
    let mut limits = Vec::new();
    let n = node.points.len();
    for i in 0..n {
        if !node.closed && (i == 0 || i == n - 1) {
            continue;
        }
        let (a, b, c) = (
            node.points[(i + n - 1) % n],
            node.points[i],
            node.points[(i + 1) % n],
        );
        let turn = cross(a, b, c);
        let inside_turn = turn.signum() == node.area2.signum();
        let unreachable = turn != 0
            && match side {
                CutSide::Inside => inside_turn,
                CutSide::Outside => !inside_turn,
                CutSide::Left => turn > 0,
                CutSide::Right => turn < 0,
                _ => false,
            };
        if unreachable {
            let angle = (turn.unsigned_abs() as f64).atan2(dot(a, b, c) as f64);
            limits.push(CornerClearance {
                feature_id: id,
                vertex: i,
                tool_radius: radius,
                angle_radians: angle,
                min_unreachable_distance: radius * (1.0 / (angle / 2.0).sin() - 1.0),
            });
        }
    }
    limits
}
