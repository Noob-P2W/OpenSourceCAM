//! Bounded mesh import and the bundled, shell-free OpenCascade CAD-kernel adapter.
mod cad;
mod mesh;

use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub const MAX_IMPORT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TRIANGLES: usize = 100_000;
pub const MAX_VERTICES: usize = MAX_TRIANGLES * 3;
pub const MAX_COORDINATE: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Stl,
    Obj,
    Step,
    Iges,
}

impl SourceFormat {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "stl" => Ok(Self::Stl),
            "obj" => Ok(Self::Obj),
            "step" | "stp" => Ok(Self::Step),
            "iges" | "igs" => Ok(Self::Iges),
            _ => Err("Supported model files: STL, OBJ, STEP/STP and IGES/IGS".into()),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Stl => "STL",
            Self::Obj => "OBJ",
            Self::Step => "STEP",
            Self::Iges => "IGES",
        }
    }
    pub fn is_cad(self) -> bool {
        matches!(self, Self::Step | Self::Iges)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    #[default]
    Millimetres,
    Inches,
}

impl Units {
    pub fn factor(self) -> f64 {
        match self {
            Self::Millimetres => 1.0,
            Self::Inches => 25.4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_import_input()?;
        if self
            .triangles
            .iter()
            .any(|triangle| area_squared(triangle.map(|i| self.vertices[i as usize])) <= 0.0)
        {
            return Err("Mesh contains a degenerate triangle".into());
        }
        Ok(())
    }

    /// Only new model imports may discard zero-area facets. Project validation stays strict.
    pub fn repair_degenerate_triangles(&mut self) -> Result<usize, String> {
        self.validate_import_input()?;
        let before = self.triangles.len();
        self.triangles
            .retain(|triangle| area_squared(triangle.map(|i| self.vertices[i as usize])) > 0.0);
        let removed = before - self.triangles.len();
        if self.triangles.is_empty() {
            return Err(
                "Model contains no usable triangles after removing degenerate faces".into(),
            );
        }
        if removed > 0 {
            let mut used = vec![false; self.vertices.len()];
            for triangle in &self.triangles {
                for index in triangle {
                    used[*index as usize] = true;
                }
            }
            let mut remap = vec![0; self.vertices.len()];
            let mut vertices = Vec::new();
            for (index, point) in self.vertices.iter().enumerate() {
                if used[index] {
                    remap[index] = vertices.len() as u32;
                    vertices.push(*point);
                }
            }
            for triangle in &mut self.triangles {
                for index in triangle {
                    *index = remap[*index as usize];
                }
            }
            self.vertices = vertices;
        }
        self.validate()?;
        Ok(removed)
    }

    pub(crate) fn validate_import_input(&self) -> Result<(), String> {
        if self.triangles.is_empty()
            || self.triangles.len() > MAX_TRIANGLES
            || self.vertices.is_empty()
            || self.vertices.len() > MAX_VERTICES
        {
            return Err(
                "Mesh must contain 1–100,000 triangles and at most 300,000 vertices".into(),
            );
        }
        if self
            .vertices
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
        {
            return Err(
                "Mesh coordinates must be finite and within +/-1,000,000 source units".into(),
            );
        }
        for triangle in &self.triangles {
            if triangle.iter().any(|i| *i as usize >= self.vertices.len()) {
                return Err("Mesh face references an invalid vertex".into());
            }
        }
        Ok(())
    }

    pub fn bounds(&self) -> Bounds {
        let mut bounds = Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        // Ignore unused OBJ vertices when fitting the actual surface.
        for triangle in &self.triangles {
            for index in triangle {
                let v = self.vertices[*index as usize];
                for (axis, value) in v.into_iter().enumerate() {
                    bounds.min[axis] = bounds.min[axis].min(value);
                    bounds.max[axis] = bounds.max[axis].max(value);
                }
            }
        }
        bounds
    }
}

pub fn normal(vertices: [[f64; 3]; 3]) -> [f64; 3] {
    let a: [f64; 3] = std::array::from_fn(|i| vertices[1][i] - vertices[0][i]);
    let b: [f64; 3] = std::array::from_fn(|i| vertices[2][i] - vertices[0][i]);
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn area_squared(vertices: [[f64; 3]; 3]) -> f64 {
    normal(vertices).iter().map(|v| v * v).sum()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    pub fn size(self) -> [f64; 3] {
        std::array::from_fn(|i| self.max[i] - self.min[i])
    }
    pub fn corners(self) -> [[f64; 3]; 8] {
        std::array::from_fn(|i| {
            std::array::from_fn(|axis| {
                if i & (1 << axis) == 0 {
                    self.min[axis]
                } else {
                    self.max[axis]
                }
            })
        })
    }
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    /// Zero-area facets removed during model import, before any geometry links exist.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub removed_degenerate_triangles: usize,
    pub source_name: String,
    pub format: SourceFormat,
    pub mesh: Mesh,
    /// Original CAD exchange text, retained for a future topology/feature workflow.
    /// It is data only and never executed. STEP/IGES mesh coordinates are in mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cad_source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    #[serde(default)]
    pub rotation: [f64; 3],
    pub geometry: Arc<Geometry>,
    pub units: Units,
    pub offset: [f64; 3],
    pub visible: bool,
}

impl PartialEq for Part {
    fn eq(&self, other: &Self) -> bool {
        self.rotation == other.rotation
            && self.units == other.units
            && self.offset == other.offset
            && self.visible == other.visible
            && (Arc::ptr_eq(&self.geometry, &other.geometry) || self.geometry == other.geometry)
    }
}

impl Part {
    pub fn validate(&self) -> Result<(), String> {
        self.geometry.mesh.validate()?;
        if self
            .rotation
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 36000.)
        {
            return Err("Part rotation must be finite and within ±36,000 degrees".into());
        }
        if self.geometry.source_name.len() > 1024 {
            return Err("Source name is too long".into());
        }
        if self
            .geometry
            .cad_source
            .as_ref()
            .is_some_and(|s| s.len() as u64 > MAX_IMPORT_BYTES)
        {
            return Err("Embedded CAD source exceeds 32 MiB".into());
        }
        if self.geometry.format.is_cad() && self.units != Units::Millimetres {
            return Err(
                "CAD geometry is already converted to millimetres by the CAD kernel".into(),
            );
        }
        if self
            .offset
            .iter()
            .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
        {
            return Err("Model offsets must be finite and within +/-1,000,000 mm".into());
        }
        if self
            .bounds()
            .corners()
            .iter()
            .flatten()
            .any(|v| v.abs() > MAX_COORDINATE)
        {
            return Err("Positioned model exceeds the +/-1,000,000 mm coordinate limit".into());
        }
        Ok(())
    }
    pub fn point(&self, point: [f64; 3]) -> [f64; 3] {
        let scaled = point.map(|v| v * self.units.factor());
        let rotated = if self.rotation == [0.; 3] {
            scaled
        } else {
            crate::blank::apply(crate::blank::rotation(self.rotation), scaled)
        };
        std::array::from_fn(|i| rotated[i] + self.offset[i])
    }
    pub fn rotation_matrix(&self) -> [f32; 9] {
        let matrix = crate::blank::rotation(self.rotation);
        std::array::from_fn(|i| matrix[i / 3][i % 3] as f32)
    }
    pub fn matrix(&self) -> [[f64; 4]; 4] {
        let m = crate::blank::rotation(self.rotation);
        std::array::from_fn(|col| {
            if col == 3 {
                [self.offset[0], self.offset[1], self.offset[2], 1.]
            } else {
                [
                    m[col][0] * self.units.factor(),
                    m[col][1] * self.units.factor(),
                    m[col][2] * self.units.factor(),
                    0.,
                ]
            }
        })
    }
    pub fn bounds(&self) -> Bounds {
        let mut bounds = Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        for p in &self.geometry.mesh.vertices {
            let p = self.point(*p);
            for (i, x) in p.iter().enumerate() {
                bounds.min[i] = bounds.min[i].min(*x);
                bounds.max[i] = bounds.max[i].max(*x)
            }
        }
        bounds
    }
    pub fn orient_centered(&mut self, rotation: [f64; 3]) -> Result<(), String> {
        let old = self.bounds();
        let old_rotation = self.rotation;
        self.rotation = rotation;
        if let Err(e) = self.validate() {
            self.rotation = old_rotation;
            return Err(e);
        }
        let new = self.bounds();
        for i in 0..3 {
            self.offset[i] += (old.min[i] + old.max[i] - new.min[i] - new.max[i]) / 2.
        }
        self.validate()
    }
    /// Put the oriented part's lower XY corner at the margin and its top at Z=0.
    pub fn align_to_top(&mut self, margin: f64) {
        let b = self.bounds();
        self.offset[0] += margin - b.min[0];
        self.offset[1] += margin - b.min[1];
        self.offset[2] -= b.max[2];
    }
}

pub fn import_file(path: &Path, units: Units) -> Result<Part, String> {
    import_with_cancel(path, units, &AtomicBool::new(false))
}

pub fn import_with_cancel(path: &Path, units: Units, cancel: &AtomicBool) -> Result<Part, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open model: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_IMPORT_BYTES {
        return Err("Model exceeds the 32 MiB import limit".into());
    }
    let mut data = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Model exceeds the 32 MiB import limit".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Import cancelled".into());
    }
    import_bytes(path, units, &data, cancel)
}

