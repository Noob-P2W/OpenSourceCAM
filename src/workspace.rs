//! Positioned machine and workholding reference models, independent of the part.
use crate::{
    import::{Bounds, Part},
    machine::MachineSetup,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Machine,
    Vise,
    Fixture,
}
impl ModelKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Machine => "Machine",
            Self::Vise => "Vise",
            Self::Fixture => "Fixture",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    #[default]
    Work,
    Machine,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceModel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clamp: Option<crate::vise::Profile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vise: Option<crate::workholding::ViseSpec>,
    pub id: u64,
    pub name: String,
    pub kind: ModelKind,
    pub frame: Frame,
    pub part: Part,
    /// Extrinsic X, then Y, then Z rotation about the source origin, degrees.
    pub rotation: [f64; 3],
}
impl WorkspaceModel {
    pub fn new(id: u64, kind: ModelKind, part: Part) -> Self {
        Self {
            vise: None,
            clamp: None,
            id,
            name: part.geometry.source_name.clone(),
            kind,
            frame: if kind == ModelKind::Machine {
                Frame::Machine
            } else {
                Frame::Work
            },
            part,
            rotation: [0.0; 3],
        }
    }
    pub fn rotation_matrix(&self) -> [f32; 9] {
        let columns = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|p| {
            self.rotate(crate::blank::apply(
                crate::blank::rotation(self.part.rotation),
                p,
            ))
        });
        std::array::from_fn(|i| columns[i / 3][i % 3] as f32)
    }
    fn rotate(&self, mut p: [f64; 3]) -> [f64; 3] {
        for (axis, angle) in self.rotation.iter().enumerate() {
            let (sin, cos) = angle.to_radians().sin_cos();
            let a = (axis + 1) % 3;
            let b = (axis + 2) % 3;
            let x = p[a];
            let y = p[b];
            p[a] = cos * x - sin * y;
            p[b] = sin * x + cos * y;
        }
        p
    }
    pub fn offset(&self, machine: Option<&MachineSetup>) -> [f64; 3] {
        std::array::from_fn(|i| {
            self.part.offset[i]
                - if self.frame == Frame::Machine {
                    machine.map_or(0.0, |m| m.work_origin[i])
                } else {
                    0.0
                }
        })
    }
    pub fn point(&self, p: [f64; 3], machine: Option<&MachineSetup>) -> [f64; 3] {
        let p = self.rotate(crate::blank::apply(
            crate::blank::rotation(self.part.rotation),
            p.map(|v| v * self.part.units.factor()),
        ));
        let o = self.offset(machine);
        std::array::from_fn(|i| p[i] + o[i])
    }
    pub fn bounds(&self, machine: Option<&MachineSetup>) -> Bounds {
        let mut b = Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        for corner in self.part.geometry.mesh.bounds().corners() {
            let p = self.point(corner, machine);
            for (axis, value) in p.into_iter().enumerate() {
                b.min[axis] = b.min[axis].min(value);
                b.max[axis] = b.max[axis].max(value);
            }
        }
        b
    }
    pub fn validate(&self, machine: Option<&MachineSetup>) -> Result<(), String> {
        self.part.validate()?;
        if let Some(profile) = &self.clamp {
            profile.validate_geometry(&self.part.geometry)?;
        }
        self.validate_placement(machine)
    }

    pub fn validate_placement(&self, machine: Option<&MachineSetup>) -> Result<(), String> {
        if self.id == 0
            || self.id > 1_000_000_000
            || self.name.trim().is_empty()
            || self.name.len() > 1024
        {
            return Err("Workspace models require a valid ID and name".into());
        }
        if self
            .part
            .offset
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1e6)
        {
            return Err("Invalid workspace translation".into());
        }
        if self
            .rotation
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 360.0)
        {
            return Err("Model rotations must be finite and between -360 and 360 degrees".into());
        }
        if self.frame == Frame::Machine && machine.is_none() {
            return Err("A model in machine coordinates requires a machine definition".into());
        }
        if self
            .bounds(machine)
            .corners()
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
        {
            return Err("Positioned workspace model exceeds coordinate limits".into());
        }
        Ok(())
    }
}

