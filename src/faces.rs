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
pub fn face(part: &Part, triangle: usize) -> Result<Face, String> {
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
    for (id, t) in part.geometry.mesh.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|i| part.point(part.geometry.mesh.vertices[i as usize]));
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
