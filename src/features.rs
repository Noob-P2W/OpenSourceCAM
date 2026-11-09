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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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

#[derive(Clone, Debug, Serialize)]
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
}
impl Feature {
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
            let loops = if class == Class::Other {
                Vec::new()
            } else {
                trace_loops(&boundary).unwrap_or_default()
            };
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
    let topology = Topology::build(part, cancel)?;
    let bounds = part.bounds();
    let top_z = bounds.max[2];
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

/// Copy reviewed geometry into a disabled operation; do not guess a suitable tool.
pub fn operation(project: &Project, feature: &Feature) -> Result<Operation, String> {
    if project.process != Process::Milling {
        return Err("Recognized operations currently require the Milling workspace".into());
    }
    if project.operations.len() >= 1000 {
        return Err("Maximum 1,000 operations".into());
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
    let mut op = Operation {
        model_link: None,
        settings: Default::default(),
        tool_id: Some(tool.id),
        preset_id: Some(project.active_preset.unwrap_or(tool.default_preset)),
        name: format!(
            "Recognized {} (review)",
            feature.kind.label().to_lowercase()
        ),
        enabled: true,
        strategy,
        x: if matches!(strategy, Strategy::Drill | Strategy::CircularPocket) {
            feature.center[0]
        } else {
            feature.min[0]
        },
        y: if matches!(strategy, Strategy::Drill | Strategy::CircularPocket) {
            feature.center[1]
        } else {
            feature.min[1]
        },
        width: feature.diameter.unwrap_or(feature.max[0] - feature.min[0]),
        height: feature.diameter.unwrap_or(feature.max[1] - feature.min[1]),
        depth: -feature.bottom_z,
    };
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
