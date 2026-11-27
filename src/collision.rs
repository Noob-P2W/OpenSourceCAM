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
    ) -> Result<Option<&Triangle>, String> {
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
            if swept_triangle(t.points, bounds, from, to, radius) {
                return Ok(Some(t));
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
fn closed_model(
    triangles: &mut [Triangle],
    geometry: &crate::import::Geometry,
    shells: &mut usize,
    budget: &mut Budget<'_>,
) -> Result<(), String> {
    if geometry.bodies.is_empty() {
        return closed_fixture(triangles, shells, budget);
    }
    // Validate each source component independently. Contact between two closed
    // CAD bodies must not turn their common edges into non-manifold edges.
    for body in &geometry.bodies {
        let end = body
            .first_triangle
            .checked_add(body.triangle_count)
            .ok_or("Invalid CAD component range")?;
        let faces = triangles
            .get_mut(body.first_triangle..end)
            .ok_or("Missing CAD component triangles")?;
        closed_fixture(faces, shells, budget)
            .map_err(|e| format!("Component '{}': {e}", body.name))?;
    }
    Ok(())
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
pub(crate) enum ToolSection {
    Cutter,
    Shaft,
    Holder,
}

impl ToolSection {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cutter => "cutter",
            Self::Shaft => "shaft",
            Self::Holder => "holder",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Envelope {
    section: ToolSection,
    pub(crate) radius: f64,
    pub(crate) low: f64,
    pub(crate) high: f64,
}
pub(crate) fn envelopes(project: &Project, operation: usize) -> Result<Vec<Envelope>, String> {
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
            section: ToolSection::Cutter,
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
                section: ToolSection::Shaft,
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
                section: ToolSection::Holder,
                radius: diameter / 2.0,
                low: stickout,
                high: stickout + h.dimension("overall_length"),
            });
        }
    } else {
        let (length, flute, shank) = match project.machine.as_ref().map(|m| &m.tooling) {
            Some(crate::machine::Tooling::Milling {
                stickout,
                flute_length,
                shank_diameter,
                ..
            }) => (*stickout, flute_length.min(*stickout), *shank_diameter),
            _ => return Err("Fixture verification needs a defined milling tool length".into()),
        };
        let wider_shank = shank > tool.diameter && length > flute;
        result.push(Envelope {
            section: ToolSection::Cutter,
            radius: tool.diameter / 2.,
            low: 0.,
            high: if wider_shank { flute } else { length },
        });
        if wider_shank {
            result.push(Envelope {
                section: ToolSection::Shaft,
                radius: shank / 2.,
                low: flute,
                high: length,
            });
        }
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
fn swept_triangle(
    points: [Point; 3],
    bounds: Bounds,
    from: [f64; 2],
    to: [f64; 2],
    radius: f64,
) -> bool {
    let mut polygon = clip(&points, bounds.min[2], true);
    polygon = clip(&polygon, bounds.max[2], false);
    let points: Vec<_> = polygon.iter().map(|p| [p[0], p[1]]).collect();
    inside(from, &points)
        || inside(to, &points)
        || (0..points.len()).any(|i| {
            segment_distance(from, to, points[i], points[(i + 1) % points.len()]) <= radius
        })
}

#[derive(serde::Serialize)]
pub(crate) struct ContactFinding {
    pub fixture: u64,
    pub name: String,
    pub movement: usize,
    pub section: &'static str,
    pub fraction: f64,
    pub tip: [f64; 3],
    pub point: [f64; 3],
    pub patch: Vec<[f64; 3]>,
    pub contained: bool,
}

pub(crate) fn inspect(project: &Project, path: &Toolpath) -> (Vec<ContactFinding>, Option<String>) {
    let mut findings = Vec::new();
    let error = verify(
        project,
        path,
        &mut Budget {
            remaining: MAX_WORK,
            cancel: &AtomicBool::new(false),
        },
        0.,
        Some(&mut findings),
    )
    .err();
    (findings, error)
}

struct ContactPose {
    fraction: f64,
    tip: Point,
    patch: Vec<Point>,
}

fn contact_location(
    move_: &Move,
    envelope: Envelope,
    triangle: &Triangle,
    budget: &mut Budget<'_>,
) -> Result<ContactPose, String> {
    let at = |t: f64| crate::cam::Point3 {
        x: move_.from.x + t * (move_.to.x - move_.from.x),
        y: move_.from.y + t * (move_.to.y - move_.from.y),
        z: move_.from.z + t * (move_.to.z - move_.from.z),
    };
    let mut prefix = move_.clone();
    let [mut lo, mut hi] = [0., 1.];
    for _ in 0..20 {
        budget.tick()?;
        let mid = (lo + hi) / 2.;
        prefix.to = at(mid);
        if swept_triangle(
            triangle.points,
            move_bounds(&prefix, envelope),
            [prefix.from.x, prefix.from.y],
            [prefix.to.x, prefix.to.y],
            envelope.radius + 0.01,
        ) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let tip = at(hi);
    let size = envelope.radius + 0.08;
    let mut patch = triangle.points.to_vec();
    for (axis, limit, greater) in [
        (0, tip.x - size, true),
        (0, tip.x + size, false),
        (1, tip.y - size, true),
        (1, tip.y + size, false),
        (2, tip.z + envelope.low - 0.08, true),
        (2, tip.z + envelope.high + 0.08, false),
    ] {
        patch = crate::surface::clip(patch, axis, limit, greater);
    }
    Ok(ContactPose {
        fraction: hi,
        tip: [tip.x, tip.y, tip.z],
        patch,
    })
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
        0.,
        None,
    )
}
pub(crate) fn check_with_margin(
    project: &Project,
    path: &Toolpath,
    margin: f64,
) -> Result<(), String> {
    verify(
        project,
        path,
        &mut Budget {
            remaining: MAX_WORK,
            cancel: &AtomicBool::new(false),
        },
        margin,
        None,
    )
}
fn verify(
    project: &Project,
    path: &Toolpath,
    budget: &mut Budget<'_>,
    margin: f64,
    mut report: Option<&mut Vec<ContactFinding>>,
) -> Result<(), String> {
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
        closed_model(
            &mut tree.triangles[start..],
            &model.part.geometry,
            &mut shells,
            budget,
        )
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
            for e in &mut tool {
                e.radius += margin;
                e.low -= margin;
                e.high += margin;
            }
            current = Some(movement.operation);
        }
        for envelope in &tool {
            let hit = tree.contact(
                0,
                move_bounds(movement, *envelope),
                [movement.from.x, movement.from.y],
                [movement.to.x, movement.to.y],
                envelope.radius + 0.01,
                budget,
            )?;
            let center = [
                movement.from.x,
                movement.from.y,
                movement.from.z + (envelope.low + envelope.high) / 2.,
            ];
            let (model, contained) = if let Some(hit) = hit {
                (hit.model, false)
            } else if let Some(model) = if changed {
                tree.containing(center, budget)?
            } else {
                None
            } {
                (model, true)
            } else {
                continue;
            };
            let fixture = &project.workspace[model];
            if let Some(findings) = report.as_deref_mut() {
                let section = envelope.section.name();
                if findings.iter().any(|hit| {
                    hit.fixture == fixture.id
                        && hit.section == section
                        && path.moves[hit.movement].operation == movement.operation
                }) {
                    continue;
                }
                if findings.len() >= 48 {
                    return Err(
                        "More fixture contacts exist than the Issues panel can display".into(),
                    );
                }
                let ContactPose {
                    fraction,
                    tip,
                    patch,
                } = if let Some(hit) = hit {
                    contact_location(movement, *envelope, hit, budget)?
                } else {
                    ContactPose {
                        fraction: 0.,
                        tip: [movement.from.x, movement.from.y, movement.from.z],
                        patch: Vec::new(),
                    }
                };
                let point = if patch.is_empty() {
                    [tip[0], tip[1], tip[2] + (envelope.high + envelope.low) / 2.]
                } else {
                    std::array::from_fn(|i| {
                        patch.iter().map(|p| p[i]).sum::<f64>() / patch.len() as f64
                    })
                };
                findings.push(ContactFinding {
                    fixture: fixture.id,
                    name: fixture.name.clone(),
                    movement: index,
                    section,
                    fraction,
                    tip,
                    point,
                    patch,
                    contained,
                });
            } else if contained {
                return Err(format!("Tool/shaft/holder envelope is contained inside fixture '{}' at move {}; NC export is blocked",fixture.name,index+1));
            } else {
                return Err(format!(
                    "Tool/shaft/holder envelope contacts fixture '{}' at move {} ({})",
                    fixture.name,
                    index + 1,
                    if movement.motion == Motion::Rapid {
                        "rapid"
                    } else {
                        "cutting"
                    }
                ));
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
    closed_model(
        &mut tree.triangles,
        &model.part.geometry,
        &mut 0,
        &mut budget,
    )?;
    tree.build(0, tree.triangles.len(), &mut budget)?;
    if tree.containing(center, &mut budget)?.is_some() {
        return Err("Stock is enclosed inside a solid vise body".into());
    }
    Ok(())
}

/// Reusable conservative fixture-height queries for local rapid clearance.
/// Surface XY projections suffice for an upper bound; export still performs the
/// full closed-shell and swept-envelope collision preflight.
pub(crate) struct FixtureClearance {
    tree: Tree,
    remaining: usize,
}
impl FixtureClearance {
    pub(crate) fn build(project: &Project, cancel: &AtomicBool) -> Result<Self, String> {
        let mut tree = Tree {
            triangles: Vec::new(),
            nodes: Vec::new(),
        };
        let mut budget = Budget {
            remaining: MAX_WORK,
            cancel,
        };
        for model in project
            .workspace
            .iter()
            .filter(|m| m.kind != ModelKind::Machine)
        {
            for triangle in &model.part.geometry.mesh.triangles {
                budget.tick()?;
                tree.triangles.push(Triangle {
                    model: 0,
                    shell: 0,
                    points: triangle.map(|i| {
                        model.point(
                            model.part.geometry.mesh.vertices[i as usize],
                            project.machine.as_ref(),
                        )
                    }),
                });
            }
        }
        if !tree.triangles.is_empty() {
            tree.build(0, tree.triangles.len(), &mut budget)?;
        }
        Ok(Self {
            tree,
            remaining: MAX_WORK,
        })
    }
    pub(crate) fn top(&mut self, a: [f64; 2], b: [f64; 2], envelopes: &[Envelope]) -> Option<f64> {
        if self.tree.nodes.is_empty() {
            return Some(f64::NEG_INFINITY);
        }
        let cancel = AtomicBool::new(false);
        let mut budget = Budget {
            remaining: self.remaining,
            cancel: &cancel,
        };
        let result = (|| {
            let mut top = f64::NEG_INFINITY;
            for e in envelopes {
                let r = e.radius + 0.01;
                let xy = Bounds {
                    min: [a[0].min(b[0]) - r, a[1].min(b[1]) - r, f64::NEG_INFINITY],
                    max: [a[0].max(b[0]) + r, a[1].max(b[1]) + r, f64::INFINITY],
                };
                let mut stack = vec![0];
                while let Some(id) = stack.pop() {
                    budget.tick()?;
                    let n = &self.tree.nodes[id];
                    if !n.bounds.overlaps(xy) || n.bounds.max[2] - e.low <= top {
                        continue;
                    }
                    if let Some(children) = n.children {
                        stack.extend(children);
                        continue;
                    }
                    for t in &self.tree.triangles[n.start..n.end] {
                        budget.tick()?;
                        let p = t.points.map(|q| [q[0], q[1]]);
                        if inside(a, &p)
                            || inside(b, &p)
                            || (0..3).any(|i| segment_distance(a, b, p[i], p[(i + 1) % 3]) <= r)
                        {
                            top = top.max(
                                t.points
                                    .iter()
                                    .map(|q| q[2])
                                    .fold(f64::NEG_INFINITY, f64::max)
                                    - e.low
                                    + 0.01,
                            );
                        }
                    }
                }
            }
            Ok::<_, String>(top)
        })();
        self.remaining = budget.remaining;
        result.ok()
    }
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
    fn local_fixture_height_includes_bodies_and_falls_back_on_exhaustion() {
        let cancel = AtomicBool::new(false);
        let mut tree = Tree {
            triangles: cube(1., 2., false),
            nodes: Vec::new(),
        };
        tree.build(
            0,
            tree.triangles.len(),
            &mut Budget {
                remaining: MAX_WORK,
                cancel: &cancel,
            },
        )
        .unwrap();
        let mut f = FixtureClearance {
            tree,
            remaining: MAX_WORK,
        };
        let e = Envelope {
            section: ToolSection::Cutter,
            radius: 0.2,
            low: 0.,
            high: 4.,
        };
        assert!(f.top([1.5, 0.], [1.5, 3.], &[e]).unwrap() >= 2.01);
        assert_eq!(f.top([-2., 0.], [-2., 3.], &[e]), Some(f64::NEG_INFINITY));
        let shaft = Envelope {
            section: ToolSection::Shaft,
            radius: 1.2,
            low: 0.5,
            high: 5.,
        };
        assert!(f.top([0., 0.], [0., 3.], &[e, shaft]).unwrap() >= 1.51);
        f.remaining = 0;
        assert!(f.top([0., 0.], [3., 3.], &[e]).is_none());
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

#[cfg(test)]
mod legacy_envelope_regression {
    #[test]
    fn measured_legacy_shank_is_included_when_wider_than_the_cutting_tool() {
        let mut p = crate::model::Project::default();
        p.tool.diameter = 3.;
        p.machine.as_mut().unwrap().tooling = crate::machine::Tooling::Milling {
            kind: crate::machine::CutterKind::EndMill,
            flute_length: 10.,
            overall_length: 60.,
            stickout: 30.,
            shank_diameter: 12.,
        };
        let env = super::envelopes(&p, 0).unwrap();
        assert!(env
            .iter()
            .any(|e| e.radius == 6. && e.low == 10. && e.high == 30.));
        assert!(env
            .iter()
            .any(|e| e.radius == 1.5 && e.low == 0. && e.high == 10.));
    }
}
