mod milling2d;
use crate::model::{Project, Strategy, FORMAT_VERSION};

const MAX_MOVES: usize = 200_000;
const MAX_LAYERS: usize = 2_000;
const MAX_ROWS: usize = 10_000;
const EPS: f64 = 1e-7;

// A model opening is not proof that stock above it is already air. Only an
// earlier whole-blank facing pass establishes a cleared starting level here.
fn material_top(project: &Project, index: usize) -> f64 {
    let requested = project.operations[index].settings.start_depth.unwrap_or(0.);
    let cleared = project.operations[..index]
        .iter()
        .filter(|op| op.enabled && op.strategy == Strategy::Facing)
        .map(|op| op.depth - op.settings.axial_allowance)
        .fold(0., f64::max);
    requested.min(cleared)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
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

#[derive(Clone, Debug)]
pub struct Toolpath {
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
        self.path.moves.push(Move {
            feed: rate,
            from: self.position,
            to,
            motion,
            operation: self.operation,
        });
        self.position = to;
        Ok(())
    }

    fn retract(&mut self) -> Result<(), String> {
        self.go(
            Point3 {
                z: self.clearance,
                ..self.position
            },
            Motion::Rapid,
        )
    }

    fn enter(&mut self, x: f64, y: f64, z: f64) -> Result<(), String> {
        self.retract()?;
        self.go(
            Point3 {
                x,
                y,
                z: self.clearance,
            },
            Motion::Rapid,
        )?;
        // Feed from clearance. Never rapid down into unverified stock.
        self.go(Point3 { x, y, z }, Motion::Plunge)
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
        let [x0, y0, x1, y1] = bounds;
        if entry == crate::operation::Entry::Plunge {
            return self.enter(x0, y0, z);
        }
        if entry == crate::operation::Entry::Ramp {
            let length = x1 - x0;
            if length <= EPS {
                return Err("Ramp entry needs room along X".into());
            }
            let passes = ((previous - z).abs() / (length * 3_f64.to_radians().tan()))
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
        let radius = ((x1 - x0).min(y1 - y0) / 4.).min(1.);
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
        let turns = ((previous - z).abs() / (circumference * 3_f64.to_radians().tan()))
            .ceil()
            .max(1.) as usize;
        if turns > MAX_LAYERS {
            return Err("Helix entry exceeds its turn budget".into());
        }
        self.enter(cx + radius, cy, previous)?;
        for i in 1..=turns * segments {
            let t = i as f64 / (turns * segments) as f64;
            let a = std::f64::consts::TAU * turns as f64 * t;
            self.feed_cut(
                cx + radius * a.cos(),
                cy + radius * a.sin(),
                previous + (z - previous) * t,
            )?
        }
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
        self.cut(x0, y0 - offset, z)?;
        for (cx, cy, start) in corners {
            self.cut(cx + offset * start.cos(), cy + offset * start.sin(), z)?;
            let sign = if clockwise { -1. } else { 1. };
            for i in 0..segments {
                let a = start + sign * (i as f64 + 0.5) * delta;
                self.cut(cx + outer * a.cos(), cy + outer * a.sin(), z)?;
            }
            let end = start + sign * sweep;
            self.cut(cx + offset * end.cos(), cy + offset * end.sin(), z)?;
        }
        self.cut(x0, y0 - offset, z)
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
        let angle =
            (2.0 * (1.0 - (tolerance / radius).min(1.0)).acos()).min(std::f64::consts::TAU / 24.0);
        let segments = (std::f64::consts::TAU / angle).ceil() as usize;
        if segments > 4096 {
            return Err(
                "Circular path exceeds 4,096 segments per ring; increase chord tolerance".into(),
            );
        }
        for i in 1..=segments {
            let a = std::f64::consts::TAU * i as f64 / segments as f64
                * if clockwise { -1.0 } else { 1.0 };
            let (sin, cos) = a.sin_cos();
            self.cut(x + radius * cos, y + radius * sin, z)?;
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
                    || op.settings.spring_passes > 0)
            {
                return Err(
                    "Stock allowances and contour spring passes do not apply to drilling".into(),
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

pub fn validate(project: &Project) -> Result<(), String> {
    validate_setup(project)?;
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
                let clockwise = project.library_tool(Some(op))?.is_none_or(|t| t.clockwise);
                let climb = mode == crate::operation::MillingMode::Climb;
                let internal = matches!(
                    op.strategy,
                    Strategy::Pocket | Strategy::CircularPocket | Strategy::InsideProfile
                ) || (op.strategy.is_round() && !op.settings.outside)
                    || (op.strategy == Strategy::Chamfer && !op.settings.outside);
                let cw = if op.has_chain() && !op.settings.closed {
                    (climb == clockwise)
                        == if op.model_link.is_some() {
                            op.settings.outside
                        } else {
                            match op.strategy {
                                Strategy::OutsideProfile => true,
                                Strategy::InsideProfile => false,
                                _ => op.settings.outside,
                            }
                        }
                } else {
                    internal != (climb == clockwise)
                };
                op.settings.direction = if cw {
                    crate::operation::Direction::Clockwise
                } else {
                    crate::operation::Direction::Counterclockwise
                };
            }
        }
        return generate_with_cancel(&resolved, cancel);
    }
    validate(project)?;
    let mut surface_budget = crate::surface::Budget::new(cancel);
    let mut b = Builder {
        path: Toolpath {
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
        let r = tool.diameter / 2.0;
        b.operation = index;
        if op.strategy.is_new_2d() || op.has_chain() {
            milling2d::generate(project, op, index, &mut b, cancel)?;
            b.clearance = project.travel_clearance();
            b.retract()?;
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
                    let z = -(layer as f64 * tool.step_down).min(depth) + 0.0002;
                    for chain in clearing.layer(
                        z,
                        op.settings.direction == crate::operation::Direction::Clockwise,
                        &mut surface_budget,
                    )? {
                        if let Some(first) = chain.first() {
                            b.enter(first.x, first.y, z)?;
                            for point in chain.iter().skip(1) {
                                b.cut(point.x, point.y, z)?;
                            }
                            b.retract()?;
                        }
                    }
                }
                b.clearance = project.travel_clearance();
                b.retract()?;
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
            b.clearance = project.travel_clearance();
            b.retract()?;
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
            b.clearance = project.travel_clearance();
            b.retract()?;
            continue;
        }
        for layer in 1..=layers {
            let z = -(top + layer as f64 * tool.step_down).min(depth);
            if op.strategy == Strategy::Facing {
                if op.settings.radial_allowance > 0.0 {
                    return Err("Whole-blank facing does not accept radial stock to leave".into());
                }
                let outside = -r - 0.5;
                let far = project.stock.width + r + 0.5;
                let rows = (project.stock.height / (tool.diameter * tool.step_over))
                    .ceil()
                    .max(1.0) as usize;
                if rows > MAX_ROWS {
                    return Err("Facing exceeds 10,000 rows".into());
                }
                let passes = if layer == layers {
                    1 + op.settings.spring_passes
                } else {
                    1
                };
                let feed = b.feed;
                for pass in 0..passes {
                    if pass > 0 {
                        b.feed = op.settings.finish_feed.unwrap_or(feed);
                    }
                    for row in 0..=rows {
                        let y = project.stock.height * row as f64 / rows as f64;
                        let (start, end) =
                            if op.settings.raster == crate::operation::Raster::NegativeX {
                                (far, outside)
                            } else if op.settings.raster == crate::operation::Raster::PositiveX
                                || row % 2 == 0
                            {
                                (outside, far)
                            } else {
                                (far, outside)
                            };
                        if row == 0 || op.settings.raster != crate::operation::Raster::Bidirectional
                        {
                            b.enter(start, y, z)?;
                        }
                        b.cut(start, y, z)?;
                        b.cut(end, y, z)?;
                    }
                    b.retract()?;
                }
                b.feed = feed;
                b.retract()?;
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
                b.enter(op.x, op.y - outer, z)?;
                for ring in 0..=rings {
                    let offset = outer - (outer - final_offset) * ring as f64 / rings as f64;
                    b.outside_contour(
                        [op.x, op.y, op.x + op.width, op.y + op.height],
                        offset,
                        z,
                        op.settings.direction == crate::operation::Direction::Clockwise,
                        op.settings.path_tolerance.unwrap_or(0.01),
                    )?;
                }
                if layer == layers {
                    let feed = b.feed;
                    b.feed = op.settings.finish_feed.unwrap_or(feed);
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
                for pass in 0..passes {
                    if pass > 0 {
                        b.feed = op.settings.finish_feed.unwrap_or(feed)
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
                && op.settings.entry == crate::operation::Entry::Plunge
                && op.settings.pocket_pattern == crate::operation::PocketPattern::Raster
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
                && op.settings.pocket_pattern == crate::operation::PocketPattern::Offset
            {
                let cx = (x0 + x1) / 2.;
                let cy = (y0 + y1) / 2.;
                let hx = (x1 - x0) / 2.;
                let hy = (y1 - y0) / 2.;
                let rings = (hx.hypot(hy) / (tool.diameter * tool.step_over))
                    .ceil()
                    .max(1.) as usize;
                if rings > MAX_ROWS {
                    return Err("Offset pocket exceeds 10,000 contours".into());
                }
                b.cut(cx, cy, z)?;
                for ring in 1..=rings {
                    let k = ring as f64 / rings as f64;
                    let a = cx - hx * k;
                    let c = cx + hx * k;
                    let d = cy - hy * k;
                    let e = cy + hy * k;
                    let points = if op.settings.direction == crate::operation::Direction::Clockwise
                    {
                        [(a, d), (a, e), (c, e), (c, d), (a, d)]
                    } else {
                        [(a, d), (c, d), (c, e), (a, e), (a, d)]
                    };
                    for (x, y) in points {
                        b.cut(x, y, z)?
                    }
                }
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
            let outline = if op.settings.direction == crate::operation::Direction::Clockwise {
                [(x0, y1), (x1, y1), (x1, y0), (x0, y0)]
            } else {
                [(x1, y0), (x1, y1), (x0, y1), (x0, y0)]
            };
            for (x, y) in outline {
                b.cut(x, y, z)?;
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
        b.clearance = project.travel_clearance();
        b.retract()?;
    }
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
