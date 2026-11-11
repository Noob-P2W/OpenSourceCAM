//! Bounded, cell-centre 2.5D material removal driven by the actual posted moves.
use crate::{
    cam::{Motion, Point3, Toolpath},
    features::{self, FeatureKind, Report},
    import::{Geometry, Mesh, SourceFormat},
    model::Project,
    tool_library::ToolKind,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub resolution: [usize; 2],
    pub cell_size: [f64; 2],
    pub progress: f64,
    pub removed_volume_mm3: f64,
    pub remaining_volume_mm3: f64,
    pub excess_material_mm3: Option<f64>,
    pub sampled_gouge_mm3: Option<f64>,
    pub backing_mm: Option<f64>,
    pub fixture_models: usize,
    pub fixture_contact: Option<String>,
    pub notes: Vec<String>,
}
#[derive(Clone)]
pub struct Simulation {
    pub summary: Summary,
    pub geometry: Arc<Geometry>,
    pub heights: Vec<f64>,
}

struct TargetColumn {
    desired_height: f64,
    solid: Option<[f64; 2]>,
}
fn expected(report: &Report, x: f64, y: f64, stock_bottom: f64) -> Option<TargetColumn> {
    let e = report.exterior.as_ref()?;
    if report.machining_error.is_some() {
        return None;
    }
    let base = e.bottom_z.max(stock_bottom);
    if x < e.min[0] || x > e.max[0] || y < e.min[1] || y > e.max[1] {
        return Some(TargetColumn {
            desired_height: base,
            solid: None,
        });
    }
    let mut height = e.top_z;
    let mut through = false;
    for f in &report.features {
        let inside = match f.kind {
            FeatureKind::RectangularPocket => {
                x >= f.min[0] && x <= f.max[0] && y >= f.min[1] && y <= f.max[1]
            }
            FeatureKind::CircularHole => {
                (x - f.center[0]).hypot(y - f.center[1]) <= f.diameter.unwrap_or(0.0) / 2.0 + 1e-9
            }
        };
        if inside {
            height = height.min(f.bottom_z);
            through |= f.through;
        }
    }
    let bottom = e.bottom_z.max(stock_bottom);
    let top = height.min(0.0);
    Some(TargetColumn {
        desired_height: height.max(stock_bottom),
        solid: (!through && top > bottom).then_some([bottom, top]),
    })
}

