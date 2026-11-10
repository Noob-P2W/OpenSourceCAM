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
    contact: Option<crate::contact::Contact>,
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
            .ok_or("3D surface work limit exceeded; use a coarser roughing grid, relax finishing sampling/contact tolerance, or split the job")?;
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
    pub(crate) fn finishing(
        part: &Part,
        project: &Project,
        budget: &mut Budget,
    ) -> Result<Self, String> {
        crate::features::validate_closed(part, budget.1)?;
        crate::blank::contains_part(&project.stock, part)?;
        Ok(Self {
            contact: Some(crate::contact::Contact::build(part, budget)?),
            bottom: -project.stock.thickness,
        })
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
        if let Some(contact) = &self.contact {
            return Ok(contact.height(
                segment,
                y,
                r,
                ball,
                op.settings.radial_allowance + SAFETY,
                op.settings.surface_step.unwrap_or(0.5),
                op.settings.path_tolerance.unwrap_or(0.01),
                self.bottom.max(-op.depth),
                budget,
            )? + op.settings.axial_allowance
                + SAFETY);
        }
        unreachable!("Contact hierarchy is always initialized")
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
        if count > 10000 {
            return Err(
                "3D finishing exceeds 10,000 rows; increase stepover or split the job".into(),
            );
        }
        let mut samples = 0usize;
        let mut rows = Vec::new();
        for row in 0..=count {
            let y = project.stock.height * row as f64 / count as f64;
            let first = self.height([start, start], y, r, ball, op, budget)?;
            let last = self.height([end, end], y, r, ball, op, budget)?;
            let mut stack = vec![(start, end, first, last)];
            let mut spans = Vec::new();
            while let Some((a, b, ha, hb)) = stack.pop() {
                budget.tick()?;
                let h = self.height([a, b], y, r, ball, op, budget)?;
                if b - a <= step || h - ha.min(hb) <= op.settings.path_tolerance.unwrap_or(0.01) {
                    samples += 1;
                    if samples > 100_000 {
                        return Err("3D finishing exceeds 100,000 adaptive spans; relax surface sampling/contact tolerance or split the job".into());
                    }
                    spans.push((a, b, h));
                } else {
                    let mid = (a + b) / 2.;
                    let hm = self.height([mid, mid], y, r, ball, op, budget)?;
                    stack.push((mid, b, hm, hb));
                    stack.push((a, mid, ha, hm));
                }
            }
            let mut points = Vec::with_capacity(spans.len() + 1);
            for (i, &(a, _, h)) in spans.iter().enumerate() {
                points.push(Point3 {
                    x: a,
                    y,
                    z: h.max(if i > 0 { spans[i - 1].2 } else { h }),
                });
            }
            points.push(Point3 {
                x: end,
                y,
                z: spans.last().unwrap().2,
            });
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
