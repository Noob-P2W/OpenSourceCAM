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
    origin: [f64; 3],
    direction: [f64; 3],
    points: &[[f64; 3]],
    closed: bool,
) -> Result<bool, String> {
    let (id, _) = ray_hit(part, origin, direction)?;
    let face = face(part, id)?;
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
    let q = [
        (a[0] + b[0]) / 2. - sign * d[1] / len * 0.001,
        (a[1] + b[1]) / 2. + sign * d[0] / len * 0.001,
    ];
    let material = face.fill.iter().any(|t| {
        let cross = |a: [f64; 3], b: [f64; 3]| {
            (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])
        };
        let c = [cross(t[0], t[1]), cross(t[1], t[2]), cross(t[2], t[0])];
        c.iter().all(|v| *v >= -1e-10) || c.iter().all(|v| *v <= 1e-10)
    });
    Ok(material)
}
