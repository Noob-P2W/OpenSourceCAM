//! Physical stock solids and rigid placement, shared by planning and display.
use crate::{
    import::{Bounds, Mesh, Part},
    model::{Project, Stock},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StockProfile {
    #[default]
    Rectangular,
    OrientedBox {
        size: [f64; 3],
        rotation: [f64; 3],
    },
    Cylinder {
        diameter: f64,
        length: f64,
        rotation: [f64; 3],
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockKind {
    Rectangular,
    Cylinder,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockDefinition {
    pub kind: StockKind,
    pub size: [f64; 3],
    pub rotation: [f64; 3],
}

pub fn rotation(angles: [f64; 3]) -> [[f64; 3]; 3] {
    let mut columns = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    for p in &mut columns {
        for (axis, angle) in angles.iter().enumerate() {
            let (mut s, mut c) = angle.rem_euclid(360.).to_radians().sin_cos();
            if s.abs() < 1e-12 {
                s = 0.
            }
            if c.abs() < 1e-12 {
                c = 0.
            }
            let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
            let (x, y) = (p[a], p[b]);
            p[a] = c * x - s * y;
            p[b] = s * x + c * y;
        }
    }
    columns
}
pub fn apply(matrix: [[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| (0..3).map(|j| matrix[j][i] * p[j]).sum())
}
pub fn inverse(matrix: [[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    matrix.map(|column| (0..3).map(|i| column[i] * p[i]).sum())
}
fn valid_rotation(r: [f64; 3]) -> Result<(), String> {
    if r.iter().any(|v| !v.is_finite() || v.abs() > 36000.) {
        Err("Rotation must be finite and within ±36,000 degrees".into())
    } else {
        Ok(())
    }
}
impl StockDefinition {
    pub fn build(&self) -> Result<Stock, String> {
        valid_rotation(self.rotation)?;
        if self
            .size
            .iter()
            .any(|v| !v.is_finite() || *v < 0.001 || *v > 100_000.)
        {
            return Err("Stock dimensions must be 0.001–100,000 mm".into());
        }
        let m = rotation(self.rotation);
        let extent: [f64; 3] = match self.kind {
            StockKind::Rectangular => {
                std::array::from_fn(|i| (0..3).map(|j| m[j][i].abs() * self.size[j]).sum())
            }
            StockKind::Cylinder => std::array::from_fn(|i| {
                self.size[0] * (m[0][i] * m[0][i] + m[1][i] * m[1][i]).sqrt()
                    + self.size[2] * m[2][i].abs()
            }),
        };
        if extent[2] > 10000. || extent[0] > 100000. || extent[1] > 100000. {
            return Err("Positioned blank exceeds supported stock limits".into());
        }
        let profile = match self.kind {
            StockKind::Rectangular if self.rotation.iter().all(|v| v.rem_euclid(360.) == 0.) => {
                StockProfile::Rectangular
            }
            StockKind::Rectangular => StockProfile::OrientedBox {
                size: self.size,
                rotation: self.rotation,
            },
            StockKind::Cylinder => StockProfile::Cylinder {
                diameter: self.size[0],
                length: self.size[2],
                rotation: self.rotation,
            },
        };
        Ok(Stock {
            width: extent[0],
            height: extent[1],
            thickness: extent[2],
            profile,
        })
    }
}
impl Stock {
    pub fn definition(&self) -> StockDefinition {
        match self.profile {
            StockProfile::Rectangular => StockDefinition {
                kind: StockKind::Rectangular,
                size: [self.width, self.height, self.thickness],
                rotation: [0.; 3],
            },
            StockProfile::OrientedBox { size, rotation } => StockDefinition {
                kind: StockKind::Rectangular,
                size,
                rotation,
            },
            StockProfile::Cylinder {
                diameter,
                length,
                rotation,
            } => StockDefinition {
                kind: StockKind::Cylinder,
                size: [diameter, diameter, length],
                rotation,
            },
        }
    }
    pub fn validate_profile(&self) -> Result<(), String> {
        if self.profile == StockProfile::Rectangular {
            return Ok(());
        }
        let s = self.definition().build()?;
        if [
            s.width - self.width,
            s.height - self.height,
            s.thickness - self.thickness,
        ]
        .iter()
        .any(|v| v.abs() > 1e-7)
        {
            return Err("Stock envelope does not match its physical shape; edit stock dimensions using the stock definition".into());
        }
        Ok(())
    }
    pub fn contains(&self, p: [f64; 3], tolerance: f64) -> bool {
        let d = self.definition();
        let c = [self.width / 2., self.height / 2., -self.thickness / 2.];
        let q = inverse(rotation(d.rotation), std::array::from_fn(|i| p[i] - c[i]));
        match d.kind {
            StockKind::Rectangular => (0..3).all(|i| q[i].abs() <= d.size[i] / 2. + tolerance),
            StockKind::Cylinder => {
                q[0].hypot(q[1]) <= d.size[0] / 2. + tolerance
                    && q[2].abs() <= d.size[2] / 2. + tolerance
            }
        }
    }
    pub fn contains_disc(&self, center: [f64; 3], radius: f64) -> bool {
        let d = self.definition();
        let m = rotation(d.rotation);
        let q = inverse(
            m,
            [
                center[0] - self.width / 2.,
                center[1] - self.height / 2.,
                center[2] + self.thickness / 2.,
            ],
        );
        match d.kind {
            StockKind::Rectangular => (0..3)
                .all(|i| q[i].abs() + radius * m[i][0].hypot(m[i][1]) <= d.size[i] / 2. + 1e-7),
            StockKind::Cylinder => {
                q[0].hypot(q[1]) + radius <= d.size[0] / 2. + 1e-7
                    && q[2].abs() + radius * m[2][0].hypot(m[2][1]) <= d.size[2] / 2. + 1e-7
            }
        }
    }
    /// Exact intersection of a vertical ray with the convex blank, in work Z.
    pub fn column(&self, x: f64, y: f64) -> Option<[f64; 2]> {
        if self.profile == StockProfile::Rectangular {
            return (x >= 0. && x <= self.width && y >= 0. && y <= self.height)
                .then_some([-self.thickness, 0.]);
        }
        let d = self.definition();
        let matrix = rotation(d.rotation);
        let origin = inverse(
            matrix,
            [
                x - self.width / 2.,
                y - self.height / 2.,
                self.thickness / 2.,
            ],
        );
        let direction = inverse(matrix, [0., 0., 1.]);
        let mut range = [f64::NEG_INFINITY, f64::INFINITY];
        let mut slab = |axis: usize, half: f64| -> bool {
            let (o, v) = (origin[axis], direction[axis]);
            if v.abs() < 1e-12 {
                return o.abs() <= half + 1e-9;
            }
            let (a, b) = ((-half - o) / v, (half - o) / v);
            range[0] = range[0].max(a.min(b));
            range[1] = range[1].min(a.max(b));
            range[0] <= range[1]
        };
        match d.kind {
            StockKind::Rectangular => {
                for (i, size) in d.size.iter().enumerate() {
                    if !slab(i, size / 2.) {
                        return None;
                    }
                }
            }
            StockKind::Cylinder => {
                if !slab(2, d.size[2] / 2.) {
                    return None;
                }
                let a = direction[0].powi(2) + direction[1].powi(2);
                let b = 2. * (origin[0] * direction[0] + origin[1] * direction[1]);
                let c = origin[0].powi(2) + origin[1].powi(2) - (d.size[0] / 2.).powi(2);
                if a < 1e-12 {
                    if c > 1e-9 {
                        return None;
                    }
                } else {
                    let disc = b * b - 4. * a * c;
                    if disc < 0. {
                        return None;
                    }
                    let s = disc.sqrt();
                    range[0] = range[0].max((-b - s) / (2. * a));
                    range[1] = range[1].min((-b + s) / (2. * a));
                }
            }
        }
        (range[0] < range[1] - 1e-10).then_some(range)
    }
    pub fn outline(&self) -> Vec<[f64; 3]> {
        let d = self.definition();
        let m = rotation(d.rotation);
        let c = [self.width / 2., self.height / 2., -self.thickness / 2.];
        let point = |p| {
            let q = apply(m, p);
            std::array::from_fn(|i| q[i] + c[i])
        };
        let mut lines = Vec::new();
        let n = if d.kind == StockKind::Cylinder { 64 } else { 4 };
        let ring = |i: usize, z: f64| -> [f64; 3] {
            if d.kind == StockKind::Cylinder {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                point([d.size[0] / 2. * a.cos(), d.size[0] / 2. * a.sin(), z])
            } else {
                let p = [[-1., -1.], [1., -1.], [1., 1.], [-1., 1.]][i % 4];
                point([p[0] * d.size[0] / 2., p[1] * d.size[1] / 2., z])
            }
        };
        for i in 0..n {
            for z in [-d.size[2] / 2., d.size[2] / 2.] {
                lines.extend([ring(i, z), ring((i + 1) % n, z)])
            }
            if n == 4 || i % 8 == 0 {
                lines.extend([ring(i, -d.size[2] / 2.), ring(i, d.size[2] / 2.)])
            }
        }
        lines
    }
}
pub fn contains_part(stock: &Stock, part: &Part) -> Result<(), String> {
    if part
        .geometry
        .mesh
        .vertices
        .iter()
        .any(|p| !stock.contains(part.point(*p), 0.002))
    {
        Err("The part extends outside the physical stock. Fit stock or adjust its dimensions and the part placement.".into())
    } else {
        Ok(())
    }
}
/// Fit the selected solid to the oriented part. Top/backing are along machining Z.
pub fn fit(project: &mut Project, margin: f64, top: f64, backing: f64) -> Result<(), String> {
    if [margin, top, backing]
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || *v > 10000.)
    {
        return Err("Blank allowances must be 0–10,000 mm".into());
    }
    let mut definition = project.stock.definition();
    let matrix = rotation(definition.rotation);
    let part = project
        .part
        .as_mut()
        .ok_or("Import a part before fitting stock")?;
    part.validate()?;
    let mut bounds = Bounds {
        min: [f64::INFINITY; 3],
        max: [f64::NEG_INFINITY; 3],
    };
    for p in &part.geometry.mesh.vertices {
        let p = part.point(*p);
        for shift in [0., top, -backing] {
            let q = inverse(matrix, [p[0], p[1], p[2] + shift]);
            for (i, x) in q.iter().enumerate() {
                bounds.min[i] = bounds.min[i].min(*x);
                bounds.max[i] = bounds.max[i].max(*x)
            }
        }
    }
    let center = std::array::from_fn(|i| (bounds.min[i] + bounds.max[i]) / 2.);
    definition.size = std::array::from_fn(|i| {
        if i < 2 {
            bounds.max[i] - bounds.min[i] + 2. * margin
        } else {
            (bounds.max[i] - bounds.min[i]).max(0.001)
        }
    });
    if definition.kind == StockKind::Cylinder {
        let mut radius: f64 = 0.;
        for p in &part.geometry.mesh.vertices {
            let p = part.point(*p);
            for shift in [0., top, -backing] {
                let q = inverse(matrix, [p[0], p[1], p[2] + shift]);
                radius = radius.max((q[0] - center[0]).hypot(q[1] - center[1]))
            }
        }
        definition.size[0] = (2. * (radius + margin)).max(0.001);
        definition.size[1] = definition.size[0];
    }
    let stock = definition.build()?;
    let world_center = apply(matrix, center);
    let desired = [stock.width / 2., stock.height / 2., -stock.thickness / 2.];
    for i in 0..3 {
        part.offset[i] += desired[i] - world_center[i]
    }
    project.stock = stock;
    contains_part(&project.stock, part)
}
pub fn mesh_from_boxes(boxes: &[([f64; 3], [f64; 3])]) -> Mesh {
    let mut mesh = Mesh {
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    for (lo, hi) in boxes {
        let k = mesh.vertices.len() as u32;
        mesh.vertices.extend([
            [lo[0], lo[1], lo[2]],
            [hi[0], lo[1], lo[2]],
            [hi[0], hi[1], lo[2]],
            [lo[0], hi[1], lo[2]],
            [lo[0], lo[1], hi[2]],
            [hi[0], lo[1], hi[2]],
            [hi[0], hi[1], hi[2]],
            [lo[0], hi[1], hi[2]],
        ]);
        for t in [
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
        ] {
            mesh.triangles.push(t.map(|i| i + k))
        }
    }
    mesh
}
