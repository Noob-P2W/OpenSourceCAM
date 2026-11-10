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
            let path = post::checked_path(p)?;
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
}
impl PlaybackPreview {
    pub fn new(project: &Project) -> Result<Self, String> {
        Ok(Self {
            path: post::checked_path(project)?,
        })
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
        GeometryQuery::Faces => Ok(json!(crate::faces::list(part)?)),
        GeometryQuery::Face { triangle } => Ok(json!(crate::faces::face(part, triangle)?)),
        GeometryQuery::Pick { origin, direction } => {
            Ok(json!(crate::faces::pick(part, origin, direction)?))
        }
        GeometryQuery::FacingDraft { triangle } => {
            Ok(json!(crate::faces::facing(project, triangle)?))
        }
        GeometryQuery::FeatureDraft { index, strategy } => {
            use crate::{features::FeatureKind, model::Strategy};
            let report = crate::features::recognize_project(project)?;
            let feature = report
                .features
                .get(index)
                .ok_or("Recognize and select an available feature")?;
            let mut op = crate::features::operation(project, feature)?;
            let compatible = match feature.kind {
                FeatureKind::RectangularPocket => {
                    matches!(strategy, Strategy::Pocket | Strategy::InsideProfile)
                }
                FeatureKind::CircularHole => {
                    if op.strategy.is_drilling() {
                        strategy.is_drilling()
                    } else {
                        strategy == Strategy::CircularPocket
                    }
                }
            };
            if !compatible {
                return Err("This strategy does not match the selected feature and tool. Choose a pocket/inside contour for a rectangular pocket, or circular pocket/drilling for a hole".into());
            }
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
    let path = post::checked_path(project)?;
    let sim = stock::simulate(
        project,
        &path,
        progress,
        resolution,
        &AtomicBool::new(false),
    )?;
    Ok(json!({"summary":sim.summary,"heights":sim.heights,"mesh":sim.geometry.mesh}))
}

pub fn stock_obj(project: &Project, progress: f64, resolution: usize) -> Result<String, String> {
    let path = post::checked_path(project)?;
    let simulation = stock::simulate(
        project,
        &path,
        progress,
        resolution,
        &AtomicBool::new(false),
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
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> String {
        serde_json::to_string(&Project::default()).unwrap()
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
