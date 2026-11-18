//! Connected planar mesh faces and three-axis setup orientation.
use crate::{
    import::{normal, Part},
    model::{Operation, Project, Strategy},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Serialize)]
pub struct Face {
    pub id: usize,
    pub normal: [f64; 3],
    pub center: [f64; 3],
    pub area: f64,
    pub triangle_count: usize,
    pub opening_count: usize,
    pub planar_direction: bool,
    pub horizontal: bool,
    pub z: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<[f64; 3]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fill: Vec<[[f64; 3]; 3]>,
}
type BoundaryEdges = BTreeMap<([i64; 3], [i64; 3]), ([f64; 3], [f64; 3], usize)>;
struct Group {
    face: Face,
    triangles: Vec<usize>,
}
fn groups(part: &Part) -> Result<Vec<Group>, String> {
    part.validate()?;
    let mesh = &part.geometry.mesh;
    let mut vertices = Vec::new();
    let mut weld = BTreeMap::new();
    let mut ids = Vec::new();
    let owners = if part.geometry.bodies.is_empty() {
        None
    } else {
        Some(part.geometry.body_owners()?)
    };
    for (index, v) in mesh.vertices.iter().enumerate() {
        let p = part.point(*v);
        let key = p.map(|v| (v / 0.00001).round() as i64);
        let id = *weld
            .entry((owners.as_ref().map_or(usize::MAX, |v| v[index]), key))
            .or_insert_with(|| {
                vertices.push(p);
                vertices.len() - 1
            });
        ids.push(id);
    }
    let triangles: Vec<_> = mesh
        .triangles
        .iter()
        .map(|t| t.map(|i| ids[i as usize]))
        .collect();
    let normals: Vec<_> = triangles
        .iter()
        .map(|t| {
            let n = normal(t.map(|i| vertices[i]));
            let length = n.iter().map(|x| x * x).sum::<f64>().sqrt();
            if length > 1e-15 {
                n.map(|v| v / length)
            } else {
                [0.; 3]
            }
        })
        .collect();
    let edge = |a: usize, b: usize| if a < b { (a, b) } else { (b, a) };
    let mut edges: BTreeMap<_, Vec<usize>> = BTreeMap::new();
    for (i, t) in triangles.iter().enumerate() {
        for j in 0..3 {
            edges.entry(edge(t[j], t[(j + 1) % 3])).or_default().push(i)
        }
    }
    let mut seen = vec![false; triangles.len()];
    let mut groups = Vec::new();
    for seed in 0..triangles.len() {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let n = normals[seed];
        if n == [0.; 3] {
            continue;
        }
        let origin = vertices[triangles[seed][0]];
        let mut queue = VecDeque::from([seed]);
        let mut members = Vec::new();
        let mut area = 0.;
        let mut center = [0.; 3];
        while let Some(id) = queue.pop_front() {
            let t = triangles[id];
            members.push(id);
            let raw = normal(t.map(|i| vertices[i]));
            let a = raw.iter().map(|v| v * v).sum::<f64>().sqrt() / 2.;
            area += a;
            for i in 0..3 {
                center[i] += t.iter().map(|v| vertices[*v][i]).sum::<f64>() / 3. * a;
            }
            for j in 0..3 {
                for next in &edges[&edge(t[j], t[(j + 1) % 3])] {
                    if seen[*next] {
                        continue;
                    }
                    let aligned = (0..3).map(|i| n[i] * normals[*next][i]).sum::<f64>() > 1. - 1e-8;
                    let planar = triangles[*next].iter().all(|v| {
                        (0..3)
                            .map(|i| (vertices[*v][i] - origin[i]) * n[i])
                            .sum::<f64>()
                            .abs()
                            <= 0.002
                    });
                    if aligned && planar {
                        seen[*next] = true;
                        queue.push_back(*next)
                    }
                }
            }
        }
        if area < 1e-10 {
            continue;
        }
        center = center.map(|v| v / area);
        let mut face_edges = BTreeSet::new();
        let mut face_vertices = BTreeSet::new();
        for &id in &members {
            let t = triangles[id];
            for i in 0..3 {
                face_vertices.insert(t[i]);
                face_edges.insert(edge(t[i], t[(i + 1) % 3]));
            }
        }
        let planar_direction = if members.len() > 2 {
            let member_set: BTreeSet<_> = members.iter().copied().collect();
            let mut perimeter = 0.;
            let mut hard_edges = 0.;
            for &(a, b) in &face_edges {
                let outside: Vec<_> = edges[&(a, b)]
                    .iter()
                    .filter(|i| !member_set.contains(i))
                    .copied()
                    .collect();
                if outside.is_empty() {
                    continue;
                }
                let length = (0..3)
                    .map(|i| (vertices[a][i] - vertices[b][i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                perimeter += length;
                if outside
                    .iter()
                    .any(|i| (0..3).map(|axis| n[axis] * normals[*i][axis]).sum::<f64>() < 0.5)
                {
                    hard_edges += length;
                }
            }
            perimeter > 0. && hard_edges >= perimeter * 0.5
        } else {
            false
        };
        let opening_count =
            (1 + face_edges.len()).saturating_sub(face_vertices.len() + members.len());
        groups.push(Group {
            face: Face {
                id: seed,
                normal: n,
                center,
                area,
                triangle_count: members.len(),
                opening_count,
                planar_direction,
                horizontal: n[2] > 1. - 1e-8,
                z: origin[2],
                segments: Vec::new(),
                fill: Vec::new(),
            },
            triangles: members,
        });
    }
    Ok(groups)
}
pub(crate) fn survey_faces(part: &Part) -> Result<Vec<Face>, String> {
    Ok(groups(part)?
        .into_iter()
        .map(|g| g.face)
        .filter(|f| {
            f.opening_count > 0
                || (f.triangle_count > 2 && f.planar_direction)
                || f.normal.iter().any(|n| n.abs() > 1. - 1e-8)
        })
        .collect())
}
pub fn list(part: &Part) -> Result<Vec<Face>, String> {
    let mut faces: Vec<_> = groups(part)?.into_iter().map(|g| g.face).collect();
    faces.sort_by(|a, b| b.area.total_cmp(&a.area).then(a.id.cmp(&b.id)));
    faces.truncate(500);
    Ok(faces)
}
/// Complete display selections at planar boundaries rather than mesh diagonals.
/// This does not change recognition candidates or machining geometry.
pub fn selection(part: &Part, triangles: &[usize]) -> Result<Vec<usize>, String> {
    let count = part.geometry.mesh.triangles.len();
    if triangles.len() > count || triangles.iter().any(|&id| id >= count) {
        return Err("Invalid surface selection".into());
    }
    let selected: BTreeSet<_> = triangles.iter().copied().collect();
    let mut result = Vec::new();
    for group in groups(part)? {
        if group.triangles.iter().any(|id| selected.contains(id)) {
            result.extend(group.triangles);
        }
    }
    result.sort_unstable();
    Ok(result)
}
pub fn face(part: &Part, triangle: usize) -> Result<Face, String> {
    if !part.geometry.bodies.is_empty() {
        part.validate()?;
    }
    // A detailed vise can contain hundreds of thousands of faces in screws and
    // unrelated bodies. Resolve the selected component only, preserving source IDs.
    if let Some(body) = part.geometry.body_at(triangle) {
        let source = &part.geometry;
        let b = &source.bodies[body];
        let mut remap = vec![u32::MAX; source.mesh.vertices.len()];
        let mut vertices = Vec::new();
        let triangles = source.mesh.triangles
            [b.first_triangle..b.first_triangle + b.triangle_count]
            .iter()
            .map(|t| {
                t.map(|i| {
                    let index = &mut remap[i as usize];
                    if *index == u32::MAX {
                        *index = vertices.len() as u32;
                        vertices.push(source.mesh.vertices[i as usize]);
                    }
                    *index
                })
            })
            .collect();
        let mut local = part.clone();
        local.geometry = std::sync::Arc::new(crate::import::Geometry {
            bodies: Vec::new(),
            removed_degenerate_triangles: 0,
            source_name: source.source_name.clone(),
            format: source.format,
            mesh: crate::import::Mesh {
                vertices,
                triangles,
            },
            cad_source: None,
        });
        let mut selected = face(&local, triangle - b.first_triangle)?;
        selected.id += b.first_triangle;
        return Ok(selected);
    }
    let group = groups(part)?
        .into_iter()
        .find(|g| g.triangles.contains(&triangle))
        .ok_or("Select a non-degenerate planar model face")?;
    let mut face = group.face;
    let mut edges: BoundaryEdges = BTreeMap::new();
    for id in group.triangles {
        let p = part.geometry.mesh.triangles[id]
            .map(|v| part.point(part.geometry.mesh.vertices[v as usize]));
        face.fill.push(p);
        for i in 0..3 {
            let (mut a, mut b) = (p[i], p[(i + 1) % 3]);
            let (mut ka, mut kb) = (
                a.map(|v| (v / 0.00001).round() as i64),
                b.map(|v| (v / 0.00001).round() as i64),
            );
            if kb < ka {
                std::mem::swap(&mut ka, &mut kb);
                std::mem::swap(&mut a, &mut b);
            }
            let entry = edges.entry((ka, kb)).or_insert((a, b, 0));
            entry.2 += 1;
        }
    }
    for (a, b, count) in edges.into_values() {
        if count == 1 {
            face.segments.extend([a, b])
        }
    }
    Ok(face)
}
pub fn pick(part: &Part, origin: [f64; 3], direction: [f64; 3]) -> Result<Face, String> {
    face(part, ray_hit(part, origin, direction)?.0)
}
pub(crate) fn ray_hit(
    part: &Part,
    origin: [f64; 3],
    direction: [f64; 3],
) -> Result<(usize, f64), String> {
    if origin
        .iter()
        .chain(direction.iter())
        .any(|v| !v.is_finite())
    {
        return Err("Invalid picking ray".into());
    }
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
    let sub = |a: [f64; 3], b: [f64; 3]| std::array::from_fn(|i| a[i] - b[i]);
    let mut nearest = f64::INFINITY;
    let mut selected = None;
    let vertices: Vec<_> = part
        .geometry
        .mesh
        .vertices
        .iter()
        .map(|v| part.point(*v))
        .collect();
    for (id, t) in part.geometry.mesh.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|i| vertices[i as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let p = cross(direction, v);
        let det = dot(u, p);
        if det.abs() < 1e-12 {
            continue;
        }
        let q = sub(origin, a);
        let s = dot(q, p) / det;
        if !(-1e-8..=1. + 1e-8).contains(&s) {
            continue;
        }
        let r = cross(q, u);
        let w = dot(direction, r) / det;
        if w < -1e-8 || s + w > 1. + 1e-8 {
            continue;
        }
        let distance = dot(v, r) / det;
        if distance >= 0. && distance < nearest {
            nearest = distance;
            selected = Some(id)
        }
    }
    Ok((selected.ok_or("No model face at this point")?, nearest))
}
pub fn setup(project: &mut Project, triangle: usize, align: bool) -> Result<(), String> {
    let selected = face(
        project
            .part
            .as_ref()
            .ok_or("Import a model before selecting a plane")?,
        triangle,
    )?;
    setup_selected(project, &selected, align)
}
pub(crate) fn setup_selected(
    project: &mut Project,
    selected: &Face,
    align: bool,
) -> Result<(), String> {
    let part = project
        .part
        .as_mut()
        .ok_or("Import a model before selecting a plane")?;
    if !selected.horizontal {
        if !align {
            return Err("This face needs a new setup: align it to +Z before 2.5D machining".into());
        }
        let t = part.geometry.mesh.triangles[selected.id]
            .map(|i| part.geometry.mesh.vertices[i as usize]);
        let n = normal(t);
        let rotation = [
            n[1].atan2(n[2]).to_degrees(),
            -n[0].atan2(n[1].hypot(n[2])).to_degrees(),
            0.,
        ];
        part.orient_centered(rotation)?;
        crate::blank::fit(project, 5., 1., 0.5)?;
    }
    project.setup_face = Some(selected.id);
    Ok(())
}
pub fn level(project: &Project) -> Result<Option<f64>, String> {
    project
        .setup_face
        .map(|id| {
            face_level(
                project
                    .part
                    .as_ref()
                    .ok_or("The selected machining plane needs its model")?,
                id,
            )
        })
        .transpose()
}
pub fn face_level(part: &Part, id: usize) -> Result<f64, String> {
    let t = part
        .geometry
        .mesh
        .triangles
        .get(id)
        .ok_or("Selected machining face is no longer available")?
        .map(|i| part.point(part.geometry.mesh.vertices[i as usize]));
    let n = normal(t);
    let length = n.iter().map(|v| v * v).sum::<f64>().sqrt();
    if length <= 1e-15 || n[2] / length < 1. - 1e-8 {
        return Err("Machining face must be aligned with +Z; align the face in Setup first".into());
    }
    Ok(t[0][2])
}
pub fn facing(project: &Project, triangle: usize) -> Result<Operation, String> {
    let part = project.part.as_ref().ok_or("Import a model first")?;
    let selected = face(part, triangle)?;
    if !selected.horizontal {
        return Err("Align this face to +Z in Setup before facing".into());
    }
    if selected.z < part.bounds().max[2] - 0.002 {
        return Err("Whole-stock facing would cut higher model geometry. Use 3D clearing for a recessed face or change the setup".into());
    }
    if selected.z >= -0.0001 {
        return Err(
            "No facing stock above this face. Add a top allowance in Setup before facing".into(),
        );
    }
    let mut op = crate::commands::operation_template(project, Strategy::Facing);
    op.depth = -part.bounds().max[2];
    op.settings.machining_face = Some(selected.id);
    op.name = "Face stock to selected model surface".into();
    crate::machining::link(&mut op, part);
    Ok(op)
}

/// Pick the nearest boundary loop of the hit face. Original edge vertices are retained.
pub fn pick_chain(
    part: &Part,
    origin: [f64; 3],
    direction: [f64; 3],
) -> Result<Vec<[f64; 3]>, String> {
    let (id, t) = ray_hit(part, origin, direction)?;
    let face = face(part, id)?;
    let hit: [f64; 3] = std::array::from_fn(|i| origin[i] + t * direction[i]);
    let loops = boundary_loops(&face)?;
    let distance = |chain: &Vec<[f64; 3]>| {
        chain
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let b = chain[(i + 1) % chain.len()];
                let v: [f64; 3] = std::array::from_fn(|j| b[j] - a[j]);
                let len = v.iter().map(|v| v * v).sum::<f64>();
                let t = ((0..3).map(|j| (hit[j] - a[j]) * v[j]).sum::<f64>() / len.max(1e-20))
                    .clamp(0., 1.);
                (0..3)
                    .map(|j| (hit[j] - a[j] - t * v[j]).powi(2))
                    .sum::<f64>()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let mut chain = loops
        .into_iter()
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        .ok_or("No boundary chain at this point")?;
    let first = chain
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let d = |p: &&[f64; 3]| (0..3).map(|i| (p[i] - hit[i]).powi(2)).sum::<f64>();
            d(a).total_cmp(&d(b))
        })
        .map(|(i, _)| i)
        .unwrap_or(0);
    chain.rotate_left(first);
    Ok(chain)
}

/// Select the boundary segment closest to the actual ray hit, or its complete loop.
pub fn pick_edges(
    part: &Part,
    origin: [f64; 3],
    direction: [f64; 3],
    single: bool,
) -> Result<Vec<[f64; 3]>, String> {
    let chain = pick_chain(part, origin, direction)?;
    if !single {
        return Ok(chain);
    }
    let (_, t) = ray_hit(part, origin, direction)?;
    let hit: [f64; 3] = std::array::from_fn(|i| origin[i] + t * direction[i]);
    let index = (0..chain.len())
        .min_by(|&a, &b| {
            let distance = |i: usize| {
                let p = chain[i];
                let q = chain[(i + 1) % chain.len()];
                let v: [f64; 3] = std::array::from_fn(|j| q[j] - p[j]);
                let len = v.iter().map(|v| v * v).sum::<f64>();
                let t = ((0..3).map(|j| (hit[j] - p[j]) * v[j]).sum::<f64>() / len.max(1e-20))
                    .clamp(0., 1.);
                (0..3)
                    .map(|j| (hit[j] - p[j] - t * v[j]).powi(2))
                    .sum::<f64>()
            };
            distance(a).total_cmp(&distance(b))
        })
        .ok_or("No edge at this point")?;
    Ok(vec![chain[index], chain[(index + 1) % chain.len()]])
}

/// Determine which side of a selected planar boundary is empty from the hit face.
pub fn waste_side(
    part: &Part,
    _origin: [f64; 3],
    _direction: [f64; 3],
    points: &[[f64; 3]],
    closed: bool,
) -> Result<bool, String> {
    let a = points[0];
    let b = points[1];
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]);
    if len < 1e-9 {
        return Ok(true);
    }
    let area = (0..points.len())
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>();
    let sign = if closed && area < 0. { -1. } else { 1. };
    let top = |q: [f64; 2]| {
        let origin = [q[0], q[1], part.bounds().max[2] + 1.];
        ray_hit(part, origin, [0., 0., -1.])
            .ok()
            .map(|(_, t)| origin[2] - t)
    };
    let midpoint = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
    let mut material = false;
    for distance in [0.001_f64, 0.01, 0.05] {
        let delta = distance.min(len / 4.);
        let normal = [-sign * d[1] / len * delta, sign * d[0] / len * delta];
        let inside = top([midpoint[0] + normal[0], midpoint[1] + normal[1]]);
        let outside = top([midpoint[0] - normal[0], midpoint[1] - normal[1]]);
        if let Some(inside) = inside {
            material = inside >= a[2] - 1e-5;
            if outside.is_none_or(|h| inside > h + 1e-5) {
                return Ok(true);
            }
            if outside.is_some_and(|h| inside < h - 1e-5) {
                return Ok(false);
            }
        } else if outside.is_some() {
            return Ok(false);
        }
    }

    Ok(material)
}

