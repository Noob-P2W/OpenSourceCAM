use crate::import::Part;
use crate::post::PostSettings;
use crate::process::Process;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FORMAT_VERSION: u32 = 11;
pub const MAX_PROJECT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Stock {
    #[serde(default)]
    pub profile: crate::blank::StockProfile,
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub diameter: f64,
    pub feed: f64,
    pub plunge: f64,
    pub spindle_rpm: u32,
    pub step_down: f64,
    /// Fraction of tool diameter, in (0, 1].
    pub step_over: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    Pocket,
    OutsideProfile,
    InsideProfile,
    Drill,
    DrillOnce,
    Facing,
    CircularPocket,
    StockRough,
    SurfaceRough,
    ParallelFinish,
}

impl Strategy {
    pub const ALL: [Self; 10] = [
        Self::Pocket,
        Self::OutsideProfile,
        Self::InsideProfile,
        Self::Drill,
        Self::DrillOnce,
        Self::Facing,
        Self::CircularPocket,
        Self::StockRough,
        Self::SurfaceRough,
        Self::ParallelFinish,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Pocket => "Rectangular pocket",
            Self::OutsideProfile => "Outside profile",
            Self::InsideProfile => "Inside profile",
            Self::Drill => "Peck drill",
            Self::DrillOnce => "Drill (continuous feed)",
            Self::Facing => "Stock facing",
            Self::CircularPocket => "Circular pocket / bore",
            Self::StockRough => "Clear stock outside part",
            Self::SurfaceRough => "3D raster roughing",
            Self::ParallelFinish => "3D parallel finishing",
        }
    }
    pub fn is_drilling(self) -> bool {
        matches!(self, Self::Drill | Self::DrillOnce)
    }
    pub fn is_surface(self) -> bool {
        matches!(self, Self::SurfaceRough | Self::ParallelFinish)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    #[serde(default)]
    pub model_link: Option<crate::machining::ModelLink>,
    #[serde(default)]
    pub settings: crate::operation::Settings,
    #[serde(default)]
    pub tool_id: Option<u64>,
    #[serde(default)]
    pub preset_id: Option<u64>,
    pub name: String,
    pub enabled: bool,
    pub strategy: Strategy,
    /// Lower-left corner of a rectangle, or the hole center for drilling.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Positive distance below stock top Z=0.
    pub depth: f64,
}

impl Default for Operation {
    fn default() -> Self {
        Self {
            model_link: None,
            settings: Default::default(),
            tool_id: None,
            preset_id: None,
            name: "Pocket".into(),
            enabled: true,
            strategy: Strategy::Pocket,
            x: 20.0,
            y: 18.0,
            width: 60.0,
            height: 40.0,
            depth: 3.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    #[serde(default)]
    pub workspace: Vec<crate::workspace::WorkspaceModel>,
    #[serde(default)]
    pub tool_library: crate::tool_library::ToolLibrary,
    #[serde(default)]
    pub active_tool: Option<u64>,
    #[serde(default)]
    pub active_preset: Option<u64>,
    pub format_version: u32,
    pub name: String,
    pub stock: Stock,
    #[serde(default)]
    pub process: Process,
    #[serde(default)]
    pub post: PostSettings,
    #[serde(default)]
    pub machine: Option<crate::machine::MachineSetup>,
    pub tool: Tool,
    pub clearance: f64,
    pub operations: Vec<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<Part>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            workspace: Vec::new(),
            tool_library: Default::default(),
            active_tool: None,
            active_preset: None,
            format_version: FORMAT_VERSION,
            name: "Aluminium tray".into(),
            process: Process::Milling,
            post: PostSettings::default(),
            machine: crate::machine::MachineSetup::for_process(Process::Milling),
            stock: Stock {
                profile: Default::default(),
                width: 120.0,
                height: 80.0,
                thickness: 12.0,
            },
            tool: Tool {
                name: "6 mm flat end mill".into(),
                diameter: 6.0,
                feed: 600.0,
                plunge: 120.0,
                spindle_rpm: 12000,
                step_down: 1.0,
                step_over: 0.4,
            },
            clearance: 5.0,
            operations: vec![
                Operation::default(),
                Operation {
                    model_link: None,
                    settings: Default::default(),
                    name: "Perimeter finish".into(),
                    strategy: Strategy::OutsideProfile,
                    x: 10.0,
                    y: 8.0,
                    width: 100.0,
                    height: 64.0,
                    depth: 2.0,
                    enabled: true,
                    tool_id: None,
                    preset_id: None,
                },
            ],
            part: None,
        }
    }
}

