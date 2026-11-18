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
    let numeric_z = if matches!(op.strategy, Strategy::AreaRough | Strategy::StockRough) {
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
// Project geometry above the current cut plane. Recompute at each stepdown so
// upper passes clear over lower features while keeping sloped walls protected.
fn area_paths(
    p: &Project,
    op: &Operation,
    level: f64,
    cancel: &AtomicBool,
) -> Result<Vec<Vec<Point3>>, String> {
    let (mut rings, z) = face_region(op)?;
    let contour_clearing = op.strategy == Strategy::StockRough && op.has_chain();
    let retained = contour_clearing.then(|| rings[0].clone());
    if op.model_link.is_some() {
        rings.truncate(1);
    }
    let tool = p.operation_tool(op)?;
    let stock_boundary = op
        .settings
        .clearing_boundary
        .map_or(op.model_link.is_some() || contour_clearing, |b| {
            b == crate::operation::ClearingBoundary::Stock
        });
    if stock_boundary {
        let allowance = tool.diameter / 2. + op.settings.radial_allowance;
        // The outer boundary is open to air: extend it so cutter centres can
        // overrun stock edges rather than leaving a tool-radius rim behind.
        let margin =
            2. * allowance + 2. * 2_f64.sqrt() * op.settings.surface_step.unwrap_or(0.25) + 0.5;
        let floor = rings[0][0][2];
        rings[0] = vec![
            [-margin, -margin, floor],
            [p.stock.width + margin, -margin, floor],
            [p.stock.width + margin, p.stock.height + margin, floor],
            [-margin, p.stock.height + margin, floor],
        ];
    }
    let mut obstacles = retained.into_iter().collect::<Vec<_>>();
    if let Some(part) = &p.part {
        for triangle in &part.geometry.mesh.triangles {
            let points: Vec<_> = triangle
                .iter()
                .map(|i| part.point(part.geometry.mesh.vertices[*i as usize]))
                .collect();
            let floor = if op.strategy == Strategy::StockRough && !contour_clearing {
                f64::NEG_INFINITY
            } else {
                level - op.settings.axial_allowance
            };
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
        return Err("2D offset clearing bottom must be at or below stock top".into());
    }
    let mut clearing = crate::roughing::Clearing::planar(
        &rings,
        &obstacles,
        tool.diameter / 2. + op.settings.radial_allowance,
        tool.diameter * tool.step_over.min(0.5),
        op.settings.surface_step.unwrap_or(0.25),
        cancel,
    )?;
    if op.strategy == Strategy::StockRough {
        clearing.exterior_only(p.stock.width, p.stock.height);
    }
    let mut budget = crate::surface::Budget::new(cancel);
    let reverse = op.settings.direction == Direction::Clockwise;
    clearing.layer(level, reverse, &mut budget)
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
pub(super) fn needs_stock_proof(op: &Operation) -> bool {
    !op.settings.precleared
        && (op.strategy == Strategy::SurfaceSmooth
            || matches!(
                op.strategy,
                Strategy::Bore
                    | Strategy::Circular
                    | Strategy::Thread
                    | Strategy::Engrave
                    | Strategy::Chamfer
                    | Strategy::Trace
            ))
}
pub(super) fn verify_stock(
    p: &Project,
    op: &Operation,
    index: usize,
    state: &mut crate::material::RemainingStock,
) -> Result<(), String> {
    if !needs_stock_proof(op) {
        return Ok(());
    }
    let tool = p.operation_tool(op)?;
    let clear = if op.strategy == Strategy::SurfaceSmooth {
        let (rings, z) = face_region(op)?;
        crate::planar::raster(&rings, tool.diameter / 2., tool.diameter * tool.step_over)?
            .iter()
            .all(|[a, b]| {
                state.is_clear(
                    Point3 {
                        x: a[0],
                        y: a[1],
                        z: z + tool.step_down,
                    },
                    Point3 {
                        x: b[0],
                        y: b[1],
                        z: z + tool.step_down,
                    },
                    tool.diameter / 2.,
                    0.,
                )
            })
    } else if matches!(
        op.strategy,
        Strategy::Bore | Strategy::Circular | Strategy::Thread
    ) {
        let depth = op.depth - op.settings.axial_allowance;
        let low = op.width / 2.
            - (depth - start(op)) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
        let engagement = if op.strategy == Strategy::Thread {
            0.
        } else {
            tool.diameter * tool.step_over.min(0.5)
        };
        if op.settings.outside {
            state.ring_clear(
                [op.x, op.y],
                low + op.settings.radial_allowance + engagement,
                op.width / 2. + 2. * tool.diameter,
                -depth,
            )
        } else {
            let pilot = (low - op.settings.radial_allowance - engagement).max(tool.diameter / 2.);
            state.is_clear(
                Point3 {
                    x: op.x,
                    y: op.y,
                    z: -depth,
                },
                Point3 {
                    x: op.x,
                    y: op.y,
                    z: -depth,
                },
                pilot,
                0.,
            )
        }
    } else {
        // Face clearing can prove exposure even if a conservative cell straddles
        // the selected edge. Otherwise every referenced cell must be exposed.
        let mut points = curve(op);
        if op.settings.closed && !points.is_empty() {
            points.push(points[0]);
        }
        points.windows(2).all(|pair| {
            let [a, b] = [pair[0], pair[1]];
            a[2].min(b[2]) >= -cleared_top(p, index) - EPS
                || state.is_clear(
                    Point3 {
                        x: a[0],
                        y: a[1],
                        z: a[2],
                    },
                    Point3 {
                        x: b[0],
                        y: b[1],
                        z: b[2],
                    },
                    0.,
                    0.,
                )
        })
    };
    if clear {
        Ok(())
    } else {
        Err("Remaining stock is not proven clear/exposed here; generate preceding roughing or confirm externally pre-cleared stock".into())
    }
}
pub(super) fn model_exterior(
    p: &Project,
    op: &Operation,
    cancel: &AtomicBool,
) -> Result<Vec<Vec<Point3>>, String> {
    area_paths(p, op, -op.depth, cancel)
}
pub(super) fn clear_model_exterior(
    chains: &[Vec<Point3>],
    z: f64,
    b: &mut Builder,
    cancel: &AtomicBool,
) -> Result<(), String> {
    for chain in chains {
        let layer = chain.iter().map(|q| Point3 { z, ..*q }).collect::<Vec<_>>();
        cut_chain(&layer, b, cancel)?;
    }
    b.retract()
}
fn cut_chain(chain: &[Point3], b: &mut Builder, cancel: &AtomicBool) -> Result<(), String> {
    // Rotate a closed loop without reversing its milling direction or crossing
    // its retained island. The entry segment uses normal clearance checks.
    let rotated;
    let chain = if chain.len() > 3 && chain[0].distance(*chain.last().unwrap()) < 1e-7 {
        let n = chain.len() - 1;
        let nearest = (0..n)
            .min_by(|a, c| {
                b.position
                    .distance(chain[*a])
                    .total_cmp(&b.position.distance(chain[*c]))
            })
            .unwrap();
        rotated = (0..=n)
            .map(|i| chain[(nearest + i) % n])
            .collect::<Vec<_>>();
        &rotated[..]
    } else {
        chain
    };

    let radius = b.cutter.map_or(0., |t| t.radius) + b.certificate_error;
    if chain
        .windows(2)
        .all(|w| b.remaining.is_clear(w[0], w[1], radius, 0.))
    {
        return Ok(());
    }
    if let Some(first) = chain.first() {
        b.enter(first.x, first.y, first.z)?;
        for q in chain.iter().skip(1) {
            cancelled(cancel)?;
            b.feed_cut(q.x, q.y, q.z)?;
        }
        b.retract()?;
    }
    Ok(())
}
fn contour_target(op: &Operation) -> Option<f64> {
    (op.has_chain()
        && matches!(
            op.strategy,
            Strategy::InsideProfile | Strategy::OutsideProfile
        ))
    .then(|| {
        curve(op)[0][2]
            + op.settings.height_offset.unwrap_or(-op.depth)
            + op.settings.axial_allowance
    })
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
    if matches!(op.strategy, Strategy::SurfaceSmooth | Strategy::AreaRough)
        || (op.strategy == Strategy::StockRough && op.has_chain())
    {
        if (op.strategy == Strategy::AreaRough
            || (op.strategy == Strategy::StockRough && op.has_chain()))
            && (op.settings.spring_passes > 0 || op.settings.finish_feed.is_some())
        {
            return Err("2D offset clearing has no separate finishing/spring pass; add a contour or planar finishing operation".into());
        }
        if op.strategy == Strategy::SurfaceSmooth && op.settings.spring_passes > 0 {
            return Err("Planar finishing does not use contour spring passes; add another finishing operation".into());
        }
        if op.strategy == Strategy::StockRough
            && (!op.settings.closed || op.settings.path.len() < 3)
        {
            return Err("Outside clearing requires a closed planar contour".into());
        }
        if op.strategy == Strategy::StockRough
            && op.settings.clearing_boundary == Some(crate::operation::ClearingBoundary::Selected)
        {
            return Err(
                "Outside clearing uses the stock boundary around the retained island".into(),
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
        if op.strategy == Strategy::AreaRough
            || (op.strategy == Strategy::StockRough && op.has_chain())
        {
            if op.settings.height_offset.unwrap_or(0.) < 0. {
                return Err("2D offset clearing cannot cut below the selected face".into());
            }
            if (-z / tool.step_down).ceil() > MAX_LAYERS as f64 {
                return Err("Too many area roughing depth passes".into());
            }
            area_paths(p, op, z, &AtomicBool::new(false))?;
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
    let axial_travel = contour_target(op).map_or(depth - effective_top, |target| {
        if op.settings.precleared {
            0.
        } else {
            (-material_top(p, index) - target).max(0.)
        }
    });
    if (axial_travel / tool.step_down).ceil() > MAX_LAYERS as f64 {
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
        positive(op.width, "Bore diameter", p.stock.width.max(p.stock.height))?;
        if !op.settings.outside && op.width <= tool.diameter + 2. * op.settings.radial_allowance {
            return Err("Bore must be wider than the cutter and allowance".into());
        }
        let taper = op.settings.taper_angle.unwrap_or(0.);
        if op.strategy == Strategy::Circular && taper != 0. {
            return Err(
                "Use Bore milling for tapered walls; Circular milling requires a cylindrical wall"
                    .into(),
            );
        }
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
            if op.settings.finish_feed.is_some() {
                return Err("Thread milling uses cutting feed, not a separate finish feed".into());
            }
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
    let _ = phase;
    if ((b.position.x - center[0]).hypot(b.position.y - center[1]) - radius).abs() > 1e-5 {
        return Err("Finish circle start differs from its radius".into());
    }
    b.arc_xy(center, sign * std::f64::consts::TAU, z, tolerance)
}
// Quarter-circle entry/exit tangent to the finished wall. All points remain on
// the cleared side: inside a bore, outside a boss. This is computed independently
// from public machining guidance; it does not use controller cutter compensation.
#[derive(Clone, Copy)]
struct WallLead {
    wall: [f64; 2],
    radius: f64,
    phase: f64,
    lead: f64,
    z: f64,
    sign: f64,
    outside: bool,
    tolerance: f64,
}
impl WallLead {
    fn draw(self, b: &mut Builder, enter: bool) -> Result<(), String> {
        let Self {
            wall,
            radius,
            phase,
            lead,
            z,
            sign,
            outside,
            tolerance,
        } = self;
        let radial = [phase.cos(), phase.sin()];
        let tangent = [-phase.sin(), phase.cos()];
        let side = if outside { 1. } else { -1. };
        let center = [
            wall[0] + (radius + side * lead) * radial[0],
            wall[1] + (radius + side * lead) * radial[1],
        ];
        let sweep = if outside { -sign } else { sign } * std::f64::consts::FRAC_PI_2;
        if enter {
            let start = [
                wall[0] + (radius + side * lead) * radial[0] - sign * lead * tangent[0],
                wall[1] + (radius + side * lead) * radial[1] - sign * lead * tangent[1],
            ];
            b.enter(start[0], start[1], z)?;
        }
        b.arc_xy(center, sweep, z, tolerance)
    }
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
    if matches!(op.strategy, Strategy::SurfaceSmooth | Strategy::AreaRough)
        || (op.strategy == Strategy::StockRough && op.has_chain())
    {
        if op.strategy == Strategy::StockRough
            && (!op.settings.closed || op.settings.path.len() < 3)
        {
            return Err("Outside clearing requires a closed planar contour".into());
        }
        let (rings, z) = face_region(op)?;
        let paths = if op.strategy == Strategy::SurfaceSmooth {
            crate::planar::raster(&rings, tool.diameter / 2., tool.diameter * tool.step_over)?
        } else {
            Vec::new()
        };
        if op.strategy == Strategy::SurfaceSmooth {
            b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
        }
        let layers = if op.strategy == Strategy::AreaRough
            || (op.strategy == Strategy::StockRough && op.has_chain())
        {
            ((-z / tool.step_down).ceil() as usize).max(1)
        } else {
            1
        };
        for layer in 1..=layers {
            let level = if op.strategy == Strategy::AreaRough
                || (op.strategy == Strategy::StockRough && op.has_chain())
            {
                -(layer as f64 * tool.step_down).min(-z)
            } else {
                z
            };
            if op.strategy == Strategy::AreaRough
                || (op.strategy == Strategy::StockRough && op.has_chain())
            {
                for chain in area_paths(p, op, level, cancel)? {
                    cut_chain(&chain, b, cancel)?;
                }
                continue;
            }
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
        b.feed = tool.feed;
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
        let reverse = if op.settings.optimize_entry && !op.settings.closed {
            let point = |q: [f64; 3]| Point3 {
                x: q[0],
                y: q[1],
                z: b.position.z,
            };
            b.position.distance(point(*points.last().unwrap()))
                < b.position.distance(point(points[0]))
        } else if op.settings.closed {
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
        if op.settings.closed && !points.is_empty() {
            let nearest = (0..points.len())
                .min_by(|a, c| {
                    let distance = |q: [f64; 3]| (q[0] - b.position.x).hypot(q[1] - b.position.y);
                    distance(points[*a]).total_cmp(&distance(points[*c]))
                })
                .unwrap();
            points.rotate_left(nearest);
        }
        let target = contour_target(op);
        let axial_top = -material_top(p, index);
        let layers = target.map_or(layers, |z| {
            if op.settings.precleared {
                1
            } else {
                (((axial_top - z).max(0.) / tool.step_down).ceil() as usize).max(1)
            }
        });
        for layer in 1..=layers {
            cancelled(cancel)?;
            let z = if let Some(target) = target {
                let absolute = if op.settings.precleared {
                    target
                } else {
                    (axial_top - layer as f64 * tool.step_down).max(target)
                };
                absolute - points[0][2]
            } else if depth <= 0. {
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
                if layer == layers {
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
        if op.strategy == Strategy::Circular {
            for layer in 1..=layers {
                cancelled(cancel)?;
                let d = (top + layer as f64 * tool.step_down).min(depth);
                let radius =
                    r - (d - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
                let lead = WallLead {
                    wall: [op.x, op.y],
                    radius,
                    phase: 0.,
                    lead: (tool.diameter * 0.25).min(radius * 0.5),
                    z: -d,
                    sign,
                    outside: op.settings.outside,
                    tolerance,
                };
                let wall_feed = if layer == layers {
                    op.settings.finish_feed.unwrap_or(tool.feed)
                } else {
                    tool.feed
                };
                b.feed = b.lead_in_feed.min(wall_feed);
                lead.draw(b, true)?;
                b.feed = wall_feed;
                for _ in 0..(1 + if layer == layers {
                    op.settings.spring_passes
                } else {
                    0
                }) {
                    b.circle(op.x, op.y, radius, -d, clockwise, tolerance)?;
                }
                b.feed = b.lead_out_feed.min(wall_feed);
                lead.draw(b, false)?;
                b.feed = tool.feed;
                b.retract()?;
            }
            return Ok(());
        }
        let phase;
        b.feed = b.lead_in_feed;
        if op.settings.outside {
            b.enter(op.x + r + tool.diameter / 2., op.y, -top)?;
        } else {
            b.enter(op.x, op.y, -top)?;
        }
        b.feed_cut(op.x + r, op.y, -top)?;
        {
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
            b.feed = tool.feed;
            let steps = (turns * segments(radius, tolerance)? as f64).ceil() as usize;
            if steps > MAX_MOVES {
                return Err(
                    "Helical cut exceeds the move budget; increase pitch or tolerance".into(),
                );
            }
            if op.settings.taper_angle.unwrap_or(0.) == 0. {
                b.arc_xy([op.x, op.y], phase, -depth, tolerance)?;
            } else {
                for i in 1..=steps {
                    if i % 256 == 0 {
                        cancelled(cancel)?
                    }
                    let t = i as f64 / steps as f64;
                    let d = top + (depth - top) * t;
                    let a = sign * std::f64::consts::TAU * turns * t;
                    let radius = radius
                        - (d - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
                    b.feed_cut(op.x + radius * a.cos(), op.y + radius * a.sin(), -d)?;
                }
            }
            if op.strategy != Strategy::Thread {
                let radius =
                    r - (depth - top) * op.settings.taper_angle.unwrap_or(0.).to_radians().tan();
                b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
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
        b.feed = b
            .lead_out_feed
            .min(op.settings.finish_feed.unwrap_or(tool.feed));
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
        b.feed = tool.feed;
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
        for _pass in 0..(1 + if layer == layers {
            op.settings.spring_passes
        } else {
            0
        }) {
            if layer == layers {
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
