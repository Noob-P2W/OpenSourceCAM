//! UI-independent application commands. Native tools and browser adapters share validation.
use crate::{cam, model::Project, post, stock};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Generate,
    Recognize,
    Export,
    Simulate,
    PlanStock,
    PlanSurface,
}
impl std::str::FromStr for Command {
    type Err = String;
    fn from_str(id: &str) -> Result<Self, Self::Err> {
        match id {
            "recognize" => Ok(Self::Recognize),
            "generate" => Ok(Self::Generate),
            "export" => Ok(Self::Export),
            "simulate" => Ok(Self::Simulate),
            "plan" => Ok(Self::PlanStock),
            "surface" => Ok(Self::PlanSurface),
            _ => Err(format!("Unknown application command: {id}")),
        }
    }
}
pub fn execute(data: &str, action: Command) -> Result<Value, String> {
    let p = Project::from_json(data.as_bytes())?;
    execute_project(&p, action)
}
pub fn execute_project(p: &Project, action: Command) -> Result<Value, String> {
    cam::validate_setup(p)?;
    match action {
        Command::Recognize => Ok(json!(crate::features::recognize_project(p)?)),
        Command::Generate => {
            let path = post::checked_geometry_path(p)?;
            Ok(path_value(&path))
        }
        Command::Export => Ok(json!(post::gcode(p)?)),
        Command::Simulate => simulate_project(p, 1.0, 96),
        Command::PlanStock | Command::PlanSurface => {
            let plan = plan_project(p, action, &Default::default())?;
            Ok(json!(plan.project))
        }
    }
}
/// Shared estimated-time timeline for tool display and stock playback.
pub fn path_value(path: &cam::Toolpath) -> Value {
    let mut elapsed = 0.;
    let moves:Vec<_>=path.moves.iter().map(|m|{
        let start=elapsed;
        elapsed+=m.from.distance(m.to)/m.feed*60.;
        json!({"from":[m.from.x,m.from.y,m.from.z],"to":[m.to.x,m.to.y,m.to.z],"rapid":m.motion==cam::Motion::Rapid,"operation":m.operation,"start_seconds":start,"end_seconds":elapsed})
    }).collect();
    json!({"moves":moves,"seconds":path.estimated_seconds,"cut":path.cut_length})
}
/// A bounded path cache owned by an editing adapter; discard after every edit.
/// Geometry, cutter footprints and partial-move removal remain in the shared core.
pub struct PlaybackPreview {
    pub path: cam::Toolpath,
    frame: std::cell::RefCell<Option<FrameCache>>,
}
struct FrameCache {
    grid: stock::Grid,
    resolution: usize,
    epoch: u32,
    triangles: Vec<usize>,
}
pub struct StockFrame {
    pub metadata: Value,
    pub vertices: Vec<f32>,
}
impl PlaybackPreview {
    pub fn new(project: &Project) -> Result<Self, String> {
        Ok(Self {
            path: post::checked_geometry_path(project)?,
            frame: std::cell::RefCell::new(None),
        })
    }
    pub fn frame(
        &self,
        project: &Project,
        progress: f64,
        resolution: usize,
        epoch: u32,
        limit: u64,
    ) -> Result<StockFrame, String> {
        if ![50_000_000, 100_000_000, 200_000_000].contains(&limit) {
            return Err("Choose a supported simulation calculation budget".into());
        }
        let mut cache = self.frame.borrow_mut().take();
        let reset = cache.as_ref().is_none_or(|c| {
            c.epoch != epoch || c.resolution != resolution || progress < c.grid.progress
        });
        if reset {
            cache = Some(FrameCache {
                grid: stock::Grid::new(project, resolution)?,
                resolution,
                epoch,
                triangles: vec![0; resolution.div_ceil(8)],
            });
        }
        let mut state = cache.unwrap();
        let cancel = AtomicBool::new(false);
        state.grid.advance(&self.path, progress, limit, &cancel)?;
        let grid = &mut state.grid;
        let mut vertices = Vec::<f32>::new();
        let mut patches = Vec::new();
        for strip in 0..grid.ny.div_ceil(8) {
            let rows = strip * 8..((strip + 1) * 8).min(grid.ny);
            if !grid.dirty[rows.clone()].iter().any(|v| *v) {
                continue;
            }
            let max = if resolution <= 192 {
                crate::import::MAX_TRIANGLES
            } else {
                2_000_000
            };
            let mesh = grid.mesh_rows(rows.clone(), max, &cancel)?;
            state.triangles[strip] = mesh.triangles.len();
            if state.triangles.iter().sum::<usize>() > max {
                return Err("Stock preview surface exceeds its triangle budget".into());
            }
            let offset = vertices.len();
            for t in &mesh.triangles {
                let p = t.map(|i| mesh.vertices[i as usize]);
                let raw = crate::import::normal(p);
                let length = raw.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-20);
                let n = raw.map(|v| (v / length) as f32);
                for point in p {
                    vertices.extend(point.map(|v| v as f32));
                    vertices.extend(n);
                }
            }
            patches.push(json!({"strip":strip,"offset":offset,"length":vertices.len()-offset}));
            grid.dirty[rows].fill(false);
        }
        let (removed, remaining) = grid.volumes();
        let metadata = json!({"reset":reset,"epoch":epoch,"resolution":resolution,"patches":patches,"evaluations":grid.evaluations,"visited_cells":grid.visited,"summary":{"resolution":[grid.nx,grid.ny],"cell_size":[grid.dx,grid.dy],"progress":progress,"removed_volume_mm3":removed,"remaining_volume_mm3":remaining}});
        *self.frame.borrow_mut() = Some(state);
        Ok(StockFrame { metadata, vertices })
    }
    pub fn snapshot(
        &self,
        project: &Project,
        progress: f64,
        resolution: usize,
    ) -> Result<Value, String> {
        let sim = stock::preview(
            project,
            &self.path,
            progress,
            resolution,
            &AtomicBool::new(false),
        )?;
        Ok(json!({"summary":sim.summary,"heights":sim.heights,"mesh":sim.geometry.mesh}))
    }
}
/// Defaults for an operation settings draft; committing still validates the job.
pub fn operation_template(
    p: &Project,
    strategy: crate::model::Strategy,
) -> crate::model::Operation {
    use crate::model::{Operation, Strategy};
    let s = &p.stock;
    let mut op = Operation {
        strategy,
        name: strategy.label().into(),
        tool_id: p.active_tool,
        preset_id: p.active_preset,
        x: s.width * 0.25,
        y: s.height * 0.25,
        width: s.width * 0.5,
        height: s.height * 0.5,
        depth: s.thickness.min(3.),
        ..Operation::default()
    };
    match strategy {
        Strategy::Drill | Strategy::DrillOnce | Strategy::CircularPocket => {
            op.x = s.width / 2.;
            op.y = s.height / 2.;
            op.width = s.width.min(s.height) * 0.5;
            op.height = op.width;
        }
        Strategy::Facing | Strategy::SurfaceRough | Strategy::ParallelFinish => {
            op.x = 0.;
            op.y = 0.;
            op.width = s.width;
            op.height = s.height;
            if strategy == Strategy::Facing {
                if let Some(part) = &p.part {
                    op.depth = -part.bounds().max[2];
                    op.settings.machining_face = p.setup_face;
                    crate::machining::link(&mut op, part);
                }
            }
            if strategy.is_surface() {
                op.depth = p
                    .part
                    .as_ref()
                    .map_or(s.thickness, |part| -part.bounds().min[2]);
                op.settings.surface_step = Some(0.5);
                if let Some(part) = &p.part {
                    crate::machining::link(&mut op, part)
                }
            }
        }
        Strategy::StockRough => {
            if let Ok(report) = crate::features::recognize_project(p) {
                if let Some(e) = report.exterior {
                    op.x = e.min[0];
                    op.y = e.min[1];
                    op.width = e.max[0] - e.min[0];
                    op.height = e.max[1] - e.min[1];
                    op.depth = -e.bottom_z;
                }
            }
        }
        _ => {}
    }
    if !strategy.is_surface() && strategy != Strategy::Facing {
        if let Ok(Some(level)) = crate::faces::level(p) {
            if level < -0.002 {
                op.settings.start_depth = Some(-level);
                op.depth = (-level + 3.).min(p.stock.thickness);
            }
        }
    }
    op
}

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeometryQuery {
    Faces,
    Survey,
    PickFeature {
        origin: [f64; 3],
        direction: [f64; 3],
    },
    FeatureGeometry {
        feature: crate::features::Feature,
        strategy: crate::model::Strategy,
    },
    Face {
        triangle: usize,
    },
    Pick {
        origin: [f64; 3],
        direction: [f64; 3],
    },
    FeatureDraft {
        index: usize,
        strategy: crate::model::Strategy,
    },
    FacingDraft {
        triangle: usize,
    },
    ExteriorDraft {
        strategy: crate::model::Strategy,
    },
}
pub fn geometry_query(project: &Project, query: GeometryQuery) -> Result<Value, String> {
    let part = project
        .part
        .as_ref()
        .ok_or("Import a model to select machining geometry")?;
    match query {
        GeometryQuery::PickFeature { origin, direction } => Ok(json!(
            crate::features::pick_feature(project, origin, direction)?
        )),
        GeometryQuery::Survey => crate::features::survey(project),
        GeometryQuery::FeatureGeometry { feature, strategy } => {
            // Resolve client selection against recognition; never trust submitted dimensions.
            let report = crate::features::recognize_level_for_selection(part, feature.opening_z)?;
            let found = report
                .features
                .iter()
                .find(|f| {
                    f.kind == feature.kind
                        && f.center == feature.center
                        && (f.bottom_z - feature.bottom_z).abs() < 0.002
                })
                .ok_or("Selected feature changed; recognize the model again")?;
            Ok(json!(crate::features::draft(project, found, strategy)?))
        }
        GeometryQuery::Faces => Ok(json!(crate::faces::list(part)?)),
        GeometryQuery::Face { triangle } => Ok(json!(crate::faces::face(part, triangle)?)),
        GeometryQuery::Pick { origin, direction } => {
            Ok(json!(crate::faces::pick(part, origin, direction)?))
        }
        GeometryQuery::FacingDraft { triangle } => {
            Ok(json!(crate::faces::facing(project, triangle)?))
        }
        GeometryQuery::FeatureDraft { index, strategy } => {
            let report = crate::features::recognize_project(project)?;
            let feature = report
                .features
                .get(index)
                .ok_or("Recognize and select an available feature")?;
            let mut op = crate::features::draft(project, feature, strategy)?;
            op.strategy = strategy;
            op.name = format!("{} · selected {}", strategy.label(), feature.kind.label());
            crate::machining::link(&mut op, part);
            Ok(json!(op))
        }
        GeometryQuery::ExteriorDraft { strategy } => {
            use crate::model::Strategy;
            if !matches!(strategy, Strategy::StockRough | Strategy::OutsideProfile) {
                return Err(
                    "Choose outside clearing or outside contour for the part exterior".into(),
                );
            }
            let e = crate::features::recognize_project(project)?
                .exterior
                .ok_or("The selected setup does not have a supported rectangular exterior")?;
            let mut op = operation_template(project, strategy);
            op.x = e.min[0];
            op.y = e.min[1];
            op.width = e.max[0] - e.min[0];
            op.height = e.max[1] - e.min[1];
            op.depth = -e.bottom_z;
            op.settings.machining_face = project.setup_face;
            crate::machining::link(&mut op, part);
            Ok(json!(op))
        }
    }
}
pub fn simulate_project(
    project: &Project,
    progress: f64,
    resolution: usize,
) -> Result<Value, String> {
    simulate_project_budget(project, progress, resolution, 50_000_000)
}
pub fn simulate_project_budget(
    project: &Project,
    progress: f64,
    resolution: usize,
    limit: u64,
) -> Result<Value, String> {
    let path = post::checked_geometry_path(project)?;
    let sim = stock::simulate_budget(
        project,
        &path,
        progress,
        resolution,
        &AtomicBool::new(false),
        limit,
    )?;
    Ok(json!({"summary":sim.summary,"heights":sim.heights,"mesh":sim.geometry.mesh}))
}