impl Project {
    pub fn library_tool(
        &self,
        op: Option<&Operation>,
    ) -> Result<Option<&crate::tool_library::LibraryTool>, String> {
        let id = op.and_then(|op| op.tool_id).or(self.active_tool);
        if self.tool_library.tools.is_empty() {
            return if id.is_none() {
                Ok(None)
            } else {
                Err("Operation references a missing library tool".into())
            };
        }
        let tool = self
            .tool_library
            .get(id.ok_or("Select an active library tool")?)?;
        if !tool.uses.contains(&self.process) {
            return Err(format!(
                "{} is not assigned to {}",
                tool.name,
                self.process.label()
            ));
        }
        Ok(Some(tool))
    }

    pub fn operation_tool(&self, op: &Operation) -> Result<Tool, String> {
        let mut resolved = if let Some(tool) = self.library_tool(Some(op))? {
            use crate::tool_library::ToolKind;
            if !matches!(
                tool.kind,
                ToolKind::FlatEndMill
                    | ToolKind::Drill
                    | ToolKind::FaceMill
                    | ToolKind::BallEndMill
            ) {
                return Err(format!(
                    "{} toolpaths are not implemented; the definition can be stored in the library",
                    tool.kind.label()
                ));
            }
            if op.strategy.is_drilling() && tool.kind != ToolKind::Drill {
                return Err("Select a drill tool definition for drilling operations".into());
            }
            if tool.kind == ToolKind::Drill && !op.strategy.is_drilling() {
                return Err("A drill tool can only be used for drilling operations".into());
            }
            if tool.kind == ToolKind::BallEndMill && !op.strategy.is_surface() {
                return Err(
                    "Ball end mills currently require a 3D roughing or finishing operation".into(),
                );
            }
            if tool.kind == ToolKind::FaceMill && op.strategy != Strategy::Facing {
                return Err("A face mill is currently supported only for stock facing".into());
            }
            if op.model_link.is_some()
                && op.depth - op.settings.axial_allowance
                    > tool
                        .dimension("flute_length")
                        .min(tool.dimension("stickout"))
                        + 1e-7
            {
                return Err("Model-linked machining depth exceeds the tool's defined flute length or stickout; select a suitable measured tool".into());
            }
            if !tool.center_cutting
                && !matches!(op.strategy, Strategy::Facing | Strategy::StockRough)
            {
                return Err("Vertical entry requires a center-cutting tool".into());
            }
            let preset = tool.preset(op.preset_id)?;
            if preset.values.get("dwell").is_some_and(|v| *v > 0.0) {
                return Err("Dwell cycles are not implemented for this strategy".into());
            }
            if !preset.strategies.is_empty() && !preset.strategies.contains(&op.strategy) {
                return Err("Cutting preset is not assigned to this strategy".into());
            }
            tool.cutting_tool(op.preset_id)?
        } else {
            self.tool.clone()
        };
        op.settings.validate()?;
        if let Some(v) = op.settings.feed {
            resolved.feed = v;
        }
        if let Some(v) = op.settings.plunge {
            resolved.plunge = v;
        }
        if let Some(v) = op.settings.spindle_rpm {
            if op.settings.feed.is_none() {
                if let Some(tool) = self.library_tool(Some(op))? {
                    if tool.preset(op.preset_id)?.feed_mode
                        != crate::tool_library::FeedMode::PerMinute
                    {
                        resolved.feed *= v as f64 / resolved.spindle_rpm as f64;
                    }
                }
            }
            resolved.spindle_rpm = v;
        }
        if let Some(v) = op.settings.step_down {
            resolved.step_down = v;
        }
        if let Some(v) = op.settings.step_over_percent {
            resolved.step_over = v / 100.0;
        }
        if let Some(tool) = self.library_tool(Some(op))? {
            if resolved.spindle_rpm as f64 > tool.dimension("maximum_rpm") {
                return Err("Operation spindle override exceeds the tool's rated RPM".into());
            }
        }
        // A per-revolution drilling preset must drive axial feed too.
        // Explicit plunge overrides retain precedence over derived feed.
        if op.strategy.is_drilling() && op.settings.plunge.is_none() {
            if let Some(tool) = self.library_tool(Some(op))? {
                if tool.kind == crate::tool_library::ToolKind::Drill
                    && tool.preset(op.preset_id)?.feed_mode
                        != crate::tool_library::FeedMode::PerMinute
                {
                    resolved.plunge = resolved.feed;
                }
            }
        }
        Ok(resolved)
    }