pub fn simulate(
    project: &Project,
    path: &Toolpath,
    progress: f64,
    resolution: usize,
    cancel: &AtomicBool,
) -> Result<Simulation, String> {
    calculate(
        project, path, progress, resolution, cancel, true, 50_000_000,
    )
}
/// Display snapshots use an already preflighted path and omit repeated target
/// recognition/collision reports. Material removal and resource limits are shared.
pub fn preview(
    project: &Project,
    path: &Toolpath,
    progress: f64,
    resolution: usize,
    cancel: &AtomicBool,
) -> Result<Simulation, String> {
    calculate(
        project, path, progress, resolution, cancel, false, 50_000_000,
    )
}
pub fn simulate_budget(
    project: &Project,
    path: &Toolpath,
    progress: f64,
    resolution: usize,
    cancel: &AtomicBool,
    limit: u64,
) -> Result<Simulation, String> {
    if ![50_000_000, 100_000_000, 200_000_000].contains(&limit) {
        return Err("Choose a supported simulation calculation budget".into());
    }
    calculate(project, path, progress, resolution, cancel, true, limit)
}
fn calculate(
    project: &Project,
    path: &Toolpath,
    progress: f64,
    resolution: usize,
    cancel: &AtomicBool,
    verify: bool,
    limit: u64,
) -> Result<Simulation, String> {
    if !(16..=512).contains(&resolution)
        || !progress.is_finite()
        || !(0.0..=1.0).contains(&progress)
    {
        return Err("Stock preview needs 16–512 cells on its longest side and a progress fraction from 0 to 1".into());
    }
    crate::cam::validate_setup(project)?;
    let mut grid = Grid::new(project, resolution)?;
    grid.advance(path, progress, limit, cancel)?;
    let s = &project.stock;
    let nx = grid.nx;
    let ny = grid.ny;
    let dx = grid.dx;
    let dy = grid.dy;
    let bottom = grid.bottom;
    let heights = &grid.heights;
    let floors = &grid.floors;
    let initial = &grid.initial;
    let target = if verify {
        project
            .part
            .as_ref()
            .and_then(|part| features::recognize_with_cancel(part, cancel).ok())
    } else {
        None
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("Stock preview cancelled".into());
    }
    let mut excess = 0.0;
    let mut gouge = 0.0;
    let comparable = target.as_ref().is_some_and(|r| {
        r.machining_error.is_none()
            && r.exterior.as_ref().is_some_and(|e| {
                let tolerance = features::TOLERANCE;
                e.min[0] >= -tolerance
                    && e.min[1] >= -tolerance
                    && e.max[0] <= s.width + tolerance
                    && e.max[1] <= s.height + tolerance
                    && e.top_z <= tolerance
                    && e.bottom_z >= bottom - tolerance
            })
    });
    for y in 0..ny {
        if cancel.load(Ordering::Relaxed) {
            return Err("Stock preview cancelled".into());
        }
        for x in 0..nx {
            if initial[y * nx + x] <= floors[y * nx + x] + 1e-10 {
                continue;
            }
            if let Some(column) = target
                .as_ref()
                .filter(|_| comparable)
                .and_then(|r| expected(r, (x as f64 + 0.5) * dx, (y as f64 + 0.5) * dy, bottom))
            {
                let actual = heights[y * nx + x];
                let delta = actual - column.desired_height;
                if delta > 0.01 {
                    excess += delta * dx * dy;
                }
                if let Some([bottom, top]) = column.solid {
                    if actual < top - 0.01 {
                        gouge += (top - actual.max(bottom)).max(0.0) * dx * dy;
                    }
                }
            }
        }
    }
    let area = dx * dy;
    let mesh = grid.mesh_rows(
        0..ny,
        if resolution <= 192 {
            100_000
        } else {
            2_000_000
        },
        cancel,
    )?;
    let mut notes=vec!["Cell-centre heightfield approximation; small features and exact boundary shapes depend on preview resolution. This is not a full solid-removal certification.".into(),"Machine bodies and spindle kinematics are not collision checked. Fixture checks use conservative cylindrical cutting-tool, shaft and defined-holder envelopes, not exact tool solids.".into()];
    if !comparable {
        notes.push("Target comparison requires supported 2.5D geometry fully inside the blank; only stock removal is shown for this target.".into());
    }
    let fixture_contact = if verify {
        crate::collision::check_with_cancel(project, path, cancel).err()
    } else {
        None
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("Stock preview cancelled".into());
    }
    notes.push("Backing is the nominal blank allowance below the part. Local drilling clearance can remove backing without gouging the target part. Separate enclosed fixture shells are checked as a conservative union.".into());
    let remaining = heights
        .iter()
        .zip(floors)
        .map(|(h, f)| (h - f).max(0.) * area)
        .sum();
    let removed = initial
        .iter()
        .zip(heights)
        .map(|(a, h)| (a - h).max(0.) * area)
        .sum();
    let summary = Summary {
        resolution: [nx, ny],
        cell_size: [dx, dy],
        progress,
        removed_volume_mm3: removed,
        remaining_volume_mm3: remaining,
        excess_material_mm3: comparable.then_some(excess),
        sampled_gouge_mm3: comparable.then_some(gouge),
        backing_mm: target
            .as_ref()
            .and_then(|r| r.exterior.as_ref())
            .map(|e| (e.bottom_z - bottom).max(0.0)),
        fixture_models: project
            .workspace
            .iter()
            .filter(|m| m.kind != crate::workspace::ModelKind::Machine)
            .count(),
        fixture_contact,
        notes,
    };
    Ok(Simulation {
        summary,
        geometry: Arc::new(Geometry {
            removed_degenerate_triangles: 0,
            source_name: "Simulated remaining stock".into(),
            format: SourceFormat::Stl,
            mesh,
            cad_source: None,
        }),
        heights: grid.heights,
    })
}

