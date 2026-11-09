//! Local, process-independent tool definitions. All dimensions are stored in mm.
use crate::{
    model::{Project, Strategy, Tool},
    post::Coolant,
    process::Process,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    BallEndMill,
    BullNoseEndMill,
    FlatEndMill,
    FaceMill,
    TaperedMill,
    RadiusMill,
    ChamferMill,
    DovetailMill,
    LollipopMill,
    SlotMill,
    ThreadMill,
    BarrelMill,
    LensMill,
    OvalMill,
    TaperBarrelMill,
    BoringBar,
    Counterbore,
    Drill,
    CenterDrill,
    SpotDrill,
    Reamer,
    Countersink,
    TapLeft,
    TapRight,
    TurningGeneral,
    TurningBoring,
    TurningGrooving,
    TurningThreading,
    Waterjet,
    Laser,
    Plasma,
    Probe,
    Holder,
    PrintingHead,
    GrindingWheel,
    WireEdm,
    Electrode,
}

#[derive(Clone, Copy, Debug)]
pub struct Parameter {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}
fn dim(key: &'static str, label: &'static str, value: f64) -> Parameter {
    Parameter {
        key,
        label,
        unit: "mm",
        default: value,
        min: 0.001,
        max: 100_000.0,
    }
}
fn value(
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    default: f64,
    min: f64,
    max: f64,
) -> Parameter {
    Parameter {
        key,
        label,
        unit,
        default,
        min,
        max,
    }
}
fn angle(key: &'static str, label: &'static str, default: f64) -> Parameter {
    value(key, label, "deg", default, 0.01, 179.99)
}