pub fn validate(models: &[WorkspaceModel], machine: Option<&MachineSetup>) -> Result<(), String> {
    if models.len() > 16
        || models
            .iter()
            .map(|m| m.part.geometry.mesh.triangles.len())
            .sum::<usize>()
            > 300_000
    {
        return Err("Workspace supports at most 16 models and 300,000 reference triangles".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for m in models {
        if !ids.insert(m.id) {
            return Err("Duplicate workspace model ID".into());
        }
        m.validate(machine)?;
    }
    Ok(())
}

fn editable(project: &mut crate::model::Project, id: u64) -> Result<&mut WorkspaceModel, String> {
    let m = project
        .workspace
        .iter_mut()
        .find(|m| m.id == id)
        .ok_or("Reference model not found")?;
    if m.clamp.as_ref().is_some_and(|c| c.closed_by > 1e-7) {
        return Err("Open the jaw before changing its placement".into());
    }
    Ok(m)
}

pub fn position(
    project: &mut crate::model::Project,
    id: u64,
    offset: [f64; 3],
    rotation: [f64; 3],
    frame: Frame,
) -> Result<(), String> {
    let machine = project.machine.clone();
    let m = editable(project, id)?;
    if rotation != m.rotation || frame != m.frame {
        m.clamp = None;
    }
    m.part.offset = offset;
    m.rotation = rotation;
    m.frame = frame;
    m.validate(machine.as_ref())
}

fn component_vertices(m: &WorkspaceModel, body: usize) -> Result<Vec<usize>, String> {
    let g = &m.part.geometry;
    let b = g.bodies.get(body).ok_or("CAD component not found")?;
    let mut used = vec![false; g.mesh.vertices.len()];
    for t in &g.mesh.triangles[b.first_triangle..b.first_triangle + b.triangle_count] {
        for i in t {
            used[*i as usize] = true;
        }
    }
    Ok(used
        .iter()
        .enumerate()
        .filter_map(|(i, used)| used.then_some(i))
        .collect())
}

pub fn placement_info(
    project: &crate::model::Project,
    id: u64,
    body: Option<usize>,
) -> Result<serde_json::Value, String> {
    let m = project
        .workspace
        .iter()
        .find(|m| m.id == id)
        .ok_or("Reference model not found")?;
    m.validate(project.machine.as_ref())?;
    let indices = if let Some(body) = body {
        component_vertices(m, body)?
    } else {
        let mesh = &m.part.geometry.mesh;
        let mut used = vec![false; mesh.vertices.len()];
        for triangle in &mesh.triangles {
            for i in triangle {
                used[*i as usize] = true;
            }
        }
        used.iter()
            .enumerate()
            .filter_map(|(i, used)| used.then_some(i))
            .collect()
    };
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for i in indices {
        let p = m.point(m.part.geometry.mesh.vertices[i], project.machine.as_ref());
        for a in 0..3 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    let mut center: [f64; 3] = std::array::from_fn(|i| (min[i] + max[i]) / 2.);
    // A surface centroid transforms rigidly even for asymmetric components;
    // a freshly computed bounding-box center would shift after rotation.
    if let Some(body) = body {
        let g = &m.part.geometry;
        let b = &g.bodies[body];
        let mut weighted = [0.; 3];
        let mut area = 0.;
        for triangle in &g.mesh.triangles[b.first_triangle..b.first_triangle + b.triangle_count] {
            let points = triangle.map(|i| g.mesh.vertices[i as usize]);
            let n = crate::import::normal(points);
            let weight = n.iter().map(|v| v * v).sum::<f64>().sqrt();
            area += weight;
            for i in 0..3 {
                weighted[i] += points.iter().map(|p| p[i]).sum::<f64>() / 3. * weight;
            }
        }
        center = m.point(weighted.map(|v| v / area), project.machine.as_ref());
    }
    Ok(serde_json::json!({"center":center,"min":min,"max":max}))
}

pub fn center(project: &mut crate::model::Project, id: u64) -> Result<(), String> {
    let info = placement_info(project, id, None)?;
    let delta = [
        project.stock.width / 2. - info["center"][0].as_f64().unwrap(),
        project.stock.height / 2. - info["center"][1].as_f64().unwrap(),
    ];
    let m = editable(project, id)?;
    m.part.offset[0] += delta[0];
    m.part.offset[1] += delta[1];
    Ok(())
}

/// Position a complete CAD component in work coordinates. Rotations are explicit
/// increments about its current center; no imported mates or other bodies move.
pub fn position_component(
    project: &mut crate::model::Project,
    id: u64,
    body: usize,
    center: [f64; 3],
    rotate: [f64; 3],
) -> Result<(), String> {
    if center
        .iter()
        .any(|v| !v.is_finite() || v.abs() > crate::import::MAX_COORDINATE)
        || rotate.iter().any(|v| !v.is_finite() || v.abs() > 36000.)
    {
        return Err(
            "Component position and rotation must be finite and within supported limits".into(),
        );
    }
    let info = placement_info(project, id, Some(body))?;
    let original: [f64; 3] = std::array::from_fn(|i| info["center"][i].as_f64().unwrap());
    let machine = project.machine.clone();
    let m = editable(project, id)?;
    let indices = component_vertices(m, body)?;
    let zero = m.point([0.; 3], machine.as_ref());
    let basis: [[f64; 3]; 3] = std::array::from_fn(|i| {
        let mut p = [0.; 3];
        p[i] = 1.;
        let q = m.point(p, machine.as_ref());
        std::array::from_fn(|j| q[j] - zero[j])
    });
    let factor = m.part.units.factor();
    let rotation = crate::blank::rotation(rotate);
    let updated: Vec<_> = indices
        .iter()
        .map(|i| {
            let world = m.point(m.part.geometry.mesh.vertices[*i], machine.as_ref());
            let relative =
                crate::blank::apply(rotation, std::array::from_fn(|a| world[a] - original[a]));
            let target: [f64; 3] = std::array::from_fn(|a| relative[a] + center[a] - zero[a]);
            std::array::from_fn(|i| {
                (0..3).map(|j| basis[i][j] * target[j]).sum::<f64>() / (factor * factor)
            })
        })
        .collect();
    let geometry = std::sync::Arc::make_mut(&mut m.part.geometry);
    for (i, p) in indices.into_iter().zip(updated) {
        geometry.mesh.vertices[i] = p;
    }
    m.clamp = None;
    m.vise = None;
    m.validate(machine.as_ref())
}
