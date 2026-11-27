//! Structured diagnostics for the editor. Export continues to use strict preflight.
use crate::{
    cam::{Motion, Move, Point3, Toolpath},
    model::Project,
};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(crate) struct Issue {
    pub id: String,
    pub title: &'static str,
    pub detail: String,
    pub remedy: &'static str,
    pub severity: &'static str,
    pub target: Target,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Target {
    Operation {
        index: usize,
        tab: &'static str,
        field: &'static str,
    },
    Machine {
        label: String,
    },
    Setup {
        tab: &'static str,
        field: &'static str,
    },
    Fixture {
        id: u64,
    },
    Operations,
}
#[derive(Serialize)]
pub(crate) struct Location {
    pub move_index: usize,
    pub operation: usize,
    pub seconds: f64,
    pub tip: [f64; 3],
    pub point: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<crate::collision::ContactFinding>,
}

pub(crate) fn describe(project: &Project, error: String, operation: Option<usize>) -> Issue {
    let from_text = error.strip_prefix("Operation ").and_then(|s| {
        s.split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)
    });
    let linked = project.operations.iter().position(|op| {
        error
            .strip_prefix(&op.name)
            .is_some_and(|rest| rest.starts_with(": "))
    });
    let message = error.to_lowercase();
    let one_of = |words: &[&str]| words.iter().any(|word| message.contains(word));
    let (title, remedy, target) = if one_of(&[
        "imported part or placement changed",
        "linked geometry changed",
        "linked machining operations",
    ]) {
        let target = from_text
            .or(linked)
            .or(operation)
            .map_or(Target::Operations, |index| Target::Operation {
                index,
                tab: "geometry",
                field: "pick_geometry",
            });
        ("Model link needs rebuilding", "Reselect the machining geometry or rebuild the model-derived operations for this setup.", target)
    } else if one_of(&[
        "tool-change",
        "tool change",
        " atc",
        " m6",
        "length-offset register",
        "length register",
    ]) {
        (
            "Tool change needs review",
            "Check the change station, controller mode, and measured tool register assignments.",
            Target::Machine {
                label: "Tool changing".into(),
            },
        )
    } else if one_of(&["postprocessor", "g5 cubic spline"]) {
        (
            "Postprocessor settings",
            "Select a post and motion output supported by your controller.",
            Target::Machine {
                label: "Motion output".into(),
            },
        )
    } else if let Some(index) = from_text
        .or(operation)
        .filter(|i| *i < project.operations.len())
    {
        let (title, remedy, tab, field) = if one_of(&[
            "step down",
            "step-down",
            "stepdown",
            "too many depth passes",
        ]) {
            (
                "Stepdown exceeds the operation limits",
                "Review the stepdown and the selected tool's reach for this operation.",
                "passes",
                "step_down",
            )
        } else if one_of(&["stepover", "step-over", "raster rows"]) {
            (
                "Stepover needs review",
                "Set a stepover that fits the cutter and the selected geometry.",
                "passes",
                "step_over_percent",
            )
        } else if one_of(&["rest machining", "earlier enabled roughing"]) {
            ("Rest machining needs an earlier roughing pass", "Place a larger-cutter roughing operation before this operation, or turn off rest machining in Passes.", "passes", "rest_machining")
        } else if one_of(&["clearance", "retract height", "retract override"]) {
            ("Clearance or retract height", "Set the required clearance above the stock and workholding. Keep the retract at or below that clearance.", "heights", "clearance")
        } else if one_of(&["radial allowance", "radial stock", "cutter compensation"]) {
            (
                "Radial compensation needs review",
                "Check the radial stock allowance, selected chain and cutter diameter.",
                "passes",
                "radial_allowance",
            )
        } else if one_of(&["axial allowance", "axial stock"]) {
            ("Axial stock allowance needs review", "Adjust the stock to leave so the finish depth stays inside the intended cut envelope.", "passes", "axial_allowance")
        } else if one_of(&["spindle", "rpm"]) {
            (
                "Spindle speed needs review",
                "Set a spindle speed within the measured spindle and cutter limits.",
                "feeds",
                "spindle_rpm",
            )
        } else if one_of(&["feed exceeds", "cut feed", "feed override"]) {
            (
                "Cut feed needs review",
                "Set a feed within the machine and axis limits.",
                "feeds",
                "feed",
            )
        } else if one_of(&["plunge feed"]) {
            (
                "Plunge feed needs review",
                "Check the plunge feed for the selected cutter and material.",
                "feeds",
                "plunge",
            )
        } else if one_of(&[
            "reach",
            "flute",
            "stickout",
            "tool diameter",
            "select a measured",
            "cutter-sized",
            "tool preset",
            "library tool",
            "tool not found",
            "choose a chamfer",
        ]) {
            ("Tool or reach needs review", "Verify the cutter dimensions and preset, or select a tool suitable for the feature.", "tool", "tool_id")
        } else if one_of(&[
            "outside the physical blank",
            "outside physical stock",
            "outside stock",
            "fit the blank",
        ]) {
            ("Cut extends outside the stock", "Check the selected geometry and stock fit. Verify the operation lies within the measured blank.", "geometry", "pick_geometry")
        } else if one_of(&["remaining stock", "pre-cleared", "precleared"]) {
            (
                "Material remains at the finishing entry",
                "Rough the area first, or confirm that the selected area has already been cleared.",
                "geometry",
                "precleared",
            )
        } else if one_of(&["depth", "bottom z", "floor z"]) {
            let field = if project.operations[index].has_chain()
                && matches!(
                    project.operations[index].strategy,
                    crate::model::Strategy::Pocket
                        | crate::model::Strategy::AreaRough
                        | crate::model::Strategy::StockRough
                        | crate::model::Strategy::SurfaceSmooth
                        | crate::model::Strategy::InsideProfile
                        | crate::model::Strategy::OutsideProfile
                ) {
                "height_offset"
            } else {
                "depth"
            };
            ("Cut depth needs review", "Check the target height against the selected feature, stock bottom and preceding operations.", "geometry", field)
        } else if one_of(&["grid", "calculation budget", "surface sampling"]) {
            (
                "Toolpath calculation exceeds its limit",
                "Use a coarser grid or split the setup into smaller operations.",
                "passes",
                "surface_step",
            )
        } else {
            (
                "Operation needs review",
                "Review the selected geometry and cutting settings for this operation.",
                "geometry",
                "pick_geometry",
            )
        };
        (title, remedy, Target::Operation { index, tab, field })
    } else if one_of(&["machine", "axis", "spindle", "controller"]) {
        (
            "Machine configuration",
            "Check the configured machine travel, axes and controller against the physical setup.",
            Target::Machine {
                label: "CNC name".into(),
            },
        )
    } else if one_of(&["stock", "blank", "clearance", "placement"]) {
        let field = if message.contains("clearance") {
            "clearance"
        } else {
            "0"
        };
        (
            "Stock or setup needs review",
            "Review the blank dimensions, part placement and setup clearance.",
            Target::Setup {
                tab: "stock",
                field,
            },
        )
    } else {
        (
            "Workspace needs review",
            "Review the reported condition before generating another program.",
            Target::Operations,
        )
    };
    Issue {
        id: format!("settings:{error}"),
        title,
        detail: error,
        remedy,
        severity: "error",
        target,
        location: None,
    }
}

