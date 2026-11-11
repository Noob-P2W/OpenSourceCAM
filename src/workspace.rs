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
        for axis in 0..3 {
            let (sin, cos) = self.rotation[axis].to_radians().sin_cos();
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
            profile.validate(&self.part.geometry.mesh)?;
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
