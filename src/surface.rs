//! Conservative top-side, three-axis machining of closed triangle meshes.
use crate::{
    cam::Point3,
    import::Part,
    model::{Operation, Project, Strategy},
    tool_library::ToolKind,
};

const SAFETY: f64 = 0.0002;

/// Size the blank and create a linked roughing/finishing pair. Geometry and cutting
/// checks run on a clone, so failure leaves the caller's job untouched.
pub fn from_stock(
    project: &Project,
    options: &crate::machining::PlanOptions,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<crate::machining::Plan, String> {
    use crate::process::Process;
    if project.process != Process::Milling {
        return Err("3D stock planning requires Milling".into());
    }
    if !project.operations.is_empty() && !options.replace_operations {
        return Err("Select Replace existing operations before rebuilding this job".into());
    }
    for v in [options.xy_margin, options.top_allowance, options.backing] {
        if !v.is_finite() || !(0.0..=10000.0).contains(&v) {
            return Err("Blank allowances must be between 0 and 10,000 mm".into());
        }
    }
    let mut result = project.clone();
    result.ensure_tool_library();
    let tool = result
        .library_tool(None)?
        .ok_or("Choose a library end mill")?;
    if !matches!(tool.kind, ToolKind::FlatEndMill | ToolKind::BallEndMill) || !tool.center_cutting {
        return Err("3D stock planning requires a center-cutting flat or ball end mill".into());
    }
    let id = tool.id;
    let preset = result.active_preset.unwrap_or(tool.default_preset);
    if options.fit_stock {
        crate::blank::fit(
            &mut result,
            options.xy_margin,
            options.top_allowance,
            options.backing,
        )?;
    }
    let part = result.part.as_ref().ok_or("Import a target model first")?;
    crate::blank::contains_part(&result.stock, part)?;
    let bounds = part.bounds();
    let report = crate::features::recognize_with_cancel(part, cancel)?;
    let mut operations = Vec::new();
    for strategy in [Strategy::SurfaceRough, Strategy::ParallelFinish] {
        let mut op = Operation {
            strategy,
            name: strategy.label().into(),
            x: 0.0,
            y: 0.0,
            width: result.stock.width,
            height: result.stock.height,
            depth: -bounds.min[2],
            tool_id: Some(id),
            preset_id: Some(preset),
            ..Operation::default()
        };
        op.settings.surface_step = Some(0.5);
        if strategy == Strategy::SurfaceRough {
            op.settings.axial_allowance = 0.3_f64.min(op.depth * 0.1);
        } else {
            op.settings.step_over_percent = Some(12.5);
        }
        crate::machining::link(&mut op, part);
        operations.push(op);
    }
    result.operations = operations;
    crate::cam::generate_with_cancel(&result, cancel)?;
    Ok(crate::machining::Plan{project:result,report,notes:vec![
        "Three-axis upper-surface machining: undercuts and inaccessible material remain for another setup.".into(),
        "The grid conservatively protects the imported triangle mesh. Reduce grid step and stepover for a finer finish; CAD tessellation also limits accuracy.".into(),
        "Constant-Z offset roughing clears successive depth layers and links connected regions at cutting depth. Separate regions retract. Entry/link cuts can use full tool width; this is conventional offset clearing, without a constant-engagement guarantee.".into(),
        "Backing remains attached and needs a second setup. Stock preview is sampled material removal, not exact solid or machine-kinematics verification.".into(),
    ]})
}
pub(crate) struct Field {
    heights: Vec<f64>,
    nx: usize,
    ny: usize,
    dx: f64,
    dy: f64,
    bottom: f64,
}
pub(crate) struct Budget<'a>(usize, &'a std::sync::atomic::AtomicBool);
impl<'a> Budget<'a> {
    pub(crate) fn new(cancel: &'a std::sync::atomic::AtomicBool) -> Self {
        Self(50_000_000, cancel)
    }
    pub(crate) fn tick(&mut self) -> Result<(), String> {
        if self.1.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("3D machining cancelled".into());
        }
        self.0 = self
            .0
            .checked_sub(1)
            .ok_or("3D surface work limit exceeded; increase grid step or split the job")?;
        Ok(())
    }
}
pub(crate) fn clip(poly: Vec<[f64; 3]>, axis: usize, bound: f64, greater: bool) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    if poly.is_empty() {
        return out;
    }
    let mut a = *poly.last().unwrap();
    for b in poly {
        let inside = |p: [f64; 3]| {
            if greater {
                p[axis] >= bound
            } else {
                p[axis] <= bound
            }
        };
        if inside(a) != inside(b) {
            let t = (bound - a[axis]) / (b[axis] - a[axis]);
            out.push(std::array::from_fn(|i| a[i] + t * (b[i] - a[i])));
        }
        if inside(b) {
            out.push(b);
        }
        a = b;
    }
    out
}
pub(crate) fn validate(project: &Project, index: usize) -> Result<(), String> {
    let op = &project.operations[index];
    let part = project
        .part
        .as_ref()
        .ok_or("3D machining requires an imported closed model")?;
    if op.model_link.is_none() {
        return Err(
            "3D operations must be linked to their target model; add a new 3D operation".into(),
        );
    }
    if project.library_tool(Some(op))?.is_none() {
        return Err("3D machining requires a library flat or ball end mill".into());
    }
    if op.x.abs() > 1e-7
        || op.y.abs() > 1e-7
        || (op.width - project.stock.width).abs() > 1e-7
        || (op.height - project.stock.height).abs() > 1e-7
    {
        return Err(
            "3D machining currently covers the whole blank; use X/Y 0 and stock width/height"
                .into(),
        );
    }
    let bounds = part.bounds();
    if bounds.min[0] < -1e-7
        || bounds.min[1] < -1e-7
        || bounds.max[0] > project.stock.width + 1e-7
        || bounds.max[1] > project.stock.height + 1e-7
        || bounds.max[2] > 1e-7
        || bounds.min[2] < -project.stock.thickness - 1e-7
    {
        return Err("The complete target model must fit inside the stock, below Z=0".into());
    }
    if op.settings.spring_passes > 0 || op.settings.finish_feed.is_some() {
        return Err("3D strategies use their cutting feed; contour spring passes and finish-feed overrides are unsupported".into());
    }
    if op.strategy == Strategy::ParallelFinish
        && !project.operations[..index].iter().any(|r| {
            r.enabled
                && r.strategy == Strategy::SurfaceRough
                && r.depth >= op.depth
                && r.settings.axial_allowance
                    <= project.operation_tool(op).map_or(0.0, |t| t.step_down)
                && r.settings.radial_allowance
                    <= project.operation_tool(op).map_or(0.0, |t| t.diameter * 0.5)
        })
    {
        return Err("3D finishing requires an earlier enabled 3D roughing operation covering this depth, with finishing stock within the finish tool's step-down and radius".into());
    }
    Ok(())
}
impl Field {
    pub(crate) fn build(
        part: &Part,
        project: &Project,
        step: f64,
        budget: &mut Budget,
    ) -> Result<Self, String> {
        crate::features::validate_closed(part, budget.1)?;
        crate::blank::contains_part(&project.stock, part)?;
        let nx = (project.stock.width / step).ceil() as usize;
        let ny = (project.stock.height / step).ceil() as usize;
        if nx > 2048 || ny > 2048 || nx.checked_mul(ny).is_none_or(|n| n > 262_144) {
            return Err(
                "3D grid exceeds 262,144 cells or 2,048 cells per axis; increase grid step".into(),
            );
        }
        let mut field = Self {
            heights: vec![-project.stock.thickness; nx * ny],
            nx,
            ny,
            dx: project.stock.width / nx as f64,
            dy: project.stock.height / ny as f64,
            bottom: -project.stock.thickness,
        };
        for tri in &part.geometry.mesh.triangles {
            let vertices = tri.map(|i| part.point(part.geometry.mesh.vertices[i as usize]));
            let low = std::array::from_fn::<_, 2, _>(|i| {
                vertices.iter().map(|v| v[i]).fold(f64::INFINITY, f64::min)
            });
            let high = std::array::from_fn::<_, 2, _>(|i| {
                vertices
                    .iter()
                    .map(|v| v[i])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            let x0 = ((low[0] / field.dx).floor().max(0.0) as usize).min(nx - 1);
            let x1 = ((high[0] / field.dx).floor().max(0.0) as usize).min(nx - 1);
            let y0 = ((low[1] / field.dy).floor().max(0.0) as usize).min(ny - 1);
            let y1 = ((high[1] / field.dy).floor().max(0.0) as usize).min(ny - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    budget.tick()?;
                    let mut poly = vertices.to_vec();
                    for (axis, bound, greater) in [
                        (0, x as f64 * field.dx, true),
                        (0, (x + 1) as f64 * field.dx, false),
                        (1, y as f64 * field.dy, true),
                        (1, (y + 1) as f64 * field.dy, false),
                    ] {
                        poly = clip(poly, axis, bound, greater);
                    }
                    for p in poly {
                        field.heights[y * nx + x] = field.heights[y * nx + x].max(p[2]);
                    }
                }
            }
        }
        Ok(field)
    }
    pub(crate) fn height(
        &self,
        segment: [f64; 2],
        y: f64,
        r: f64,
        ball: bool,
        op: &Operation,
        budget: &mut Budget,
    ) -> Result<f64, String> {
        let [a, b] = segment;
        let extra = op.settings.radial_allowance + SAFETY;
        let reach = r + extra;
        let x0 = ((a.min(b) - reach) / self.dx).floor().max(0.0) as usize;
        let x1 = (((a.max(b) + reach) / self.dx).floor().max(0.0) as usize).min(self.nx - 1);
        let y0 = ((y - reach) / self.dy).floor().max(0.0) as usize;
        let y1 = (((y + reach) / self.dy).floor().max(0.0) as usize).min(self.ny - 1);
        let mut height = self.bottom.max(-op.depth) + op.settings.axial_allowance + SAFETY;
        for iy in y0..=y1 {
            for ix in x0..=x1 {
                budget.tick()?;
                let dx = (ix as f64 * self.dx - a.max(b))
                    .max(a.min(b) - (ix + 1) as f64 * self.dx)
                    .max(0.0);
                let dy = (iy as f64 * self.dy - y)
                    .max(y - (iy + 1) as f64 * self.dy)
                    .max(0.0);
                let distance = dx.hypot(dy);
                if distance <= reach {
                    let d = (distance - extra).max(0.0).min(r);
                    let profile = if ball {
                        r - (r * r - d * d).max(0.0).sqrt()
                    } else {
                        0.0
                    };
                    height = height.max(
                        self.heights[iy * self.nx + ix] - profile
                            + op.settings.axial_allowance
                            + SAFETY,
                    );
                }
            }
        }
        Ok(height)
    }
    pub(crate) fn rows(
        &self,
        project: &Project,
        op: &Operation,
        budget: &mut Budget,
    ) -> Result<Vec<Vec<Point3>>, String> {
        let tool = project.operation_tool(op)?;
        let ball = project
            .library_tool(Some(op))?
            .is_some_and(|t| t.kind == ToolKind::BallEndMill);
        let r = tool.diameter * 0.5;
        let step = op.settings.surface_step.unwrap_or(0.5);
        let count = (project.stock.height / (tool.diameter * tool.step_over.min(0.5)))
            .ceil()
            .max(1.0) as usize;
        let start = -r - 0.5;
        let end = project.stock.width + r + 0.5;
        let segments = ((end - start) / step).ceil().max(1.0) as usize;
        if count > 10000
            || (count + 1)
                .checked_mul(segments + 1)
                .is_none_or(|n| n > 100_000)
        {
            return Err(
                "3D raster exceeds 100,000 samples; increase grid step or step-over".into(),
            );
        }
        let mut rows = Vec::new();
        for row in 0..=count {
            let y = project.stock.height * row as f64 / count as f64;
            let mut points: Vec<_> = (0..=segments)
                .map(|i| Point3 {
                    x: start + (end - start) * i as f64 / segments as f64,
                    y,
                    z: -op.depth,
                })
                .collect();
            let mut heights = Vec::new();
            for pair in points.windows(2) {
                heights.push(self.height([pair[0].x, pair[1].x], y, r, ball, op, budget)?);
            }
            for (i, p) in points.iter_mut().enumerate() {
                p.z = heights[i.min(segments - 1)].max(heights[i.saturating_sub(1)]);
            }
            if points
                .iter()
                .any(|p| p.z >= op.settings.clearance.unwrap_or(project.clearance))
            {
                return Err("3D contact height reaches clearance; increase clearance".into());
            }
            if op.settings.raster == crate::operation::Raster::NegativeX
                || (op.settings.raster == crate::operation::Raster::Bidirectional && row % 2 == 1)
            {
                points.reverse();
            }
            rows.push(points);
        }
        Ok(rows)
    }
}