pub(crate) fn setup(project: &Project) -> Vec<Issue> {
    let mut issues: Vec<_> = crate::cam::inspection_setup(project)
        .into_iter()
        .map(|error| describe(project, error, None))
        .collect();
    if issues.is_empty() {
        if let Err(error) = crate::post::validate_tool_changes(project) {
            issues.push(describe(project, error, None));
        }
    }
    issues.extend(advisories(project));
    issues
}

pub(crate) fn advisories(project: &Project) -> Vec<Issue> {
    let mut issues = Vec::new();
    if let Some(error) = project
        .part
        .as_ref()
        .and_then(|part| crate::blank::contains_part(&project.stock, part).err())
    {
        issues.push(Issue {
            id: format!("stock_fit:{error}"),
            title: "Part extends beyond the blank",
            detail: error,
            remedy: "Fit stock to the part or adjust the blank dimensions and part placement.",
            severity: "warning",
            target: Target::Setup {
                tab: "stock",
                field: "0",
            },
            location: None,
        });
    }
    issues
}

fn xyz(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn location(index: usize, movement: &Move, row_start: f64, t: f64) -> Location {
    let [a, b] = [xyz(movement.from), xyz(movement.to)];
    let tip = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
    Location {
        move_index: index,
        operation: movement.operation,
        seconds: row_start + movement.seconds() * t,
        tip,
        point: tip,
        contact: None,
    }
}

pub(crate) fn path(project: &Project, path: &Toolpath) -> Vec<Issue> {
    let mut starts = Vec::with_capacity(path.moves.len());
    let mut elapsed = 0.;
    let mut result = Vec::new();
    let mut reported = BTreeSet::new();
    for (i, movement) in path.moves.iter().enumerate() {
        starts.push(elapsed);
        elapsed += movement.seconds();
        let Some(machine) = project.machine.as_ref().filter(|_| result.len() < 48) else {
            continue;
        };
        if matches!(movement.motion, Motion::ToolChange { .. }) {
            continue;
        }
        for (prefix, point) in [(0., movement.from), (1., movement.to)] {
            if let Err(error) = machine.check_point(point) {
                let axis = error
                    .strip_prefix("Tool-tip ")
                    .and_then(|rest| rest.chars().next())
                    .unwrap_or('X');
                let slot = "XYZ".find(axis).unwrap_or(0);
                let position = xyz(point)[slot] + machine.work_origin[slot];
                let posted = format!("{:.4}", xyz(point)[slot])
                    .parse::<f64>()
                    .unwrap_or(position)
                    + machine.work_origin[slot];
                let lower = position.min(posted) < machine.axis_min[slot] - 1e-7;
                let id = format!("travel:{}:{axis}:{lower}", movement.operation);
                if reported.insert(id.clone()) {
                    result.push(Issue {
                        id,
                        title: "Machine travel limit",
                        detail: format!("{error}. Move {}.", i + 1),
                        remedy: "Check the work origin, stock placement and measured axis limits. For a Z violation, check the setup clearance.",
                        severity: "error",
                        target: Target::Machine { label: format!("axis {} {axis} · mm", if lower { "min" } else { "max" }) },
                        location: Some(location(i, movement, starts[i], prefix)),
                    });
                }
            }
        }
        if let Err(error) = machine.check_move(movement) {
            let key = format!("feed:{}:{error}", movement.operation);
            if reported.insert(key.clone()) {
                result.push(Issue {
                    id: key,
                    title: "Machine feed limit",
                    detail: format!("{error}. Move {}.", i + 1),
                    remedy: "Reduce the operation's feed for this move and verify the machine's axis feed limits.",
                    severity: "error",
                    target: Target::Operation { index: movement.operation, tab: "feeds",
                        field: if movement.motion == Motion::Plunge { "plunge" } else { "feed" } },
                    location: Some(location(i, movement, starts[i], 0.5)),
                });
            }
        }
    }
    let (contacts, error) = crate::collision::inspect(project, path);
    for contact in contacts {
        let index = contact.movement;
        let movement = &path.moves[index];
        let section = contact.section;
        let mut where_ = location(index, movement, starts[index], contact.fraction);
        where_.tip = contact.tip;
        where_.point = contact.point;
        let point = where_.point;
        let fixture = contact.fixture;
        let detail = format!(
            "The {section} envelope {} fixture ‘{}’ on move {} ({}). Estimated contact near X {:.3}, Y {:.3}, Z {:.3} mm.",
            if contact.contained { "starts inside" } else { "contacts" },
            contact.name, index + 1, if movement.motion == Motion::Rapid { "rapid" } else { "cutting" },
            point[0], point[1], point[2],
        );
        where_.contact = Some(contact);
        result.push(Issue {
            id: format!("fixture:{fixture}:{}:{section}", movement.operation),
            title: "Fixture contact",
            detail,
            remedy: "Check fixture placement and the measured tool assembly. Adjust the clearance, tool reach or cut route to clear the highlighted area.",
            severity: "error",
            target: Target::Fixture { id: fixture },
            location: Some(where_),
        });
    }
    if let Some(error) = error {
        result.push(Issue { id: format!("fixture_check:{error}"), title: "Fixture verification incomplete",
            detail: error, remedy: "Check that fixture models are closed solids. If the check reached its calculation limit, split the job or simplify the fixture model.",
            target: Target::Setup { tab: "workholding", field: "frame" }, severity: "error", location: None });
    }
    if let Err(error) = crate::post::validate_tool_changes(project) {
        result.push(describe(project, error, None));
    }
    result.sort_by(|a, b| {
        a.location
            .as_ref()
            .map_or(f64::INFINITY, |l| l.seconds)
            .total_cmp(&b.location.as_ref().map_or(f64::INFINITY, |l| l.seconds))
    });
    result
}
