//! Editable generic machine definitions. Limits refer to the tool tip, not fixtures.
use crate::{cam::Point3, model::Project, process::Process};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutterKind {
    #[default]
    EndMill,
    Drill,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Tooling {
    Milling {
        kind: CutterKind,
        flute_length: f64,
        overall_length: f64,
        stickout: f64,
        shank_diameter: f64,
    },
    Turning {
        nose_radius: f64,
        insert_angle: f64,
        holder_width: f64,
        max_rpm: f64,
    },
    Printing {
        nozzle_diameter: f64,
        filament_diameter: f64,
        layer_height: f64,
        nozzle_temperature: f64,
        bed_temperature: f64,
    },
    Laser {
        power_watts: f64,
        spot_diameter: f64,
        focal_length: f64,
    },
    Plasma {
        current_amps: f64,
        kerf_width: f64,
        cut_height: f64,
        pierce_height: f64,
    },
    Grinding {
        wheel_diameter: f64,
        wheel_width: f64,
        bore_diameter: f64,
        max_rpm: f64,
    },
    WireEdm {
        wire_diameter: f64,
        spark_gap: f64,
        taper_angle: f64,
    },
    SinkerEdm {
        electrode_width: f64,
        electrode_depth: f64,
        electrode_height: f64,
        spark_gap: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineSetup {
    #[serde(default)]
    pub configuration: Configuration,
    #[serde(default)]
    pub tool_changer: ToolChanger,
    pub name: String,
    pub tool_name: String,
    /// Tool-tip travel bounds in machine coordinates, in mm.
    pub axis_min: [f64; 3],
    pub axis_max: [f64; 3],
    /// Machine coordinates of stock lower-left/top (work XYZ=0).
    pub work_origin: [f64; 3],
    pub enforce_limits: bool,
    pub check_tool_reach: bool,
    pub rapid_feed: f64,
    pub max_feed: f64,
    pub max_spindle_rpm: u32,
    pub tooling: Tooling,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub controller: String,
    pub axis_feed: [f64; 3],
    pub axis_acceleration: [f64; 3],
    pub home: [Option<f64>; 3],
    pub minimum_rpm: u32,
    pub reverse_spindle: bool,
    pub table_size: [f64; 2],
    pub maximum_load_kg: Option<f64>,
    pub spindle_power_kw: Option<f64>,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            manufacturer: String::new(),
            model: String::new(),
            serial: String::new(),
            controller: String::new(),
            axis_feed: [1_000_000.0; 3],
            axis_acceleration: [500.0; 3],
            home: [None; 3],
            minimum_rpm: 0,
            reverse_spindle: true,
            table_size: [500.0, 400.0],
            maximum_load_kg: None,
            spindle_power_kw: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeMode {
    #[default]
    MountedTool,
    MillingAtc,
    TurningTurret,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolChanger {
    pub mode: ChangeMode,
    pub capacity: u32,
    pub controller_managed: bool,
}
impl Default for ToolChanger {
    fn default() -> Self {
        Self {
            mode: ChangeMode::MountedTool,
            capacity: 20,
            controller_managed: false,
        }
    }
}

/// Portable machine/tool preset. Stock, operations and controller selection stay
/// with the project. Versioning is independent of the project file schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupProfile {
    pub format_version: u32,
    pub process: Process,
    pub machine: MachineSetup,
    pub milling_tool: Option<crate::model::Tool>,
}

impl SetupProfile {
    pub fn from_project(project: &Project) -> Result<Self, String> {
        let profile = Self {
            format_version: if project.tool_library.tools.is_empty() {
                1
            } else {
                3
            },
            process: project.process,
            machine: project.machine.clone().ok_or("Mixed setup is planned")?,
            milling_tool: if project.process == Process::Milling
                && project.tool_library.tools.is_empty()
            {
                Some(project.tool.clone())
            } else {
                None
            },
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn apply(&self, project: &mut Project) -> Result<(), String> {
        if self.process != project.process {
            return Err("Machine profile belongs to a different process".into());
        }
        self.validate()?;
        project.machine = Some(self.machine.clone());
        if let Some(tool) = &self.milling_tool {
            project.tool = tool.clone();
        }
        if self.format_version == 1 && !project.tool_library.tools.is_empty() {
            let old = project.active_tool;
            let mut bridge = Project::new_for_process(project.process);
            bridge.machine = Some(self.machine.clone());
            bridge.tool = project.tool.clone();
            bridge.post = project.post.clone();
            if let Some(tool) = project.library_tool(None)? {
                bridge.post.coolant = tool.preset(project.active_preset)?.coolant;
            }
            bridge.ensure_tool_library();
            let mut imported = bridge.tool_library.tools.remove(0);
            imported.id = project.tool_library.next_id();
            imported.post.number = project.tool_library.next_number(imported.kind.process());
            let id = imported.id;
            project.tool_library.tools.push(imported);
            project.active_tool = Some(id);
            project.active_preset = Some(1);
            for op in &mut project.operations {
                if op.tool_id == old || op.tool_id.is_none() {
                    op.tool_id = Some(id);
                    op.preset_id = None;
                }
            }
        }
        // Geometry may require later edits (for example, smaller stock or travel).
        // Applying a valid setup regenerates and blocks export when it will not fit.
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if !matches!(self.format_version, 1..=3) {
            return Err("Unsupported machine profile version".into());
        }
        if self.format_version >= 2 {
            if self.milling_tool.is_some() {
                return Err("Machine profiles store tools in the separate tool library".into());
            }
            return self.machine.validate(self.process);
        }
        if self.milling_tool.is_some() != (self.process == Process::Milling) {
            return Err("Machine profile has missing or incompatible cutting data".into());
        }
        // Validate the reusable definition independently of the current job's
        // geometry, reach and operation choices. Regeneration validates the job.
        let mut setup = Project::new_for_process(self.process);
        setup.machine = Some(self.machine.clone());
        if let Some(tool) = &self.milling_tool {
            setup.tool = tool.clone();
        }
        crate::cam::validate_setup(&setup)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if text.len() + 1 > 128 * 1024 {
            return Err("Machine profile exceeds 128 KiB".into());
        }
        crate::files::write_atomic(path, (text + "\n").as_bytes()).map_err(|e| e.to_string())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 128 * 1024 {
            return Err("Machine profile exceeds 128 KiB".into());
        }
        let profile: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        profile.validate()?;
        Ok(profile)
    }
}

fn range(value: f64, min: f64, max: f64, name: &str) -> Result<(), String> {
    if !value.is_finite() || value < min || value > max {
        Err(format!("{name} must be finite and between {min} and {max}"))
    } else {
        Ok(())
    }
}

impl Tooling {
    pub fn process(&self) -> Process {
        match self {
            Self::Milling { .. } => Process::Milling,
            Self::Turning { .. } => Process::Turning,
            Self::Printing { .. } => Process::Printing3d,
            Self::Laser { .. } => Process::LaserCutting,
            Self::Plasma { .. } => Process::PlasmaCutting,
            Self::Grinding { .. } => Process::Grinding,
            Self::WireEdm { .. } => Process::WireEdm,
            Self::SinkerEdm { .. } => Process::SinkerEdm,
        }
    }

    fn validate(&self) -> Result<(), String> {
        let positive = |v, name| range(v, 0.001, 100_000.0, name);
        match *self {
            Self::Milling {
                flute_length,
                overall_length,
                stickout,
                shank_diameter,
                ..
            } => {
                for (v, n) in [
                    (flute_length, "Flute length"),
                    (overall_length, "Tool length"),
                    (stickout, "Stickout"),
                    (shank_diameter, "Shank diameter"),
                ] {
                    positive(v, n)?;
                }
                if flute_length > stickout || stickout > overall_length {
                    return Err(
                        "Tool lengths must satisfy flute length <= stickout <= overall length"
                            .into(),
                    );
                }
            }
            Self::Turning {
                nose_radius,
                insert_angle,
                holder_width,
                max_rpm,
            } => {
                positive(nose_radius, "Insert nose radius")?;
                range(insert_angle, 1.0, 179.0, "Insert angle")?;
                positive(holder_width, "Holder width")?;
                positive(max_rpm, "Tool maximum RPM")?;
            }
            Self::Printing {
                nozzle_diameter,
                filament_diameter,
                layer_height,
                nozzle_temperature,
                bed_temperature,
            } => {
                positive(nozzle_diameter, "Nozzle diameter")?;
                positive(filament_diameter, "Filament diameter")?;
                range(layer_height, 0.001, nozzle_diameter, "Layer height")?;
                range(nozzle_temperature, 0.0, 500.0, "Nozzle temperature")?;
                range(bed_temperature, 0.0, 200.0, "Bed temperature")?;
            }
            Self::Laser {
                power_watts,
                spot_diameter,
                focal_length,
            } => {
                positive(power_watts, "Laser power")?;
                positive(spot_diameter, "Spot diameter")?;
                positive(focal_length, "Focal length")?;
            }
            Self::Plasma {
                current_amps,
                kerf_width,
                cut_height,
                pierce_height,
            } => {
                positive(current_amps, "Plasma current")?;
                positive(kerf_width, "Kerf width")?;
                positive(cut_height, "Cut height")?;
                range(pierce_height, cut_height, 100_000.0, "Pierce height")?;
            }
            Self::Grinding {
                wheel_diameter,
                wheel_width,
                bore_diameter,
                max_rpm,
            } => {
                positive(wheel_diameter, "Wheel diameter")?;
                positive(wheel_width, "Wheel width")?;
                positive(bore_diameter, "Wheel bore")?;
                positive(max_rpm, "Wheel maximum RPM")?;
                if bore_diameter >= wheel_diameter {
                    return Err("Wheel bore must be smaller than wheel diameter".into());
                }
            }
            Self::WireEdm {
                wire_diameter,
                spark_gap,
                taper_angle,
            } => {
                positive(wire_diameter, "Wire diameter")?;
                range(spark_gap, 0.0, 10.0, "Spark gap")?;
                range(taper_angle, -45.0, 45.0, "Taper angle")?;
            }
            Self::SinkerEdm {
                electrode_width,
                electrode_depth,
                electrode_height,
                spark_gap,
            } => {
                positive(electrode_width, "Electrode width")?;
                positive(electrode_depth, "Electrode depth")?;
                positive(electrode_height, "Electrode height")?;
                range(spark_gap, 0.0, 10.0, "Spark gap")?;
            }
        }
        Ok(())
    }
}

impl MachineSetup {
    /// Generic editable starting points, not verified manufacturer presets.
    pub fn for_process(process: Process) -> Option<Self> {
        let tooling = match process {
            Process::Milling => Tooling::Milling {
                kind: CutterKind::EndMill,
                flute_length: 25.0,
                overall_length: 60.0,
                stickout: 35.0,
                shank_diameter: 6.0,
            },
            Process::Turning => Tooling::Turning {
                nose_radius: 0.4,
                insert_angle: 80.0,
                holder_width: 12.0,
                max_rpm: 3000.0,
            },
            Process::Printing3d => Tooling::Printing {
                nozzle_diameter: 0.4,
                filament_diameter: 1.75,
                layer_height: 0.2,
                nozzle_temperature: 200.0,
                bed_temperature: 60.0,
            },
            Process::LaserCutting => Tooling::Laser {
                power_watts: 10.0,
                spot_diameter: 0.1,
                focal_length: 50.0,
            },
            Process::PlasmaCutting => Tooling::Plasma {
                current_amps: 40.0,
                kerf_width: 1.2,
                cut_height: 1.5,
                pierce_height: 3.0,
            },
            Process::Grinding => Tooling::Grinding {
                wheel_diameter: 150.0,
                wheel_width: 20.0,
                bore_diameter: 32.0,
                max_rpm: 3000.0,
            },
            Process::WireEdm => Tooling::WireEdm {
                wire_diameter: 0.25,
                spark_gap: 0.02,
                taper_angle: 0.0,
            },
            Process::SinkerEdm => Tooling::SinkerEdm {
                electrode_width: 10.0,
                electrode_depth: 10.0,
                electrode_height: 30.0,
                spark_gap: 0.05,
            },
            Process::Mixed => return None,
        };
        Some(Self {
            configuration: Configuration::default(),
            tool_changer: ToolChanger::default(),
            name: format!("Generic {} machine", process.label().to_lowercase()),
            tool_name: format!("{} tool 1", process.label()),
            axis_min: [0.0, 0.0, -200.0],
            axis_max: [500.0, 400.0, 100.0],
            work_origin: [0.0; 3],
            enforce_limits: false,
            check_tool_reach: true,
            rapid_feed: 3000.0,
            max_feed: 100_000.0,
            max_spindle_rpm: 100_000,
            tooling,
        })
    }

    pub fn validate(&self, process: Process) -> Result<(), String> {
        let config = &self.configuration;
        for text in [
            &config.manufacturer,
            &config.model,
            &config.serial,
            &config.controller,
        ] {
            if text.len() > 4096 {
                return Err("Machine metadata is too long".into());
            }
        }
        for axis in 0..3 {
            range(config.axis_feed[axis], 0.001, 1e6, "Axis maximum feed")?;
            range(
                config.axis_acceleration[axis],
                0.001,
                1e6,
                "Axis acceleration",
            )?;
            if let Some(home) = config.home[axis] {
                range(
                    home,
                    self.axis_min[axis],
                    self.axis_max[axis],
                    "Axis home reference",
                )?;
            }
        }
        if config.minimum_rpm > self.max_spindle_rpm {
            return Err("Minimum spindle RPM exceeds maximum".into());
        }
        for size in config.table_size {
            range(size, 0.001, 1e6, "Table size")?;
        }
        for value in [config.maximum_load_kg, config.spindle_power_kw]
            .into_iter()
            .flatten()
        {
            range(value, 0.001, 1e6, "Machine rating")?;
        }
        if !(1..=9999).contains(&self.tool_changer.capacity) {
            return Err("Tool changer capacity must be between 1 and 9999".into());
        }
        if (self.tool_changer.mode == ChangeMode::MillingAtc && process != Process::Milling)
            || (self.tool_changer.mode == ChangeMode::TurningTurret && process != Process::Turning)
        {
            return Err("Tool changer type does not match the machine process".into());
        }
        if self.tooling.process() != process {
            return Err("Machine/tooling definition does not match the project process".into());
        }
        for axis in 0..3 {
            range(
                self.axis_min[axis],
                -1_000_000.0,
                1_000_000.0,
                "Axis minimum",
            )?;
            range(
                self.axis_max[axis],
                -1_000_000.0,
                1_000_000.0,
                "Axis maximum",
            )?;
            range(
                self.work_origin[axis],
                -1_000_000.0,
                1_000_000.0,
                "Work origin",
            )?;
            if self.axis_min[axis] >= self.axis_max[axis] {
                return Err("Axis minimum must be below maximum".into());
            }
        }
        range(self.rapid_feed, 0.001, 1_000_000.0, "Rapid feed")?;
        range(self.max_feed, 0.001, 1_000_000.0, "Maximum cutting feed")?;
        if !(1..=1_000_000).contains(&self.max_spindle_rpm) {
            return Err("Maximum spindle RPM must be between 1 and 1,000,000".into());
        }
        self.tooling.validate()
    }

    pub fn check_point(&self, point: Point3) -> Result<(), String> {
        if !self.enforce_limits {
            return Ok(());
        }
        for (axis, coordinate) in [point.x, point.y, point.z].into_iter().enumerate() {
            let machine = coordinate + self.work_origin[axis];
            // Use exactly the post's decimal conversion. Multiplying and using
            // round() differs at ties, e.g. 0.03125 exports as 0.0312, not 0.0313.
            let posted = format!("{coordinate:.4}")
                .parse::<f64>()
                .unwrap_or(f64::NAN)
                + self.work_origin[axis];
            if [machine, posted].into_iter().any(|position| {
                !position.is_finite()
                    || position < self.axis_min[axis] - 1e-7
                    || position > self.axis_max[axis] + 1e-7
            }) {
                return Err(format!(
                    "Tool-tip {} travel exceeds machine limits: {machine:.4} mm ({} to {})",
                    ["X", "Y", "Z"][axis],
                    self.axis_min[axis],
                    self.axis_max[axis]
                ));
            }
        }
        Ok(())
    }

    pub fn check_move(&self, movement: &crate::cam::Move) -> Result<(), String> {
        if movement.motion == crate::cam::Motion::Rapid {
            return Ok(());
        }
        let posted_feed = format!("{:.4}", movement.feed)
            .parse::<f64>()
            .unwrap_or(f64::NAN);
        let feed = movement.feed.max(posted_feed);
        if feed > self.max_feed + 1e-7 {
            return Err("Operation feed exceeds machine maximum".into());
        }
        if self
            .configuration
            .axis_feed
            .iter()
            .all(|limit| *limit + 1e-7 >= feed)
        {
            return Ok(());
        }
        for posted in [false, true] {
            let delta = [
                movement.to.x - movement.from.x,
                movement.to.y - movement.from.y,
                movement.to.z - movement.from.z,
            ];
            let delta = if posted {
                let a = [movement.from.x, movement.from.y, movement.from.z];
                let b = [movement.to.x, movement.to.y, movement.to.z];
                std::array::from_fn(|i| {
                    format!("{:.4}", b[i]).parse::<f64>().unwrap_or(f64::NAN)
                        - format!("{:.4}", a[i]).parse::<f64>().unwrap_or(f64::NAN)
                })
            } else {
                delta
            };
            let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
            if length > 1e-9 {
                for (axis, d) in delta.into_iter().enumerate() {
                    if feed * d.abs() / length > self.configuration.axis_feed[axis] + 1e-7 {
                        return Err(format!(
                            "{} axis feed exceeds its configured limit",
                            ["X", "Y", "Z"][axis]
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn validate_project(project: &Project) -> Result<(), String> {
    if project.process == Process::Mixed {
        return if project.machine.is_none() {
            Ok(())
        } else {
            Err("Mixed machine setup will be developed last".into())
        };
    }
    let machine = project
        .machine
        .as_ref()
        .ok_or("Machine setup is required")?;
    machine.validate(project.process)?;
    if machine.tool_changer.mode == ChangeMode::TurningTurret {
        let tool = project
            .library_tool(None)?
            .ok_or("Turning turret requires a library tool")?;
        let station = tool
            .post
            .pocket
            .ok_or("Active turning tool requires a turret station")?;
        if station > machine.tool_changer.capacity {
            return Err("Turning tool station exceeds turret capacity".into());
        }
        tool.post
            .turning_offset
            .ok_or("Active turning tool requires a turning offset register")?;
    }
    if project.process == Process::Milling {
        for op in project.operations.iter().filter(|op| op.enabled) {
            let tool = project.operation_tool(op)?;
            if tool.spindle_rpm > machine.max_spindle_rpm {
                return Err("Spindle speed exceeds machine maximum RPM".into());
            }
            if tool.spindle_rpm < machine.configuration.minimum_rpm {
                return Err("Spindle speed is below machine minimum RPM".into());
            }
        }
        if !project.tool_library.tools.is_empty() {
            for op in project.operations.iter().filter(|op| op.enabled) {
                let tool = project.operation_tool(op)?;
                if tool.feed > machine.max_feed || tool.plunge > machine.max_feed {
                    return Err("Cut/plunge feed exceeds machine maximum feed".into());
                }
                if tool.spindle_rpm > machine.max_spindle_rpm {
                    return Err("Spindle speed exceeds machine maximum RPM".into());
                }
                if tool.spindle_rpm < machine.configuration.minimum_rpm {
                    return Err("Operation spindle RPM is below the machine minimum".into());
                }
                if let Some(record) = project.library_tool(Some(op))? {
                    if !machine.configuration.reverse_spindle && !record.clockwise {
                        return Err(
                            "Machine configuration does not support reverse spindle rotation"
                                .into(),
                        );
                    }
                    if machine.check_tool_reach
                        && (op.depth - op.settings.axial_allowance
                            > record.dimension("flute_length")
                            || op.depth - op.settings.axial_allowance
                                > record.dimension("stickout"))
                    {
                        return Err("Operation depth exceeds flute length or tool stickout".into());
                    }
                }
            }
            return Ok(());
        }
        if project.tool.feed > machine.max_feed || project.tool.plunge > machine.max_feed {
            return Err("Cut/plunge feed exceeds machine maximum feed".into());
        }
        if project.tool.spindle_rpm > machine.max_spindle_rpm {
            return Err("Spindle speed exceeds machine maximum RPM".into());
        }
        if let Tooling::Milling {
            kind,
            flute_length,
            stickout,
            ..
        } = machine.tooling
        {
            for op in project.operations.iter().filter(|op| op.enabled) {
                if machine.check_tool_reach && (op.depth > flute_length || op.depth > stickout) {
                    return Err("Operation depth exceeds flute length or tool stickout".into());
                }
                if kind == CutterKind::Drill && op.strategy != crate::model::Strategy::Drill {
                    return Err("A drill tool can only be used for drilling operations".into());
                }
            }
        }
    }
    Ok(())
}
