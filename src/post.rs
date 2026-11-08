use crate::process::Process;
use crate::{
    cam::{generate, Motion},
    model::Project,
};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostProcessor {
    #[default]
    Generic,
    Mach3,
    Grbl,
    LinuxCnc,
    Klipper,
}

impl PostProcessor {
    pub const ALL: [Self; 5] = [
        Self::Generic,
        Self::Mach3,
        Self::Grbl,
        Self::LinuxCnc,
        Self::Klipper,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Mach3 => "mach3",
            Self::Grbl => "grbl",
            Self::LinuxCnc => "linuxcnc",
            Self::Klipper => "klipper",
        }
    }
    pub fn parse(id: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|p| p.id() == id)
            .ok_or_else(|| format!("Unknown postprocessor '{id}'; use --posts"))
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Generic => "Generic metric",
            Self::Mach3 => "Mach3 Mill",
            Self::Grbl => "GRBL 1.1",
            Self::LinuxCnc => "LinuxCNC",
            Self::Klipper => "Klipper (planned)",
        }
    }
    pub fn compatible(self, process: Process) -> bool {
        match self {
            Self::Generic => true,
            Self::Klipper => process == Process::Printing3d,
            _ => process == Process::Milling,
        }
    }
    pub fn ready(self, process: Process) -> bool {
        process == Process::Milling && self != Self::Klipper
    }
    pub fn extension(self) -> &'static str {
        if self == Self::Klipper {
            "gcode"
        } else {
            "nc"
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkOffset {
    #[default]
    G54,
    G55,
    G56,
    G57,
    G58,
    G59,
}
impl WorkOffset {
    pub const ALL: [Self; 6] = [
        Self::G54,
        Self::G55,
        Self::G56,
        Self::G57,
        Self::G58,
        Self::G59,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::G54 => "G54",
            Self::G55 => "G55",
            Self::G56 => "G56",
            Self::G57 => "G57",
            Self::G58 => "G58",
            Self::G59 => "G59",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coolant {
    #[default]
    Off,
    Flood,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostSettings {
    #[serde(default)]
    pub use_tool_coolant: bool,
    pub processor: PostProcessor,
    pub work_offset: WorkOffset,
    pub coolant: Coolant,
}
impl PostSettings {
    pub fn for_process(process: Process) -> Self {
        Self {
            processor: if process == Process::Printing3d {
                PostProcessor::Klipper
            } else {
                PostProcessor::Generic
            },
            ..Self::default()
        }
    }
    pub fn validate(&self, process: Process) -> Result<(), String> {
        if !self.processor.compatible(process) {
            return Err(format!(
                "{} is not compatible with {}",
                self.processor.label(),
                process.label()
            ));
        }
        Ok(())
    }
}

/// Complete NC preflight shared by validation and export.
pub fn checked_path(project: &Project) -> Result<crate::cam::Toolpath, String> {
    project.post.validate(project.process)?;
    if !project.post.processor.ready(project.process) {
        return Err(format!(
            "{} export for {} is not implemented yet",
            project.post.processor.label(),
            project.process.label()
        ));
    }
    let path = generate(project)?;
    crate::collision::check(project, &path)?;
    validate_tool_changes(project)?;
    Ok(path)
}

/// Metric, absolute G-code. Regenerates and preflights current project data.
pub fn gcode(project: &Project) -> Result<String, String> {
    let path = checked_path(project)?;
    let processor = project.post.processor;
    let percent = matches!(processor, PostProcessor::Mach3 | PostProcessor::LinuxCnc);
    let mut output = String::new();
    if percent {
        output.push_str("%\n");
    }
    writeln!(
        output,
        "(OpenSourceCAM {} - {})",
        env!("CARGO_PKG_VERSION"),
        processor.label()
    )
    .unwrap();
    let automatic = project
        .machine
        .as_ref()
        .is_some_and(|m| m.tool_changer.mode == crate::machine::ChangeMode::MillingAtc);
    output.push_str(if automatic {
        "(Metric - stock top Z0 - controller-managed ATC)\n"
    } else {
        "(Metric - stock top Z0 - mounted single tool)\n"
    });
    if project.part.is_some() {
        output.push_str(
            if project
                .operations
                .iter()
                .any(|op| op.enabled && op.model_link.is_some())
            {
                "(Imported target - linked machining operations)\n"
            } else {
                "(Manual operations - imported model is a reference)\n"
            },
        );
    }
    writeln!(output, "(Project: {})", comment(&project.name)).unwrap();
    output.push_str("G21\nG90\nG17\nG94\nG40\nG49\nG80\n");
    writeln!(output, "{}", project.post.work_offset.code()).unwrap();
    if matches!(processor, PostProcessor::Mach3 | PostProcessor::LinuxCnc) {
        output.push_str("G61\n");
    }
    output.push_str("M9\n");
    let first = project
        .operations
        .iter()
        .find(|op| op.enabled)
        .ok_or("No enabled operations")?;
    let first_tool = project.operation_tool(first)?;
    let first_record = project.library_tool(Some(first))?;
    let initial_coolant = tool_coolant(project, first)?;
    writeln!(
        output,
        "(Tool: {} / diameter {:.4} mm)",
        comment(&first_tool.name),
        first_tool.diameter
    )
    .unwrap();
    if automatic {
        output.push_str("M5\n");
        let record = first_record.ok_or("ATC requires a library tool")?;
        writeln!(
            output,
            "T{} M6\nG43 H{}",
            record.post.number,
            record
                .post
                .length_offset
                .ok_or("ATC requires a length offset")?
        )
        .unwrap();
    }
    // With ATC, the configured M6 routine establishes safe changer positioning
    // before compensation is enabled. The CAM program does not guess its path.
    writeln!(output, "G0 Z{:.4}", project.travel_clearance()).unwrap();
    if automatic {
        // M6 may leave XY at the changer. Restore the validated path start even
        // when the planner omitted a zero-distance positioning move.
        let start = path.moves.first().ok_or("No generated moves")?.from;
        writeln!(output, "G0 X{:.4} Y{:.4}", start.x, start.y).unwrap();
    }
    writeln!(
        output,
        "{} S{}",
        if first_record.is_none_or(|t| t.clockwise) {
            "M3"
        } else {
            "M4"
        },
        first_tool.spindle_rpm
    )
    .unwrap();
    if initial_coolant == Coolant::Flood {
        output.push_str("M8\n");
    }
    let mut current_op = None;
    let mut current_tool = first_record.map(|t| t.id);
    let mut current_rpm = first_tool.spindle_rpm;
    let mut current_coolant = initial_coolant;
    for movement in &path.moves {
        if current_op != Some(movement.operation) {
            current_op = Some(movement.operation);
            let op = &project.operations[movement.operation];
            let tool = project.operation_tool(op)?;
            let record = project.library_tool(Some(op))?;
            let coolant = tool_coolant(project, op)?;
            let id = record.map(|t| t.id);
            if current_tool != id {
                let record = record.ok_or("Missing operation tool")?;
                writeln!(
                    output,
                    "G0 Z{:.4}\nM5\nM9\nT{} M6\nG43 H{}\nG0 Z{:.4}",
                    project.travel_clearance(),
                    record.post.number,
                    record
                        .post
                        .length_offset
                        .ok_or("ATC requires a length offset")?,
                    project.travel_clearance()
                )
                .unwrap();
                writeln!(output, "G0 X{:.4} Y{:.4}", movement.from.x, movement.from.y).unwrap();
                writeln!(
                    output,
                    "{} S{}",
                    if record.clockwise { "M3" } else { "M4" },
                    tool.spindle_rpm
                )
                .unwrap();
                if coolant == Coolant::Flood {
                    output.push_str("M8\n");
                }
            } else {
                if tool.spindle_rpm != current_rpm {
                    writeln!(
                        output,
                        "{} S{}",
                        if record.is_none_or(|t| t.clockwise) {
                            "M3"
                        } else {
                            "M4"
                        },
                        tool.spindle_rpm
                    )
                    .unwrap();
                }
                if coolant != current_coolant {
                    output.push_str(if coolant == Coolant::Flood {
                        "M8\n"
                    } else {
                        "M9\n"
                    });
                }
            }
            current_tool = id;
            current_rpm = tool.spindle_rpm;
            current_coolant = coolant;
            writeln!(
                output,
                "(Operation {}: {})",
                movement.operation + 1,
                comment(&op.name)
            )
            .unwrap();
        }
        match movement.motion {
            Motion::Rapid => {
                if (movement.from.z - movement.to.z).abs() > 1e-7 {
                    writeln!(output, "G0 Z{:.4}", movement.to.z).unwrap();
                } else {
                    writeln!(output, "G0 X{:.4} Y{:.4}", movement.to.x, movement.to.y).unwrap();
                }
            }
            motion => {
                let _ = motion;
                let feed = movement.feed;
                writeln!(
                    output,
                    "G1 X{:.4} Y{:.4} Z{:.4} F{:.4}",
                    movement.to.x, movement.to.y, movement.to.z, feed
                )
                .unwrap();
            }
        }
    }
    writeln!(
        output,
        "G0 Z{:.4}\nM5\nM9\n{}",
        project.travel_clearance(),
        if processor == PostProcessor::Generic {
            "M2"
        } else {
            "M30"
        }
    )
    .unwrap();
    if percent {
        output.push_str("%\n");
    }
    Ok(output)
}

fn tool_coolant(project: &Project, op: &crate::model::Operation) -> Result<Coolant, String> {
    if !project.post.use_tool_coolant || project.tool_library.tools.is_empty() {
        return Ok(project.post.coolant);
    }
    Ok(match project.library_tool(Some(op))? {
        Some(t) => t.preset(op.preset_id)?.coolant,
        None => Coolant::Off,
    })
}

pub fn validate_tool_changes(project: &Project) -> Result<(), String> {
    use crate::machine::ChangeMode;
    let automatic = project
        .machine
        .as_ref()
        .is_some_and(|m| m.tool_changer.mode == ChangeMode::MillingAtc);
    let mut used = std::collections::BTreeSet::new();
    let mut pockets = std::collections::BTreeSet::new();
    let mut numbers = std::collections::BTreeSet::new();
    let mut offsets = std::collections::BTreeSet::new();
    for op in project.operations.iter().filter(|op| op.enabled) {
        if let Some(tool) = project.library_tool(Some(op))? {
            if tool.post.mount_offset.iter().any(|v| v.abs() > 1e-9) {
                return Err(
                    "Nonzero head mounting offsets require a supported kinematics configuration"
                        .into(),
                );
            }
            if tool.post.break_control {
                return Err("Tool break-control cycles are not implemented for this post".into());
            }
            if automatic && used.insert(tool.id) {
                let machine = project.machine.as_ref().ok_or("ATC requires a machine")?;
                if !matches!(
                    project.post.processor,
                    PostProcessor::Mach3 | PostProcessor::LinuxCnc
                ) {
                    return Err(
                        "This post does not support ATC; choose Mach3 Mill or LinuxCNC".into(),
                    );
                }
                if !machine.tool_changer.controller_managed {
                    return Err(
                        "Configure a controller-managed safe M6 routine before ATC export".into(),
                    );
                }
                let pocket = tool
                    .post
                    .pocket
                    .ok_or("Every ATC tool requires a pocket assignment")?;
                if pocket > machine.tool_changer.capacity
                    || !pockets.insert(pocket)
                    || !numbers.insert(tool.post.number)
                {
                    return Err(
                        "ATC pocket/tool assignments conflict or exceed machine capacity".into(),
                    );
                }
                let offset = tool
                    .post
                    .length_offset
                    .ok_or("Every ATC tool requires a calibrated length-offset register")?;
                if !offsets.insert(offset) {
                    return Err(
                        "Different ATC tools require separate calibrated length-offset registers"
                            .into(),
                    );
                }
                if tool.post.manual_change {
                    return Err(
                        "A manual-change tool cannot be used in automatic tool-change mode".into(),
                    );
                }
            } else {
                used.insert(tool.id);
            }
        } else if automatic {
            return Err("ATC requires library tool assignments".into());
        }
    }
    if used.len() > 1 && !automatic {
        return Err("Multiple tools require a configured ATC and compatible post; mounted-tool mode supports one tool".into());
    }
    Ok(())
}

fn comment(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " ._- /".contains(*c))
        .take(32)
        .collect()
}
