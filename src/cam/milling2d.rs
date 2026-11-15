//! Bounded 2D tool-center cycles. All paths use the ordinary move, travel and fixture checks.
use super::*;
use crate::{model::Operation, operation::Direction};
use std::sync::atomic::{AtomicBool, Ordering};
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Machining cancelled".into())
    } else {
        Ok(())
    }
}
fn start(op: &Operation) -> f64 {
    op.settings.start_depth.unwrap_or(0.)
}
fn curve(op: &Operation) -> Vec<[f64; 3]> {
    if op.settings.path.is_empty() {
        vec![[op.x, op.y, 0.], [op.x + op.width, op.y, 0.]]
    } else {
        op.settings.path.clone()
    }
}
type FaceRegion = (Vec<Vec<[f64; 3]>>, f64);
fn face_region(op: &Operation) -> Result<FaceRegion, String> {
    let numeric_z = if op.strategy == Strategy::AreaRough {
        -op.depth
    } else {
        0.
    };
    let outer = if op.settings.path.is_empty() {
        vec![
            [op.x, op.y, numeric_z],
            [op.x + op.width, op.y, numeric_z],
            [op.x + op.width, op.y + op.height, numeric_z],
            [op.x, op.y + op.height, numeric_z],
        ]
    } else {
        op.settings.path.clone()
    };
    let mut rings = vec![outer];
    rings.extend(op.settings.islands.clone());
    let level = rings[0][0][2];
    if rings
        .iter()
        .flatten()
        .any(|p| !p.iter().all(|v| v.is_finite()) || (p[2] - level).abs() > 1e-6)
    {
        return Err("Select a horizontal planar target face".into());
    }
    Ok((
        rings,
        level + op.settings.height_offset.unwrap_or(0.) + op.settings.axial_allowance,
    ))
}
// Project all geometry above the target plane, including tapered/overhanging walls.
// Keeping this envelope at every layer is conservative and protects raised features.
fn area_paths(p: &Project, op: &Operation) -> Result<Vec<[[f64; 2]; 2]>, String> {
    let (rings, z) = face_region(op)?;
    let tool = p.operation_tool(op)?;
    let mut obstacles = Vec::new();
    if let Some(part) = &p.part {
        for triangle in &part.geometry.mesh.triangles {
            let points: Vec<_> = triangle
                .iter()
                .map(|i| part.point(part.geometry.mesh.vertices[*i as usize]))
                .collect();
            let floor = rings[0][0][2];
            if points.iter().all(|q| q[2] <= floor + 1e-6) {
                continue;
            }
            let mut clipped = Vec::new();
            for i in 0..3 {
                let a = points[i];
                let b = points[(i + 1) % 3];
                if a[2] > floor {
                    clipped.push(a);
                }
                if (a[2] > floor) != (b[2] > floor) {
                    let t = (floor - a[2]) / (b[2] - a[2]);
                    clipped.push(std::array::from_fn(|k| a[k] + t * (b[k] - a[k])));
                }
            }
            if !clipped.is_empty() {
                obstacles.push(clipped);
            }
        }
    }
    if z > 1e-7 {
        return Err("Area roughing bottom must be at or below stock top".into());
    }
    crate::planar::raster_obstacles(
        &rings,
        tool.diameter / 2. + op.settings.radial_allowance,
        tool.diameter * tool.step_over,
        &obstacles,
    )
}
fn compensated(p: &Project, op: &Operation) -> Result<Vec<[f64; 3]>, String> {
    let points = curve(op);
    let tool = p.operation_tool(op)?;
    let radius = if op.strategy == Strategy::Chamfer {
        let record = p
            .library_tool(Some(op))?
            .ok_or("Select a measured chamfer mill")?;
        let contact = record.dimension("tip_diameter") / 2.
            + (op.depth - op.settings.axial_allowance)
                * (record.dimension("included_angle").to_radians() / 2.).tan();
        let width = op.settings.chamfer_width.unwrap_or(0.3);
        if contact < width {
            return Err(
                "Increase tip depth so the cutter reaches the requested chamfer width".into(),
            );
        }
        contact - width
    } else if op.strategy == Strategy::Trace && op.settings.trace_offset {
        tool.diameter / 2.
    } else {
        return Ok(points);
    };
    if radius < 1e-7 {
        return Ok(points);
    }
    crate::contour::offset(&points, op.settings.closed, radius, !op.settings.outside)
}
pub(super) fn validate(p: &Project, op: &Operation, index: usize) -> Result<(), String> {
    let tool = p.operation_tool(op)?;
    validate_cutting_tool(&tool)?;
    let depth = if op.has_chain()
        && matches!(
            op.strategy,
            Strategy::InsideProfile | Strategy::OutsideProfile
        ) {
        op.settings.height_offset.map_or(op.depth, |h| -h) - op.settings.axial_allowance
    } else {
        op.depth - op.settings.axial_allowance
    };
    if matches!(op.strategy, Strategy::SurfaceSmooth | Strategy::AreaRough) {
        if op.strategy == Strategy::SurfaceSmooth && !op.settings.precleared {
            return Err(
                "Confirm the selected face is already rough-machined before surface smoothing"
                    .into(),
            );
        }
        let (rings, z) = face_region(op)?;
        if rings.iter().flatten().any(|q| !p.stock.contains(*q, 0.002)) {
            return Err(
                "The selected surface extends outside physical stock; fit the blank in Setup"
                    .into(),
            );
        }
        if let Some(record) = p.library_tool(Some(op))? {
            if -z
                > record
                    .dimension("stickout")
                    .min(record.dimension("flute_length"))
                    + EPS
            {
                return Err("Planar machining exceeds measured cutter reach".into());
            }
        }
        if z > op.settings.clearance.unwrap_or(p.clearance) || z < -p.stock.thickness - EPS {
            return Err("Planar machining height is outside stock/clearance limits".into());
        }
        if op.strategy == Strategy::AreaRough {
            if op.settings.height_offset.unwrap_or(0.) < 0. {
                return Err("Area roughing cannot cut below the selected face".into());
            }
            if (-z / tool.step_down).ceil() > MAX_LAYERS as f64 {
                return Err("Too many area roughing depth passes".into());
            }
            area_paths(p, op)?;
        } else {
            crate::planar::raster(&rings, tool.diameter / 2., tool.diameter * tool.step_over)?;
        }
        return Ok(());
    }
    if !(op.has_chain()
        && op.settings.height_offset.is_some()
        && matches!(
            op.strategy,
            Strategy::InsideProfile | Strategy::OutsideProfile
        ))
    {
        positive(
            depth - start(op),
            "Depth below operation top",
            p.stock.thickness,
        )?;
    }
    if start(op) < 0. || depth > p.stock.thickness + EPS || !op.x.is_finite() || !op.y.is_finite() {
        return Err("Invalid operation heights or center".into());
    }
    let clearance = op.settings.clearance.unwrap_or(p.clearance);
    if op.settings.retract.unwrap_or(clearance) > clearance {
        return Err("Retract height cannot exceed clearance".into());
    }
    let effective_top = if matches!(op.strategy, Strategy::Slot | Strategy::SmoothRough) {
        material_top(p, index)
    } else {
        start(op)
    };
    if ((depth - effective_top) / tool.step_down).ceil() > MAX_LAYERS as f64 {
        return Err("Too many depth passes (maximum 2,000)".into());
    }
    let reach = if op.has_chain() {
        depth - curve(op).iter().map(|p| p[2]).fold(0., f64::min)
    } else {
        depth
    };
    if let Some(record) = p.library_tool(Some(op))? {
        if reach > record.dimension("stickout") + EPS
            || (op.strategy != Strategy::Thread && reach > record.dimension("flute_length") + EPS)
        {
            return Err("Cut depth exceeds tool stickout or flute length".into());
        }
    }
    if matches!(
        op.strategy,
        Strategy::Thread | Strategy::Engrave | Strategy::Chamfer
    ) && p.library_tool(Some(op))?.is_none()
    {
        return Err(
            "Select a measured library cutter for thread milling, engraving or chamfering".into(),
        );
    }
    if let Some(id) = op.settings.machining_face {
        crate::faces::face_level(
            p.part
                .as_ref()
                .ok_or("Selected face requires a workpiece")?,
            id,
        )?;
    }
    if matches!(
        op.strategy,
        Strategy::Trace | Strategy::Engrave | Strategy::Chamfer
    ) && start(op) != 0.
    {
        return Err(
            "Chain heights already define the operation top; leave Top depth at zero".into(),
        );
    }
    if op.has_chain() {
        if op.strategy.is_curve() && op.settings.radial_allowance != 0. {
            return Err("Tool-center chains do not use radial stock allowance".into());
        }
        let mut points = compensated(p, op)?;
        if points.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Chain coordinates must be finite".into());
        }
        if matches!(
            op.strategy,
            Strategy::InsideProfile | Strategy::OutsideProfile
        ) {
            points = crate::contour::offset(
                &points,
                op.settings.closed,
                tool.diameter / 2. + op.settings.radial_allowance,
                if !op.settings.closed && op.model_link.is_some() {
                    !op.settings.outside
                } else {
                    op.strategy == Strategy::InsideProfile
                },
            )?;
        }
        if points.len() < 2 {
            return Err("Select a chain with at least two points".into());
        }
        if points.windows(2).any(|s| s[0] == s[1]) {
            return Err("Contour has consecutive duplicate points".into());
        }
        if op.strategy == Strategy::Slot && (op.height - tool.diameter).abs() > 0.002 {
            return Err("Centerline slotting needs a cutter matching the slot width; use Pocket to clear a wider slot".into());
        }
        if (op.strategy == Strategy::Slot && points.iter().any(|v| v[2].abs() > EPS))
            || (op.strategy == Strategy::Chamfer
                && points.iter().any(|v| (v[2] - points[0][2]).abs() > EPS))
        {
            return Err(
                "Slot and chamfer chains must be flat; use Trace for varying-Z chains".into(),
            );
        }
        let radius = tool.diameter / 2.;
        if op.strategy == Strategy::OutsideProfile
            && curve(op)
                .iter()
                .any(|q| q[0] < 0. || q[1] < 0. || q[0] > p.stock.width || q[1] > p.stock.height)
        {
            return Err("The retained contour extends outside stock".into());
        }
        for q in points {
            if -depth + q[2] < -p.stock.thickness - EPS || -depth + q[2] > clearance {
                return Err("Chain depth extends outside supported stock heights".into());
            }
            if !matches!(op.strategy, Strategy::OutsideProfile | Strategy::Chamfer)
                && !(op.strategy == Strategy::Trace && op.settings.trace_offset)
                && !p.stock.contains_disc([q[0], q[1], -depth + q[2]], radius)
            {
                return Err("Contour cutter footprint extends outside the physical blank".into());
            }
        }
        if matches!(op.strategy, Strategy::Engrave | Strategy::Chamfer) {
            let record = p.library_tool(Some(op))?.unwrap();
            if op.strategy == Strategy::Chamfer
                && op.settings.chamfer_angle.is_some_and(|a| {
                    (record.dimension("included_angle") - (180. - 2. * a)).abs() > 0.5
                })
            {
                return Err("Choose a chamfer mill matching the measured chamfer angle".into());
            }
            let radius = record.dimension("tip_diameter") / 2.
                + (depth - start(op))
                    * (record.dimension("included_angle").to_radians() / 2.).tan();
            if radius > tool.diameter / 2. + EPS {
                return Err("V-cutter penetration exceeds its cutting cone".into());
            }
        }
    } else if op.strategy.is_round() {
        if !op.settings.precleared {
            return Err("Confirm the bore is already cleared before using a wall-finishing or thread cycle; use Pocket to remove solid stock".into());
        }
        positive(op.width, "Bore diameter", p.stock.width.max(p.stock.height))?;
        if !op.settings.outside && op.width <= tool.diameter + 2. * op.settings.radial_allowance {
            return Err("Bore must be wider than the cutter and allowance".into());
        }
        let taper = op.settings.taper_angle.unwrap_or(0.);
        if taper < 0. || ((op.strategy == Strategy::Thread || op.settings.outside) && taper != 0.) {
            return Err("Only inward cylindrical/conical bore walls are supported; thread cycles require a cylindrical bore".into());
        }
        let low = op.width / 2. - (depth - start(op)) * taper.to_radians().tan();
        if !op.settings.outside && low <= tool.diameter / 2. + op.settings.radial_allowance {
            return Err("Taper leaves insufficient room for the cutter at the bottom".into());
        }
        let extra = if op.strategy == Strategy::Thread {
            op.settings.thread_depth.unwrap_or(0.3)
        } else {
            0.
        };
        let r = op.width / 2. + if op.settings.outside { 0. } else { extra };
        if op.x - r < 0.
            || op.y - r < 0.
            || op.x + r > p.stock.width
            || op.y + r > p.stock.height
            || !p.stock.contains_disc([op.x, op.y, -depth], r)
        {
            return Err("Circular cut extends outside the physical blank".into());
        }
        if op.strategy == Strategy::Thread {
            if op.settings.spring_passes != 0 {
                return Err("Use a separate thread operation for a repeat pass".into());
            }
            let record = p.library_tool(Some(op))?.unwrap();
            let thread_depth = op.settings.thread_depth.unwrap_or(0.3);
            if thread_depth > record.dimension("tooth_height")
                || thread_depth <= op.settings.radial_allowance
            {
                return Err("Radial thread depth must fit the measured tooth height and exceed stock allowance".into());
            }
            let pitch = op
                .settings
                .pitch
                .unwrap_or(record.dimension("thread_pitch"));
            if (record.dimension("thread_pitch") - pitch).abs() > 0.0001 {
                return Err(
                    "Operation pitch must match the thread mill's measured thread pitch".into(),
                );
            }
            if extra > (tool.diameter - record.dimension("neck_diameter")) / 2. {
                return Err(
                    "Thread depth would drive the tool neck into the existing bore wall".into(),
                );
            }
        }
    } else {
        positive(op.width, "Roughing width", p.stock.width)?;
        positive(op.height, "Roughing height", p.stock.height)?;
        if op.x < 0.
            || op.y < 0.
            || op.x + op.width > p.stock.width
            || op.y + op.height > p.stock.height
        {
            return Err("Roughing region extends outside stock".into());
        }
        if op.width < tool.diameter * 2. + 2. * op.settings.radial_allowance
            || op.height < tool.diameter * 2. + 2. * op.settings.radial_allowance
        {
            return Err("Smooth roughing needs a region at least two cutter diameters wide; use Offset pocket for smaller regions".into());
        }
        if tool.step_over > 0.4 {
            return Err(
                "Smooth roughing forward advance must be at most 40% of cutter diameter".into(),
            );
        }
        for xy in [
            [op.x, op.y],
            [op.x + op.width, op.y],
            [op.x, op.y + op.height],
            [op.x + op.width, op.y + op.height],
        ] {
            if !p.stock.contains_disc([xy[0], xy[1], -depth], 0.) {
                return Err("Roughing region extends outside the physical blank".into());
            }
        }
    }
    Ok(())
}
fn segments(radius: f64, tolerance: f64) -> Result<usize, String> {
    let n = (std::f64::consts::TAU
        / (2. * (1. - (tolerance / radius).min(1.)).acos()).min(std::f64::consts::TAU / 24.))
    .ceil() as usize;
    if n > 4096 {
        Err("Circular cut exceeds 4,096 segments per revolution".into())
    } else {
        Ok(n.max(24))
    }
}
fn finish_circle(
    b: &mut Builder,
    center: [f64; 2],
    radius: f64,
    z: f64,
    phase: f64,
    tolerance: f64,
    sign: f64,
) -> Result<(), String> {
    let count = segments(radius, tolerance)?;
    for i in 1..=count {
        let a = phase + sign * std::f64::consts::TAU * i as f64 / count as f64;
        b.feed_cut(
            center[0] + radius * a.cos(),
            center[1] + radius * a.sin(),
            z,
        )?;
    }
    Ok(())
}
pub(super) fn generate(
    p: &Project,
    op: &Operation,
    index: usize,
    b: &mut Builder,
    cancel: &AtomicBool,
) -> Result<(), String> {
    cancelled(cancel)?;
    let tool = p.operation_tool(op)?;
    let depth = if op.has_chain()
        && matches!(
            op.strategy,
            Strategy::InsideProfile | Strategy::OutsideProfile
        ) {
        op.settings.height_offset.map_or(op.depth, |h| -h) - op.settings.axial_allowance
    } else {
        op.depth - op.settings.axial_allowance
    };
    let top = if matches!(op.strategy, Strategy::SmoothRough | Strategy::Slot) {
        material_top(p, index)
    } else {
        start(op)
    };
    if matches!(op.strategy, Strategy::SurfaceSmooth | Strategy::AreaRough) {
        let (rings, z) = face_region(op)?;
        let paths = if op.strategy == Strategy::AreaRough {
            area_paths(p, op)?
        } else {
            crate::planar::raster(&rings, tool.diameter / 2., tool.diameter * tool.step_over)?
        };
        let layers = if op.strategy == Strategy::AreaRough {
            ((-z / tool.step_down).ceil() as usize).max(1)
        } else {
            1
        };
        for layer in 1..=layers {
            let level = if op.strategy == Strategy::AreaRough {
                -(layer as f64 * tool.step_down).min(-z)
            } else {
                z
            };
            for &[mut a, mut c] in &paths {
                cancelled(cancel)?;
                if (op.settings.raster == crate::operation::Raster::PositiveX && a[0] > c[0])
                    || (op.settings.raster == crate::operation::Raster::NegativeX && a[0] < c[0])
                {
                    std::mem::swap(&mut a, &mut c);
                }
                b.enter(a[0], a[1], level)?;
                b.feed_cut(c[0], c[1], level)?;
                b.retract()?;
            }
        }
        return Ok(());
    }
    let layers = (((depth - top) / tool.step_down).ceil() as usize).max(1);
    if op.has_chain() {
        let mut points = compensated(p, op)?;
        if matches!(
            op.strategy,
            Strategy::InsideProfile | Strategy::OutsideProfile
        ) {
            points = crate::contour::offset(
                &points,
                op.settings.closed,
                tool.diameter / 2. + op.settings.radial_allowance,
                if !op.settings.closed && op.model_link.is_some() {
                    !op.settings.outside
                } else {
                    op.strategy == Strategy::InsideProfile
                },
            )?;
        }
        let area = (0..points.len())
            .map(|i| {
                let a = points[i];
                let c = points[(i + 1) % points.len()];
                a[0] * c[1] - c[0] * a[1]
            })
            .sum::<f64>();
        let reverse = if op.settings.closed {
            (area < 0.) != (op.settings.direction == Direction::Clockwise)
        } else {
            op.settings.direction == Direction::Clockwise
        };
        if reverse {
            if op.settings.closed {
                points[1..].reverse();
            } else {
                points.reverse();
            }
        }
        for layer in 1..=layers {
            cancelled(cancel)?;
            let z = if depth <= 0. {
                -depth
            } else {
                -(top + layer as f64 * tool.step_down).min(depth)
            };
            let first = points[0];
            b.enter(first[0], first[1], z + first[2])?;
            let passes = if layer == layers {
                1 + op.settings.spring_passes
            } else {
                1
            };
            for pass in 0..passes {
                if pass > 0 {
                    b.feed = op.settings.finish_feed.unwrap_or(tool.feed)
                }
                for q in points.iter().skip(1) {
                    b.feed_cut(q[0], q[1], z + q[2])?
                }
                if op.settings.closed {
                    b.feed_cut(first[0], first[1], z + first[2])?
                } else if pass + 1 < passes {
                    b.enter(first[0], first[1], z + first[2])?
                }
            }
            b.feed = tool.feed;
            b.retract()?;
        }
        return Ok(());
    }
    if op.strategy.is_round() {
        let r = if op.settings.outside {
            op.width / 2. + tool.diameter / 2. + op.settings.radial_allowance
        } else {
            op.width / 2. - tool.diameter / 2. - op.settings.radial_allowance
        };
        let tolerance = op.settings.path_tolerance.unwrap_or(0.01);
        let clockwise = op.settings.direction == Direction::Clockwise;
        let sign = if clockwise { -1. } else { 1. };
        let mut phase = 0.;
        if op.settings.outside {
            b.enter(op.x + r + tool.diameter / 2., op.y, -top)?;
        } else {
            b.enter(op.x, op.y, -top)?;
        }
        b.feed_cut(op.x + r, op.y, -top)?;
        if op.strategy == Strategy::Circular {
            for layer in 1..=layers {
                cancelled(cancel)?;
                let d = (top + layer as f64 * tool.step_down).min(depth);
                let radius =
                    r - (d - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
                b.go(
                    Point3 {
                        x: op.x + radius,
                        y: op.y,
                        z: -d,
                    },
                    Motion::Plunge,
                )?;
                b.circle(op.x, op.y, radius, -d, clockwise, tolerance)?;
            }
        } else {
            let pitch = if op.strategy == Strategy::Thread {
                op.settings.pitch.unwrap_or(
                    p.library_tool(Some(op))?
                        .map_or(1., |t| t.dimension("thread_pitch")),
                )
            } else {
                tool.step_down
            };
            let turns = (depth - top) / pitch;
            phase = sign * std::f64::consts::TAU * turns;
            let extra = if op.strategy == Strategy::Thread {
                op.settings.thread_depth.unwrap_or(0.3)
            } else {
                0.
            };
            let radius = r + if op.settings.outside { -extra } else { extra };
            b.feed_cut(op.x + radius, op.y, -top)?;
            let steps = (turns * segments(radius, tolerance)? as f64).ceil() as usize;
            if steps > MAX_MOVES {
                return Err(
                    "Helical cut exceeds the move budget; increase pitch or tolerance".into(),
                );
            }
            for i in 1..=steps {
                if i % 256 == 0 {
                    cancelled(cancel)?
                }
                let t = i as f64 / steps as f64;
                let d = top + (depth - top) * t;
                let a = sign * std::f64::consts::TAU * turns * t;
                let radius =
                    radius - (d - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
                b.feed_cut(op.x + radius * a.cos(), op.y + radius * a.sin(), -d)?;
            }
            if op.strategy != Strategy::Thread {
                let radius =
                    r - (depth - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
                finish_circle(b, [op.x, op.y], radius, -depth, phase, tolerance, sign)?;
            }
        }
        if op.strategy != Strategy::Thread {
            let radius =
                r - (depth - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
            for _ in 0..op.settings.spring_passes {
                b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
                finish_circle(b, [op.x, op.y], radius, -depth, phase, tolerance, sign)?
            }
        }
        b.feed = tool.feed;
        let exit_radius = r + tool.diameter / 2.;
        b.feed_cut(
            if op.settings.outside {
                op.x + exit_radius * phase.cos()
            } else {
                op.x
            },
            if op.settings.outside {
                op.y + exit_radius * phase.sin()
            } else {
                op.y
            },
            -depth,
        )?;
        b.retract()?;
        return Ok(());
    }
    // A translating circular motion gives smooth trochoidal rows. This is not a
    // constant-engagement solver; finish the boundary separately at each layer.
    let radius = tool.diameter / 2.;
    let offset = radius + op.settings.radial_allowance;
    let x0 = op.x + offset;
    let y0 = op.y + offset;
    let x1 = op.x + op.width - offset;
    let y1 = op.y + op.height - offset;
    let rows = ((y1 - y0 - 2. * radius) / (tool.diameter * tool.step_over))
        .ceil()
        .max(1.) as usize;
    let turns = ((x1 - x0 - 2. * radius) / (tool.diameter * tool.step_over))
        .ceil()
        .max(1.) as usize;
    if rows > MAX_ROWS || turns > MAX_ROWS {
        return Err("Smooth roughing exceeds its row/loop budget".into());
    }
    let n = segments(radius, op.settings.path_tolerance.unwrap_or(0.01))?;
    for layer in 1..=layers {
        cancelled(cancel)?;
        let z = -(top + layer as f64 * tool.step_down).min(depth);
        for row in 0..=rows {
            let cy = y0 + radius + (y1 - y0 - 2. * radius) * row as f64 / rows as f64;
            let (a, d) = if row % 2 == 0 {
                (x0 + radius, x1 - radius)
            } else {
                (x1 - radius, x0 + radius)
            };
            b.enter(a + radius, cy, z)?;
            for i in 1..=turns * n {
                if i % 256 == 0 {
                    cancelled(cancel)?
                }
                let t = i as f64 / (turns * n) as f64;
                let angle = std::f64::consts::TAU
                    * turns as f64
                    * t
                    * if op.settings.direction == Direction::Clockwise {
                        -1.
                    } else {
                        1.
                    };
                let cx = a + (d - a) * t;
                b.feed_cut(cx + radius * angle.cos(), cy + radius * angle.sin(), z)?;
            }
            b.retract()?;
        }
        b.enter(x0, y0, z)?;
        let corners = if op.settings.direction == Direction::Clockwise {
            [(x0, y1), (x1, y1), (x1, y0), (x0, y0)]
        } else {
            [(x1, y0), (x1, y1), (x0, y1), (x0, y0)]
        };
        for pass in 0..(1 + if layer == layers {
            op.settings.spring_passes
        } else {
            0
        }) {
            if pass > 0 {
                b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
            }
            for (x, y) in corners {
                b.feed_cut(x, y, z)?
            }
        }
        b.feed = tool.feed;
        b.retract()?;
    }
    Ok(())
}
