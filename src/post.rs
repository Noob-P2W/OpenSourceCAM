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
    let path = checked_geometry_path(project)?;
    validate_tool_changes(project)?;
    validate_manual_routes(project, &path)?;
    Ok(path)
}

/// Path planning/simulation does not require an export tool-change configuration.
/// Geometry, setup, tool capability and fixture preflight remain mandatory.
pub fn checked_geometry_path(project: &Project) -> Result<crate::cam::Toolpath, String> {
    let path = generate(project)?;
    crate::collision::check(project, &path)?;
    Ok(path)
}
fn manual(project: &Project) -> bool {
    let mut tools = std::collections::BTreeSet::new();
    for op in project.operations.iter().filter(|o| o.enabled) {
        tools.insert(op.tool_id.or(project.active_tool));
    }
    let mode = project
        .machine
        .as_ref()
        .map_or(crate::machine::ChangeMode::MountedTool, |m| {
            m.tool_changer.mode
        });
    mode == crate::machine::ChangeMode::Manual
        || (mode == crate::machine::ChangeMode::MountedTool && tools.len() > 1)
}
fn manual_change(
    output: &mut String,
    project: &Project,
    op: &crate::model::Operation,
) -> Result<(), String> {
    let record = project
        .library_tool(Some(op))?
        .ok_or("Manual tool changes require library tools")?;
    let p = project
        .machine
        .as_ref()
        .and_then(|m| m.tool_changer.manual_position)
        .ok_or("Configure a manual tool-change position in Setup / Machine")?;
    writeln!(
        output,
        "G0 Z{:.4}\nM5\nM9\nG0 Z{:.4}\nG0 X{:.4} Y{:.4}",
        project.travel_clearance(),
        p[2],
        p[0],
        p[1]
    )
    .unwrap();
    writeln!(
        output,
        "(MANUAL CHANGE - T{} {} - calibrated H{})",
        record.post.number,
        comment(&record.name),
        record
            .post
            .length_offset
            .ok_or("Assign a calibrated H length-offset register to every manual-change tool")?
    )
    .unwrap();
    if project.post.processor == PostProcessor::LinuxCnc {
        // LinuxCNC's configured hal_manualtoolchange provides the operator handshake.
        writeln!(output, "T{} M6", record.post.number).unwrap();
    } else {
        // M0 is mandatory; M1 can be disabled by the controller's optional-stop switch.
        writeln!(output, "T{}\nM0", record.post.number).unwrap();
    }
    writeln!(
        output,
        "G21\nG90\nG17\nG94\n{}",
        project.post.work_offset.code()
    )
    .unwrap();
    writeln!(
        output,
        "G43 H{}\nG0 Z{:.4}",
        record.post.length_offset.unwrap(),
        project.travel_clearance()
    )
    .unwrap();
    Ok(())
}
fn validate_manual_routes(project: &Project, path: &crate::cam::Toolpath) -> Result<(), String> {
    if !manual(project) {
        return Ok(());
    }
    use crate::cam::{Motion, Move, Point3};
    let machine = project
        .machine
        .as_ref()
        .ok_or("Manual changes need a machine setup")?;
    let p = machine
        .tool_changer
        .manual_position
        .ok_or("Configure a manual tool-change position in Setup / Machine")?;
    let position = Point3 {
        x: p[0],
        y: p[1],
        z: p[2],
    };
    if position.z < project.travel_clearance() {
        return Err("Manual tool-change Z must be at or above setup travel clearance".into());
    }
    let mut routes = crate::cam::Toolpath {
        moves: Vec::new(),
        cut_length: 0.,
        rapid_length: 0.,
        estimated_seconds: 0.,
    };
    let mut last = None;
    let mut previous_index = None;
    for movement in &path.moves {
        let id = project
            .library_tool(Some(&project.operations[movement.operation]))?
            .map(|t| t.id);
        if id != last && previous_index.is_some() {
            let start = Point3 {
                x: movement.from.x,
                y: movement.from.y,
                z: project.travel_clearance(),
            };
            let high = Point3 {
                z: position.z,
                ..start
            };
            let old = previous_index.unwrap_or(movement.operation);
            let low = Point3 {
                z: project.travel_clearance(),
                ..position
            };
            for (from, to, operation) in [
                (start, high, old),
                (high, position, old),
                (position, low, movement.operation),
                (low, start, movement.operation),
            ] {
                if from.distance(to) <= 1e-9 {
                    continue;
                }
                let m = Move {
                    from,
                    to,
                    motion: Motion::Rapid,
                    feed: machine.rapid_feed,
                    operation,
                };
                machine.check_point(from)?;
                machine.check_point(to)?;
                machine.check_move(&m)?;
                routes.moves.push(m);
            }
        }
        last = id;
        previous_index = Some(movement.operation);
    }
    crate::collision::check(project, &routes)
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
    let manual = manual(project);
    output.push_str(if automatic {
        "(Metric - stock top Z0 - controller-managed ATC)\n"
    } else if manual {
        "(Metric - stock top Z0 - manual changes with calibrated tool lengths)\n"
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
    if manual {
        let record = first_record.ok_or("Manual changes require library tools")?;
        writeln!(
            output,
            "(START WITH T{} ALREADY MOUNTED AND CALIBRATED)\nM5\nT{}\nG43 H{}",
            record.post.number,
            record.post.number,
            record.post.length_offset.unwrap()
        )
        .unwrap();
    }
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
                if manual {
                    manual_change(&mut output, project, op)?;
                } else {
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
                }
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
    if manual(project) {
        if project.post.processor == PostProcessor::Grbl {
            return Err("GRBL has no M6 or calibrated H registers. Plan/simulate multiple tools here; export one tool per file for a manual change and re-zero between programs".into());
        }
        let machine = project
            .machine
            .as_ref()
            .ok_or("Manual changes require machine setup")?;
        if project.post.processor == PostProcessor::LinuxCnc
            && !machine.tool_changer.controller_managed
        {
            return Err("Enable the configured LinuxCNC manual M6 / hal_manualtoolchange handshake in Setup / Machine; no ATC is required".into());
        }
        if machine.tool_changer.manual_position.is_none() {
            return Err("Configure a manual tool-change position in Setup / Machine".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut numbers = std::collections::BTreeSet::new();
        let mut registers = std::collections::BTreeSet::new();
        for op in project.operations.iter().filter(|o| o.enabled) {
            let tool = project
                .library_tool(Some(op))?
                .ok_or("Manual changes require library tool assignments")?;
            if ids.insert(tool.id) {
                if !numbers.insert(tool.post.number) {
                    return Err("Different manual-change tools need unique tool numbers".into());
                }
                let h=tool.post.length_offset.ok_or("Assign a calibrated H length-offset register to each manual-change tool in the tool library")?;
                if !registers.insert(h) {
                    return Err(
                        "Different manual-change tools require separate calibrated H registers"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn comment(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " ._- /".contains(*c))
        .take(32)
        .collect()
}