    pub fn travel_clearance(&self) -> f64 {
        self.operations
            .iter()
            .filter(|op| op.enabled)
            .filter_map(|op| op.settings.clearance)
            .fold(self.clearance, f64::max)
    }

    /// Bridge older single-tool projects into the dedicated library UI. The old
    /// fields remain readable for CLI compatibility; library selections take precedence.
    pub fn ensure_tool_library(&mut self) {
        if !self.tool_library.tools.is_empty() {
            return;
        }
        use crate::{
            machine::{CutterKind, Tooling},
            tool_library::{LibraryTool, ToolKind},
        };
        let kind = match self.machine.as_ref().map(|m| &m.tooling) {
            Some(Tooling::Milling {
                kind: CutterKind::Drill,
                ..
            }) => ToolKind::Drill,
            Some(Tooling::Turning { .. }) => ToolKind::TurningGeneral,
            Some(Tooling::Printing { .. }) => ToolKind::PrintingHead,
            Some(Tooling::Laser { .. }) => ToolKind::Laser,
            Some(Tooling::Plasma { .. }) => ToolKind::Plasma,
            Some(Tooling::Grinding { .. }) => ToolKind::GrindingWheel,
            Some(Tooling::WireEdm { .. }) => ToolKind::WireEdm,
            Some(Tooling::SinkerEdm { .. }) => ToolKind::Electrode,
            _ => ToolKind::FlatEndMill,
        };
        let mut record = LibraryTool::new(1, 1, kind);
        record.uses = vec![self.process];
        if self.process == Process::Milling {
            record.name = self.tool.name.clone();
            for key in ["diameter", "shaft_diameter"] {
                record.geometry.insert(key.into(), self.tool.diameter);
            }
            for (key, v) in [
                ("cut_feed", self.tool.feed),
                ("plunge_feed", self.tool.plunge),
                ("spindle_rpm", self.tool.spindle_rpm as f64),
                ("ramp_spindle_rpm", self.tool.spindle_rpm as f64),
                ("step_down", self.tool.step_down),
                ("peck_depth", self.tool.step_down),
                ("step_over", self.tool.step_over * 100.0),
            ] {
                record.presets[0].values.insert(key.into(), v);
            }
            record.presets[0].coolant = self.post.coolant;
            self.post.use_tool_coolant = true;
            record.geometry.insert(
                "maximum_rpm".into(),
                self.machine
                    .as_ref()
                    .map_or(100000.0, |m| m.max_spindle_rpm as f64),
            );
        } else if let Some(machine) = &self.machine {
            record.name = machine.tool_name.clone();
        }
        if let Some(machine) = &self.machine {
            let pairs: Vec<(&str, f64)> = match machine.tooling {
                Tooling::Milling {
                    flute_length,
                    overall_length,
                    stickout,
                    shank_diameter,
                    ..
                } => vec![
                    ("flute_length", flute_length),
                    ("shoulder_length", flute_length),
                    ("overall_length", overall_length),
                    ("stickout", stickout),
                    ("gauge_length", stickout),
                    ("shaft_diameter", shank_diameter),
                ],
                Tooling::Turning {
                    nose_radius,
                    insert_angle,
                    holder_width,
                    ..
                } => vec![
                    ("nose_radius", nose_radius),
                    ("insert_angle", insert_angle),
                    ("holder_width", holder_width),
                ],
                Tooling::Printing {
                    nozzle_diameter,
                    filament_diameter,
                    ..
                } => vec![
                    ("nozzle_diameter", nozzle_diameter),
                    ("filament_diameter", filament_diameter),
                ],
                Tooling::Laser {
                    power_watts,
                    spot_diameter,
                    focal_length,
                } => vec![
                    ("rated_power", power_watts),
                    ("spot_diameter", spot_diameter),
                    ("focal_length", focal_length),
                ],
                Tooling::Plasma { kerf_width, .. } => vec![("kerf_width", kerf_width)],
                Tooling::Grinding {
                    wheel_diameter,
                    wheel_width,
                    bore_diameter,
                    max_rpm,
                } => vec![
                    ("diameter", wheel_diameter),
                    ("wheel_width", wheel_width),
                    ("bore_diameter", bore_diameter),
                    ("maximum_rpm", max_rpm),
                ],
                Tooling::WireEdm { wire_diameter, .. } => vec![("wire_diameter", wire_diameter)],
                Tooling::SinkerEdm {
                    electrode_width,
                    electrode_depth,
                    electrode_height,
                    ..
                } => vec![
                    ("width", electrode_width),
                    ("depth", electrode_depth),
                    ("height", electrode_height),
                ],
            };
            for (key, v) in pairs {
                record.geometry.insert(key.into(), v);
            }
            let settings: Vec<(&str, f64)> = match machine.tooling {
                Tooling::Printing {
                    layer_height,
                    nozzle_temperature,
                    bed_temperature,
                    ..
                } => vec![
                    ("layer_height", layer_height),
                    ("first_layer_height", layer_height),
                    ("nozzle_temperature", nozzle_temperature),
                    ("bed_temperature", bed_temperature),
                ],
                Tooling::Plasma {
                    current_amps,
                    cut_height,
                    pierce_height,
                    ..
                } => vec![
                    ("current", current_amps),
                    ("cut_height", cut_height),
                    ("pierce_height", pierce_height),
                ],
                Tooling::WireEdm { spark_gap, .. } | Tooling::SinkerEdm { spark_gap, .. } => {
                    vec![("spark_gap", spark_gap)]
                }
                Tooling::Grinding { max_rpm, .. } | Tooling::Turning { max_rpm, .. } => vec![
                    ("spindle_rpm", max_rpm.min(3000.0)),
                    ("ramp_spindle_rpm", max_rpm.min(3000.0)),
                ],
                _ => Vec::new(),
            };
            for (key, v) in settings {
                record.presets[0].values.insert(key.into(), v);
            }
        }
        self.tool_library.tools.push(record);
        self.active_tool = Some(1);
        self.active_preset = Some(1);
        for op in &mut self.operations {
            op.tool_id = Some(1);
            op.preset_id = Some(1);
        }
    }