/// Minimum cutter surface height over the part of a linear XYZ move covering P.
/// In particular, a sloping move must not stamp its lowest endpoint across its entire length.
fn swept_height(
    p: [f64; 2],
    a: Point3,
    b: Point3,
    r: f64,
    ball: bool,
    cone: Option<f64>,
) -> Option<f64> {
    let vx = b.x - a.x;
    let vy = b.y - a.y;
    let l2 = vx * vx + vy * vy;
    let profile = |d: f64| {
        if ball {
            r - (r * r - d * d).max(0.0).sqrt()
        } else {
            cone.map_or(0.0, |k| d / k)
        }
    };
    if l2 < 1e-20 {
        let d = (p[0] - a.x).hypot(p[1] - a.y);
        return (d <= r).then(|| a.z.min(b.z) + profile(d));
    }
    let t0 = ((p[0] - a.x) * vx + (p[1] - a.y) * vy) / l2;
    let perpendicular = (p[0] - a.x - vx * t0).powi(2) + (p[1] - a.y - vy * t0).powi(2);
    if perpendicular > r * r + 1e-10 {
        return None;
    }
    let perpendicular = perpendicular.min(r * r);
    let half = ((r * r - perpendicular) / l2).sqrt();
    let lo = (t0 - half).max(0.0);
    let hi = (t0 + half).min(1.0);
    if lo > hi + 1e-10 {
        return None;
    }
    let lo = lo.min(hi).clamp(0.0, 1.0);
    let hi = hi.max(lo).clamp(0.0, 1.0);
    let length = l2.sqrt();
    let dz = b.z - a.z;
    let slope = dz / length;
    let t = if ball {
        (t0 - slope * ((r * r - perpendicular) / (1.0 + slope * slope)).sqrt() / length)
            .clamp(lo, hi)
    } else if let Some(k) = cone {
        let s = slope * k;
        if s >= 1.0 {
            lo
        } else if s <= -1.0 {
            hi
        } else {
            (t0 - s * (perpendicular / (1.0 - s * s)).sqrt() / length).clamp(lo, hi)
        }
    } else if dz >= 0.0 {
        lo
    } else {
        hi
    };
    let d = (perpendicular + l2 * (t - t0).powi(2)).sqrt();
    Some(a.z + dz * t + profile(d))
}

