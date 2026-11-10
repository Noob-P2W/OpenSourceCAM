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
    calculate(project, path, progress, resolution, cancel, true)
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
    calculate(project, path, progress, resolution, cancel, false)
}
fn calculate(
    project: &Project,
    path: &Toolpath,
    progress: f64,
    resolution: usize,
    cancel: &AtomicBool,
    verify: bool,
) -> Result<Simulation, String> {
    if !(16..=192).contains(&resolution)
        || !progress.is_finite()
        || !(0.0..=1.0).contains(&progress)
    {
        return Err("Stock preview needs 16–192 cells on its longest side and a progress fraction from 0 to 1".into());
    }
    crate::cam::validate_setup(project)?;
    let s = &project.stock;
    let longest = s.width.max(s.height);
    let nx = (s.width / longest * resolution as f64).ceil().max(1.0) as usize;
    let ny = (s.height / longest * resolution as f64).ceil().max(1.0) as usize;
    let dx = s.width / nx as f64;
    let dy = s.height / ny as f64;
    let bottom = -s.thickness;
    let mut floors = vec![bottom; nx * ny];
    let mut heights = vec![bottom; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            if let Some([lo, hi]) = s.column((x as f64 + 0.5) * dx, (y as f64 + 0.5) * dy) {
                floors[y * nx + x] = lo;
                heights[y * nx + x] = hi;
            }
        }
    }
    let initial = heights.clone();
    let mut work = 0u64;
    let until = path.estimated_seconds * progress;
    let mut elapsed = 0.0;
    let mut operation = None;
    let mut radius = 0.0;
    let mut point_angle = None;
    let mut ball = false;
    for movement in &path.moves {
        if cancel.load(Ordering::Relaxed) {
            return Err("Stock preview cancelled".into());
        }
        if elapsed >= until {
            break;
        }
        if operation != Some(movement.operation) {
            let op = project
                .operations
                .get(movement.operation)
                .ok_or("Invalid toolpath operation index")?;
            let tool = project.operation_tool(op)?;
            radius = tool.diameter / 2.0;
            point_angle = None;
            ball = false;
            if let Some(record) = project.library_tool(Some(op))? {
                ball = record.kind == ToolKind::BallEndMill;
                if record.kind == ToolKind::Drill {
                    point_angle = Some((record.dimension("point_angle").to_radians() / 2.0).tan());
                }
            } else if matches!(
                project.machine.as_ref().map(|m| &m.tooling),
                Some(crate::machine::Tooling::Milling {
                    kind: crate::machine::CutterKind::Drill,
                    ..
                })
            ) {
                return Err("Stock verification of a drill needs a tool-library definition with its point angle".into());
            }
            operation = Some(movement.operation);
        }
        let duration = movement.from.distance(movement.to) / movement.feed * 60.0;
        let fraction = ((until - elapsed) / duration).clamp(0.0, 1.0);
        elapsed += duration;
        if movement.motion == Motion::Rapid {
            continue;
        }
        let from = movement.from;
        let to = Point3 {
            x: from.x + (movement.to.x - from.x) * fraction,
            y: from.y + (movement.to.y - from.y) * fraction,
            z: from.z + (movement.to.z - from.z) * fraction,
        };
        let minx = ((from.x.min(to.x) - radius) / dx - 0.5).ceil().max(0.0) as usize;
        let maxx = ((from.x.max(to.x) + radius) / dx - 0.5)
            .floor()
            .min((nx - 1) as f64);
        let miny = ((from.y.min(to.y) - radius) / dy - 0.5).ceil().max(0.0) as usize;
        let maxy = ((from.y.max(to.y) + radius) / dy - 0.5)
            .floor()
            .min((ny - 1) as f64);
        if maxx < minx as f64 || maxy < miny as f64 {
            continue;
        }
        work += (maxx as usize + 1 - minx) as u64 * (maxy as usize + 1 - miny) as u64;
        if work > 50_000_000 {
            return Err("Stock preview exceeds 50 million cell evaluations; reduce preview resolution or split the job".into());
        }
        for y in miny..=maxy as usize {
            for x in minx..=maxx as usize {
                let p = [(x as f64 + 0.5) * dx, (y as f64 + 0.5) * dy];
                if let Some(height) = swept_height(p, from, to, radius, ball, point_angle) {
                    heights[y * nx + x] = heights[y * nx + x].min(height.max(floors[y * nx + x]));
                }
            }
        }
    }
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
    let mut mesh = Mesh {
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    let mut quad = |points: [[f64; 3]; 4]| -> Result<(), String> {
        if mesh.triangles.len() + 2 > crate::import::MAX_TRIANGLES {
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
    for y in 0..ny {
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
                quad([
                    [a, c + dy, floor],
                    [b, c + dy, floor],
                    [b, c, floor],
                    [a, c, floor],
                ])?;
            }
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
        .zip(&floors)
        .map(|(h, f)| (h - f).max(0.) * area)
        .sum();
    let removed = initial
        .iter()
        .zip(&heights)
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
            source_name: "Simulated remaining stock".into(),
            format: SourceFormat::Stl,
            mesh,
            cad_source: None,
        }),
        heights,
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

#[cfg(test)]
mod sweep_tests {
    use super::*;
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