    pub fn new_for_process(process: Process) -> Self {
        Self {
            name: format!("Untitled {} project", process.label().to_lowercase()),
            process,
            post: PostSettings::for_process(process),
            machine: crate::machine::MachineSetup::for_process(process),
            operations: Vec::new(),
            ..Self::default()
        }
    }

    /// Start a geometry-only project. Imported surfaces do not imply a machining operation.
    pub fn from_part(mut part: Part) -> Result<Self, String> {
        part.validate()?;
        part.align_to_top(5.0);
        let size = part.bounds().size();
        let project = Self {
            name: part.geometry.source_name.clone(),
            stock: Stock {
                profile: Default::default(),
                width: size[0] + 10.0,
                height: size[1] + 10.0,
                thickness: size[2].max(0.001),
            },
            operations: Vec::new(),
            part: Some(part),
            ..Self::default()
        };
        crate::cam::validate_setup(&project)?;
        Ok(project)
    }

    /// Migrate stored drafts without requiring a finished machining setup.
    pub fn migrate(&mut self) -> Result<(), String> {
        // Older projects were implicitly milling projects.
        if (self.format_version == 1 && self.part.is_none())
            || (2..=10).contains(&self.format_version)
        {
            if self.machine.is_none() {
                self.machine = crate::machine::MachineSetup::for_process(self.process);
                if let Some(machine) = &mut self.machine {
                    if let crate::machine::Tooling::Milling { shank_diameter, .. } =
                        &mut machine.tooling
                    {
                        // Older files contain no measured reach. Keep their
                        // paths available without inventing physical tool sizes.
                        machine.check_tool_reach = false;
                        *shank_diameter = self.tool.diameter;
                    }
                }
            }
            self.format_version = FORMAT_VERSION;
        }
        if self.format_version != FORMAT_VERSION {
            return Err(format!(
                "Unsupported project version {}",
                self.format_version
            ));
        }
        Ok(())
    }

    /// Load bounded input, migrate v1 projects, and validate the setup and geometry.
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_PROJECT_BYTES {
            return Err("Project exceeds the 128 MiB size limit".into());
        }
        use std::io::Read;
        let mut data = Vec::new();
        file.take(MAX_PROJECT_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 > MAX_PROJECT_BYTES {
            return Err("Project exceeds the 128 MiB size limit".into());
        }
        Self::from_json(&data)
    }

    /// Load the shared project format from memory (including browser uploads).
    pub fn from_json(data: &[u8]) -> Result<Self, String> {
        if data.len() as u64 > MAX_PROJECT_BYTES {
            return Err("Project exceeds the 128 MiB size limit".into());
        }
        let mut project: Self = serde_json::from_slice(data).map_err(|e| e.to_string())?;
        project.migrate()?;
        crate::cam::validate_setup(&project)?;
        Ok(project)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        crate::cam::validate_setup(self)?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if text.len() as u64 + 1 > MAX_PROJECT_BYTES {
            return Err("Project exceeds the 128 MiB size limit".into());
        }
        crate::files::write_atomic(path, (text + "\n").as_bytes()).map_err(|e| e.to_string())
    }
}
