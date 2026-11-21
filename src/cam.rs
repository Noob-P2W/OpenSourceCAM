mod milling2d;
use crate::model::{Project, Strategy, FORMAT_VERSION};

const MAX_MOVES: usize = 200_000;
const MAX_LAYERS: usize = 2_000;
const MAX_ROWS: usize = 10_000;
const EPS: f64 = 1e-7;

// A model opening is not proof that stock above it is already air. Only an
// earlier whole-blank facing pass establishes a cleared starting level here.
fn cleared_top(project: &Project, index: usize) -> f64 {
    project.operations[..index]
        .iter()
        .filter(|op| op.enabled && op.strategy == Strategy::Facing)
        .map(|op| op.depth - op.settings.axial_allowance)
        .fold(0., f64::max)
}
fn material_top(project: &Project, index: usize) -> f64 {
    let cleared = cleared_top(project, index);
    project.operations[index]
        .settings
        .start_depth
        .unwrap_or(cleared)
        .min(cleared)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3 {
    pub fn distance(self, other: Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    ToolChange { seconds: f64, route_known: bool },
    Rapid,
    Cut,
    Plunge,
}

#[derive(Clone, Debug)]
pub struct Move {
    pub feed: f64,
    pub from: Point3,
    pub to: Point3,
    pub motion: Motion,
    pub operation: usize,
}

impl Move {
    pub fn seconds(&self) -> f64 {
        match self.motion {
            Motion::ToolChange { seconds, .. } => seconds,
            _ => self.from.distance(self.to) / self.feed * 60.,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArcSpan {
    pub start: usize,
    pub end: usize,
    pub center: [f64; 2],
    pub sweep: f64,
}
#[derive(Clone, Debug)]
pub struct Toolpath {
    pub arcs: Vec<ArcSpan>,
    pub moves: Vec<Move>,
    pub cut_length: f64,
    pub rapid_length: f64,
    /// Uses the configured rapid feed; excludes acceleration and spindle delay.
    pub estimated_seconds: f64,
}

struct Builder {
    path: Toolpath,
    position: Point3,
    operation: usize,
    clearance: f64,
    feed: f64,
    plunge: f64,
    rapid: f64,
    remaining: crate::material::RemainingStock,
    cutter: Option<crate::stock::Footprint>,
    certificate_error: f64,
    stock_clearance: Option<f64>,
    optimize_links: bool,
    bodies: Vec<crate::collision::Envelope>,
    fixtures: Option<crate::collision::FixtureClearance>,
    ramp_angle: f64,
    entry_radius: f64,
    ramp_feed: f64,
    lead_in_feed: f64,
    lead_out_feed: f64,
}

struct EntryPlan {
    points: [Point3; 4],
    certified: bool,
}

impl Builder {
    fn go(&mut self, to: Point3, motion: Motion) -> Result<(), String> {
        if self.path.moves.len() >= MAX_MOVES {
            return Err("Toolpath exceeds 200,000 moves; increase step-down or step-over".into());
        }
        let distance = self.position.distance(to);
        if distance <= EPS {
            return Ok(());
        }
        let rate = match motion {
            Motion::ToolChange { .. } => {
                return Err("Tool-change pauses belong to the playback sequence".into())
            }
            Motion::Rapid => {
                self.path.rapid_length += distance;
                self.rapid
            }
            Motion::Plunge => {
                self.path.cut_length += distance;
                self.plunge
            }
            Motion::Cut => {
                self.path.cut_length += distance;
                self.feed
            }
        };
        self.path.estimated_seconds += distance / rate * 60.0;
        let movement = Move {
            feed: rate,
            from: self.position,
            to,
            motion,
            operation: self.operation,
        };
        self.remaining
            .apply(&movement, self.cutter, self.certificate_error);
        self.path.moves.push(movement);
        self.position = to;
        Ok(())
    }

    fn stock_height(&mut self, a: [f64; 2], b: [f64; 2]) -> Option<f64> {
        if self.bodies.is_empty() {
            return None;
        }
        let mut height = f64::NEG_INFINITY;
        for e in &self.bodies {
            height = height.max(self.remaining.top(a, b, e.radius)? - e.low);
        }
        if let Some(fixtures) = &mut self.fixtures {
            height = height.max(fixtures.top(a, b, &self.bodies)?);
        }
        Some(height)
    }
    fn retract(&mut self) -> Result<(), String> {
        let mut z = self.clearance;
        if let Some(gap) = self.stock_clearance {
            let q = [self.position.x, self.position.y];
            if let Some(top) = self.stock_height(q, q) {
                let local = (top + gap).max(self.position.z);
                if local <= self.clearance {
                    z = local;
                }
            }
        }
        self.go(Point3 { z, ..self.position }, Motion::Rapid)
    }
    fn enter(&mut self, x: f64, y: f64, z: f64) -> Result<(), String> {
        let top = self
            .stock_clearance
            .and_then(|_| self.stock_height([self.position.x, self.position.y], [x, y]));
        let plan = self.entry_plan(Point3 { x, y, z }, top);
        self.follow_entry(plan)
    }
    fn entry_plan(&mut self, target: Point3, top: Option<f64>) -> EntryPlan {
        let Point3 { x, y, z } = target;
        let mut travel = self.clearance;
        let mut certified = false;
        if let Some(gap) = self.stock_clearance {
            if let Some(top) = top {
                let local = (top + gap).max(z);
                if local <= self.clearance {
                    travel = local;
                    certified = true;
                }
            }
        }
        let mut approach = travel;
        if let Some(gap) = self.stock_clearance {
            if let Some(top) = self.stock_height([x, y], [x, y]) {
                approach = (top + gap).max(z).min(travel);
            }
        }
        EntryPlan {
            points: [
                Point3 {
                    z: travel,
                    ..self.position
                },
                Point3 { x, y, z: travel },
                Point3 { x, y, z: approach },
                target,
            ],
            certified,
        }
    }
    fn follow_entry(&mut self, plan: EntryPlan) -> Result<(), String> {
        for (i, point) in plan.points.into_iter().enumerate() {
            self.go(
                point,
                if i == 3 {
                    Motion::Plunge
                } else {
                    Motion::Rapid
                },
            )?;
        }
        Ok(())
    }
    /// Replace a long, already-cleared contour link only when the entire tool
    /// and fixture route is certified, and the replacement is actually faster.
    fn cleared_link(&mut self, to: Point3, follow_mm: f64) -> Result<bool, String> {
        let Some(top) = self.stock_height([self.position.x, self.position.y], [to.x, to.y]) else {
            return Ok(false);
        };
        if top <= to.z.min(self.position.z) + 1e-8 {
            self.feed_cut(to.x, to.y, to.z)?;
            return Ok(true);
        }
        if !self.optimize_links {
            return Ok(false);
        }
        let plan = self.entry_plan(to, Some(top));
        let mut previous = self.position;
        let mut seconds = 0.;
        for (i, point) in plan.points.iter().enumerate() {
            let rate = if i == 3 { self.plunge } else { self.rapid };
            seconds += previous.distance(*point) / rate * 60.;
            previous = *point;
        }
        if plan.certified && seconds + 0.02 < follow_mm / self.feed * 60. {
            self.follow_entry(plan)?;
            return Ok(true);
        }
        Ok(false)
    }
    fn facing_y_clear(
        &mut self,
        width: f64,
        height: f64,
        radius: f64,
        rows: usize,
        z: f64,
    ) -> bool {
        let Some(fixtures) = &mut self.fixtures else {
            return true;
        };
        let outside = -radius - 0.5;
        let far = height + radius + 0.5;
        for row in 0..=rows {
            let x = width * row as f64 / rows as f64;
            if fixtures
                .top([x, outside], [x, far], &self.bodies)
                .is_none_or(|h| h > z + 1e-8)
            {
                return false;
            }
        }
        [outside, far].into_iter().all(|y| {
            fixtures
                .top([0., y], [width, y], &self.bodies)
                .is_some_and(|h| h <= z + 1e-8)
        })
    }
    fn cut(&mut self, x: f64, y: f64, z: f64) -> Result<(), String> {
        self.go(Point3 { x, y, z }, Motion::Cut)
    }
    fn feed_cut(&mut self, x: f64, y: f64, z: f64) -> Result<(), String> {
        let feed = self.feed;
        let dz = self.position.z - z;
        if dz > EPS {
            let distance = self.position.distance(Point3 { x, y, z });
            self.feed = self.feed.min(self.plunge * distance / dz);
        }
        let result = self.cut(x, y, z);
        self.feed = feed;
        result
    }
    fn pocket_entry(
        &mut self,
        bounds: [f64; 4],
        previous: f64,
        z: f64,
        entry: crate::operation::Entry,
        tolerance: f64,
    ) -> Result<(), String> {
        if entry == crate::operation::Entry::Plunge {
            return self.enter(bounds[0], bounds[1], z);
        }
        let feed = self.feed;
        self.feed = self.ramp_feed.min(feed);
        let result = self.ramped_entry(bounds, previous, z, entry, tolerance);
        self.feed = feed;
        result
    }
    fn ramped_entry(
        &mut self,
        bounds: [f64; 4],
        previous: f64,
        z: f64,
        entry: crate::operation::Entry,
        tolerance: f64,
    ) -> Result<(), String> {
        let [x0, y0, x1, y1] = bounds;
        if entry == crate::operation::Entry::Plunge {
            return self.enter(x0, y0, z);
        }
        if entry == crate::operation::Entry::Ramp {
            let length = x1 - x0;
            if length <= EPS {
                return Err("Ramp entry needs room along X".into());
            }
            let passes = ((previous - z).abs() / (length * self.ramp_angle.to_radians().tan()))
                .ceil()
                .max(1.) as usize;
            if passes > MAX_LAYERS {
                return Err("Ramp entry exceeds its pass budget".into());
            }
            self.enter(x0, y0, previous)?;
            for i in 1..=passes {
                self.feed_cut(
                    if i % 2 == 1 { x1 } else { x0 },
                    y0,
                    previous + (z - previous) * i as f64 / passes as f64,
                )?
            }
            return self.feed_cut(x0, y0, z);
        }
        let radius = ((x1 - x0).min(y1 - y0) / 2.).min(self.entry_radius);
        if radius <= EPS {
            return Err("Helix entry needs a wider pocket".into());
        }
        let cx = x0 + radius;
        let cy = y0 + radius;
        let segments = (std::f64::consts::TAU / (2. * (1. - (tolerance / radius).min(1.)).acos()))
            .ceil()
            .max(48.) as usize;
        if segments > 4096 {
            return Err("Helix entry exceeds its segment budget".into());
        }
        let circumference =
            2. * radius * (std::f64::consts::PI / segments as f64).sin() * segments as f64;
        let turns = ((previous - z).abs() / (circumference * self.ramp_angle.to_radians().tan()))
            .ceil()
            .max(1.) as usize;
        if turns > MAX_LAYERS {
            return Err("Helix entry exceeds its turn budget".into());
        }
        self.enter(cx + radius, cy, previous)?;
        self.arc_xy([cx, cy], std::f64::consts::TAU * turns as f64, z, tolerance)?;
        self.feed_cut(x0, y0, z)
    }
    fn outside_contour(
        &mut self,
        bounds: [f64; 4],
        offset: f64,
        z: f64,
        clockwise: bool,
        tolerance: f64,
    ) -> Result<(), String> {
        for p in Self::outside_points(bounds, offset, z, clockwise, tolerance)? {
            self.cut(p.x, p.y, p.z)?;
        }
        Ok(())
    }
    fn outside_points(
        bounds: [f64; 4],
        offset: f64,
        z: f64,
        clockwise: bool,
        tolerance: f64,
    ) -> Result<Vec<Point3>, String> {
        let [x0, y0, x1, y1] = bounds;
        let sweep = std::f64::consts::FRAC_PI_2;
        let angle = (2. * (offset / (offset + tolerance)).acos()).min(std::f64::consts::TAU / 24.);
        let segments = (sweep / angle).ceil() as usize;
        if segments > 1024 {
            return Err("Outside corner exceeds 1,024 segments; increase chord tolerance".into());
        }
        let delta = sweep / segments as f64;
        let outer = offset / (delta / 2.).cos();
        let corners = if clockwise {
            [
                (x0, y0, -sweep),
                (x0, y1, std::f64::consts::PI),
                (x1, y1, sweep),
                (x1, y0, 0.),
            ]
        } else {
            [
                (x1, y0, -sweep),
                (x1, y1, 0.),
                (x0, y1, sweep),
                (x0, y0, std::f64::consts::PI),
            ]
        };
        let mut points = vec![Point3 {
            x: x0,
            y: y0 - offset,
            z,
        }];
        for (cx, cy, start) in corners {
            points.push(Point3 {
                x: cx + offset * start.cos(),
                y: cy + offset * start.sin(),
                z,
            });
            let sign = if clockwise { -1. } else { 1. };
            for i in 0..segments {
                let a = start + sign * (i as f64 + 0.5) * delta;
                points.push(Point3 {
                    x: cx + outer * a.cos(),
                    y: cy + outer * a.sin(),
                    z,
                });
            }
            let end = start + sign * sweep;
            points.push(Point3 {
                x: cx + offset * end.cos(),
                y: cy + offset * end.sin(),
                z,
            });
        }
        points.push(Point3 {
            x: x0,
            y: y0 - offset,
            z,
        });
        Ok(points)
    }
    fn circle(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        z: f64,
        clockwise: bool,
        tolerance: f64,
    ) -> Result<(), String> {
        if radius <= EPS {
            return Ok(());
        }
        if ((self.position.x - x).hypot(self.position.y - y) - radius).abs() > 1e-5 {
            return Err("Circular start is not on the commanded circle".into());
        }
        self.arc_xy(
            [x, y],
            if clockwise {
                -std::f64::consts::TAU
            } else {
                std::f64::consts::TAU
            },
            z,
            tolerance,
        )
    }
    fn arc_xy(
        &mut self,
        center: [f64; 2],
        sweep: f64,
        end_z: f64,
        tolerance: f64,
    ) -> Result<(), String> {
        let origin = self.position;
        let radius = (origin.x - center[0]).hypot(origin.y - center[1]);
        if !radius.is_finite() || radius <= EPS || !sweep.is_finite() || sweep.abs() < 1e-12 {
            return Err("Invalid circular motion".into());
        }
        let angle =
            (2. * (1. - (tolerance / radius).min(1.)).acos()).min(std::f64::consts::TAU / 24.);
        let per_turn = (std::f64::consts::TAU / angle).ceil() as usize;
        if per_turn > 4096 {
            return Err("Circular path exceeds 4,096 segments per revolution".into());
        }
        let parts = (sweep.abs() / std::f64::consts::PI).ceil().max(1.) as usize;
        if parts > MAX_MOVES {
            return Err("Helical motion exceeds its turn budget".into());
        }
        let phase = (origin.y - center[1]).atan2(origin.x - center[0]);
        for part in 0..parts {
            let start = self.path.moves.len();
            let n = (per_turn as f64 * sweep.abs() / parts as f64 / std::f64::consts::TAU)
                .ceil()
                .max(2.) as usize;
            for i in 1..=n {
                let t = (part as f64 + i as f64 / n as f64) / parts as f64;
                let a = phase + sweep * t;
                self.feed_cut(
                    center[0] + radius * a.cos(),
                    center[1] + radius * a.sin(),
                    origin.z + (end_z - origin.z) * t,
                )?;
            }
            let end = self.path.moves.len();
            if end - start == n
                && self.path.moves[start..end].iter().all(|m| {
                    m.motion == Motion::Cut
                        && (m.feed - self.path.moves[start].feed).abs()
                            < 1e-7 * self.path.moves[start].feed.max(1.)
                })
            {
                self.path.arcs.push(ArcSpan {
                    start,
                    end,
                    center,
                    sweep: sweep / parts as f64,
                });
            }
        }
        Ok(())
    }
}

fn positive(value: f64, label: &str, max: f64) -> Result<(), String> {
    if !value.is_finite() || value < 0.001 || value > max + EPS {
        Err(format!(
            "{label} must be finite and between 0.001 and {max}"
        ))
    } else {
        Ok(())
    }
}

pub fn validate_cutting_tool(tool: &crate::model::Tool) -> Result<(), String> {
    // Output uses four decimal places, so the minimum is safely representable.
    positive(tool.diameter, "Tool diameter", 1000.0)?;
    positive(tool.feed, "Cut feed", 100_000.0)?;
    positive(tool.plunge, "Plunge feed", 100_000.0)?;
    positive(tool.step_down, "Step-down", 1000.0)?;
    if !tool.step_over.is_finite() || !(0.01..=1.0).contains(&tool.step_over) {
        return Err("Step-over must be between 1% and 100%".into());
    }
    if !(1..=100_000).contains(&tool.spindle_rpm) {
        return Err("Spindle speed must be between 1 and 100,000 RPM".into());
    }
    Ok(())
}

/// Project validity, including geometry-only projects that have no toolpath yet.
pub fn validate_setup(project: &Project) -> Result<(), String> {
    validate_setup_geometry(project)?;
    if project
        .operations
        .iter()
        .any(|op| op.enabled && milling2d::needs_stock_proof(op))
    {
        generate(project)?;
    }
    Ok(())
}
fn validate_setup_geometry(project: &Project) -> Result<(), String> {
    if project.format_version != FORMAT_VERSION {
        return Err("Unsupported project format version".into());
    }
    project.post.validate(project.process)?;
    crate::machining::validate_links(project)?;
    crate::workspace::validate(&project.workspace, project.machine.as_ref())?;
    crate::machine::validate_project(project)?;
    positive(project.stock.width, "Stock width", 100_000.0)?;
    positive(project.stock.height, "Stock height", 100_000.0)?;
    positive(project.stock.thickness, "Stock thickness", 10_000.0)?;
    project.stock.validate_profile()?;
    if project
        .stock_fit_margin
        .is_some_and(|v| !v.is_finite() || !(0. ..=10000.).contains(&v))
    {
        return Err("Invalid stock fitting margin".into());
    }
    if project
        .stock_fit_allowances
        .is_some_and(|a| a.iter().any(|v| !v.is_finite() || *v < 0. || *v > 10000.))
    {
        return Err("Blank allowances must be 0–10,000 mm".into());
    }
    positive(project.clearance, "Clearance", 10_000.0)?;
    if project.tool_library.tools.is_empty() {
        validate_cutting_tool(&project.tool)?;
    }
    project.tool_library.validate()?;
    if let Some(tool) = project.library_tool(None)? {
        tool.preset(project.active_preset)?;
    }
    for op in &project.operations {
        op.settings.validate()?;
        if let Some(tool) = project.library_tool(Some(op))? {
            tool.preset(op.preset_id)?;
        }
    }
    if project.operations.len() > 1000 {
        return Err("At most 1,000 operations are supported".into());
    }
    if !project.process.supports_toolpaths() && !project.operations.is_empty() {
        return Err(format!(
            "{} strategies are not implemented; milling operations cannot be used in this process",
            project.process.label()
        ));
    }
    if let Some(part) = &project.part {
        part.validate()?;
    }
    for (index, op) in project
        .operations
        .iter()
        .enumerate()
        .filter(|(_, op)| op.enabled)
    {
        let check = || -> Result<(), String> {
            if op.strategy.is_new_2d() || op.has_chain() {
                milling2d::validate(project, op, index)?;
                return Ok(());
            }
            if op.strategy.is_surface() {
                crate::surface::validate(project, index)?;
            }
            if op.strategy == Strategy::StockRough {
                if let Some(part) = &project.part {
                    let b = part.bounds();
                    if op.x > b.min[0] + EPS
                        || op.y > b.min[1] + EPS
                        || op.x + op.width < b.max[0] - EPS
                        || op.y + op.height < b.max[1] - EPS
                    {
                        return Err(
                            "Outside roughing must enclose the complete workpiece silhouette"
                                .into(),
                        );
                    }
                }
            }
            let tool = project.operation_tool(op)?;
            validate_cutting_tool(&tool)?;
            if (op.strategy.is_surface() || op.strategy == Strategy::Facing)
                && op.settings.start_depth.unwrap_or(0.) != 0.
            {
                return Err("Facing and 3D strategies start from stock top Z=0".into());
            }
            if let Some(id) = op.settings.machining_face {
                crate::faces::face_level(
                    project
                        .part
                        .as_ref()
                        .ok_or("Selected machining face needs its model")?,
                    id,
                )?;
            }
            if op.strategy == Strategy::Facing {
                if let Some(part) = &project.part {
                    let limit = -part.bounds().max[2];
                    if limit <= 0.0001 {
                        return Err("No facing stock above the model. Add a top allowance in Setup before facing".into());
                    }
                    if op.depth - op.settings.axial_allowance > limit + 1e-7 {
                        return Err("Facing would cut into the target model. Face only to its highest surface; use 3D clearing for recessed areas".into());
                    }
                }
            }
            positive(op.depth, "Depth", project.stock.thickness)?;
            positive(
                op.depth - op.settings.axial_allowance - op.settings.start_depth.unwrap_or(0.),
                "Depth below operation top after axial stock allowance",
                project.stock.thickness,
            )?;
            let clearance = op.settings.clearance.unwrap_or(project.clearance);
            if op.settings.retract.unwrap_or(clearance) > clearance {
                return Err("Retract height cannot exceed operation clearance height".into());
            }
            if op.strategy.is_drilling()
                && (op.settings.radial_allowance > 0.0
                    || op.settings.axial_allowance > 0.0
                    || op.settings.spring_passes > 0
                    || op.settings.finish_feed.is_some())
            {
                return Err(
                    "Stock allowances, finish feeds and contour spring passes do not apply to drilling".into(),
                );
            }
            if op.strategy != Strategy::DrillOnce
                && ((op.depth - op.settings.axial_allowance - material_top(project, index))
                    / tool.step_down)
                    .ceil()
                    > MAX_LAYERS as f64
            {
                return Err("Too many depth passes (maximum 2,000)".into());
            }
            if !op.x.is_finite() || !op.y.is_finite() || op.x < 0.0 || op.y < 0.0 {
                return Err("X and Y must be finite, non-negative stock coordinates".into());
            }
            if op.strategy.is_drilling() {
                let r = tool.diameter / 2.0;
                if op.x < r
                    || op.y < r
                    || op.x + r > project.stock.width
                    || op.y + r > project.stock.height
                {
                    return Err("Drill diameter extends outside stock".into());
                }
            } else if op.strategy == Strategy::CircularPocket {
                positive(
                    op.width,
                    "Pocket diameter",
                    project.stock.width.max(project.stock.height),
                )?;
                let radius = op.width / 2.0;
                if op.x < radius
                    || op.y < radius
                    || op.x + radius > project.stock.width
                    || op.y + radius > project.stock.height
                {
                    return Err("Circular feature extends outside stock".into());
                }
                if op.width + crate::features::TOLERANCE
                    < tool.diameter + 2.0 * op.settings.radial_allowance
                {
                    return Err(
                        "Circular feature is smaller than the cutter plus radial allowance".into(),
                    );
                }
            } else {
                positive(op.width, "Width", project.stock.width)?;
                positive(op.height, "Height", project.stock.height)?;
                if op.x + op.width > project.stock.width + EPS
                    || op.y + op.height > project.stock.height + EPS
                {
                    return Err("Rectangle extends outside stock".into());
                }
                if !matches!(
                    op.strategy,
                    Strategy::OutsideProfile
                        | Strategy::Facing
                        | Strategy::StockRough
                        | Strategy::SurfaceRough
                        | Strategy::ParallelFinish
                ) && (op.width <= tool.diameter + 2.0 * op.settings.radial_allowance
                    || op.height <= tool.diameter + 2.0 * op.settings.radial_allowance)
                {
                    return Err("Inside geometry must be larger than the tool diameter".into());
                }
                if op.strategy == Strategy::Facing
                    && (op.x.abs() > EPS
                        || op.y.abs() > EPS
                        || (op.width - project.stock.width).abs() > EPS
                        || (op.height - project.stock.height).abs() > EPS)
                {
                    return Err("Stock facing currently covers the whole blank; set X/Y to 0 and width/height to stock size".into());
                }
                if op.strategy == Strategy::Pocket {
                    let rows =
                        ((op.height - tool.diameter) / (tool.diameter * tool.step_over)).ceil();
                    if rows > MAX_ROWS as f64 {
                        return Err("Pocket exceeds 10,000 raster rows".into());
                    }
                }
            }
            if project.stock.profile != crate::blank::StockProfile::Rectangular
                && !matches!(
                    op.strategy,
                    Strategy::Facing
                        | Strategy::StockRough
                        | Strategy::SurfaceRough
                        | Strategy::ParallelFinish
                )
            {
                let points = if matches!(
                    op.strategy,
                    Strategy::Drill | Strategy::DrillOnce | Strategy::CircularPocket
                ) {
                    let radius = if op.strategy.is_drilling() {
                        tool.diameter / 2.
                    } else {
                        op.width / 2.
                    };
                    if !project.stock.contains_disc([op.x, op.y, -op.depth], radius) {
                        return Err("Circular operation extends outside the physical blank".into());
                    }
                    Vec::new()
                } else {
                    vec![
                        [op.x, op.y],
                        [op.x + op.width, op.y],
                        [op.x + op.width, op.y + op.height],
                        [op.x, op.y + op.height],
                    ]
                };
                for xy in points {
                    let floor = project
                        .stock
                        .column(xy[0], xy[1])
                        .ok_or("Operation extends beyond the physical blank")?;
                    if -op.depth < floor[0] - EPS {
                        return Err(
                            "Operation depth extends below the physical blank at this position"
                                .into(),
                        );
                    }
                }
            }
            Ok(())
        };
        check().map_err(|e| format!("Operation {}: {e}", index + 1))?;
    }
    Ok(())
}

fn validate_for_generation(project: &Project) -> Result<(), String> {
    validate_setup_geometry(project)?;
    if !project.process.supports_toolpaths() {
        return Err(format!(
            "{} toolpaths and postprocessing are not implemented yet",
            project.process.label()
        ));
    }
    if !project.operations.iter().any(|op| op.enabled) {
        return Err("Create a machining plan or add and enable an operation".into());
    }
    Ok(())
}

pub fn validate(project: &Project) -> Result<(), String> {
    generate(project).map(|_| ())
}

fn same_tool_next(project: &Project, index: usize) -> Result<bool, String> {
    let current = &project.operations[index];
    let Some(next) = project.operations[index + 1..].iter().find(|o| o.enabled) else {
        return Ok(false);
    };
    Ok(current.settings.optimize_links
        && next.settings.optimize_links
        && current.tool_id.or(project.active_tool) == next.tool_id.or(project.active_tool))
}
pub fn generate(project: &Project) -> Result<Toolpath, String> {
    generate_with_cancel(project, &std::sync::atomic::AtomicBool::new(false))
}
pub fn generate_with_cancel(
    project: &Project,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Toolpath, String> {
    if project
        .operations
        .iter()
        .any(|o| o.settings.milling_mode.is_some())
    {
        let mut resolved = project.clone();
        for op in &mut resolved.operations {
            if let Some(mode) = op.settings.milling_mode.take() {
                if op.strategy == Strategy::Thread {
                    continue;
                }
                // Closed contours retain climb milling. Open chains can choose
                // their entry endpoint from the actual current tool position.
                if mode == crate::operation::MillingMode::Optimized
                    && op.has_chain()
                    && !op.settings.closed
                {
                    op.settings.optimize_entry = true;
                }
                op.settings.optimize_links = mode == crate::operation::MillingMode::Optimized;
                let clockwise = project.library_tool(Some(op))?.is_none_or(|t| t.clockwise);
                let cw = crate::operation::clockwise_for(op, mode, clockwise);
                op.settings.direction = if cw {
                    crate::operation::Direction::Clockwise
                } else {
                    crate::operation::Direction::Counterclockwise
                };
            }
        }
        return generate_with_cancel(&resolved, cancel);
    }
    validate_for_generation(project)?;
    if project
        .operations
        .iter()
        .any(|o| o.enabled && o.strategy == Strategy::StockRough && o.model_link.is_some())
    {
        crate::features::validate_closed(
            project
                .part
                .as_ref()
                .ok_or("Outside clearing needs its model")?,
            cancel,
        )?;
    }
    let mut surface_budget = crate::surface::Budget::new(cancel);
    let mut b = Builder {
        path: Toolpath {
            arcs: Vec::new(),
            moves: Vec::new(),
            cut_length: 0.0,
            rapid_length: 0.0,
            estimated_seconds: 0.0,
        },
        // Program begins with a clearance retract; initial machine Z is unknown.
        position: Point3 {
            x: 0.0,
            y: 0.0,
            z: project.travel_clearance(),
        },
        operation: 0,
        clearance: project.travel_clearance(),
        feed: project.tool.feed,
        plunge: project.tool.plunge,
        remaining: crate::material::RemainingStock::new(project),
        cutter: None,
        certificate_error: 0.,
        stock_clearance: None,
        optimize_links: false,
        bodies: Vec::new(),
        fixtures: if project.operations.iter().any(|o| {
            o.enabled && (o.settings.stock_clearance.is_some() || o.settings.optimize_links)
        }) {
            Some(crate::collision::FixtureClearance::build(project, cancel)?)
        } else {
            None
        },
        ramp_angle: 3.,
        entry_radius: project.tool.diameter / 4.,
        ramp_feed: project.tool.feed,
        lead_in_feed: project.tool.feed,
        lead_out_feed: project.tool.feed,
        rapid: project.machine.as_ref().map_or(3000.0, |m| m.rapid_feed),
    };
    for (index, op) in project
        .operations
        .iter()
        .enumerate()
        .filter(|(_, op)| op.enabled)
    {
        let tool = project.operation_tool(op)?;
        b.clearance = op.settings.clearance.unwrap_or(project.clearance);
        b.feed = tool.feed;
        b.plunge = tool.plunge;
        milling2d::verify_stock(project, op, index, &mut b.remaining)
            .map_err(|e| format!("Operation {} ({}): {e}", index + 1, op.name))?;
        b.cutter = crate::stock::footprint(project, index)?;
        b.certificate_error = op.settings.path_tolerance.unwrap_or(0.01) + 0.0002;
        b.stock_clearance = op.settings.stock_clearance.or_else(|| {
            op.settings
                .optimize_links
                .then_some(0.5_f64.min(b.clearance))
        });
        b.optimize_links = op.settings.optimize_links;
        b.bodies =
            if b.stock_clearance.is_some() || project.process == crate::process::Process::Milling {
                crate::collision::envelopes(project, index)?
            } else {
                Vec::new()
            };
        if b.stock_clearance.is_some_and(|g| g > b.clearance) {
            return Err("Stock clearance gap cannot exceed operation clearance".into());
        }

        let preset = project
            .library_tool(Some(op))?
            .map(|t| t.preset(op.preset_id))
            .transpose()?;
        let value = |name: &str, fallback: f64| {
            preset
                .and_then(|p| p.values.get(name))
                .copied()
                .unwrap_or(fallback)
        };
        b.entry_radius = tool.diameter / 4.;
        b.ramp_angle = value("ramp_angle", 3.).min(3.);
        b.ramp_feed = value("ramp_feed", tool.feed).min(tool.feed);
        b.lead_in_feed = value("lead_in_feed", tool.feed).min(tool.feed);
        b.lead_out_feed = value("lead_out_feed", tool.feed).min(tool.feed);
        if matches!(op.strategy, Strategy::Pocket | Strategy::CircularPocket)
            && op.settings.entry != crate::operation::Entry::Plunge
            && b.ramp_angle <= 0.
        {
            return Err(
                "The selected tool preset needs a positive ramp angle for ramp/helix entry".into(),
            );
        }

        let r = tool.diameter / 2.0;
        b.operation = index;

        if op.strategy.is_new_2d() || op.has_chain() {
            milling2d::generate(project, op, index, &mut b, cancel)?;
            if !same_tool_next(project, index)? {
                b.clearance = project.travel_clearance();
                b.stock_clearance = None;
                b.retract()?;
            }
            continue;
        }
        let depth = op.depth - op.settings.axial_allowance;
        let top = material_top(project, index);
        let layers = ((depth - top) / tool.step_down).ceil() as usize;
        if op.strategy.is_surface() {
            let field = crate::surface::Field::finishing(
                project.part.as_ref().unwrap(),
                project,
                &mut surface_budget,
            )?;
            if op.strategy == Strategy::SurfaceRough {
                let clearing =
                    crate::roughing::Clearing::build(&field, project, op, &mut surface_budget)?;
                for layer in 1..=layers {
                    let z = -(top + layer as f64 * tool.step_down).min(depth) + 0.0002;
                    for chain in clearing.layer_from(
                        z,
                        op.settings.direction == crate::operation::Direction::Clockwise,
                        &mut surface_budget,
                        op.settings.optimize_links.then_some(b.position),
                    )? {
                        milling2d::cut_chain(&chain, &mut b, cancel)?;
                    }
                }
                if !same_tool_next(project, index)? {
                    b.clearance = project.travel_clearance();
                    b.stock_clearance = None;
                    b.retract()?;
                }
                continue;
            }
            let rows = field.rows(project, op, &mut surface_budget)?;
            let passes = if op.strategy == Strategy::ParallelFinish {
                1
            } else {
                layers
            };
            for layer in 1..=passes {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Machining cancelled".into());
                }
                let z = -(layer as f64 * tool.step_down).min(depth) + 0.0002;
                for row in &rows {
                    let mut engaged = false;
                    for pair in row.windows(2) {
                        let finish = op.strategy == Strategy::ParallelFinish;
                        if finish || pair.iter().all(|p| p.z <= z + EPS) {
                            let a = pair[0];
                            let end = pair[1];
                            if !engaged {
                                b.enter(a.x, a.y, if finish { a.z } else { z })?;
                                engaged = true;
                            }
                            b.cut(end.x, end.y, if finish { end.z } else { z })?;
                        } else if engaged {
                            b.retract()?;
                            engaged = false;
                        }
                    }
                    b.retract()?;
                }
            }
            if !same_tool_next(project, index)? {
                b.clearance = project.travel_clearance();
                b.stock_clearance = None;
                b.retract()?;
            }
            continue;
        }
        if op.strategy.is_drilling() {
            // G81/G83-equivalent expanded motions: XY only at clearance, then
            // rapid through verified air to R and feed to the drill-tip depth.
            let retract = op.settings.retract.unwrap_or(b.clearance);
            b.retract()?;
            b.go(
                Point3 {
                    x: op.x,
                    y: op.y,
                    z: b.clearance,
                },
                Motion::Rapid,
            )?;
            b.go(
                Point3 {
                    x: op.x,
                    y: op.y,
                    z: retract,
                },
                Motion::Rapid,
            )?;
            let pecks = if op.strategy == Strategy::DrillOnce {
                1
            } else {
                layers
            };
            let gap = op.settings.peck_clearance.unwrap_or(0.254);
            for peck in 1..=pecks {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Machining cancelled".into());
                }
                if peck > 1 {
                    // Only re-enter this same, already drilled bore. Stop above
                    // its previous bottom; all entry into new material is feed.
                    let previous = -(top + (peck - 1) as f64 * tool.step_down).min(depth);
                    b.go(
                        Point3 {
                            x: op.x,
                            y: op.y,
                            z: (previous + gap).min(retract),
                        },
                        Motion::Rapid,
                    )?;
                }
                let z = if op.strategy == Strategy::DrillOnce {
                    -depth
                } else {
                    -(top + peck as f64 * tool.step_down).min(depth)
                };
                b.go(
                    Point3 {
                        x: op.x,
                        y: op.y,
                        z,
                    },
                    Motion::Plunge,
                )?;
                b.go(
                    Point3 {
                        x: op.x,
                        y: op.y,
                        z: retract,
                    },
                    Motion::Rapid,
                )?;
            }
            if !same_tool_next(project, index)? {
                b.clearance = project.travel_clearance();
                b.stock_clearance = None;
                b.retract()?;
            }
            continue;
        }
        // The complete exterior silhouette is independent of cut height. Build
        // its offset chains once, then reuse XY geometry at every axial level.
        let exterior = if layers > 0
            && op.strategy == Strategy::StockRough
            && op.model_link.is_some()
            && project.part.is_some()
        {
            Some(milling2d::model_exterior(project, op, cancel)?)
        } else {
            None
        };
        for layer in 1..=layers {
            let z = -(top + layer as f64 * tool.step_down).min(depth);
            if op.strategy == Strategy::Facing {
                if op.settings.radial_allowance > 0.0 {
                    return Err("Whole-blank facing does not accept radial stock to leave".into());
                }
                let optimized_raster = b.optimize_links
                    && op.settings.raster == crate::operation::Raster::Bidirectional;
                let candidate_rows = (project.stock.width / (tool.diameter * tool.step_over))
                    .ceil()
                    .max(1.0) as usize;
                let along_y = optimized_raster
                    && project.stock.height > project.stock.width
                    && candidate_rows <= MAX_ROWS
                    && b.facing_y_clear(
                        project.stock.width,
                        project.stock.height,
                        r,
                        candidate_rows,
                        z,
                    );
                let (length, width) = if along_y {
                    (project.stock.height, project.stock.width)
                } else {
                    (project.stock.width, project.stock.height)
                };
                let outside = -r - 0.5;
                let far = length + r + 0.5;
                let rows = (width / (tool.diameter * tool.step_over)).ceil().max(1.) as usize;
                if rows > MAX_ROWS {
                    return Err("Facing exceeds 10,000 rows".into());
                }
                let passes = if layer == layers {
                    1 + op.settings.spring_passes
                } else {
                    1
                };
                let feed = b.feed;
                for _pass in 0..passes {
                    if layer == layers {
                        b.feed = op.settings.finish_feed.unwrap_or(feed);
                    }
                    let across = if along_y { b.position.x } else { b.position.y };
                    let along = if along_y { b.position.y } else { b.position.x };
                    let reverse_rows = optimized_raster && across > width / 2.;
                    let reverse_first = optimized_raster && along > length / 2.;
                    for ordinal in 0..=rows {
                        let row = if reverse_rows {
                            rows - ordinal
                        } else {
                            ordinal
                        };
                        let y = width * row as f64 / rows as f64;
                        let (start, end) =
                            if op.settings.raster == crate::operation::Raster::NegativeX {
                                (far, outside)
                            } else if op.settings.raster == crate::operation::Raster::PositiveX
                                || ((ordinal % 2 == 0) != reverse_first)
                            {
                                (outside, far)
                            } else {
                                (far, outside)
                            };
                        let point = |u, v| if along_y { (v, u) } else { (u, v) };
                        let a = point(start, y);
                        let c = point(end, y);
                        if ordinal == 0
                            || op.settings.raster != crate::operation::Raster::Bidirectional
                        {
                            b.enter(a.0, a.1, z)?;
                        }
                        b.cut(a.0, a.1, z)?;
                        b.cut(c.0, c.1, z)?;
                    }
                    b.retract()?;
                }
                b.feed = feed;
                b.retract()?;
                continue;
            }
            if let Some(chains) = &exterior {
                milling2d::clear_model_exterior(chains, z, &mut b, cancel)?;
                continue;
            }
            if op.strategy == Strategy::StockRough {
                let final_offset = r + op.settings.radial_allowance;
                // Rounded contours must start beyond the farthest stock corner,
                // including its diagonal distance from the retained rectangle.
                let outer =
                    op.x.max(project.stock.width - op.x - op.width)
                        .hypot(op.y.max(project.stock.height - op.y - op.height))
                        + r
                        + 0.5;
                if final_offset > outer {
                    return Err(
                        "Outside stock allowance exceeds the supported clearing region".into(),
                    );
                }
                let rings = ((outer - final_offset) / (tool.diameter * tool.step_over.min(0.5)))
                    .ceil()
                    .max(1.0) as usize;
                if rings > MAX_ROWS {
                    return Err("Outside clearing exceeds 10,000 contours".into());
                }
                if !b.optimize_links {
                    b.enter(op.x, op.y - outer, z)?;
                }
                let mut chain = Vec::new();
                for ring in 0..=rings {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err("Machining cancelled".into());
                    }
                    let offset = outer - (outer - final_offset) * ring as f64 / rings as f64;
                    if layer == layers && ring == rings {
                        if b.optimize_links
                            && !chain.is_empty()
                            && op.settings.finish_feed.is_some()
                        {
                            milling2d::cut_chain(&chain, &mut b, cancel)?;
                            chain.clear();
                        }
                        b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
                    }
                    let bounds = [op.x, op.y, op.x + op.width, op.y + op.height];
                    let cw = op.settings.direction == crate::operation::Direction::Clockwise;
                    let tolerance = op.settings.path_tolerance.unwrap_or(0.01);
                    if b.optimize_links {
                        chain.extend(Builder::outside_points(bounds, offset, z, cw, tolerance)?);
                        if chain.len() > MAX_MOVES {
                            return Err("Outside clearing exceeds its path point budget".into());
                        }
                    } else {
                        b.outside_contour(bounds, offset, z, cw, tolerance)?;
                    }
                }
                if b.optimize_links && !chain.is_empty() {
                    milling2d::cut_chain(&chain, &mut b, cancel)?;
                }
                if layer == layers {
                    let feed = b.feed;
                    b.feed = op.settings.finish_feed.unwrap_or(feed);
                    if b.optimize_links && op.settings.spring_passes > 0 {
                        b.enter(op.x, op.y - final_offset, z)?;
                    }
                    for _ in 0..op.settings.spring_passes {
                        b.outside_contour(
                            [op.x, op.y, op.x + op.width, op.y + op.height],
                            final_offset,
                            z,
                            op.settings.direction == crate::operation::Direction::Clockwise,
                            op.settings.path_tolerance.unwrap_or(0.01),
                        )?;
                    }
                    b.feed = feed;
                }
                b.retract()?;
                continue;
            }
            if op.strategy == Strategy::CircularPocket {
                let radius =
                    ((op.width - tool.diameter) / 2.0 - op.settings.radial_allowance).max(0.0);
                if op.settings.entry == crate::operation::Entry::Plunge {
                    b.enter(op.x, op.y, z)?
                } else {
                    let h = (radius / 2_f64.sqrt()).min(r / 2.);
                    b.pocket_entry(
                        [op.x - h, op.y - h, op.x + h, op.y + h],
                        -(top + (layer - 1) as f64 * tool.step_down),
                        z,
                        op.settings.entry,
                        op.settings.path_tolerance.unwrap_or(0.01),
                    )?;
                    b.cut(op.x, op.y, z)?;
                }
                if radius > EPS {
                    let rings =
                        (radius / (tool.diameter * tool.step_over)).ceil().max(1.0) as usize;
                    if rings > MAX_ROWS {
                        return Err("Circular pocket exceeds 10,000 rings".into());
                    }
                    for ring in 1..=rings {
                        let current = radius * ring as f64 / rings as f64;
                        b.cut(op.x + current, op.y, z)?;
                        if layer == layers && ring == rings {
                            b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
                        }
                        b.circle(
                            op.x,
                            op.y,
                            current,
                            z,
                            op.settings.direction == crate::operation::Direction::Clockwise,
                            op.settings.path_tolerance.unwrap_or(0.01),
                        )?;
                    }
                    if layer == layers {
                        let feed = b.feed;
                        b.feed = op.settings.finish_feed.unwrap_or(feed);
                        for _ in 0..op.settings.spring_passes {
                            b.circle(
                                op.x,
                                op.y,
                                radius,
                                z,
                                op.settings.direction == crate::operation::Direction::Clockwise,
                                op.settings.path_tolerance.unwrap_or(0.01),
                            )?;
                        }
                        b.feed = feed;
                    }
                }
                b.retract()?;
                continue;
            }
            if op.strategy == Strategy::OutsideProfile {
                let offset = r + op.settings.radial_allowance;
                b.enter(op.x, op.y - offset, z)?;
                let feed = b.feed;
                let passes = if layer == layers {
                    1 + op.settings.spring_passes
                } else {
                    1
                };
                for _pass in 0..passes {
                    if layer == layers {
                        b.feed = op.settings.finish_feed.unwrap_or(feed);
                    }
                    b.outside_contour(
                        [op.x, op.y, op.x + op.width, op.y + op.height],
                        offset,
                        z,
                        op.settings.direction == crate::operation::Direction::Clockwise,
                        op.settings.path_tolerance.unwrap_or(0.01),
                    )?;
                }
                b.feed = feed;
                b.retract()?;
                continue;
            }
            let offset = r + op.settings.radial_allowance;
            let x0 = op.x + offset;
            let y0 = op.y + offset;
            let x1 = op.x + op.width - offset;
            let y1 = op.y + op.height - offset;
            if op.strategy == Strategy::Pocket
                && op.settings.pocket_pattern == crate::operation::PocketPattern::Offset
            {
                // Parallel inward offsets, traversed from the core outwards.
                // Enter the core directly rather than slotting diagonally from
                // the pocket corner through uncut material.
                // Square offset corners need overlap in both axes; stepover is
                // a maximum, capped here to prevent diagonal corner cusps.
                let step = (tool.diameter * tool.step_over).min(r);
                let core_inset = (((x1 - x0).min(y1 - y0) / 2.) - r * 0.5).max(0.);
                let rings = (core_inset / step).ceil() as usize;
                if rings > MAX_ROWS {
                    return Err("Offset pocket exceeds 10,000 contours".into());
                }
                let inset = core_inset;
                b.pocket_entry(
                    [x0 + inset, y0 + inset, x1 - inset, y1 - inset],
                    -(top + (layer - 1) as f64 * tool.step_down),
                    z,
                    op.settings.entry,
                    op.settings.path_tolerance.unwrap_or(0.01),
                )?;
                let mut previous_y = y0 + inset;
                for ring in (0..=rings).rev() {
                    let inset = if rings == 0 {
                        0.
                    } else {
                        core_inset * ring as f64 / rings as f64
                    };
                    let (a, c, d, e) = (x0 + inset, x1 - inset, y0 + inset, y1 - inset);
                    // Orthogonal stepover links avoid diagonal slotting between laps.
                    b.cut(a, previous_y, z)?;
                    b.cut(a, d, z)?;
                    let points = if op.settings.direction == crate::operation::Direction::Clockwise
                    {
                        [(a, e), (c, e), (c, d), (a, d)]
                    } else {
                        [(c, d), (c, e), (a, e), (a, d)]
                    };
                    if layer == layers && ring == 0 {
                        b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
                    }
                    for (x, y) in points {
                        b.cut(x, y, z)?;
                    }
                    previous_y = d;
                }
            } else if op.strategy == Strategy::Pocket
                && op.settings.entry == crate::operation::Entry::Plunge
                && op.settings.raster == crate::operation::Raster::NegativeX
            {
                b.enter(x1, y0, z)?;
            } else if op.strategy == Strategy::Pocket {
                b.pocket_entry(
                    [x0, y0, x1, y1],
                    -(top + (layer - 1) as f64 * tool.step_down),
                    z,
                    op.settings.entry,
                    op.settings.path_tolerance.unwrap_or(0.01),
                )?;
            } else {
                b.enter(x0, y0, z)?;
            }
            if op.strategy == Strategy::Pocket
                && op.settings.pocket_pattern == crate::operation::PocketPattern::Raster
            {
                let rows = ((y1 - y0) / (tool.diameter * tool.step_over)).ceil() as usize;
                for row in 0..=rows {
                    let y = y0 + (y1 - y0) * row as f64 / rows as f64;
                    let (start, end) = if op.settings.raster == crate::operation::Raster::NegativeX
                    {
                        (x1, x0)
                    } else if op.settings.raster == crate::operation::Raster::PositiveX
                        || row % 2 == 0
                    {
                        (x0, x1)
                    } else {
                        (x1, x0)
                    };
                    if op.settings.raster != crate::operation::Raster::Bidirectional && row > 0 {
                        b.enter(start, y, z)?;
                    }
                    b.cut(start, y, z)?;
                    b.cut(end, y, z)?;
                }
                // Finish all walls after the raster, including the opposite side.
                b.cut(x0, y1, z)?;
                b.cut(x0, y0, z)?;
            }
            // Internal centerlines are offset inward; cutter radius remains at internal corners.
            if layer == layers {
                b.feed = op.settings.finish_feed.unwrap_or(tool.feed);
            }
            let outline = if op.settings.direction == crate::operation::Direction::Clockwise {
                [(x0, y1), (x1, y1), (x1, y0), (x0, y0)]
            } else {
                [(x1, y0), (x1, y1), (x0, y1), (x0, y0)]
            };
            if !(op.strategy == Strategy::Pocket
                && op.settings.pocket_pattern == crate::operation::PocketPattern::Offset)
            {
                for (x, y) in outline {
                    b.cut(x, y, z)?;
                }
            }

            if layer == layers {
                let previous = b.feed;
                b.feed = op.settings.finish_feed.unwrap_or(previous);
                for _ in 0..op.settings.spring_passes {
                    for (x, y) in outline {
                        b.cut(x, y, z)?;
                    }
                }
                b.feed = previous;
            }
            b.retract()?;
        }
        if !same_tool_next(project, index)? {
            b.clearance = project.travel_clearance();
            b.stock_clearance = None;
            b.retract()?;
        }
    }
    b.stock_clearance = None;
    b.clearance = project.travel_clearance();
    b.retract()?;
    if let Some(machine) = &project.machine {
        // Include both endpoints: the first clearance point can be outside travel
        // even if the first cutting endpoint is inside it. Linear segments lie
        // entirely within an axis-aligned envelope when both endpoints do.
        for movement in &b.path.moves {
            machine.check_point(movement.from)?;
            machine.check_point(movement.to)?;
            machine.check_move(movement)?;
        }
    }
    Ok(b.path)
}
