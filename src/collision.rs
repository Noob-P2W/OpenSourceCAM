//! Conservative cylindrical cutter/shaft/holder envelopes against fixture triangles.
//! Visibility does not disable a fixture check. Machine bodies are not articulated.
use crate::{
    cam::{Motion, Move, Toolpath},
    model::Project,
    workspace::ModelKind,
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_WORK: usize = 20_000_000;
struct Budget<'a> {
    remaining: usize,
    cancel: &'a AtomicBool,
}
impl Budget<'_> {
    fn tick(&mut self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("Fixture verification cancelled".into());
        }
        self.remaining = self.remaining.checked_sub(1).ok_or("Fixture verification exceeded its work limit; simplify fixtures or split the job. NC export is blocked.")?;
        Ok(())
    }
}
type Point = [f64; 3];
#[derive(Clone, Copy)]
struct Bounds {
    min: Point,
    max: Point,
}
impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    fn include(&mut self, p: Point) {
        for (axis, v) in p.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(v);
            self.max[axis] = self.max[axis].max(v);
        }
    }
    fn overlaps(self, other: Self) -> bool {
        (0..3).all(|i| self.min[i] <= other.max[i] && self.max[i] >= other.min[i])
    }
    fn contains(self, point: Point) -> bool {
        (0..3).all(|i| point[i] >= self.min[i] && point[i] <= self.max[i])
    }
    fn ray(self, point: Point, direction: Point) -> bool {
        let mut near = 0.0_f64;
        let mut far = f64::INFINITY;
        for i in 0..3 {
            let a = (self.min[i] - point[i]) / direction[i];
            let b = (self.max[i] - point[i]) / direction[i];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
        near <= far
    }
}
struct Triangle {
    points: [Point; 3],
    model: usize,
    shell: usize,
}
struct Node {
    bounds: Bounds,
    children: Option<[usize; 2]>,
    start: usize,
    end: usize,
}
struct Tree {
    triangles: Vec<Triangle>,
    nodes: Vec<Node>,
}
impl Tree {
    fn build(
        &mut self,
        start: usize,
        end: usize,
        budget: &mut Budget<'_>,
    ) -> Result<usize, String> {
        let mut bounds = Bounds::empty();
        for t in &self.triangles[start..end] {
            budget.tick()?;
            for p in t.points {
                bounds.include(p);
            }
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            children: None,
            start,
            end,
        });
        if end - start > 8 {
            let axis = (0..3)
                .max_by(|a, b| {
                    (bounds.max[*a] - bounds.min[*a]).total_cmp(&(bounds.max[*b] - bounds.min[*b]))
                })
                .unwrap();
            let middle = start + (end - start) / 2;
            self.triangles[start..end].select_nth_unstable_by(middle - start, |a, b| {
                a.points
                    .iter()
                    .map(|p| p[axis])
                    .sum::<f64>()
                    .total_cmp(&b.points.iter().map(|p| p[axis]).sum::<f64>())
            });
            let a = self.build(start, middle, budget)?;
            let b = self.build(middle, end, budget)?;
            self.nodes[id].children = Some([a, b]);
        }
        Ok(id)
    }
    fn contact(
        &self,
        node: usize,
        bounds: Bounds,
        from: [f64; 2],
        to: [f64; 2],
        radius: f64,
        budget: &mut Budget<'_>,
    ) -> Result<Option<usize>, String> {
        budget.tick()?;
        let n = &self.nodes[node];
        if !n.bounds.overlaps(bounds) {
            return Ok(None);
        }
        if let Some([a, b]) = n.children {
            if let Some(hit) = self.contact(a, bounds, from, to, radius, budget)? {
                return Ok(Some(hit));
            }
            return self.contact(b, bounds, from, to, radius, budget);
        }
        for t in &self.triangles[n.start..n.end] {
            budget.tick()?;
            let mut polygon = t.points.to_vec();
            polygon = clip(&polygon, bounds.min[2], true);
            polygon = clip(&polygon, bounds.max[2], false);
            if polygon.is_empty() {
                continue;
            }
            let points: Vec<_> = polygon.iter().map(|p| [p[0], p[1]]).collect();
            if inside(from, &points)
                || inside(to, &points)
                || (0..points.len()).any(|i| {
                    segment_distance(from, to, points[i], points[(i + 1) % points.len()]) <= radius
                })
            {
                return Ok(Some(t.model));
            }
        }
        Ok(None)
    }

    fn winding(
        &self,
        node: usize,
        point: Point,
        direction: Point,
        crossings: &mut BTreeMap<usize, (usize, i32)>,
        budget: &mut Budget<'_>,
    ) -> Result<bool, String> {
        budget.tick()?;
        let n = &self.nodes[node];
        if !n.bounds.ray(point, direction) {
            return Ok(true);
        }
        if let Some([a, b]) = n.children {
            return Ok(self.winding(a, point, direction, crossings, budget)?
                && self.winding(b, point, direction, crossings, budget)?);
        }
        for triangle in &self.triangles[n.start..n.end] {
            budget.tick()?;
            match ray_triangle(point, direction, triangle.points) {
                RayHit::Clear => {}
                RayHit::Cross(sign) => {
                    crossings
                        .entry(triangle.shell)
                        .or_insert((triangle.model, 0))
                        .1 += sign
                }
                RayHit::Ambiguous => return Ok(false),
            }
        }
        Ok(true)
    }
    fn containing(&self, point: Point, budget: &mut Budget<'_>) -> Result<Option<usize>, String> {
        if !self.nodes[0].bounds.contains(point) {
            return Ok(None);
        }
        // Retry edge/vertex hits instead of counting a triangulation seam twice.
        for direction in [
            [1.0, 0.371390676, 0.529113],
            [0.193173, 1.0, 0.647719],
            [0.731191, 0.219137, 1.0],
        ] {
            let mut crossings = BTreeMap::new();
            if self.winding(0, point, direction, &mut crossings, budget)? {
                // Independent shells are a conservative union, even when an
                // imported component's winding is reversed. Overlapping solids
                // must never cancel each other's containment test.
                return Ok(crossings
                    .values()
                    .find(|(_, n)| *n != 0)
                    .map(|(model, _)| *model));
            }
        }
        Err("Fixture containment is numerically ambiguous; clearance cannot be verified. NC export is blocked.".into())
    }
}

fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: Point, b: Point) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
enum RayHit {
    Clear,
    Cross(i32),
    Ambiguous,
}
fn ray_triangle(point: Point, direction: Point, t: [Point; 3]) -> RayHit {
    let ab = sub(t[1], t[0]);
    let ac = sub(t[2], t[0]);
    let h = cross(direction, ac);
    let det = dot(ab, h);
    let scale = dot(ab, ab).sqrt() * dot(ac, ac).sqrt() * dot(direction, direction).sqrt();
    if det.abs() <= scale * 1e-12 {
        return RayHit::Clear;
    }
    let s = sub(point, t[0]);
    let u = dot(s, h) / det;
    let q = cross(s, ab);
    let v = dot(direction, q) / det;
    let distance = dot(ac, q) / det;
    if distance < -1e-7 || u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
        return RayHit::Clear;
    }
    if distance.abs() <= 1e-7 || u <= 1e-9 || v <= 1e-9 || (1.0 - u - v) <= 1e-9 {
        return RayHit::Ambiguous;
    }
    RayHit::Cross(if dot(cross(ab, ac), direction) > 0.0 {
        1
    } else {
        -1
    })
}

fn root(parents: &mut [usize], mut id: usize) -> usize {
    while parents[id] != id {
        parents[id] = parents[parents[id]];
        id = parents[id];
    }
    id
}
fn closed_fixture(
    triangles: &mut [Triangle],
    shells: &mut usize,
    budget: &mut Budget<'_>,
) -> Result<(), String> {
    // STL repeats vertices per face; weld in world millimetres before checking edges.
    let mut vertices = BTreeMap::new();
    let mut edges = BTreeMap::<(usize, usize), (u32, i32, usize)>::new();
    let mut parents: Vec<_> = (0..triangles.len()).collect();
    for (face, t) in triangles.iter().enumerate() {
        budget.tick()?;
        let mut ids = [0; 3];
        for (i, p) in t.points.into_iter().enumerate() {
            let next = vertices.len();
            ids[i] = *vertices
                .entry(p.map(|v| (v / 1e-6).round() as i64))
                .or_insert(next);
        }
        if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
            return Err("Fixture has an edge below the 0.000001 mm verification tolerance".into());
        }
        for i in 0..3 {
            let (a, b) = (ids[i], ids[(i + 1) % 3]);
            let e = edges.entry((a.min(b), a.max(b))).or_insert((0, 0, face));
            if e.0 > 0 {
                let a = root(&mut parents, e.2);
                let b = root(&mut parents, face);
                parents[a] = b;
            }
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    if edges
        .values()
        .any(|(count, balance, _)| *count != 2 || *balance != 0)
    {
        return Err("Fixture must be a closed, consistently oriented manifold mesh for reliable containment checks; repair the fixture before NC export".into());
    }
    let mut labels = BTreeMap::new();
    for (face, t) in triangles.iter_mut().enumerate() {
        budget.tick()?;
        t.shell = *labels.entry(root(&mut parents, face)).or_insert_with(|| {
            let id = *shells;
            *shells += 1;
            id
        });
    }
    Ok(())
}
fn clip(points: &[Point], level: f64, above: bool) -> Vec<Point> {
    let mut result = Vec::new();
    if points.is_empty() {
        return result;
    }
    let yes = |p: Point| if above { p[2] >= level } else { p[2] <= level };
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let ia = yes(a);
        let ib = yes(b);
        if ia {
            result.push(a);
        }
        if ia != ib {
            let f = (level - a[2]) / (b[2] - a[2]);
            result.push(std::array::from_fn(|j| a[j] + f * (b[j] - a[j])));
        }
    }
    result
}
pub(crate) fn point_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 1e-20 {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    (p[0] - a[0] - d[0] * t).hypot(p[1] - a[1] - d[1] * t)
}
fn segment_distance(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let cd = [d[0] - c[0], d[1] - c[1]];
    let ca = [c[0] - a[0], c[1] - a[1]];
    let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let denominator = cross(ab, cd);
    if denominator.abs() > 1e-12 {
        let t = cross(ca, cd) / denominator;
        let u = cross(ca, ab) / denominator;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return 0.0;
        }
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}
fn inside(p: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut yes = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes;
        }
    }
    yes
}
#[derive(Clone, Copy)]
struct Envelope {
    radius: f64,
    low: f64,
    high: f64,
}
fn envelopes(project: &Project, operation: usize) -> Result<Vec<Envelope>, String> {
    let op = project
        .operations
        .get(operation)
        .ok_or("Invalid toolpath operation index")?;
    let tool = project.operation_tool(op)?;
    let mut result = Vec::new();
    if let Some(record) = project.library_tool(Some(op))? {
        let stickout = record.dimension("stickout");
        let flute = record.dimension("flute_length").min(stickout);
        result.push(Envelope {
            radius: tool.diameter / 2.0,
            low: 0.0,
            high: flute,
        });
        let shaft = record
            .shaft
            .iter()
            .flat_map(|s| [s.lower_diameter, s.upper_diameter])
            .fold(record.dimension("shaft_diameter"), f64::max);
        if stickout > flute {
            result.push(Envelope {
                radius: shaft / 2.0,
                low: flute,
                high: stickout,
            });
        }
        if let Some(holder) = record.holder {
            let h = project.tool_library.get(holder)?;
            let diameter = h
                .shaft
                .iter()
                .flat_map(|s| [s.lower_diameter, s.upper_diameter])
                .fold(h.dimension("body_diameter"), f64::max);
            result.push(Envelope {
                radius: diameter / 2.0,
                low: stickout,
                high: stickout + h.dimension("overall_length"),
            });
        }
    } else {
        let length = match project.machine.as_ref().map(|m| &m.tooling) {
            Some(crate::machine::Tooling::Milling { stickout, .. }) => *stickout,
            _ => return Err("Fixture verification needs a defined milling tool length".into()),
        };
        result.push(Envelope {
            radius: tool.diameter / 2.0,
            low: 0.0,
            high: length,
        });
    }
    Ok(result)
}
fn move_bounds(m: &Move, e: Envelope) -> Bounds {
    let radius = e.radius + 0.01;
    Bounds {
        min: [
            m.from.x.min(m.to.x) - radius,
            m.from.y.min(m.to.y) - radius,
            m.from.z.min(m.to.z) + e.low - 0.01,
        ],
        max: [
            m.from.x.max(m.to.x) + radius,
            m.from.y.max(m.to.y) + radius,
            m.from.z.max(m.to.z) + e.high + 0.01,
        ],
    }
}
pub fn check(project: &Project, path: &Toolpath) -> Result<(), String> {
    check_with_cancel(project, path, &AtomicBool::new(false))
}
pub fn check_with_cancel(
    project: &Project,
    path: &Toolpath,
    cancel: &AtomicBool,
) -> Result<(), String> {
    verify(
        project,
        path,
        &mut Budget {
            remaining: MAX_WORK,
            cancel,
        },
    )
}
fn verify(project: &Project, path: &Toolpath, budget: &mut Budget<'_>) -> Result<(), String> {
    budget.tick()?;
    crate::workspace::validate(&project.workspace, project.machine.as_ref())?;
    let mut tree = Tree {
        triangles: Vec::new(),
        nodes: Vec::new(),
    };
    let mut shells = 0;
    for (id, model) in project
        .workspace
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind != ModelKind::Machine)
    {
        let start = tree.triangles.len();
        for triangle in &model.part.geometry.mesh.triangles {
            budget.tick()?;
            tree.triangles.push(Triangle {
                model: id,
                shell: 0,
                points: triangle.map(|i| {
                    model.point(
                        model.part.geometry.mesh.vertices[i as usize],
                        project.machine.as_ref(),
                    )
                }),
            });
        }
        closed_fixture(&mut tree.triangles[start..], &mut shells, budget)
            .map_err(|e| format!("Fixture '{}': {e}", model.name))?;
    }
    if tree.triangles.is_empty() {
        return Ok(());
    }
    tree.build(0, tree.triangles.len(), budget)?;
    let mut current = None;
    let mut tool = Vec::new();
    for (index, movement) in path.moves.iter().enumerate() {
        budget.tick()?;
        if [
            movement.from.x,
            movement.from.y,
            movement.from.z,
            movement.to.x,
            movement.to.y,
            movement.to.z,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err("Fixture verification received a non-finite toolpath point".into());
        }
        let changed = current != Some(movement.operation);
        if changed {
            tool = envelopes(project, movement.operation)?;
            current = Some(movement.operation);
        }
        for envelope in &tool {
            if let Some(model) = tree.contact(
                0,
                move_bounds(movement, *envelope),
                [movement.from.x, movement.from.y],
                [movement.to.x, movement.to.y],
                envelope.radius + 0.01,
                budget,
            )? {
                return Err(format!(
                    "Tool/shaft/holder envelope contacts fixture '{}' at move {} ({})",
                    project.workspace[model].name,
                    index + 1,
                    if movement.motion == Motion::Rapid {
                        "rapid"
                    } else {
                        "cutting"
                    }
                ));
            }
            if changed {
                let point = [
                    movement.from.x,
                    movement.from.y,
                    movement.from.z + (envelope.low + envelope.high) / 2.0,
                ];
                if let Some(model) = tree.containing(point, budget)? {
                    return Err(format!("Tool/shaft/holder envelope is contained inside fixture '{}' at move {}; NC export is blocked",project.workspace[model].name,index+1));
                }
            }
        }
    }
    Ok(())
}
/// Final rectangular-stock interference test for the geometric vise setup wizard.
pub fn check_stock_in_vise(project: &Project, id: u64) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let mut budget = Budget {
        remaining: MAX_WORK,
        cancel: &cancel,
    };
    let model = project
        .workspace
        .iter()
        .find(|m| m.id == id)
        .ok_or("Vise not found")?;
    let lo = [1e-5, 1e-5, -project.stock.thickness + 1e-5];
    let hi = [
        project.stock.width - 1e-5,
        project.stock.height - 1e-5,
        -1e-5,
    ];
    let center = std::array::from_fn(|i| (lo[i] + hi[i]) / 2.);
    let half: Point = std::array::from_fn(|i| (hi[i] - lo[i]) / 2.);
    let mut tree = Tree {
        triangles: Vec::new(),
        nodes: Vec::new(),
    };
    for t in &model.part.geometry.mesh.triangles {
        budget.tick()?;
        let points = t.map(|i| {
            model.point(
                model.part.geometry.mesh.vertices[i as usize],
                project.machine.as_ref(),
            )
        });
        let p = points.map(|v| sub(v, center));
        let edges = [sub(p[1], p[0]), sub(p[2], p[1]), sub(p[0], p[2])];
        let axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let mut tests = axes.to_vec();
        tests.push(cross(edges[0], edges[1]));
        for edge in edges {
            for a in axes {
                tests.push(cross(edge, a))
            }
        }
        let overlap = tests.iter().all(|a| {
            let values = p.map(|v| dot(v, *a));
            let min = values.into_iter().fold(f64::INFINITY, f64::min);
            let max = values.into_iter().fold(f64::NEG_INFINITY, f64::max);
            let r = (0..3).map(|i| half[i] * a[i].abs()).sum::<f64>();
            min <= r && max >= -r
        });
        if overlap {
            return Err("Vise geometry penetrates the stock at this grip depth. Adjust grip, supports or fixture geometry.".into());
        }
        tree.triangles.push(Triangle {
            model: 0,
            shell: 0,
            points,
        });
    }
    closed_fixture(&mut tree.triangles, &mut 0, &mut budget)?;
    tree.build(0, tree.triangles.len(), &mut budget)?;
    if tree.containing(center, &mut budget)?.is_some() {
        return Err("Stock is enclosed inside a solid vise body".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cube(min: f64, max: f64, reversed: bool) -> Vec<Triangle> {
        let v = [
            [min, min, min],
            [max, min, min],
            [max, max, min],
            [min, max, min],
            [min, min, max],
            [max, min, max],
            [max, max, max],
            [min, max, max],
        ];
        [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ]
        .map(|mut f| {
            if reversed {
                f.swap(0, 1);
            }
            Triangle {
                points: f.map(|i| v[i]),
                model: 0,
                shell: 0,
            }
        })
        .into()
    }
    #[test]
    fn containment_uses_closed_shells_and_handles_triangulation_seams() {
        let cancel = AtomicBool::new(false);
        for reverse in [false, true] {
            let mut b = Budget {
                remaining: MAX_WORK,
                cancel: &cancel,
            };
            let mut tree = Tree {
                triangles: cube(-1.0, 1.0, reverse),
                nodes: Vec::new(),
            };
            closed_fixture(&mut tree.triangles, &mut 0, &mut b).unwrap();
            tree.build(0, tree.triangles.len(), &mut b).unwrap();
            assert_eq!(tree.containing([0.0; 3], &mut b).unwrap(), Some(0));
            assert_eq!(tree.containing([2.0; 3], &mut b).unwrap(), None);
        }
        let f = [[1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0]];
        assert!(matches!(
            ray_triangle([0.0; 3], [1.0, 0.0, 0.0], f),
            RayHit::Ambiguous
        ));
    }
    #[test]
    fn opposite_overlapping_shells_do_not_cancel_containment() {
        let cancel = AtomicBool::new(false);
        let mut b = Budget {
            remaining: MAX_WORK,
            cancel: &cancel,
        };
        let mut triangles = cube(-3.0, 1.0, false);
        triangles.extend(cube(-1.0, 3.0, true));
        let mut tree = Tree {
            triangles,
            nodes: Vec::new(),
        };
        closed_fixture(&mut tree.triangles, &mut 0, &mut b).unwrap();
        tree.build(0, tree.triangles.len(), &mut b).unwrap();
        assert_eq!(tree.containing([0.0; 3], &mut b).unwrap(), Some(0));
    }
    #[test]
    fn shells_sharing_only_a_vertex_remain_independent() {
        let cancel = AtomicBool::new(false);
        let mut b = Budget {
            remaining: MAX_WORK,
            cancel: &cancel,
        };
        let mut triangles = cube(0.0, 1.0, false);
        triangles.extend(cube(0.0, 2.0, true));
        let mut tree = Tree {
            triangles,
            nodes: Vec::new(),
        };
        closed_fixture(&mut tree.triangles, &mut 0, &mut b).unwrap();
        tree.build(0, tree.triangles.len(), &mut b).unwrap();
        assert_eq!(tree.containing([0.5; 3], &mut b).unwrap(), Some(0));
    }
    #[test]
    fn uncertain_fixture_verification_is_bounded_and_cancellable() {
        let cancel = AtomicBool::new(false);
        let mut tree = Tree {
            triangles: cube(-1.0, 1.0, false),
            nodes: Vec::new(),
        };
        let mut b = Budget {
            remaining: 1,
            cancel: &cancel,
        };
        assert!(tree
            .build(0, tree.triangles.len(), &mut b)
            .unwrap_err()
            .contains("work limit"));
        cancel.store(true, Ordering::Relaxed);
        let project = Project::default();
        let path = crate::cam::generate(&project).unwrap();
        assert!(check_with_cancel(&project, &path, &cancel)
            .unwrap_err()
            .contains("cancelled"));
    }
}