/// Import a bounded in-memory model without filesystem access.
pub fn import_bytes(
    path: &Path,
    units: Units,
    data: &[u8],
    cancel: &AtomicBool,
) -> Result<Part, String> {
    if data.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Model exceeds the 32 MiB import limit".into());
    }
    let format = SourceFormat::from_path(path)?;
    #[cfg(target_arch = "wasm32")]
    if format.is_cad() {
        return Err("STEP/IGES import is not available in the browser yet. Use STL/OBJ or a desktop-saved project.".into());
    }
    let (mut mesh, cad_source) = match format {
        SourceFormat::Stl => (mesh::stl(data)?, None),
        SourceFormat::Obj => (mesh::obj(data)?, None),
        SourceFormat::Step | SourceFormat::Iges => {
            if units != Units::Millimetres {
                return Err(
                    "STEP/IGES units are read from the CAD file; do not apply an inch override"
                        .into(),
                );
            }
            let source = std::str::from_utf8(data)
                .map_err(|_| "CAD exchange file must be UTF-8/ASCII text")?
                .to_owned();
            let mesh = cad::convert(data, format, cancel)?;
            (mesh, Some(source))
        }
    };
    let removed_degenerate_triangles = mesh.repair_degenerate_triangles()?;
    let part = Part {
        rotation: [0.; 3],
        geometry: Arc::new(Geometry {
            removed_degenerate_triangles,
            source_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            format,
            mesh,
            cad_source,
        }),
        units,
        offset: [0.0; 3],
        visible: true,
    };
    part.validate()?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Import cancelled".into());
    }
    Ok(part)
}

pub fn cad_backend() -> Option<std::path::PathBuf> {
    cad::executable()
}
