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
        Command::Recognize => Ok(json!(crate::features::recognize(
            p.part.as_ref().ok_or("Import a part first")?
        )?)),
        Command::Generate => {
            let path = post::checked_path(p)?;
            Ok(
                json!({"moves":path.moves.iter().map(|m| json!({"from":[m.from.x,m.from.y,m.from.z],"to":[m.to.x,m.to.y,m.to.z],"rapid":m.motion==cam::Motion::Rapid,"operation":m.operation})).collect::<Vec<_>>(),"seconds":path.estimated_seconds,"cut":path.cut_length}),
            )
        }
        Command::Export => Ok(json!(post::gcode(p)?)),
        Command::Simulate => simulate_project(p, 1.0, 96),
        Command::PlanStock | Command::PlanSurface => {
            let plan = plan_project(p, action, &Default::default())?;
            Ok(json!(plan.project))
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
