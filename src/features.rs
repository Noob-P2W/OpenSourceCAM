//! Conservative recognition from a closed, consistently oriented triangle surface.
//! Results are candidates, never an automatic manufacturing or collision decision.
use crate::{
    import::{normal, Part},
    model::{Operation, Project, Strategy},
    process::Process,
    tool_library::ToolKind,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};

pub const TOLERANCE: f64 = 0.002;
const WELD: f64 = 0.0001;
const MAX_FEATURES: usize = 500;
type Edge = (usize, usize);
fn edge(a: usize, b: usize) -> Edge {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    RectangularPocket,
    CircularHole,
}
impl FeatureKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::RectangularPocket => "Rectangular pocket",
            Self::CircularHole => "Circular hole",
        }
    }
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Feature {
    pub kind: FeatureKind,
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub center: [f64; 2],
    pub diameter: Option<f64>,
    pub opening_z: f64,
    pub bottom_z: f64,
    pub through: bool,
    pub outline: Vec<[f64; 3]>,
    #[serde(default)]
    pub detail: Detail,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Prismatic,
    ChamferedBore,
    FilletedBore,
    TaperedHole,
    TaperedPocket,
    PocketWithIslands,
    OpenPocket,
}
impl Shape {
    pub fn machinable(self) -> bool {
        matches!(
            self,
            Self::Prismatic | Self::ChamferedBore | Self::FilletedBore
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Prismatic => "",
            Self::ChamferedBore => "Hole with entry chamfer",
            Self::FilletedBore => "Hole with rounded transition",
            Self::TaperedHole => "Tapered hole",
            Self::TaperedPocket => "Tapered pocket",
            Self::PocketWithIslands => "Pocket with islands",
            Self::OpenPocket => "Open pocket / ledge",
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Detail {
    pub shape: Shape,
    pub triangles: Vec<usize>,
    pub profiles: Vec<Vec<[f64; 3]>>,
    pub islands: Vec<Vec<[f64; 3]>>,
    pub floor_z: Option<f64>,
    pub floor_triangles: Vec<usize>,
}
impl Feature {
    pub fn label(&self) -> &'static str {
        if self.detail.shape == Shape::Prismatic {
            self.kind.label()
        } else {
            self.detail.shape.label()
        }
    }
    pub fn depth(&self) -> f64 {
        self.opening_z - self.bottom_z
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub exterior: Option<Exterior>,
    pub machining_error: Option<String>,
    pub features: Vec<Feature>,
    pub skipped_openings: usize,
    pub tolerance_mm: f64,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Exterior {
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub top_z: f64,
    pub bottom_z: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Up,
    Down,
    Wall,
    Other,
}
struct Component {
    class: Class,
    loops: Vec<Vec<usize>>,
    faces: Vec<usize>,
}
struct Topology {
    vertices: Vec<[f64; 3]>,
    triangles: Vec<[usize; 3]>,
    edges: BTreeMap<Edge, Vec<usize>>,
    classes: Vec<Class>,
    owners: Vec<usize>,
    components: Vec<Component>,
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Feature recognition cancelled".into())
    } else {
        Ok(())
    }
}

impl Topology {
    fn exterior(
        &self,
        top_z: f64,
        bottom_z: f64,
        mut used: BTreeSet<usize>,
        skipped: usize,
    ) -> Result<Exterior, String> {
        if skipped > 0 {
            return Err(format!("{skipped} openings have unsupported geometry. A complete stock plan cannot omit them; program supported features individually or use a suitable strategy."));
        }
        let mut outer = Vec::new();
        for (id, component) in self
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.class == Class::Up)
        {
            for boundary in &component.loops {
                if self
                    .level(boundary)
                    .is_some_and(|z| (z - top_z).abs() <= TOLERANCE)
                {
                    let points: Vec<_> = boundary.iter().map(|v| self.vertices[*v]).collect();
                    if signed_area(&points) > TOLERANCE * TOLERANCE {
                        outer.push((id, boundary, points));
                    }
                }
            }
        }
        if outer.len() != 1 {
            return Err("A complete stock plan currently needs one connected top-side part with an axis-aligned rectangular exterior".into());
        }
        let (top_id, boundary, points) = outer.pop().unwrap();
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in &points {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        if !rectangle(&points, min, max) {
            return Err("Automatic outside clearing currently requires an axis-aligned rectangular part exterior. Individual recognized cavities can still be programmed.".into());
        }
        let wall_id = self
            .across(boundary, top_id)
            .ok_or("Outside wall is not a supported vertical extrusion")?;
        let wall = &self.components[wall_id];
        if wall.class != Class::Wall || wall.loops.len() != 2 {
            return Err("Outside wall has unsupported side features or taper".into());
        }
        let bottom = wall
            .loops
            .iter()
            .find(|points| {
                self.level(points)
                    .is_some_and(|z| (z - bottom_z).abs() <= TOLERANCE)
            })
            .ok_or("Outside wall does not reach a flat model bottom")?;
        if !self.same_xy(boundary, bottom) {
            return Err("Outside contour changes with depth; 3D roughing is required".into());
        }
        let bottom_id = self
            .across(bottom, wall_id)
            .ok_or("Outside bottom boundary is ambiguous")?;
        if self.components[bottom_id].class != Class::Down {
            return Err("Model bottom is not a supported flat surface".into());
        }
        for p in wall
            .faces
            .iter()
            .flat_map(|f| self.triangles[*f])
            .map(|v| self.vertices[v])
        {
            if !(0..2).all(|i| p[i] >= min[i] - TOLERANCE && p[i] <= max[i] + TOLERANCE)
                || (0..2)
                    .flat_map(|i| [(p[i] - min[i]).abs(), (p[i] - max[i]).abs()])
                    .fold(f64::INFINITY, f64::min)
                    > TOLERANCE
            {
                return Err("Outside wall is not vertical throughout its depth".into());
            }
        }
        used.extend([top_id, wall_id, bottom_id]);
        if self
            .components
            .iter()
            .enumerate()
            .any(|(id, c)| !c.faces.is_empty() && !used.contains(&id))
        {
            return Err("The model contains surfaces, additional bodies or internal geometry not covered by these 2.5D strategies".into());
        }
        Ok(Exterior {
            min,
            max,
            top_z,
            bottom_z,
        })
    }
    fn build(part: &Part, cancel: &AtomicBool) -> Result<Self, String> {
        // Preserve real short edges on closed meshes. Quantized welding is only
        // a fallback for exchange meshes whose coincident seams differ slightly.
        match Self::build_with_weld(part, cancel, false) {
            Err(error) if error.starts_with("Recognition requires a closed,") => {
                let welded = Self::build_with_weld(part, cancel, true);
                cancelled(cancel)?;
                welded.map_err(|fallback| format!("{error} Seam welding also failed: {fallback}"))
            }
            result => result,
        }
    }
    fn build_with_weld(
        part: &Part,
        cancel: &AtomicBool,
        approximate: bool,
    ) -> Result<Self, String> {
        part.validate()?;
        let mut vertices = Vec::new();
        let mut weld = BTreeMap::new();
        let mut indices = Vec::new();
        for (i, v) in part.geometry.mesh.vertices.iter().enumerate() {
            if i % 1024 == 0 {
                cancelled(cancel)?;
            }
            let p = part.point(*v);
            let key = p.map(|x| {
                if approximate {
                    ((x / WELD).round() as i64) as u64
                } else if x == 0.0 {
                    0 // +0 and -0 are the same position.
                } else {
                    x.to_bits()
                }
            });
            let id = *weld.entry(key).or_insert_with(|| {
                vertices.push(p);
                vertices.len() - 1
            });
            indices.push(id);
        }
        let mut triangles = Vec::new();
        let mut edges: BTreeMap<Edge, Vec<usize>> = BTreeMap::new();
        let mut classes = Vec::new();
        let mut orientations = BTreeMap::<Edge, i32>::new();
        for (i, source) in part.geometry.mesh.triangles.iter().enumerate() {
            if i % 1024 == 0 {
                cancelled(cancel)?;
            }
            let t = source.map(|v| indices[v as usize]);
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                return Err(
                    "Surface details are smaller than the recognition welding tolerance".into(),
                );
            }
            let n = normal(t.map(|v| vertices[v]));
            let length = n.iter().map(|x| x * x).sum::<f64>().sqrt();
            if length <= 1e-12 {
                return Err("Recognition found a collapsed surface triangle".into());
            }
            let z = n[2] / length;
            classes.push(if z > 1.0 - 1e-10 {
                Class::Up
            } else if z < -1.0 + 1e-10 {
                Class::Down
            } else if z.abs() < 0.01 {
                Class::Wall
            } else {
                Class::Other
            });
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let e = edge(a, b);
                let adjacent = edges.entry(e).or_default();
                adjacent.push(i);
                if adjacent.len() > 2 {
                    return Err("Recognition requires a manifold surface; an edge belongs to more than two triangles".into());
                }
                *orientations.entry(e).or_default() += if a < b { 1 } else { -1 };
            }
            triangles.push(t);
        }
        if edges.values().any(|faces| faces.len() != 2) {
            return Err("Recognition requires a closed, consistently wound surface. Repair open edges or face orientation; manual operations remain available.".into());
        }
        if orientations.values().any(|sign| *sign != 0) {
            return Err(
                "Recognition found inconsistent face orientation on a closed surface".into(),
            );
        }
        let mut result = Self {
            vertices,
            triangles,
            edges,
            classes,
            owners: vec![usize::MAX; part.geometry.mesh.triangles.len()],
            components: Vec::new(),
        };
        for start in 0..result.triangles.len() {
            if start % 1024 == 0 {
                cancelled(cancel)?;
            }
            if result.owners[start] != usize::MAX {
                continue;
            }
            let id = result.components.len();
            let class = result.classes[start];
            result.owners[start] = id;
            let mut pending = vec![start];
            let mut faces = Vec::new();
            let mut boundary = Vec::new();
            while let Some(face) = pending.pop() {
                faces.push(face);
                if face % 1024 == 0 {
                    cancelled(cancel)?;
                }
                let t = result.triangles[face];
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let adjacent = &result.edges[&edge(a, b)];
                    let other = if adjacent[0] == face {
                        adjacent[1]
                    } else {
                        adjacent[0]
                    };
                    if result.classes[other] == class {
                        if result.owners[other] == usize::MAX {
                            result.owners[other] = id;
                            pending.push(other);
                        }
                    } else {
                        boundary.push((a, b));
                    }
                }
            }
            let loops = trace_loops(&boundary).unwrap_or_default();
            result.components.push(Component {
                class,
                loops,
                faces,
            });
        }
        Ok(result)
    }
    fn same_xy(&self, a: &[usize], b: &[usize]) -> bool {
        let xy = |v: usize| {
            [self.vertices[v][0], self.vertices[v][1]].map(|x| (x / WELD).round() as i64)
        };
        if a.iter().map(|v| xy(*v)).collect::<BTreeSet<_>>() == b.iter().map(|v| xy(*v)).collect() {
            return true;
        }
        // CAD tessellation can sample a cylinder's ends at different angular
        // parameters. Compare supported geometry, never just its bounds.
        let a: Vec<_> = a.iter().map(|v| self.vertices[*v]).collect();
        let b: Vec<_> = b.iter().map(|v| self.vertices[*v]).collect();
        if let (Some((ca, ra)), Some((cb, rb))) = (circle(&a), circle(&b)) {
            return (ca[0] - cb[0]).hypot(ca[1] - cb[1]) <= TOLERANCE
                && (ra - rb).abs() <= TOLERANCE;
        }
        let bounds = |points: &[[f64; 3]]| {
            let mut min = [f64::INFINITY; 2];
            let mut max = [f64::NEG_INFINITY; 2];
            for p in points {
                for i in 0..2 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
            (min, max)
        };
        let (amin, amax) = bounds(&a);
        let (bmin, bmax) = bounds(&b);
        rectangle(&a, amin, amax)
            && rectangle(&b, bmin, bmax)
            && (0..2).all(|i| {
                (amin[i] - bmin[i]).abs() <= TOLERANCE && (amax[i] - bmax[i]).abs() <= TOLERANCE
            })
    }
    fn level(&self, points: &[usize]) -> Option<f64> {
        let z = self.vertices[*points.first()?][2];
        points
            .iter()
            .all(|v| (self.vertices[*v][2] - z).abs() <= TOLERANCE)
            .then_some(z)
    }
    fn across(&self, points: &[usize], exclude: usize) -> Option<usize> {
        let mut owner: Option<usize> = None;
        for i in 0..points.len() {
            let faces = &self.edges[&edge(points[i], points[(i + 1) % points.len()])];
            let found = faces
                .iter()
                .map(|f| self.owners[*f])
                .find(|c| *c != exclude)?;
            if owner.is_some_and(|previous| previous != found) {
                return None;
            }
            owner = Some(found);
        }
        owner
    }
    fn candidate(
        &self,
        top: &[usize],
        top_component: usize,
        top_z: f64,
        bottom_z: f64,
    ) -> Option<Feature> {
        let wall_id = self.across(top, top_component)?;
        let wall = &self.components[wall_id];
        if wall.class != Class::Wall || wall.loops.len() != 2 {
            return None;
        }
        let lower = wall
            .loops
            .iter()
            .find(|points| self.level(points).is_some_and(|z| z < top_z - TOLERANCE))?;
        let low_z = self.level(lower)?;
        if !self.same_xy(top, lower) {
            return None;
        }
        let end_id = self.across(lower, wall_id)?;
        let end = &self.components[end_id];
        let through = match end.class {
            Class::Up if end.loops.len() == 1 && self.same_xy(top, &end.loops[0]) => false,
            Class::Down if (low_z - bottom_z).abs() <= TOLERANCE => true,
            _ => return None,
        };
        let outline: Vec<_> = top.iter().map(|v| self.vertices[*v]).collect();
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in &outline {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let (kind, center, diameter) = if !through && rectangle(&outline, min, max) {
            (
                FeatureKind::RectangularPocket,
                [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0],
                None,
            )
        } else {
            let (center, radius) = circle(&outline)?;
            (FeatureKind::CircularHole, center, Some(radius * 2.0))
        };
        // Independently verify every wall vertex against the proposed vertical
        // extrusion. Shifted angular sampling can tilt cylindrical facets, but
        // tapers, bowed walls and offset lower contours must not pass as vertical.
        for v in wall.faces.iter().flat_map(|face| self.triangles[*face]) {
            let point = self.vertices[v];
            let valid = if let Some(diameter) = diameter {
                ((point[0] - center[0]).hypot(point[1] - center[1]) - diameter / 2.0).abs()
                    <= TOLERANCE
            } else {
                (0..2).all(|i| point[i] >= min[i] - TOLERANCE && point[i] <= max[i] + TOLERANCE)
                    && (0..2)
                        .flat_map(|i| [(point[i] - min[i]).abs(), (point[i] - max[i]).abs()])
                        .fold(f64::INFINITY, f64::min)
                        <= TOLERANCE
            };
            if !valid {
                return None;
            }
        }
        Some(Feature {
            kind,
            min,
            max,
            center,
            diameter,
            opening_z: top_z,
            bottom_z: low_z,
            through,
            outline,
            detail: Detail {
                triangles: wall
                    .faces
                    .iter()
                    .chain(if through { [].iter() } else { end.faces.iter() })
                    .copied()
                    .collect(),
                floor_z: (!through).then_some(low_z),
                ..Default::default()
            },
        })
    }
}

fn trace_loops(edges: &[Edge]) -> Option<Vec<Vec<usize>>> {
    let mut next = BTreeMap::new();
    let mut incoming = BTreeSet::new();
    for &(a, b) in edges {
        if next.insert(a, b).is_some() || !incoming.insert(b) {
            return None;
        }
    }
    if next.keys().any(|v| !incoming.contains(v)) {
        return None;
    }
    let mut loops = Vec::new();
    while let Some((&start, _)) = next.first_key_value() {
        let mut points = vec![start];
        let mut current = next.remove(&start)?;
        while current != start {
            points.push(current);
            current = next.remove(&current)?;
        }
        if points.len() < 3 {
            return None;
        }
        loops.push(points);
    }
    Some(loops)
}
fn signed_area(points: &[[f64; 3]]) -> f64 {
    let origin = points[0];
    (0..points.len())
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            (a[0] - origin[0]) * (b[1] - origin[1]) - (b[0] - origin[0]) * (a[1] - origin[1])
        })
        .sum::<f64>()
        / 2.0
}
fn rectangle(points: &[[f64; 3]], min: [f64; 2], max: [f64; 2]) -> bool {
    if (0..2).any(|i| max[i] - min[i] < 0.01) {
        return false;
    }
    let mut corners = BTreeSet::new();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if (a[0] - b[0]).abs() > TOLERANCE && (a[1] - b[1]).abs() > TOLERANCE {
            return false;
        }
        let sides = (0..2)
            .map(|axis| {
                if (a[axis] - min[axis]).abs() <= TOLERANCE {
                    Some(0)
                } else if (a[axis] - max[axis]).abs() <= TOLERANCE {
                    Some(1)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if let (Some(x), Some(y)) = (sides[0], sides[1]) {
            corners.insert((x, y));
        }
        if sides.iter().all(Option::is_none) {
            return false;
        }
    }
    corners.len() == 4
        && (signed_area(points).abs() - (max[0] - min[0]) * (max[1] - min[1])).abs()
            <= TOLERANCE * ((max[0] - min[0]) + (max[1] - min[1]))
}
fn circle(points: &[[f64; 3]]) -> Option<([f64; 2], f64)> {
    if points.len() < 12 || points.len() > 4096 {
        return None;
    }
    let a = points[0];
    let b = points[points.len() / 3];
    let c = points[points.len() * 2 / 3];
    let b = [b[0] - a[0], b[1] - a[1]];
    let c = [c[0] - a[0], c[1] - a[1]];
    let d = 2.0 * (b[0] * c[1] - b[1] * c[0]);
    if d.abs() < 1e-9 {
        return None;
    }
    let bb = b[0] * b[0] + b[1] * b[1];
    let cc = c[0] * c[0] + c[1] * c[1];
    let center = [
        a[0] + (bb * c[1] - cc * b[1]) / d,
        a[1] + (b[0] * cc - c[0] * bb) / d,
    ];
    let radius = (a[0] - center[0]).hypot(a[1] - center[1]);
    if radius < 0.005
        || points
            .iter()
            .any(|p| ((p[0] - center[0]).hypot(p[1] - center[1]) - radius).abs() > TOLERANCE)
    {
        return None;
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if (a[0] - b[0]).hypot(a[1] - b[1]) > radius * 0.55 {
            return None;
        }
    }
    Some((center, radius))
}

pub fn recognize(part: &Part) -> Result<Report, String> {
    recognize_with_cancel(part, &AtomicBool::new(false))
}
/// Shared closed/oriented mesh precondition for model-derived surface machining.
pub(crate) fn validate_closed(part: &Part, cancel: &AtomicBool) -> Result<(), String> {
    Topology::build(part, cancel).map(|_| ())
}
pub fn recognize_with_cancel(part: &Part, cancel: &AtomicBool) -> Result<Report, String> {
    recognize_level(part, None, cancel)
}
pub fn recognize_project(project: &Project) -> Result<Report, String> {
    recognize_project_with_cancel(project, &AtomicBool::new(false))
}
pub fn recognize_project_with_cancel(
    project: &Project,
    cancel: &AtomicBool,
) -> Result<Report, String> {
    recognize_level(
        project
            .part
            .as_ref()
            .ok_or("Import a model before recognizing features")?,
        crate::faces::level(project)?,
        cancel,
    )
}
pub fn recognize_level_for_selection(part: &Part, level: f64) -> Result<Report, String> {
    let cancel = AtomicBool::new(false);
    let topology = Topology::build(part, &cancel)?;
    recognize_detailed(part, Some(level), &topology, &cancel)
}
fn recognize_level(part: &Part, level: Option<f64>, cancel: &AtomicBool) -> Result<Report, String> {
    let topology = Topology::build(part, cancel)?;
    recognize_topology(part, level, &topology, cancel)
}
fn recognize_topology(
    part: &Part,
    level: Option<f64>,
    topology: &Topology,
    cancel: &AtomicBool,
) -> Result<Report, String> {
    let bounds = part.bounds();
    let top_z = level.unwrap_or(bounds.max[2]);
    let mut features = Vec::new();
    let mut skipped_openings = 0;
    let mut used = BTreeSet::new();
    for (id, component) in topology.components.iter().enumerate() {
        cancelled(cancel)?;
        if component.class != Class::Up {
            continue;
        }
        for boundary in &component.loops {
            if !topology
                .level(boundary)
                .is_some_and(|z| (z - top_z).abs() <= TOLERANCE)
            {
                continue;
            }
            let outline: Vec<_> = boundary.iter().map(|v| topology.vertices[*v]).collect();
            if signed_area(&outline) >= -TOLERANCE * TOLERANCE {
                continue;
            }
            if let Some(feature) = topology.candidate(boundary, id, top_z, bounds.min[2]) {
                used.insert(id);
                if let Some(wall) = topology.across(boundary, id) {
                    used.insert(wall);
                    for lower in &topology.components[wall].loops {
                        if topology
                            .level(lower)
                            .is_some_and(|z| (z - feature.bottom_z).abs() <= TOLERANCE)
                        {
                            if let Some(end) = topology.across(lower, wall) {
                                used.insert(end);
                            }
                        }
                    }
                }
                features.push(feature);
                if features.len() > MAX_FEATURES {
                    return Err(
                        "Recognition exceeds 500 candidates; split the model before recognition"
                            .into(),
                    );
                }
            } else {
                skipped_openings += 1;
            }
        }
    }
    features.sort_by(|a, b| {
        a.center[0]
            .total_cmp(&b.center[0])
            .then(a.center[1].total_cmp(&b.center[1]))
    });
    let exterior = topology.exterior(top_z, bounds.min[2], used, skipped_openings);
    let machining_error = exterior.as_ref().err().cloned();
    Ok(Report{exterior:exterior.ok(),machining_error,features,skipped_openings,tolerance_mm:TOLERANCE,notes:vec![
        "Mesh-derived candidates: verify dimensions and accessibility against the source model.".into(),
        "Only top-level vertical circular holes and axis-aligned rectangular blind pockets are supported. Tapers, islands, counterbores and intersecting openings are skipped.".into(),
        "Circular fits cannot distinguish an intended circle from a sufficiently fine polygon. Review these inferred dimensions before machining.".into(),
    ]})
}

fn accessible(part: &Part, feature: &Feature) -> Result<(), String> {
    if part.bounds().max[2] <= feature.opening_z + TOLERANCE {
        return Ok(());
    }
    for triangle in &part.geometry.mesh.triangles {
        let mut poly = crate::surface::clip(
            triangle
                .iter()
                .map(|i| part.point(part.geometry.mesh.vertices[*i as usize]))
                .collect(),
            2,
            feature.opening_z + TOLERANCE,
            true,
        );
        if poly.is_empty() {
            continue;
        }
        let blocked = if let Some(diameter) = feature.diameter {
            let center = feature.center;
            let mut minimum = f64::INFINITY;
            let (mut positive, mut negative) = (false, false);
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                let d = [b[0] - a[0], b[1] - a[1]];
                let len = d[0] * d[0] + d[1] * d[1];
                let t = if len > 1e-20 {
                    ((center[0] - a[0]) * d[0] + (center[1] - a[1]) * d[1]) / len
                } else {
                    0.
                }
                .clamp(0., 1.);
                minimum =
                    minimum.min((center[0] - a[0] - t * d[0]).hypot(center[1] - a[1] - t * d[1]));
                let cross = d[0] * (center[1] - a[1]) - d[1] * (center[0] - a[0]);
                positive |= cross > 1e-10;
                negative |= cross < -1e-10;
            }
            if poly.len() >= 3 && signed_area(&poly).abs() > 1e-10 && !(positive && negative) {
                minimum = 0.;
            }
            minimum < diameter / 2. - TOLERANCE
        } else {
            for (axis, bound, greater) in [
                (0, feature.min[0] + TOLERANCE, true),
                (0, feature.max[0] - TOLERANCE, false),
                (1, feature.min[1] + TOLERANCE, true),
                (1, feature.max[1] - TOLERANCE, false),
            ] {
                poly = crate::surface::clip(poly, axis, bound, greater);
            }
            !poly.is_empty()
        };
        if blocked {
            return Err("This feature is obstructed above the selected plane. Change the setup or clear the overhang with a suitable strategy".into());
        }
    }
    Ok(())
}
/// Copy reviewed geometry into a disabled operation; do not guess a suitable tool.
pub fn operation(project: &Project, feature: &Feature) -> Result<Operation, String> {
    if crate::faces::level(project)?.is_some_and(|z| (z - feature.opening_z).abs() > TOLERANCE) {
        return Err("Recognize the selected machining plane before adding this feature".into());
    }
    if project.process != Process::Milling {
        return Err("Recognized operations currently require the Milling workspace".into());
    }
    if project.operations.len() >= 1000 {
        return Err("Maximum 1,000 operations".into());
    }
    if let Some(part) = &project.part {
        accessible(part, feature)?;
    }
    if !feature.opening_z.is_finite()
        || feature.opening_z > TOLERANCE
        || !feature.bottom_z.is_finite()
        || feature.bottom_z >= -0.001
    {
        return Err("The feature opening must be at or below stock top Z=0".into());
    }
    let tool = project
        .library_tool(None)?
        .ok_or("Choose an active tool from the tool library")?;
    let strategy = match feature.kind {
        FeatureKind::RectangularPocket if tool.kind == ToolKind::FlatEndMill => Strategy::Pocket,
        FeatureKind::CircularHole
            if matches!(tool.kind, ToolKind::Drill | ToolKind::FlatEndMill) =>
        {
            if tool.kind == ToolKind::FlatEndMill {
                let diameter = feature.diameter.ok_or("Missing hole diameter")?;
                if tool.dimension("diameter") > diameter + TOLERANCE {
                    return Err("The active end mill is larger than the circular feature".into());
                }
                Strategy::CircularPocket
            } else {
                if !feature.through && tool.kind == ToolKind::Drill {
                    return Err("A flat-bottom blind hole needs a supported bottom strategy; use a matching center-cutting flat end mill or define the drilling operation manually".into());
                }
                if feature.diameter.is_none_or(|d| {
                    !d.is_finite() || (d - tool.dimension("diameter")).abs() > TOLERANCE
                }) {
                    return Err("Hole diameter must match the active tool diameter within 0.002 mm; interpolated boring is not implemented".into());
                }
                Strategy::Drill
            }
        }
        _ => {
            return Err(
                "Choose a flat end mill for a pocket, or a matching drill/flat end mill for a hole"
                    .into(),
            )
        }
    };
    let mut op = draft(project, feature, strategy)?;
    op.settings.machining_face = project.setup_face;
    if let Some(part) = &project.part {
        crate::machining::link(&mut op, part);
    }
    op.enabled = true;
    if strategy == Strategy::Drill && feature.through {
        let angle = tool.dimension("point_angle").to_radians() / 2.0;
        op.depth += tool.dimension("diameter") / 2.0 / angle.tan();
        if op.depth > project.stock.thickness {
            return Err("Through drilling needs additional blank thickness for the drill point; increase the bottom allowance or choose a flat end mill".into());
        }
    }
    if let Some(part) = &project.part {
        crate::machining::link(&mut op, part);
    }
    let mut check = project.clone();
    check.operations = vec![op.clone()];
    crate::cam::generate(&check)?;
    op.enabled = false;
    Ok(op)
}
/// Geometry-only draft: tooling is selected in the operation editor and checked on save/generate.
pub fn draft(
    project: &Project,
    feature: &Feature,
    strategy: Strategy,
) -> Result<Operation, String> {
    if let Some(part) = &project.part {
        accessible(part, feature)?;
    }
    if !feature.detail.shape.machinable() {
        return Err("This feature needs 3D surface machining or a dedicated strategy; a simple pocket/drill would remove its taper or islands".into());
    }
    let compatible = match feature.kind {
        FeatureKind::RectangularPocket => {
            matches!(strategy, Strategy::Pocket | Strategy::InsideProfile)
        }
        FeatureKind::CircularHole => strategy == Strategy::CircularPocket || strategy.is_drilling(),
    };
    if !compatible {
        return Err("Choose a strategy compatible with this feature".into());
    }
    let mut op = Operation {
        model_link: None,
        settings: crate::operation::Settings {
            start_depth: (feature.opening_z < -TOLERANCE).then_some(-feature.opening_z),
            machining_face: None,
            ..Default::default()
        },
        tool_id: project.active_tool,
        preset_id: project.active_preset,
        name: format!(
            "Recognized {} (review)",
            feature.kind.label().to_lowercase()
        ),
        enabled: true,
        strategy,
        x: if strategy.is_drilling() || strategy == Strategy::CircularPocket {
            feature.center[0]
        } else {
            feature.min[0]
        },
        y: if strategy.is_drilling() || strategy == Strategy::CircularPocket {
            feature.center[1]
        } else {
            feature.min[1]
        },
        width: feature.diameter.unwrap_or(feature.max[0] - feature.min[0]),
        height: feature.diameter.unwrap_or(feature.max[1] - feature.min[1]),
        depth: -feature.bottom_z,
    };
    if let Some(part) = &project.part {
        crate::machining::link(&mut op, part);
    }
    op.enabled = false;
    Ok(op)
}

/// Survey supported prismatic features at every planar level in each model orientation.
/// Results remain candidates; no setup or operations are changed.
pub fn survey(project: &Project) -> Result<serde_json::Value, String> {
    let part = project.part.as_ref().ok_or("Import a model first")?;
    validate_closed(part, &AtomicBool::new(false))?;
    let faces = crate::faces::survey_faces(part)?;
    let mut directions: Vec<[f64; 3]> = Vec::new();
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut identities = BTreeMap::<String, usize>::new();
    let mut owned = BTreeSet::new();
    let cancel = AtomicBool::new(false);
    for face in faces {
        if directions
            .iter()
            .any(|n| n.iter().zip(face.normal).map(|(a, b)| a * b).sum::<f64>() > 1. - 1e-8)
        {
            continue;
        }
        if directions.len() >= 64
            || (directions.len() + 1) * part.geometry.mesh.triangles.len() > 4_000_000
        {
            return Err("Model survey exceeds its orientation work budget. Split the model; manual face selection remains available".into());
        }
        directions.push(face.normal);
        let mut oriented = project.clone();
        crate::faces::setup_selected(&mut oriented, &face, true)?;
        let aligned = oriented.part.as_ref().unwrap();
        let topology = Topology::build(aligned, &cancel)?;
        let mut levels = Vec::<f64>::new();
        for component in &topology.components {
            if component.class != Class::Up {
                continue;
            }
            for boundary in &component.loops {
                if let Some(z) = topology.level(boundary) {
                    if !levels.iter().any(|v| (v - z).abs() <= TOLERANCE) {
                        levels.push(z);
                    }
                }
            }
        }
        let matrix = crate::blank::rotation(aligned.rotation);
        let mut report = recognize_detailed(aligned, None, &topology, &cancel)?;
        for z in levels {
            if (z - aligned.bounds().max[2]).abs() > TOLERANCE {
                report
                    .features
                    .extend(recognize_detailed(aligned, Some(z), &topology, &cancel)?.features);
            }
        }
        for feature in report.features {
            if feature.detail.shape != Shape::OpenPocket {
                owned.extend(feature.detail.triangles.iter().copied());
            }
            let transform = |p: [f64; 3]| {
                let q: [f64; 3] = std::array::from_fn(|i| p[i] - aligned.offset[i]);
                part.point(std::array::from_fn(|i| {
                    (0..3).map(|j| matrix[j][i] * q[j]).sum::<f64>() / aligned.units.factor()
                }))
            };
            let outline: Vec<_> = feature.outline.iter().map(|p| transform(*p)).collect();
            let mut axis = [
                transform([feature.center[0], feature.center[1], feature.opening_z])
                    .map(|v| (v / TOLERANCE).round() as i64),
                transform([feature.center[0], feature.center[1], feature.bottom_z])
                    .map(|v| (v / TOLERANCE).round() as i64),
            ];
            axis.sort();
            let identity = if feature.through && feature.kind == FeatureKind::CircularHole {
                format!("{axis:?}:{:.3}", feature.diameter.unwrap_or(0.))
            } else {
                format!("unique:{}", results.len())
            };
            let direction =
                serde_json::json!({"normal":face.normal,"face":face.id,"current":face.horizontal});
            let entry = serde_json::json!({"label":feature.label(),"machinable":feature.detail.shape.machinable(),"marker":transform([feature.center[0],feature.center[1],feature.opening_z]),"feature":feature,"outline":outline,"normal":face.normal,"face":face.id,"current_orientation":face.horizontal,"orientations":[direction.clone()]});
            if let Some(&index) = identities.get(&identity) {
                let existing = &mut results[index];
                let mut orientations = existing["orientations"].as_array().unwrap().clone();
                orientations.push(direction);
                if face.horizontal {
                    *existing = entry;
                }
                existing["orientations"] = serde_json::json!(orientations);
            } else {
                identities.insert(identity, results.len());
                results.push(entry);
            }
            if results.len() > 500 {
                return Err("Recognition exceeds 500 candidates; split the model".into());
            }
        }
    }
    // A wall of an already recognized feature is not another open pocket when
    // the same part is inspected from a different orientation.
    results.retain(|entry| {
        entry["feature"]["detail"]["shape"] != "open_pocket"
            || entry["feature"]["detail"]["floor_triangles"]
                .as_array()
                .is_none_or(|ids| {
                    !ids.iter()
                        .all(|id| id.as_u64().is_some_and(|i| owned.contains(&(i as usize))))
                })
    });
    Ok(
        serde_json::json!({"features":results,"notes":["Inspects holes (including entry chamfers and rounded transitions), tapers, pockets with islands and open pockets across model orientations. Taper/island/open-pocket candidates are inspection geometry and require a suitable 3D or dedicated operation; simple drilling/pocketing is restricted to compatible bores and prismatic cavities. Intersecting or unclassified freeform geometry can remain unrecognized."]}),
    )
}

/// Direct selection inspects all XY opening levels, without a separate recognition action.
pub fn pick_feature(
    project: &Project,
    origin: [f64; 3],
    direction: [f64; 3],
) -> Result<Feature, String> {
    let part = project.part.as_ref().ok_or("Import a model first")?;
    if direction[2].abs() < 1e-12 {
        return Err("View the opening from above to select it".into());
    }
    let cancel = AtomicBool::new(false);
    let topology = Topology::build(part, &cancel)?;
    let mut levels = Vec::<f64>::new();
    let nearest = crate::faces::ray_hit(part, origin, direction)
        .ok()
        .map(|(_, t)| t);
    let mut found = Vec::new();
    for c in &topology.components {
        if c.class == Class::Up {
            for boundary in &c.loops {
                if let Some(z) = topology.level(boundary) {
                    if !levels.iter().any(|v| (v - z).abs() < TOLERANCE) {
                        levels.push(z);
                    }
                }
            }
        }
    }
    let mut candidates = recognize_detailed(part, None, &topology, &cancel)?.features;
    for z in levels {
        if (z - part.bounds().max[2]).abs() > TOLERANCE {
            candidates.extend(recognize_detailed(part, Some(z), &topology, &cancel)?.features);
        }
    }
    for feature in candidates {
        let z = feature.opening_z;
        let t = (z - origin[2]) / direction[2];
        if t < 0. || nearest.is_some_and(|hit| t > hit + TOLERANCE) {
            continue;
        }
        let p = [origin[0] + t * direction[0], origin[1] + t * direction[1]];
        let mut inside = false;
        for i in 0..feature.outline.len() {
            let a = feature.outline[i];
            let b = feature.outline[(i + 1) % feature.outline.len()];
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
        }
        if inside {
            found.push((t, feature));
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
        .into_iter()
        .next()
        .map(|(_, f)| f)
        .ok_or("Click inside a supported hole or rectangular pocket opening".into())
}

impl Topology {
    fn chain_candidate(
        &self,
        top: &[usize],
        top_component: usize,
        top_z: f64,
        part_bottom: f64,
    ) -> Option<Feature> {
        let outline: Vec<_> = top.iter().map(|i| self.vertices[*i]).collect();
        let top_circle = circle(&outline);
        let mut rings = vec![outline.clone()];
        let mut faces = BTreeSet::new();
        let mut previous = top_component;
        let mut boundary = top.to_vec();
        let mut level = top_z;
        let mut bore: Option<(f64, f64, f64)> = None;
        let mut curved = false;
        let mut through = false;
        let mut floor = None;
        for _ in 0..32 {
            let id = self.across(&boundary, previous)?;
            let c = &self.components[id];
            if matches!(c.class, Class::Up | Class::Down) {
                through = c.class == Class::Down && (level - part_bottom).abs() <= TOLERANCE;
                if !through && !(c.class == Class::Up && c.loops.len() == 1) {
                    return None;
                }
                if !through {
                    faces.extend(c.faces.iter().copied());
                    floor = Some(level);
                }
                break;
            }
            if c.loops.len() != 2 {
                return None;
            }
            let lower = c
                .loops
                .iter()
                .find(|l| self.level(l).is_some_and(|z| z < level - TOLERANCE))?;
            let z = self.level(lower)?;
            let points: Vec<_> = lower.iter().map(|i| self.vertices[*i]).collect();
            if c.faces.iter().flat_map(|i| self.triangles[*i]).any(|i| {
                self.vertices[i][2] > level + TOLERANCE || self.vertices[i][2] < z - TOLERANCE
            }) {
                return None;
            }
            if let (Some((center, r0)), Some((c1, r1))) = (circle(rings.last()?), circle(&points)) {
                if (center[0] - c1[0]).hypot(center[1] - c1[1]) > TOLERANCE {
                    return None;
                }
                let cylindrical = c.class == Class::Wall
                    && (r0 - r1).abs() <= TOLERANCE
                    && c.faces.iter().flat_map(|i| self.triangles[*i]).all(|i| {
                        ((self.vertices[i][0] - center[0]).hypot(self.vertices[i][1] - center[1])
                            - r0)
                            .abs()
                            <= TOLERANCE
                    });
                if cylindrical && bore.is_none_or(|(a, b, _)| level - z > a - b) {
                    bore = Some((level, z, r0));
                }
                if c.class == Class::Other {
                    curved |= c.faces.iter().flat_map(|i| self.triangles[*i]).any(|i| {
                        let p = self.vertices[i];
                        let expected = r1 + (r0 - r1) * (p[2] - z) / (level - z);
                        ((p[0] - center[0]).hypot(p[1] - center[1]) - expected).abs() > TOLERANCE
                    });
                }
            } else if top_circle.is_some() {
                return None;
            }
            faces.extend(c.faces.iter().copied());
            rings.push(points);
            previous = id;
            boundary = lower.clone();
            level = z;
        }
        if floor.is_none() && !through {
            return None;
        }
        let min =
            std::array::from_fn(|i| outline.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
        let max = std::array::from_fn(|i| {
            outline
                .iter()
                .map(|p| p[i])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        let (kind, center, diameter, shape, bottom) = if let Some((center, r)) = top_circle {
            if let Some((bore_top, bore_bottom, radius)) = bore {
                let entry_fits = rings
                    .iter()
                    .filter(|ring| ring[0][2] >= bore_top - TOLERANCE)
                    .all(|ring| circle(ring).is_some_and(|(_, r)| r >= radius - TOLERANCE));
                if !entry_fits {
                    return None;
                }
                let lower_fits = rings
                    .iter()
                    .filter(|ring| ring[0][2] < bore_bottom - TOLERANCE)
                    .all(|ring| circle(ring).is_some_and(|(_, r)| r >= radius - TOLERANCE));
                (
                    FeatureKind::CircularHole,
                    center,
                    Some(radius * 2.),
                    if curved || !lower_fits {
                        Shape::FilletedBore
                    } else {
                        Shape::ChamferedBore
                    },
                    if lower_fits { level } else { bore_bottom },
                )
            } else {
                (
                    FeatureKind::CircularHole,
                    center,
                    Some(r * 2.),
                    Shape::TaperedHole,
                    level,
                )
            }
        } else {
            (
                FeatureKind::RectangularPocket,
                [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.],
                None,
                Shape::TaperedPocket,
                level,
            )
        };
        Some(Feature {
            kind,
            min,
            max,
            center,
            diameter,
            opening_z: top_z,
            bottom_z: bottom,
            through,
            outline,
            detail: Detail {
                shape,
                triangles: faces.into_iter().collect(),
                profiles: rings,
                islands: vec![],
                floor_z: floor,
                floor_triangles: vec![],
            },
        })
    }
    fn floor_candidates(&self, top_z: f64) -> Vec<Feature> {
        let mut result = Vec::new();
        for (id, c) in self
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.class == Class::Up)
        {
            let Some(z) = c.loops.first().and_then(|l| self.level(l)) else {
                continue;
            };
            if z >= top_z - TOLERANCE {
                continue;
            }
            let Some(outer) = c
                .loops
                .iter()
                .filter(|l| {
                    signed_area(&l.iter().map(|i| self.vertices[*i]).collect::<Vec<_>>())
                        > TOLERANCE * TOLERANCE
                })
                .max_by_key(|l| l.len())
            else {
                continue;
            };
            let outline: Vec<_> = outer.iter().map(|i| self.vertices[*i]).collect();
            let mut perimeter = 0.;
            let mut steep_edges = 0.;
            let mut up = false;
            let mut open = false;
            let mut opening = z;
            let mut adjacent = BTreeSet::new();
            for i in 0..outer.len() {
                let a = self.vertices[outer[i]];
                let b = self.vertices[outer[(i + 1) % outer.len()]];
                let edge_length = (a[0] - b[0]).hypot(a[1] - b[1]);
                perimeter += edge_length;
                let edge = edge(outer[i], outer[(i + 1) % outer.len()]);
                for &face in &self.edges[&edge] {
                    if self.owners[face] == id {
                        continue;
                    }
                    let tri = self.triangles[face].map(|v| self.vertices[v]);
                    let n = normal(tri);
                    let length = n.iter().map(|v| v * v).sum::<f64>().sqrt();
                    if length > 1e-15 && (n[2] / length).abs() < 0.5 {
                        steep_edges += edge_length;
                    }
                    let hi = tri.iter().map(|p| p[2]).fold(z, f64::max);
                    let lo = tri.iter().map(|p| p[2]).fold(z, f64::min);
                    up |= hi > z + TOLERANCE;
                    open |= lo < z - TOLERANCE;
                    adjacent.insert(self.owners[face]);
                }
            }
            let islands: Vec<_> = c
                .loops
                .iter()
                .filter(|l| {
                    signed_area(&l.iter().map(|i| self.vertices[*i]).collect::<Vec<_>>())
                        < -TOLERANCE * TOLERANCE
                })
                .map(|l| l.iter().map(|i| self.vertices[*i]).collect::<Vec<_>>())
                .collect();
            if !up || (!open && islands.is_empty()) || steep_edges < perimeter * 0.5 {
                continue;
            }
            for ring in &c.loops {
                for i in 0..ring.len() {
                    for &face in &self.edges[&edge(ring[i], ring[(i + 1) % ring.len()])] {
                        if self.owners[face] != id {
                            adjacent.insert(self.owners[face]);
                        }
                    }
                }
            }
            let min: [f64; 2] =
                std::array::from_fn(|i| outline.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
            let max: [f64; 2] = std::array::from_fn(|i| {
                outline
                    .iter()
                    .map(|p| p[i])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            let mut triangles = c.faces.clone();
            for other in adjacent {
                for &face in &self.components[other].faces {
                    let tri = self.triangles[face].map(|i| self.vertices[i]);
                    if tri.iter().all(|p| {
                        p[2] >= z - TOLERANCE
                            && (0..2)
                                .all(|i| p[i] >= min[i] - TOLERANCE && p[i] <= max[i] + TOLERANCE)
                    }) {
                        opening = tri.iter().map(|p| p[2]).fold(opening, f64::max);
                        triangles.push(face);
                    }
                }
            }
            if opening <= z + TOLERANCE {
                continue;
            }
            result.push(Feature {
                kind: FeatureKind::RectangularPocket,
                min,
                max,
                center: std::array::from_fn(|i| (min[i] + max[i]) / 2.),
                diameter: None,
                opening_z: opening,
                bottom_z: z,
                through: false,
                outline,
                detail: Detail {
                    shape: if open {
                        Shape::OpenPocket
                    } else {
                        Shape::PocketWithIslands
                    },
                    triangles,
                    profiles: vec![],
                    islands,
                    floor_z: Some(z),
                    floor_triangles: c.faces.clone(),
                },
            });
        }
        result
    }
}
fn recognize_detailed(
    part: &Part,
    level: Option<f64>,
    topology: &Topology,
    cancel: &AtomicBool,
) -> Result<Report, String> {
    let mut report = recognize_topology(part, level, topology, cancel)?;
    let bounds = part.bounds();
    for (id, c) in topology
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.class == Class::Up)
    {
        for boundary in &c.loops {
            let Some(z) = topology.level(boundary) else {
                continue;
            };
            if (level.unwrap_or(bounds.max[2]) - z).abs() > TOLERANCE {
                continue;
            }
            let outline: Vec<_> = boundary.iter().map(|i| topology.vertices[*i]).collect();
            if signed_area(&outline) >= -TOLERANCE * TOLERANCE
                || topology.candidate(boundary, id, z, bounds.min[2]).is_some()
            {
                continue;
            }
            if let Some(f) = topology.chain_candidate(boundary, id, z, bounds.min[2]) {
                report.features.push(f);
            }
        }
    }
    if level.is_none() {
        report
            .features
            .extend(topology.floor_candidates(bounds.max[2]));
    }
    if report.features.len() > MAX_FEATURES {
        return Err("Recognition exceeds 500 candidates; split the model".into());
    }
    Ok(report)
}