impl ToolKind {
    pub const ALL: [Self; 37] = [
        Self::BallEndMill,
        Self::BullNoseEndMill,
        Self::FlatEndMill,
        Self::FaceMill,
        Self::TaperedMill,
        Self::RadiusMill,
        Self::ChamferMill,
        Self::DovetailMill,
        Self::LollipopMill,
        Self::SlotMill,
        Self::ThreadMill,
        Self::BarrelMill,
        Self::LensMill,
        Self::OvalMill,
        Self::TaperBarrelMill,
        Self::BoringBar,
        Self::Counterbore,
        Self::Drill,
        Self::CenterDrill,
        Self::SpotDrill,
        Self::Reamer,
        Self::Countersink,
        Self::TapLeft,
        Self::TapRight,
        Self::TurningGeneral,
        Self::TurningBoring,
        Self::TurningGrooving,
        Self::TurningThreading,
        Self::Waterjet,
        Self::Laser,
        Self::Plasma,
        Self::Probe,
        Self::Holder,
        Self::PrintingHead,
        Self::GrindingWheel,
        Self::WireEdm,
        Self::Electrode,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::BallEndMill => "Ball end mill",
            Self::BullNoseEndMill => "Bull nose end mill",
            Self::FlatEndMill => "Flat end mill",
            Self::FaceMill => "Face mill",
            Self::TaperedMill => "Tapered mill",
            Self::RadiusMill => "Radius mill",
            Self::ChamferMill => "Engrave / chamfer mill",
            Self::DovetailMill => "Dovetail mill",
            Self::LollipopMill => "Lollipop mill",
            Self::SlotMill => "Slot mill",
            Self::ThreadMill => "Thread mill",
            Self::BarrelMill => "Circle segment barrel",
            Self::LensMill => "Circle segment lens",
            Self::OvalMill => "Circle segment oval",
            Self::TaperBarrelMill => "Circle segment taper",
            Self::BoringBar => "Boring bar",
            Self::Counterbore => "Counterbore",
            Self::Drill => "Drill",
            Self::CenterDrill => "Center drill",
            Self::SpotDrill => "Spot drill",
            Self::Reamer => "Reamer",
            Self::Countersink => "Countersink",
            Self::TapLeft => "Tap left hand",
            Self::TapRight => "Tap right hand",
            Self::TurningGeneral => "Turning general",
            Self::TurningBoring => "Turning boring",
            Self::TurningGrooving => "Turning grooving",
            Self::TurningThreading => "Turning threading",
            Self::Waterjet => "Waterjet",
            Self::Laser => "Laser cutter",
            Self::Plasma => "Plasma cutter",
            Self::Probe => "Probe",
            Self::Holder => "Holder",
            Self::PrintingHead => "3D printing head",
            Self::GrindingWheel => "Grinding wheel",
            Self::WireEdm => "EDM wire",
            Self::Electrode => "EDM electrode",
        }
    }
    pub fn group(self) -> &'static str {
        use ToolKind::*;
        match self {
            BallEndMill | BullNoseEndMill | FlatEndMill | FaceMill | TaperedMill | RadiusMill
            | ChamferMill | DovetailMill | LollipopMill | SlotMill | ThreadMill | BarrelMill
            | LensMill | OvalMill | TaperBarrelMill => "Milling",
            BoringBar | Counterbore | Drill | CenterDrill | SpotDrill | Reamer | Countersink
            | TapLeft | TapRight => "Hole making",
            TurningGeneral | TurningBoring | TurningGrooving | TurningThreading => "Turning",
            Waterjet | Laser | Plasma => "Cutting",
            Probe => "Probe",
            Holder => "Holders",
            PrintingHead => "3D printing",
            GrindingWheel => "Grinding",
            WireEdm | Electrode => "EDM",
        }
    }
    pub fn process(self) -> Process {
        match self.group() {
            "Milling" | "Hole making" | "Probe" | "Holders" => Process::Milling,
            "Turning" => Process::Turning,
            "3D printing" => Process::Printing3d,
            "Grinding" => Process::Grinding,
            _ => match self {
                Self::Laser => Process::LaserCutting,
                Self::Plasma => Process::PlasmaCutting,
                Self::WireEdm => Process::WireEdm,
                Self::Electrode => Process::SinkerEdm,
                _ => Process::Mixed,
            },
        }
    }
    pub fn rotates(self) -> bool {
        matches!(self.group(), "Milling" | "Hole making")
    }
    pub fn geometry(self) -> Vec<Parameter> {
        use ToolKind::*;
        let mut p = if self.rotates() {
            vec![
                dim("diameter", "Cutting diameter", 6.0),
                dim("shaft_diameter", "Shaft diameter", 6.0),
                dim("overall_length", "Overall length", 60.0),
                dim("stickout", "Length below holder", 35.0),
                dim("shoulder_length", "Shoulder length", 25.0),
                dim("flute_length", "Flute length", 20.0),
                dim("gauge_length", "Gauge length", 35.0),
            ]
        } else {
            vec![
                dim("overall_length", "Overall length", 60.0),
                dim("gauge_length", "Gauge length", 35.0),
            ]
        };
        match self {
            BullNoseEndMill | RadiusMill => p.push(dim("corner_radius", "Corner radius", 0.75)),
            FaceMill => p.push(angle("lead_angle", "Insert lead angle", 45.0)),
            TaperedMill => p.extend([
                dim("tip_diameter", "Tip diameter", 2.5),
                angle("taper_angle", "Taper half angle", 5.0),
            ]),
            ChamferMill | Countersink => p.extend([
                value("tip_diameter", "Tip diameter", "mm", 0.1, 0.0, 100_000.0),
                angle("included_angle", "Included angle", 90.0),
            ]),
            DovetailMill => p.extend([
                dim("neck_diameter", "Neck diameter", 3.0),
                angle("included_angle", "Included angle", 60.0),
            ]),
            LollipopMill => p.extend([
                dim("neck_diameter", "Neck diameter", 3.0),
                dim("neck_length", "Neck length", 10.0),
            ]),
            SlotMill => p.extend([
                dim("neck_diameter", "Neck diameter", 3.0),
                dim("cutting_width", "Cutting width", 2.0),
                dim("arbor_diameter", "Arbor diameter", 2.0),
            ]),
            ThreadMill => p.extend([
                dim("thread_pitch", "Thread pitch", 1.0),
                angle("thread_angle", "Thread form angle", 60.0),
                dim("neck_diameter", "Neck diameter", 3.0),
                dim("tooth_height", "Tooth height", 0.5),
            ]),
            BarrelMill | LensMill | OvalMill | TaperBarrelMill => {
                p.extend([
                    dim("profile_radius", "Profile radius", 30.0),
                    dim("profile_length", "Profile length", 10.0),
                    dim("front_radius", "Front radius", 3.0),
                    dim("upper_diameter", "Upper diameter", 4.0),
                    dim("lower_diameter", "Lower diameter", 4.0),
                ]);
                if self == TaperBarrelMill {
                    p.push(angle("profile_angle", "Profile taper angle", 10.0));
                }
            }
            BoringBar => p.extend([
                dim("minimum_bore", "Minimum bore diameter", 8.0),
                dim("maximum_bore", "Maximum bore diameter", 12.0),
                dim("nose_radius", "Insert nose radius", 0.4),
            ]),
            Counterbore => p.extend([
                dim("pilot_diameter", "Pilot diameter", 3.0),
                dim("pilot_length", "Pilot length", 3.0),
                value("corner_radius", "Corner radius", "mm", 0.0, 0.0, 100_000.0),
            ]),
            Drill => p.extend([
                angle("point_angle", "Drill point angle", 118.0),
                angle("helix_angle", "Helix angle", 30.0),
            ]),
            CenterDrill => p.extend([
                dim("pilot_diameter", "Pilot diameter", 2.0),
                dim("pilot_length", "Pilot length", 3.0),
                angle("included_angle", "Included angle", 60.0),
                angle("point_angle", "Point angle", 118.0),
            ]),
            SpotDrill => p.push(angle("point_angle", "Point angle", 90.0)),
            Reamer => p.push(angle("lead_angle", "Lead-in angle", 15.0)),
            TapLeft | TapRight => p.extend([
                dim("thread_pitch", "Thread pitch", 1.0),
                angle("thread_angle", "Thread form angle", 60.0),
                dim("minor_diameter", "Minor diameter", 5.0),
                dim("pilot_diameter", "Tap drill diameter", 5.0),
            ]),
            TurningGeneral | TurningBoring | TurningGrooving | TurningThreading => {
                p.extend([
                    dim("nose_radius", "Insert nose radius", 0.4),
                    dim("insert_size", "Insert inscribed circle", 9.525),
                    dim("insert_thickness", "Insert thickness", 3.175),
                    angle("insert_angle", "Insert included angle", 80.0),
                    angle("cutting_edge_angle", "Cutting edge angle", 95.0),
                    angle("relief_angle", "Relief angle", 7.0),
                    dim("holder_width", "Holder width", 12.0),
                    dim("holder_height", "Holder height", 12.0),
                ]);
                if self == TurningBoring {
                    p.extend([
                        dim("bar_diameter", "Boring bar diameter", 10.0),
                        dim("minimum_bore", "Minimum bore", 12.0),
                        dim("maximum_depth", "Maximum bore depth", 30.0),
                    ]);
                }
                if self == TurningGrooving {
                    p.extend([
                        dim("groove_width", "Insert width", 2.0),
                        dim("maximum_depth", "Maximum groove depth", 5.0),
                    ]);
                }
                if self == TurningThreading {
                    p.extend([
                        angle("thread_angle", "Thread form angle", 60.0),
                        dim("minimum_pitch", "Minimum thread pitch", 0.5),
                        dim("maximum_pitch", "Maximum thread pitch", 3.0),
                    ]);
                }
            }
            Waterjet => p.extend([
                dim("orifice_diameter", "Orifice diameter", 0.3),
                dim("mixing_tube_diameter", "Mixing tube diameter", 1.0),
                dim("kerf_width", "Kerf width", 1.0),
                dim("standoff", "Standoff", 2.0),
                value(
                    "maximum_pressure",
                    "Maximum water pressure",
                    "bar",
                    4000.0,
                    1.0,
                    10000.0,
                ),
            ]),
            Laser => p.extend([
                value(
                    "rated_power",
                    "Rated optical power",
                    "W",
                    10.0,
                    0.001,
                    100000.0,
                ),
                dim("spot_diameter", "Spot diameter", 0.1),
                dim("focal_length", "Focal length", 50.0),
                dim("nozzle_diameter", "Nozzle diameter", 1.0),
            ]),
            Plasma => p.extend([
                value(
                    "maximum_current",
                    "Maximum current",
                    "A",
                    100.0,
                    0.1,
                    1000.0,
                ),
                dim("nozzle_diameter", "Nozzle diameter", 1.0),
                dim("kerf_width", "Kerf width", 1.2),
            ]),
            Probe => p.extend([
                dim("body_diameter", "Body diameter", 20.0),
                dim("stylus_diameter", "Stylus ball diameter", 3.0),
                dim("stylus_length", "Stylus length", 30.0),
                dim("overtravel", "Maximum overtravel", 2.0),
            ]),
            Holder => p.extend([
                dim("body_diameter", "Body diameter", 30.0),
                dim("bore_diameter", "Clamping bore diameter", 6.0),
                dim("minimum_shank_diameter", "Minimum clamping diameter", 6.0),
                dim("clamping_length", "Clamping length", 25.0),
            ]),
            PrintingHead => p.extend([
                dim("nozzle_diameter", "Nozzle diameter", 0.4),
                dim("filament_diameter", "Filament diameter", 1.75),
                dim("body_width", "Head width", 40.0),
                dim("body_depth", "Head depth", 40.0),
                value(
                    "maximum_temperature",
                    "Maximum nozzle temperature",
                    "C",
                    300.0,
                    1.0,
                    500.0,
                ),
                value(
                    "maximum_bed_temperature",
                    "Maximum bed temperature",
                    "C",
                    120.0,
                    0.0,
                    200.0,
                ),
                value(
                    "maximum_volumetric_flow",
                    "Maximum volumetric flow",
                    "mm3/s",
                    12.0,
                    0.001,
                    1000.0,
                ),
            ]),
            GrindingWheel => p.extend([
                dim("diameter", "Wheel diameter", 150.0),
                dim("wheel_width", "Wheel width", 20.0),
                dim("bore_diameter", "Bore diameter", 32.0),
                value(
                    "maximum_rpm",
                    "Maximum wheel speed",
                    "rpm",
                    3000.0,
                    1.0,
                    100000.0,
                ),
            ]),
            WireEdm => p.extend([
                dim("wire_diameter", "Wire diameter", 0.25),
                value("maximum_taper", "Maximum taper", "deg", 15.0, 0.0, 45.0),
            ]),
            Electrode => p.extend([
                dim("width", "Electrode width", 10.0),
                dim("depth", "Electrode depth", 10.0),
                dim("height", "Electrode height", 30.0),
            ]),
            FlatEndMill | BallEndMill => {}
        }
        if self.rotates() {
            p.push(value(
                "maximum_rpm",
                "Maximum rated speed",
                "rpm",
                60000.0,
                1.0,
                100000.0,
            ));
        }
        p
    }

    pub fn cutting(self) -> Vec<Parameter> {
        let mut p = if matches!(self, Self::PrintingHead | Self::Holder | Self::Probe) {
            Vec::new()
        } else {
            vec![
                value("cut_feed", "Cutting feed", "mm/min", 600.0, 0.001, 100000.0),
                value(
                    "lead_in_feed",
                    "Lead-in feed",
                    "mm/min",
                    300.0,
                    0.001,
                    100000.0,
                ),
                value(
                    "lead_out_feed",
                    "Lead-out feed",
                    "mm/min",
                    300.0,
                    0.001,
                    100000.0,
                ),
            ]
        };
        if self.rotates() || self.group() == "Turning" || self == Self::GrindingWheel {
            p.extend([
                value(
                    "spindle_rpm",
                    "Spindle speed",
                    "rpm",
                    12000.0,
                    1.0,
                    100000.0,
                ),
                value(
                    "surface_speed",
                    "Surface speed",
                    "m/min",
                    226.1947,
                    0.001,
                    100000.0,
                ),
                dim("feed_per_tooth", "Feed per tooth", 0.0125),
                dim("feed_per_revolution", "Feed per revolution", 0.1),
                value(
                    "plunge_feed",
                    "Plunge feed",
                    "mm/min",
                    120.0,
                    0.001,
                    100000.0,
                ),
                value("ramp_feed", "Ramp feed", "mm/min", 200.0, 0.001, 100000.0),
                value(
                    "transition_feed",
                    "Transition feed",
                    "mm/min",
                    600.0,
                    0.001,
                    100000.0,
                ),
                value(
                    "ramp_spindle_rpm",
                    "Ramp spindle speed",
                    "rpm",
                    12000.0,
                    1.0,
                    100000.0,
                ),
                angle("ramp_angle", "Ramp angle", 2.0),
                dim("step_down", "Maximum step-down", 1.0),
                value("step_over", "Step-over", "%", 40.0, 1.0, 100.0),
                dim("peck_depth", "Peck depth", 1.0),
                value("dwell", "Dwell", "s", 0.0, 0.0, 3600.0),
            ]);
        }
        if self.group() == "Turning" {
            p.push(value(
                "maximum_spindle_rpm",
                "CSS maximum spindle speed",
                "rpm",
                3000.0,
                1.0,
                100000.0,
            ));
        }
        use ToolKind::*;
        match self {
            PrintingHead => p.extend([
                dim("line_width", "Extrusion line width", 0.4),
                value(
                    "nozzle_temperature",
                    "Nozzle target",
                    "C",
                    200.0,
                    0.0,
                    500.0,
                ),
                value("bed_temperature", "Bed target", "C", 60.0, 0.0, 200.0),
                dim("layer_height", "Layer height", 0.2),
                dim("first_layer_height", "First layer height", 0.2),
                value("print_speed", "Print speed", "mm/s", 40.0, 0.001, 1000.0),
                value(
                    "first_layer_speed",
                    "First-layer speed",
                    "mm/s",
                    20.0,
                    0.001,
                    1000.0,
                ),
                value("pressure_advance", "Pressure advance", "s", 0.0, 0.0, 2.0),
                value("travel_speed", "Travel speed", "mm/s", 100.0, 0.001, 1000.0),
                value(
                    "retraction_length",
                    "Retraction length",
                    "mm",
                    1.0,
                    0.0,
                    100.0,
                ),
                value(
                    "retraction_speed",
                    "Retraction speed",
                    "mm/s",
                    30.0,
                    0.001,
                    1000.0,
                ),
                value("prime_speed", "Prime speed", "mm/s", 20.0, 0.001, 1000.0),
                value(
                    "extrusion_multiplier",
                    "Extrusion multiplier",
                    "ratio",
                    1.0,
                    0.01,
                    5.0,
                ),
                value("fan", "Cooling fan", "%", 100.0, 0.0, 100.0),
                value(
                    "standby_temperature",
                    "Standby temperature",
                    "C",
                    150.0,
                    0.0,
                    500.0,
                ),
            ]),
            Laser => p.extend([
                value("power", "Cut power", "%", 50.0, 0.0, 100.0),
                value("minimum_power", "Minimum power", "%", 0.0, 0.0, 100.0),
                value("pierce_power", "Pierce power", "%", 50.0, 0.0, 100.0),
                value(
                    "pulse_frequency",
                    "Pulse frequency (0=continuous)",
                    "Hz",
                    0.0,
                    0.0,
                    1000000.0,
                ),
                value("duty_cycle", "Duty cycle", "%", 100.0, 0.0, 100.0),
                value("focus_offset", "Focus offset", "mm", 0.0, -1000.0, 1000.0),
                dim("cut_height", "Cut height", 1.0),
                value("pierce_delay", "Pierce dwell", "s", 0.0, 0.0, 3600.0),
                value(
                    "gas_pressure",
                    "Assist gas pressure",
                    "bar",
                    1.0,
                    0.0,
                    100.0,
                ),
            ]),
            Plasma => p.extend([
                value("current", "Cutting current", "A", 40.0, 0.1, 1000.0),
                dim("cut_height", "Cut height", 1.5),
                dim("pierce_height", "Pierce height", 3.0),
                value("pierce_delay", "Pierce delay", "s", 0.5, 0.0, 3600.0),
                value("arc_voltage", "Arc voltage target", "V", 120.0, 0.1, 1000.0),
                value("gas_pressure", "Gas pressure", "bar", 5.0, 0.0, 100.0),
                value("postflow", "Post-flow", "s", 5.0, 0.0, 3600.0),
            ]),
            Waterjet => p.extend([
                value("pressure", "Water pressure", "bar", 3500.0, 1.0, 10000.0),
                value(
                    "abrasive_feed",
                    "Abrasive feed",
                    "g/min",
                    300.0,
                    0.0,
                    10000.0,
                ),
                value("pierce_delay", "Pierce dwell", "s", 1.0, 0.0, 3600.0),
            ]),
            Probe => p.extend([
                value("probe_feed", "Search feed", "mm/min", 100.0, 0.001, 10000.0),
                value("latch_feed", "Latch feed", "mm/min", 25.0, 0.001, 10000.0),
                dim("search_distance", "Search distance", 10.0),
                dim("retract_distance", "Retract distance", 2.0),
            ]),
            WireEdm | Electrode => p.extend([
                value("spark_gap", "Spark gap", "mm", 0.02, 0.0, 10.0),
                value("current", "Discharge current", "A", 1.0, 0.001, 1000.0),
                value("pulse_on", "Pulse on", "us", 10.0, 0.001, 100000.0),
                value("pulse_off", "Pulse off", "us", 10.0, 0.001, 100000.0),
                value("voltage", "Gap voltage", "V", 80.0, 0.1, 1000.0),
            ]),
            _ => {}
        }
        p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedMode {
    PerMinute,
    PerTooth,
    PerRevolution,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuttingPreset {
    pub constant_surface_speed: bool,
    pub id: u64,
    pub name: String,
    pub material: String,
    pub strategies: Vec<Strategy>,
    pub feed_mode: FeedMode,
    pub coolant: Coolant,
    pub relative_extrusion: bool,
    pub values: BTreeMap<String, f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub height: f64,
    pub lower_diameter: f64,
    pub upper_diameter: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPost {
    pub turning_offset: Option<u32>,
    pub mount_offset: [f64; 3],
    pub heater_channel: u32,
    pub fan_channel: u32,
    pub power_scale_max: f64,
    pub dynamic_power: bool,
    pub connector: String,
    pub interlocks: String,
    pub number: u32,
    pub length_offset: Option<u32>,
    pub diameter_offset: Option<u32>,
    pub turret: u32,
    pub pocket: Option<u32>,
    pub head_id: u32,
    pub comment: String,
    pub manual_change: bool,
    pub live_tool: bool,
    pub break_control: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryTool {
    pub id: u64,
    pub name: String,
    pub kind: ToolKind,
    pub description: String,
    pub vendor: String,
    pub product_id: String,
    pub product_link: String,
    pub material: String,
    pub clockwise: bool,
    pub flutes: u32,
    pub center_cutting: bool,
    pub uses: Vec<Process>,
    pub geometry: BTreeMap<String, f64>,
    pub shaft: Vec<Section>,
    pub holder: Option<u64>,
    pub presets: Vec<CuttingPreset>,
    pub default_preset: u64,
    pub post: ToolPost,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolLibrary {
    pub tools: Vec<LibraryTool>,
}

fn defaults(specs: Vec<Parameter>) -> BTreeMap<String, f64> {
    specs
        .into_iter()
        .map(|p| (p.key.into(), p.default))
        .collect()
}
impl LibraryTool {
    pub fn new(id: u64, number: u32, kind: ToolKind) -> Self {
        let mut tool = Self {
            id,
            name: kind.label().into(),
            kind,
            description: String::new(),
            vendor: String::new(),
            product_id: String::new(),
            product_link: String::new(),
            material: match kind {
                ToolKind::PrintingHead => "Brass",
                ToolKind::Holder => "Steel",
                ToolKind::GrindingWheel => "Abrasive",
                ToolKind::WireEdm => "Brass",
                ToolKind::Electrode => "Copper",
                ToolKind::Laser | ToolKind::Plasma | ToolKind::Waterjet | ToolKind::Probe => {
                    "Assembly"
                }
                _ => "Carbide",
            }
            .into(),
            clockwise: kind != ToolKind::TapLeft,
            flutes: if kind == ToolKind::Drill { 2 } else { 4 },
            center_cutting: true,
            uses: vec![kind.process()],
            geometry: defaults(kind.geometry()),
            shaft: Vec::new(),
            holder: None,
            presets: vec![CuttingPreset {
                constant_surface_speed: false,
                id: 1,
                name: "Default preset".into(),
                material: String::new(),
                strategies: Vec::new(),
                feed_mode: FeedMode::PerMinute,
                coolant: Coolant::Off,
                relative_extrusion: false,
                values: defaults(kind.cutting()),
            }],
            default_preset: 1,
            post: ToolPost {
                turning_offset: None,
                mount_offset: [0.0; 3],
                heater_channel: 0,
                fan_channel: 0,
                power_scale_max: 1000.0,
                dynamic_power: false,
                connector: String::new(),
                interlocks: String::new(),
                number,
                length_offset: None,
                diameter_offset: None,
                turret: 0,
                pocket: None,
                head_id: 0,
                comment: String::new(),
                manual_change: false,
                live_tool: kind.rotates() || kind == ToolKind::GrindingWheel,
                break_control: false,
            },
        };
        if kind == ToolKind::GrindingWheel || kind.group() == "Turning" {
            for key in ["spindle_rpm", "ramp_spindle_rpm"] {
                tool.presets[0].values.insert(key.into(), 3000.0);
            }
        }
        if matches!(kind, ToolKind::TapLeft | ToolKind::TapRight) {
            tool.presets[0].feed_mode = FeedMode::PerRevolution;
            for (key, v) in [
                ("spindle_rpm", 500.0),
                ("ramp_spindle_rpm", 500.0),
                ("feed_per_revolution", 1.0),
                ("cut_feed", 500.0),
            ] {
                tool.presets[0].values.insert(key.into(), v);
            }
        }
        tool
    }
    pub fn dimension(&self, key: &str) -> f64 {
        *self.geometry.get(key).unwrap_or(&0.0)
    }
    pub fn preset(&self, id: Option<u64>) -> Result<&CuttingPreset, String> {
        self.presets
            .iter()
            .find(|p| p.id == id.unwrap_or(self.default_preset))
            .ok_or_else(|| format!("{}: cutting preset not found", self.name))
    }
    pub fn cutting_tool(&self, id: Option<u64>) -> Result<Tool, String> {
        let p = self.preset(id)?;
        let get = |key| {
            p.values
                .get(key)
                .copied()
                .ok_or_else(|| format!("{}: missing cutting parameter {key}", self.name))
        };
        let rpm = get("spindle_rpm")?;
        let feed = match p.feed_mode {
            FeedMode::PerMinute => get("cut_feed")?,
            FeedMode::PerTooth => get("feed_per_tooth")? * self.flutes as f64 * rpm,
            FeedMode::PerRevolution => get("feed_per_revolution")? * rpm,
        };
        Ok(Tool {
            name: self.name.clone(),
            diameter: self.dimension("diameter"),
            feed,
            plunge: get("plunge_feed")?,
            spindle_rpm: rpm.round() as u32,
            step_down: if self.kind == ToolKind::Drill {
                get("peck_depth")?
            } else {
                get("step_down")?
            },
            step_over: get("step_over")? / 100.0,
        })
    }
}

fn parameters(values: &BTreeMap<String, f64>, specs: Vec<Parameter>) -> Result<(), String> {
    if values.len() != specs.len() {
        return Err("Tool parameters are missing or contain unknown fields".into());
    }
    for p in specs {
        let v = values
            .get(p.key)
            .ok_or_else(|| format!("Missing {}", p.label))?;
        if !v.is_finite() || *v < p.min || *v > p.max {
            return Err(format!(
                "{} must be finite and between {} and {} {}",
                p.label, p.min, p.max, p.unit
            ));
        }
        if p.unit == "rpm" && v.fract() != 0.0 {
            return Err(format!("{} must be a whole RPM value", p.label));
        }
    }
    Ok(())
}

impl ToolLibrary {
    pub fn get(&self, id: u64) -> Result<&LibraryTool, String> {
        self.tools
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("Library tool {id} not found"))
    }
    pub fn next_id(&self) -> u64 {
        self.tools.iter().map(|t| t.id).max().unwrap_or(0) + 1
    }
    pub fn next_number(&self, process: Process) -> u32 {
        self.next_number_for(&[process])
    }
    pub fn next_number_for(&self, uses: &[Process]) -> u32 {
        (1..=9999)
            .find(|n| {
                !self
                    .tools
                    .iter()
                    .any(|t| t.post.number == *n && t.uses.iter().any(|p| uses.contains(p)))
            })
            .unwrap_or(9999)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.tools.len() > 1000 {
            return Err("At most 1,000 library tools are supported".into());
        }
        let mut ids = BTreeSet::new();
        let mut numbers = BTreeSet::new();
        for t in &self.tools {
            if t.id == 0 || t.id > 1_000_000_000 || !ids.insert(t.id) {
                return Err("Library tool IDs must be unique and nonzero".into());
            }
            if t.name.trim().is_empty()
                || [
                    &t.name,
                    &t.description,
                    &t.vendor,
                    &t.product_id,
                    &t.product_link,
                    &t.material,
                    &t.post.comment,
                    &t.post.connector,
                    &t.post.interlocks,
                ]
                .iter()
                .any(|s| s.len() > 4096)
            {
                return Err("Tool name is empty or a text field exceeds 4096 bytes".into());
            }
            if !(1..=9999).contains(&t.post.number)
                || t.uses
                    .iter()
                    .any(|process| !numbers.insert((format!("{process:?}"), t.post.number)))
            {
                return Err(
                    "Tool numbers must be unique per process and between 1 and 9999".into(),
                );
            }
            if t.uses.is_empty()
                || t.uses.len() > Process::ALL.len()
                || t.uses
                    .iter()
                    .enumerate()
                    .any(|(i, p)| t.uses[..i].contains(p))
            {
                return Err("Select at least one unique operation type for each tool".into());
            }
            if !(1..=1000).contains(&t.flutes) {
                return Err("Number of flutes must be between 1 and 1000".into());
            }
            for n in [
                t.post.length_offset,
                t.post.diameter_offset,
                t.post.turning_offset,
                t.post.pocket,
            ]
            .into_iter()
            .flatten()
            {
                if !(1..=9999).contains(&n) {
                    return Err("Offset and pocket numbers must be between 1 and 9999".into());
                }
            }
            if t.post.turret > 9999 || t.post.head_id > 9999 {
                return Err("Turret/head number exceeds 9999".into());
            }
            parameters(&t.geometry, t.kind.geometry())?;
            if t.post
                .mount_offset
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
                || !t.post.power_scale_max.is_finite()
                || !(0.001..=1_000_000.0).contains(&t.post.power_scale_max)
                || t.post.heater_channel > 9999
                || t.post.fan_channel > 9999
            {
                return Err("Invalid head offsets, channels or power scaling".into());
            }
            if (t.kind == ToolKind::TapLeft && t.clockwise)
                || (t.kind == ToolKind::TapRight && !t.clockwise)
            {
                return Err("Tap spindle direction must match thread handedness".into());
            }
            if t.geometry.contains_key("minimum_bore")
                && t.geometry.contains_key("maximum_bore")
                && t.dimension("minimum_bore") > t.dimension("maximum_bore")
            {
                return Err("Minimum bore exceeds maximum bore".into());
            }
            if t.kind == ToolKind::TurningBoring
                && t.dimension("bar_diameter") > t.dimension("minimum_bore")
            {
                return Err("Boring bar exceeds minimum bore diameter".into());
            }
            if t.kind == ToolKind::TurningThreading
                && t.dimension("minimum_pitch") > t.dimension("maximum_pitch")
            {
                return Err("Minimum thread pitch exceeds maximum".into());
            }
            if t.shaft.iter().map(|s| s.height).sum::<f64>()
                + if t.kind == ToolKind::Holder {
                    0.0
                } else {
                    t.dimension("flute_length")
                }
                > t.dimension("overall_length")
            {
                return Err("Custom sections exceed overall assembly length".into());
            }

            if t.kind.rotates() {
                if t.dimension("gauge_length") < t.dimension("stickout") {
                    return Err("Gauge length must not be shorter than tool stickout".into());
                }
                if t.kind == ToolKind::TaperedMill
                    && t.dimension("tip_diameter")
                        + 2.0
                            * t.dimension("flute_length")
                            * t.dimension("taper_angle").to_radians().tan()
                        > t.dimension("diameter") + 1e-6
                {
                    return Err("Tapered cutting geometry exceeds its maximum diameter".into());
                }
                if t.dimension("flute_length") > t.dimension("shoulder_length")
                    || t.dimension("shoulder_length") > t.dimension("stickout")
                    || t.dimension("stickout") > t.dimension("overall_length")
                {
                    return Err(format!(
                        "{}: flute <= shoulder <= stickout <= overall length is required",
                        t.name
                    ));
                }
                for key in [
                    "tip_diameter",
                    "pilot_diameter",
                    "minor_diameter",
                    "neck_diameter",
                    "upper_diameter",
                    "lower_diameter",
                ] {
                    if t.geometry.contains_key(key) && t.dimension(key) > t.dimension("diameter") {
                        return Err(format!("{} exceeds cutting diameter", key));
                    }
                }
                if t.geometry.contains_key("corner_radius")
                    && t.dimension("corner_radius") > t.dimension("diameter") / 2.0
                {
                    return Err("Corner radius exceeds half the diameter".into());
                }
            }
            if t.geometry.contains_key("bore_diameter")
                && t.dimension("bore_diameter")
                    >= t.dimension(if t.kind == ToolKind::Holder {
                        "body_diameter"
                    } else {
                        "diameter"
                    })
            {
                return Err("Bore diameter must be smaller than the body/wheel diameter".into());
            }
            if t.kind == ToolKind::Holder
                && (t.dimension("minimum_shank_diameter") > t.dimension("bore_diameter")
                    || t.dimension("clamping_length") > t.dimension("overall_length"))
            {
                return Err("Invalid holder clamping range or clamping length".into());
            }
            if t.shaft.len() > 64
                || t.shaft.iter().any(|s| {
                    [s.height, s.lower_diameter, s.upper_diameter]
                        .iter()
                        .any(|v| !v.is_finite() || *v < 0.001 || *v > 100_000.0)
                })
            {
                return Err("Shaft/holder sections must have positive finite dimensions; maximum 64 sections".into());
            }
            if let Some(id) = t.holder {
                if id == t.id
                    || t.kind == ToolKind::Holder
                    || self.get(id)?.kind != ToolKind::Holder
                {
                    return Err("An assembly must reference a separate holder tool".into());
                }
                let holder = self.get(id)?;
                if t.kind.rotates()
                    && (t.dimension("shaft_diameter") < holder.dimension("minimum_shank_diameter")
                        || t.dimension("shaft_diameter") > holder.dimension("bore_diameter"))
                {
                    return Err("Tool shaft is outside the holder's clamping range".into());
                }
            }
            if t.presets.is_empty() || t.presets.len() > 64 {
                return Err("Each tool requires 1–64 cutting presets".into());
            }
            let mut presets = BTreeSet::new();
            for p in &t.presets {
                if p.id == 0
                    || p.id > 1_000_000_000
                    || !presets.insert(p.id)
                    || p.name.trim().is_empty()
                    || p.name.len() > 4096
                    || p.material.len() > 4096
                {
                    return Err("Cutting presets require unique nonzero IDs and names".into());
                }
                parameters(&p.values, t.kind.cutting())?;
                if p.strategies.len() > Strategy::ALL.len()
                    || p.strategies
                        .iter()
                        .enumerate()
                        .any(|(i, s)| p.strategies[..i].contains(s))
                {
                    return Err("Cutting preset strategy associations must be unique".into());
                }
                if !t.kind.rotates() && !p.strategies.is_empty() {
                    return Err(
                        "Milling strategy associations do not apply to this head/tool type".into(),
                    );
                }
                let v = |key: &str| *p.values.get(key).unwrap_or(&0.0);
                if t.kind == ToolKind::PrintingHead
                    && (v("layer_height") > t.dimension("nozzle_diameter")
                        || v("first_layer_height") > t.dimension("nozzle_diameter")
                        || v("nozzle_temperature") > t.dimension("maximum_temperature")
                        || v("standby_temperature") > t.dimension("maximum_temperature")
                        || v("bed_temperature") > t.dimension("maximum_bed_temperature"))
                {
                    return Err("Printing preset exceeds nozzle geometry or heater limits".into());
                }
                if t.kind == ToolKind::PrintingHead
                    && v("print_speed")
                        * v("layer_height")
                        * v("line_width")
                        * v("extrusion_multiplier")
                        > t.dimension("maximum_volumetric_flow")
                {
                    return Err("Printing preset exceeds the head's maximum volumetric flow".into());
                }
                if t.kind == ToolKind::PrintingHead
                    && v("first_layer_speed")
                        * v("first_layer_height")
                        * v("line_width")
                        * v("extrusion_multiplier")
                        > t.dimension("maximum_volumetric_flow")
                {
                    return Err(
                        "First-layer preset exceeds the head's maximum volumetric flow".into(),
                    );
                }
                if t.kind == ToolKind::Plasma
                    && (v("current") > t.dimension("maximum_current")
                        || v("pierce_height") < v("cut_height"))
                {
                    return Err("Plasma current/height settings exceed the tool definition".into());
                }
                if t.kind == ToolKind::Waterjet && v("pressure") > t.dimension("maximum_pressure") {
                    return Err("Waterjet pressure exceeds the head rating".into());
                }
                if t.kind == ToolKind::Laser && v("minimum_power") > v("power") {
                    return Err("Minimum laser power exceeds cut power".into());
                }
                if p.constant_surface_speed && t.kind.group() != "Turning" {
                    return Err("Constant surface speed is a turning preset setting".into());
                }
                if t.geometry.contains_key("maximum_rpm")
                    && (v("spindle_rpm") > t.dimension("maximum_rpm")
                        || v("ramp_spindle_rpm") > t.dimension("maximum_rpm"))
                {
                    return Err("Cutting preset exceeds the tool's rated spindle speed".into());
                }
                if t.kind.rotates() {
                    let c = t.cutting_tool(Some(p.id))?;
                    crate::cam::validate_cutting_tool(&c)?;
                } else if p.feed_mode != FeedMode::PerMinute && t.kind.group() != "Turning" {
                    return Err("This head requires feed-per-minute settings".into());
                }
            }
            t.preset(None)?;
        }
        Ok(())
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        let data = serde_json::to_vec_pretty(&LibraryFile {
            format_version: 1,
            units: "mm".into(),
            library: self.clone(),
        })
        .map_err(|e| e.to_string())?;
        if data.len() > 8 * 1024 * 1024 {
            return Err("Tool library exceeds 8 MiB".into());
        }
        crate::files::write_atomic(path, &data).map_err(|e| e.to_string())
    }
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let mut data = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > 8 * 1024 * 1024 {
            return Err("Tool library exceeds 8 MiB".into());
        }
        Self::from_json(&data)
    }
    pub fn from_json(data: &[u8]) -> Result<Self, String> {
        if data.len() > 8 * 1024 * 1024 {
            return Err("Tool library exceeds 8 MiB".into());
        }
        let file: LibraryFile = serde_json::from_slice(data).map_err(|e| e.to_string())?;
        if file.format_version != 1 {
            return Err("Unsupported tool-library version".into());
        }
        if file.units != "mm" {
            return Err("Tool library linear units must be explicitly mm".into());
        }
        file.library.validate()?;
        Ok(file.library)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    format_version: u32,
    units: String,
    library: ToolLibrary,
}

fn merge(target: &mut ToolLibrary, source: ToolLibrary) -> Result<(), String> {
    let mut candidate = target.clone();
    let mut remap = std::collections::BTreeMap::new();
    for old in &source.tools {
        let id = candidate.next_id();
        remap.insert(old.id, id);
        let mut tool = old.clone();
        tool.id = id;
        tool.post.number = candidate.next_number_for(&tool.uses);
        tool.post.pocket = None;
        tool.post.length_offset = None;
        tool.post.diameter_offset = None;
        tool.post.turret = 0;
        tool.post.turning_offset = None;
        tool.holder = None;
        candidate.tools.push(tool);
    }
    for old in &source.tools {
        if let Some(holder) = old.holder {
            let id = remap[&old.id];
            candidate
                .tools
                .iter_mut()
                .find(|t| t.id == id)
                .unwrap()
                .holder = Some(remap[&holder]);
        }
    }
    candidate.validate()?;
    *target = candidate;
    Ok(())
}

pub fn import_into(
    project: &mut Project,
    source: ToolLibrary,
    replace: bool,
) -> Result<String, String> {
    source.validate()?;
    if source.tools.is_empty() {
        return Err("The tool list contains no tools".into());
    }
    let active = source
        .tools
        .iter()
        .find(|t| t.uses.contains(&project.process) && t.kind != ToolKind::Holder)
        .map(|t| t.id);
    if replace {
        if !project.operations.is_empty() {
            return Err(
                "A library used by operations cannot be replaced; add tools instead".into(),
            );
        }
        if active.is_none() {
            return Err(
                "The file has no applicable tool for this work mode; add tools instead".into(),
            );
        }
        project.tool_library = source;
        project.active_tool = active;
        project.active_preset =
            active.and_then(|id| project.tool_library.get(id).ok().map(|t| t.default_preset));
        Ok("Library opened with its tool numbers and ATC bindings. Match them to the selected machine's controller table.".into())
    } else {
        merge(&mut project.tool_library, source)?;
        Ok("Tools added with local numbers. Device pocket/offset bindings were cleared on the imported copies; existing job tools were preserved.".into())
    }
}