fn write_obj(mesh: &Mesh, out: &mut impl std::io::Write) -> std::io::Result<()> {
    writeln!(out, "# OpenSourceCAM sampled remaining stock, millimetres")?;
    for v in &mesh.vertices {
        writeln!(out, "v {} {} {}", v[0], v[1], v[2])?;
    }
    for t in &mesh.triangles {
        writeln!(out, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
    }
    out.flush()
}
pub fn obj_data(simulation: &Simulation) -> Result<String, String> {
    let mut data = Vec::new();
    write_obj(&simulation.geometry.mesh, &mut data).map_err(|e| e.to_string())?;
    String::from_utf8(data).map_err(|e| e.to_string())
}
pub fn export_obj(simulation: &Simulation, file: &Path) -> Result<(), String> {
    crate::files::write_atomic_with(file, |file| {
        write_obj(
            &simulation.geometry.mesh,
            &mut std::io::BufWriter::new(file),
        )
    })
    .map_err(|e| e.to_string())
}

#[derive(Clone, Copy)]
struct Footprint {
    radius: f64,
    ball: bool,
    cone: Option<f64>,
}
pub(crate) struct Grid {
    pub nx: usize,
    pub ny: usize,
    pub dx: f64,
    pub dy: f64,
    pub bottom: f64,
    pub heights: Vec<f64>,
    floors: Vec<f64>,
    initial: Vec<f64>,
    tools: Vec<Footprint>,
    pub progress: f64,
    time: f64,
    index: usize,
    elapsed: f64,
    pub evaluations: u64,
    pub visited: u64,
    pub dirty: Vec<bool>,
}
impl Grid {
    pub fn new(project: &Project, resolution: usize) -> Result<Self, String> {
        if !(16..=512).contains(&resolution) {
            return Err("Stock preview needs 16–512 cells on its longest side".into());
        }
        let s = &project.stock;
        let longest = s.width.max(s.height);
        let nx = (s.width / longest * resolution as f64).ceil().max(1.) as usize;
        let ny = (s.height / longest * resolution as f64).ceil().max(1.) as usize;
        let dx = s.width / nx as f64;
        let dy = s.height / ny as f64;
        let bottom = -s.thickness;
        let mut floors = vec![bottom; nx * ny];
        let mut heights = floors.clone();
        for y in 0..ny {
            for x in 0..nx {
                if let Some([lo, hi]) = s.column((x as f64 + 0.5) * dx, (y as f64 + 0.5) * dy) {
                    floors[y * nx + x] = lo;
                    heights[y * nx + x] = hi;
                }
            }
        }
        let mut tools = Vec::new();
        for op in &project.operations {
            if !op.enabled {
                tools.push(Footprint {
                    radius: 0.,
                    ball: false,
                    cone: None,
                });
                continue;
            }
            let tool = project.operation_tool(op)?;
            let record = project.library_tool(Some(op))?;
            if record.is_none()
                && matches!(
                    project.machine.as_ref().map(|m| &m.tooling),
                    Some(crate::machine::Tooling::Milling {
                        kind: crate::machine::CutterKind::Drill,
                        ..
                    })
                )
            {
                return Err("Stock verification of a drill needs a tool-library definition with its point angle".into());
            }
            tools.push(Footprint {
                radius: tool.diameter / 2.,
                ball: record.is_some_and(|r| r.kind == ToolKind::BallEndMill),
                cone: record
                    .filter(|r| r.kind == ToolKind::Drill)
                    .map(|r| (r.dimension("point_angle").to_radians() / 2.).tan()),
            });
        }
        Ok(Self {
            nx,
            ny,
            dx,
            dy,
            bottom,
            initial: heights.clone(),
            heights,
            floors,
            tools,
            progress: 0.,
            time: 0.,
            index: 0,
            elapsed: 0.,
            evaluations: 0,
            visited: 0,
            dirty: vec![true; ny],
        })
    }
    pub fn advance(
        &mut self,
        path: &Toolpath,
        progress: f64,
        limit: u64,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        if !progress.is_finite() || !(0. ..=1.).contains(&progress) || progress < self.progress {
            return Err("Invalid forward stock progress".into());
        }
        self.evaluations = 0;
        self.visited = 0;
        let until = path.estimated_seconds * progress;
        while self.index < path.moves.len() && self.elapsed < until {
            if cancel.load(Ordering::Relaxed) {
                return Err("Stock preview cancelled".into());
            }
            let m = &path.moves[self.index];
            let duration = m.from.distance(m.to) / m.feed * 60.;
            let end = self.elapsed + duration;
            if m.motion != Motion::Rapid && duration > 0. {
                let a = ((self.time - self.elapsed) / duration).clamp(0., 1.);
                let b = ((until - self.elapsed) / duration).clamp(0., 1.);
                if b > a {
                    let interpolate = |t: f64| Point3 {
                        x: m.from.x + (m.to.x - m.from.x) * t,
                        y: m.from.y + (m.to.y - m.from.y) * t,
                        z: m.from.z + (m.to.z - m.from.z) * t,
                    };
                    let tool = *self
                        .tools
                        .get(m.operation)
                        .ok_or("Invalid toolpath operation index")?;
                    self.cut(interpolate(a), interpolate(b), tool, limit, cancel)?;
                }
            }
            if end > until {
                break;
            }
            self.index += 1;
            self.elapsed = end;
        }
        self.time = until;
        self.progress = progress;
        Ok(())
    }
    fn cut(
        &mut self,
        a: Point3,
        b: Point3,
        tool: Footprint,
        limit: u64,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let r = tool.radius;
        let y0 = (((a.y.min(b.y) - r) / self.dy - 0.5).ceil().max(0.) as usize).min(self.ny);
        let y1 = (((a.y.max(b.y) + r) / self.dy - 0.5).floor()).min((self.ny - 1) as f64);
        if y1 < y0 as f64 {
            return Ok(());
        }
        for y in y0..=y1 as usize {
            if cancel.load(Ordering::Relaxed) {
                return Err("Stock preview cancelled".into());
            }
            let cy = (y as f64 + 0.5) * self.dy;
            let Some([lo, hi]) = capsule_row(a, b, r, cy) else {
                continue;
            };
            let x0 = (((lo / self.dx - 0.5).ceil().max(0.)) as usize).min(self.nx);
            let x1 = (hi / self.dx - 0.5).floor().min((self.nx - 1) as f64);
            if x1 < x0 as f64 {
                continue;
            }
            for x in x0..=x1 as usize {
                self.visited += 1;
                if self.visited > limit * 4 {
                    return Err("Stock preview exhausted its traversal budget; use a higher calculation budget or smaller update".into());
                }
                let id = y * self.nx + x;
                if self.heights[id] <= a.z.min(b.z).max(self.floors[id]) {
                    continue;
                }
                self.evaluations += 1;
                if self.evaluations > limit {
                    return Err("Stock preview exhausted the selected calculation budget; increase Simulation calculation budget in Settings or reduce detail".into());
                }
                if let Some(h) = swept_height(
                    [(x as f64 + 0.5) * self.dx, cy],
                    a,
                    b,
                    r,
                    tool.ball,
                    tool.cone,
                ) {
                    let height = self.heights[id].min(h.max(self.floors[id]));
                    if height < self.heights[id] {
                        self.heights[id] = height;
                        for row in y.saturating_sub(1)..=(y + 1).min(self.ny - 1) {
                            self.dirty[row] = true;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn mesh_rows(
        &self,
        rows: std::ops::Range<usize>,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Mesh, String> {
        let nx = self.nx;
        let ny = self.ny;
        let dx = self.dx;
        let dy = self.dy;
        let bottom = self.bottom;
        let heights = &self.heights;
        let floors = &self.floors;
        let mut mesh = Mesh {
            vertices: Vec::new(),
            triangles: Vec::new(),
        };
        let mut quad = |points: [[f64; 3]; 4]| -> Result<(), String> {
            if mesh.triangles.len() + 2 > limit {
                return Err(
                    "Stock preview surface exceeds its triangle budget; reduce preview resolution"
                        .into(),
                );
            }
            let id = mesh.vertices.len() as u32;
            mesh.vertices.extend(points);
            mesh.triangles
                .extend([[id, id + 1, id + 2], [id, id + 2, id + 3]]);
            Ok(())
        };
        // Merge coplanar runs in X so real 2.5D results stay compact.
        for y in rows {
            if cancel.load(Ordering::Relaxed) {
                return Err("Stock preview cancelled".into());
            }
            let mut x = 0;
            while x < nx {
                let start = x;
                let h = heights[y * nx + x];
                let floor = floors[y * nx + x];
                while x < nx && heights[y * nx + x] == h && floors[y * nx + x] == floor {
                    x += 1;
                }
                if h > floor + 1e-9 {
                    let a = start as f64 * dx;
                    let b = x as f64 * dx;
                    let c = y as f64 * dy;
                    quad([[a, c, h], [b, c, h], [b, c + dy, h], [a, c + dy, h]])?;
                }
            }
            let mut start = 0;
            while start < nx {
                if heights[y * nx + start] <= floors[y * nx + start] + 1e-9 {
                    start += 1;
                    continue;
                }
                let floor = floors[y * nx + start];
                let mut end = start + 1;
                while end < nx
                    && floors[y * nx + end] == floor
                    && heights[y * nx + end] > floor + 1e-9
                {
                    end += 1;
                }
                let a = start as f64 * dx;
                let b = end as f64 * dx;
                let c = y as f64 * dy;
                quad([
                    [a, c + dy, floor],
                    [b, c + dy, floor],
                    [b, c, floor],
                    [a, c, floor],
                ])?;
                start = end;
            }
            for x in 0..nx {
                let h = heights[y * nx + x];
                let floor = floors[y * nx + x];
                if h <= floor + 1e-9 {
                    continue;
                }
                let a = x as f64 * dx;
                let b = y as f64 * dy;
                let neighbors = [
                    (
                        x.checked_sub(1).map(|x| y * nx + x),
                        [[a, b, h], [a, b + dy, h], [a, b + dy, 0.], [a, b, 0.]],
                    ),
                    (
                        (x + 1 < nx).then_some(y * nx + x + 1),
                        [
                            [a + dx, b + dy, h],
                            [a + dx, b, h],
                            [a + dx, b, 0.],
                            [a + dx, b + dy, 0.],
                        ],
                    ),
                    (
                        y.checked_sub(1).map(|y| y * nx + x),
                        [[a + dx, b, h], [a, b, h], [a, b, 0.], [a + dx, b, 0.]],
                    ),
                    (
                        (y + 1 < ny).then_some((y + 1) * nx + x),
                        [
                            [a, b + dy, h],
                            [a + dx, b + dy, h],
                            [a + dx, b + dy, 0.],
                            [a, b + dy, 0.],
                        ],
                    ),
                ];
                for (neighbor, points) in neighbors {
                    let lo = neighbor.map_or(bottom, |i| floors[i]);
                    let hi = neighbor.map_or(bottom, |i| heights[i]);
                    for (z0, z1) in [(floor, h.min(lo)), (floor.max(hi), h)] {
                        if z1 > z0 + 1e-9 {
                            let mut p = points;
                            p[0][2] = z1;
                            p[1][2] = z1;
                            p[2][2] = z0;
                            p[3][2] = z0;
                            quad(p)?
                        }
                    }
                }
            }
        }
        Ok(mesh)
    }
    pub fn volumes(&self) -> (f64, f64) {
        let area = self.dx * self.dy;
        (
            self.initial
                .iter()
                .zip(&self.heights)
                .map(|(a, h)| (a - h).max(0.) * area)
                .sum(),
            self.heights
                .iter()
                .zip(&self.floors)
                .map(|(h, f)| (h - f).max(0.) * area)
                .sum(),
        )
    }
}
/// Horizontal section of the swept XY capsule. Exact sweep checks still decide removal.
fn capsule_row(a: Point3, b: Point3, r: f64, y: f64) -> Option<[f64; 2]> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in [a, b] {
        let dy = y - p.y;
        if dy.abs() <= r + 1e-10 {
            let x = (r * r - dy * dy).max(0.).sqrt();
            lo = lo.min(p.x - x);
            hi = hi.max(p.x + x);
        }
    }
    let length = (b.x - a.x).hypot(b.y - a.y);
    if length > 1e-12 {
        let ox = -(b.y - a.y) / length * r;
        let oy = (b.x - a.x) / length * r;
        let p = [
            [a.x + ox, a.y + oy],
            [b.x + ox, b.y + oy],
            [b.x - ox, b.y - oy],
            [a.x - ox, a.y - oy],
        ];
        for i in 0..4 {
            let a = p[i];
            let b = p[(i + 1) % 4];
            if (a[1] - y).abs() < 1e-10 {
                lo = lo.min(a[0]);
                hi = hi.max(a[0]);
            }
            if (a[1] > y) != (b[1] > y) {
                let x = a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    (lo <= hi).then_some([lo - 1e-9, hi + 1e-9])
}

#[cfg(test)]
mod sweep_tests {
    use super::*;
    #[test]
    fn capsule_rows_contain_the_exact_cutter_footprint() {
        for angle in 0..36 {
            let angle = angle as f64 * std::f64::consts::TAU / 36.;
            let a = Point3 {
                x: -2.1,
                y: 0.3,
                z: -5.,
            };
            let b = Point3 {
                x: a.x + angle.cos() * 6.,
                y: a.y + angle.sin() * 6.,
                z: -2.,
            };
            for r in [0.1, 1., 3.8] {
                for y in -24..=28 {
                    let y = y as f64 * 0.4;
                    for x in -32..=24 {
                        let x = x as f64 * 0.4;
                        if swept_height([x, y], a, b, r, false, None).is_some() {
                            let row = capsule_row(a, b, r, y)
                                .expect("Exact cutter contact needs a row bound");
                            assert!(x >= row[0] - 1e-8 && x <= row[1] + 1e-8);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn sloping_cutter_sweeps_match_dense_reference() {
        for (ball, cone) in [(false, None), (true, None), (false, Some(1.2))] {
            for dz in [-6.0, 0.0, 6.0] {
                let a = Point3 {
                    x: 0.,
                    y: 0.,
                    z: -8.,
                };
                let b = Point3 {
                    x: 10.,
                    y: 2.,
                    z: -8. + dz,
                };
                for ix in -2..=12 {
                    for iy in -3..=5 {
                        let p = [ix as f64, iy as f64];
                        let r = 2.;
                        let exact = swept_height(p, a, b, r, ball, cone);
                        let mut sampled = f64::INFINITY;
                        for i in 0..=20000 {
                            let t = i as f64 / 20000.;
                            let d =
                                (p[0] - a.x - t * (b.x - a.x)).hypot(p[1] - a.y - t * (b.y - a.y));
                            if d <= r {
                                let profile = if ball {
                                    r - (r * r - d * d).max(0.).sqrt()
                                } else {
                                    cone.map_or(0., |k| d / k)
                                };
                                sampled = sampled.min(a.z + t * dz + profile);
                            }
                        }
                        if sampled.is_finite() {
                            let value = exact.unwrap();
                            assert!(value <= sampled + 1e-8);
                            assert!(sampled - value < 0.002, "{sampled} vs {value}");
                        } else if let Some(value) = exact {
                            assert!(value.is_finite());
                        }
                    }
                }
            }
        }
    }
}