pub fn stock_obj(project: &Project, progress: f64, resolution: usize) -> Result<String, String> {
    stock_obj_budget(project, progress, resolution, 50_000_000)
}
pub fn stock_obj_budget(
    project: &Project,
    progress: f64,
    resolution: usize,
    limit: u64,
) -> Result<String, String> {
    let path = post::checked_geometry_path(project)?;
    let simulation = stock::simulate_budget(
        project,
        &path,
        progress,
        resolution,
        &AtomicBool::new(false),
        limit,
    )?;
    stock::obj_data(&simulation)
}

/// The desktop and browser planners accept the same explicit blank allowances.
pub fn plan_project(
    project: &Project,
    command: Command,
    options: &crate::machining::PlanOptions,
) -> Result<crate::machining::Plan, String> {
    // Explicit replacement discards old operations before validating the planning
    // input. Stale geometry links or old overrides must not prevent rebuilding;
    // the newly planned paths still pass the full shared validation.
    let replacement = options.replace_operations.then(|| {
        let mut p = project.clone();
        p.operations.clear();
        p
    });
    let project = replacement.as_ref().unwrap_or(project);
    cam::validate_setup(project)?;
    match command {
        Command::PlanSurface => {
            crate::surface::from_stock(project, options, &AtomicBool::new(false))
        }
        Command::PlanStock => {
            crate::machining::from_stock(project, options, &AtomicBool::new(false))
        }
        _ => Err("Choose a stock or surface planner".into()),
    }
}
/// Reference values for unset editor fields; this query never edits the draft.
pub fn operation_defaults(
    project: &Project,
    op: &crate::model::Operation,
) -> Result<Value, String> {
    let mut base = op.clone();
    base.model_link = None;
    base.settings = Default::default();
    let record = project.library_tool(Some(op))?;
    let tool = project.operation_tool(&base).or_else(|_| {
        record.map_or_else(
            || Ok(project.tool.clone()),
            |t| t.cutting_tool(op.preset_id),
        )
    })?;
    let mut current = op.clone();
    current.model_link = None;
    current.settings.feed = None;
    let inherited_feed = project
        .operation_tool(&current)
        .map_or(tool.feed, |t| t.feed);
    current.settings.feed = op.settings.feed;
    current.settings.plunge = None;
    let inherited_plunge = project
        .operation_tool(&current)
        .map_or(tool.plunge, |t| t.plunge);
    let clearance = op.settings.clearance.unwrap_or(project.clearance);
    Ok(
        json!({"feed":inherited_feed,"plunge":inherited_plunge,"spindle_rpm":tool.spindle_rpm,"step_down":tool.step_down,"step_over_percent":tool.step_over*100.,"finish_feed":op.settings.feed.unwrap_or(inherited_feed),"clearance":project.clearance,"retract":clearance,"start_depth":0.,"surface_step":0.5,"path_tolerance":0.01,"peck_clearance":0.254,"tool_name":tool.name,"preset_name":record.and_then(|r|r.preset(op.preset_id).ok()).map(|p|p.name.clone())}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> String {
        serde_json::to_string(&Project::default()).unwrap()
    }
    #[test]
    fn inherited_drilling_references_follow_overrides_without_editing_the_project() {
        use crate::tool_library::{FeedMode, LibraryTool, ToolKind};
        let mut p = Project::default();
        p.ensure_tool_library();
        let mut drill = LibraryTool::new(1, 1, ToolKind::Drill);
        drill.presets[0].feed_mode = FeedMode::PerRevolution;
        drill.presets[0]
            .values
            .insert("feed_per_revolution".into(), 0.03);
        p.tool_library.tools = vec![drill];
        let op = &mut p.operations[0];
        op.strategy = crate::model::Strategy::DrillOnce;
        op.settings.spindle_rpm = Some(2000);
        op.settings.clearance = Some(8.);
        let before = serde_json::to_string(&p).unwrap();
        let refs = operation_defaults(&p, &p.operations[0]).unwrap();
        assert_eq!(refs["feed"], 60.);
        assert_eq!(refs["plunge"], 60.);
        assert_eq!(refs["retract"], 8.);
        assert_eq!(serde_json::to_string(&p).unwrap(), before);
        let mut op = p.operations[0].clone();
        op.settings.feed = Some(70.);
        op.settings.plunge = Some(30.);
        let refs = operation_defaults(&p, &op).unwrap();
        assert_eq!(refs["feed"], 60.);
        assert_eq!(refs["plunge"], 70.);
        assert_eq!(refs["finish_feed"], 70.);
    }
    #[test]
    fn malformed_commands_and_projects_are_errors() {
        assert!("not-a-command".parse::<Command>().is_err());
        assert!(execute("not-json", Command::Generate).is_err());
        let data = sample();
        let before = data.clone();
        assert!(execute(&data, Command::Simulate).is_ok());
        assert_eq!(data, before);
    }
    #[test]
    fn browser_export_matches_native_and_rejects_invalid_paths() {
        let p = Project::default();
        assert_eq!(
            execute(&sample(), Command::Export).unwrap(),
            json!(post::gcode(&p).unwrap())
        );
        assert!(!execute(&sample(), Command::Generate).unwrap()["moves"]
            .as_array()
            .unwrap()
            .is_empty());
        let mut bad = p;
        bad.operations[0].depth = -1.0;
        assert!(execute(&serde_json::to_string(&bad).unwrap(), Command::Export).is_err());
    }
}