pub fn boundary_loops(face: &Face) -> Result<Vec<Vec<[f64; 3]>>, String> {
    let edges = face.segments.as_chunks::<2>().0;
    if edges.len() > 10000 {
        return Err("Selected face boundary exceeds 10,000 edges".into());
    }
    // Use the same welding keys as face extraction. Indexed adjacency avoids a
    // repeated scan of the entire edge list for every point on a detailed loop.
    let key = |p: [f64; 3]| p.map(|v| (v / 0.00001).round() as i64);
    let mut adjacent = BTreeMap::<_, Vec<_>>::new();
    for (i, &[a, b]) in edges.iter().enumerate() {
        adjacent.entry(key(a)).or_default().push((i, b));
        adjacent.entry(key(b)).or_default().push((i, a));
    }
    if adjacent.values().any(|v| v.len() != 2) {
        return Err("Selected face has an open or branching boundary".into());
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for (seed, &[a, b]) in edges.iter().enumerate() {
        if used[seed] {
            continue;
        }
        used[seed] = true;
        let mut chain = vec![a, b];
        while key(*chain.last().unwrap()) != key(a) {
            let (i, next) = adjacent[&key(*chain.last().unwrap())]
                .iter()
                .find(|(i, _)| !used[*i])
                .ok_or("Selected face boundary does not form separate loops")?;
            used[*i] = true;
            chain.push(*next);
        }
        chain.pop();
        if chain.len() < 3 {
            return Err("Selected boundary has fewer than three vertices".into());
        }
        loops.push(chain);
    }
    Ok(loops)
}

/// Horizontal mechanical edges come from adjacent surface normals, not a picked face loop.
pub fn machining_edges(
    part: &Part,
    origin: [f64; 3],
    direction: [f64; 3],
    single: bool,
    tolerance: Option<f64>,
) -> Result<(Vec<[f64; 3]>, bool), String> {
    if origin
        .iter()
        .chain(direction.iter())
        .any(|v| !v.is_finite())
        || direction.iter().map(|v| v * v).sum::<f64>() < 1e-12
        || tolerance.is_some_and(|v| !v.is_finite() || v <= 0.)
    {
        return Err("Invalid edge-selection ray or tolerance".into());
    }
    let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
    let direction = direction.map(|v| v / length);
    let face_hit = ray_hit(part, origin, direction).ok().map(|(_, t)| t);
    type Key = [i64; 3];
    type Edge = (Key, Key);
    type Item = ([f64; 3], [f64; 3], Vec<[f64; 3]>);
    let key = |p: [f64; 3]| p.map(|v| (v / 0.00001).round() as i64);
    let vertices: Vec<_> = part
        .geometry
        .mesh
        .vertices
        .iter()
        .map(|v| part.point(*v))
        .collect();
    let mut candidates = BTreeMap::<Edge, Item>::new();
    for tri in &part.geometry.mesh.triangles {
        let p = tri.map(|i| vertices[i as usize]);
        let n = normal(p);
        let l = n.iter().map(|v| v * v).sum::<f64>().sqrt();
        if l < 1e-12 {
            continue;
        }
        let n = n.map(|v| v / l);
        for j in 0..3 {
            let mut a = p[j];
            let mut b = p[(j + 1) % 3];
            if (a[2] - b[2]).abs() > 1e-6 {
                continue;
            }
            if key(a) > key(b) {
                std::mem::swap(&mut a, &mut b);
            }
            candidates
                .entry((key(a), key(b)))
                .or_insert((a, b, Vec::new()))
                .2
                .push(n);
        }
    }
    let edges: Vec<_> = candidates
        .into_values()
        .filter(|(_, _, n)| {
            n.len() == 1
                || n.iter()
                    .skip(1)
                    .any(|v| (0..3).map(|i| v[i] * n[0][i]).sum::<f64>() < 1. - 1e-8)
        })
        .collect();
    if edges.len() > 100000 {
        return Err("Edge selection exceeds its boundary budget".into());
    }
    // Snap to the edge ray rather than requiring a triangle hit first. A
    // visible silhouette edge may project just outside its adjacent triangles.
    let metric = |a: [f64; 3], b: [f64; 3]| {
        let v: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
        let w: [f64; 3] = std::array::from_fn(|i| origin[i] - a[i]);
        let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
        let aa = dot(direction, direction);
        let bb = dot(v, v).max(1e-20);
        let c = dot(direction, v);
        let d = dot(direction, w);
        let e = dot(v, w);
        let den = aa * bb - c * c;
        let mut s = if den.abs() > 1e-12 {
            ((aa * e - c * d) / den).clamp(0., 1.)
        } else {
            (e / bb).clamp(0., 1.)
        };
        let mut t = ((c * s - d) / aa).max(0.);
        s = ((c * t + e) / bb).clamp(0., 1.);
        t = ((c * s - d) / aa).max(0.);
        let distance = (0..3)
            .map(|i| (origin[i] + t * direction[i] - a[i] - s * v[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        (distance, t)
    };
    let limit = tolerance.unwrap_or(1.);
    let &(a, b, _) = edges
        .iter()
        .filter(|e| {
            let (_, t) = metric(e.0, e.1);
            face_hit.is_none_or(|hit| t <= hit + limit)
        })
        .min_by(|a, b| {
            let x = metric(a.0, a.1);
            let y = metric(b.0, b.1);
            if (x.0 - y.0).abs() < 1e-7 {
                x.1.total_cmp(&y.1)
            } else {
                x.0.total_cmp(&y.0)
            }
        })
        .ok_or("No visible horizontal machining edge here")?;
    if metric(a, b).0 > limit {
        return Err("Point at a horizontal edge to select it".into());
    }
    let mut adjacent = BTreeMap::<Key, Vec<[f64; 3]>>::new();
    for (p, q, _) in &edges {
        if (p[2] - a[2]).abs() < 1e-6 {
            adjacent.entry(key(*p)).or_default().push(*q);
            adjacent.entry(key(*q)).or_default().push(*p);
        }
    }
    let mut chain = vec![a, b];
    let mut closed = false;
    for end in 0..2 {
        if end == 1 {
            chain.reverse();
        }
        loop {
            let current = *chain.last().unwrap();
            let previous = chain[chain.len() - 2];
            let nexts = &adjacent[&key(current)];
            if nexts.len() != 2 {
                break;
            }
            let next = *nexts
                .iter()
                .find(|p| key(**p) != key(previous))
                .ok_or("Ambiguous edge junction")?;
            if single {
                let u: [f64; 3] = std::array::from_fn(|i| current[i] - previous[i]);
                let v: [f64; 3] = std::array::from_fn(|i| next[i] - current[i]);
                let dot = (0..3).map(|i| u[i] * v[i]).sum::<f64>();
                let len = u.iter().map(|v| v * v).sum::<f64>().sqrt()
                    * v.iter().map(|v| v * v).sum::<f64>().sqrt();
                if dot / len < 1. - 1e-8 {
                    break;
                }
            }
            if key(next) == key(chain[0]) {
                closed = true;
                break;
            }
            if chain.iter().any(|p| key(*p) == key(next)) {
                return Err("Edge chain branches or crosses itself".into());
            }
            chain.push(next);
            if chain.len() > 10000 {
                return Err("Edge chain exceeds 10,000 points".into());
            }
        }
        if closed {
            break;
        }
    }
    Ok((chain, closed))
}
pub fn join_chain(
    mut path: Vec<[f64; 3]>,
    mut edge: Vec<[f64; 3]>,
) -> Result<(Vec<[f64; 3]>, bool), String> {
    if path.is_empty() {
        return Ok((edge, false));
    }
    if path.len() + edge.len() > 10000 {
        return Err("Edge chain exceeds 10,000 points".into());
    }
    let same = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
    if same(*path.last().unwrap(), edge[0]) {
        // Preserve the chosen traversal, including an edge that closes the loop.
    } else if same(*path.last().unwrap(), *edge.last().unwrap()) {
        edge.reverse();
    } else if same(path[0], edge[0]) {
        path.reverse();
    } else if same(path[0], *edge.last().unwrap()) {
        path.reverse();
        edge.reverse();
    }
    if !same(*path.last().unwrap(), edge[0]) {
        return Err("Add an edge connected to either end of the selected chain".into());
    }
    path.extend(edge.into_iter().skip(1));
    let closed = same(path[0], *path.last().unwrap());
    if closed {
        path.pop();
    }
    Ok((path, closed))
}
