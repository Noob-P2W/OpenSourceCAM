//! UI-independent application commands. Native tools and browser adapters share validation.
use crate::{cam, model::Project, post, stock};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Generate,
    Export,
    Simulate,
    PlanStock,
    PlanSurface,
}
impl std::str::FromStr for Command {
    type Err = String;
    fn from_str(id: &str) -> Result<Self, Self::Err> {
        match id {
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
    match action {
        Command::Generate => {
            let path = post::checked_path(&p)?;
            Ok(
                json!({"moves":path.moves.iter().map(|m| json!({"from":[m.from.x,m.from.y,m.from.z],"to":[m.to.x,m.to.y,m.to.z],"rapid":m.motion==cam::Motion::Rapid})).collect::<Vec<_>>(),"seconds":path.estimated_seconds,"cut":path.cut_length}),
            )
        }
        Command::Export => Ok(json!(post::gcode(&p)?)),
        Command::Simulate => {
            let path = post::checked_path(&p)?;
            let sim = stock::simulate(&p, &path, 1.0, 96, &AtomicBool::new(false))?;
            Ok(json!({"summary":sim.summary,"heights":sim.heights}))
        }
        Command::PlanStock | Command::PlanSurface => {
            let plan = if action == Command::PlanSurface {
                crate::surface::from_stock(&p, &Default::default(), &AtomicBool::new(false))?
            } else {
                crate::machining::from_stock(&p, &Default::default(), &AtomicBool::new(false))?
            };
            Ok(json!(plan.project))
        }
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
