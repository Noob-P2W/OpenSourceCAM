//! Effective operation-specific machining settings; defaults preserve older jobs.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Raster {
    #[default]
    Bidirectional,
    PositiveX,
    NegativeX,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Counterclockwise,
    Clockwise,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PocketPattern {
    #[default]
    Raster,
    Offset,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    #[default]
    Plunge,
    Ramp,
    Helix,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MillingMode {
    Climb,
    Conventional,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub height_offset: Option<f64>,
    pub milling_mode: Option<MillingMode>,
    pub islands: Vec<Vec<[f64; 3]>>,
    pub chamfer_angle: Option<f64>,
    pub machining_face: Option<usize>,
    pub pocket_pattern: PocketPattern,
    pub entry: Entry,
    /// Tool-center or contour-boundary chain in work coordinates; depth is subtracted from Z.
    pub path: Vec<[f64; 3]>,
    pub closed: bool,
    pub chamfer_width: Option<f64>,
    pub trace_offset: bool,
    pub precleared: bool,
    pub outside: bool,
    pub pitch: Option<f64>,
    pub thread_depth: Option<f64>,
    pub taper_angle: Option<f64>,
    pub start_depth: Option<f64>,
    pub surface_step: Option<f64>,
    pub path_tolerance: Option<f64>,
    pub feed: Option<f64>,
    pub plunge: Option<f64>,
    pub spindle_rpm: Option<u32>,
    pub step_down: Option<f64>,
    pub peck_clearance: Option<f64>,
    pub step_over_percent: Option<f64>,
    pub clearance: Option<f64>,
    pub retract: Option<f64>,
    pub radial_allowance: f64,
    pub axial_allowance: f64,
    pub spring_passes: u32,
    pub finish_feed: Option<f64>,
    pub raster: Raster,
    pub direction: Direction,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .height_offset
            .is_some_and(|v| !v.is_finite() || v.abs() > 10000.)
            || self
                .chamfer_angle
                .is_some_and(|v| !v.is_finite() || v <= 0. || v >= 90.)
        {
            return Err("Invalid height offset or chamfer angle".into());
        }
        if self.islands.iter().map(Vec::len).sum::<usize>() > 10000
            || self
                .islands
                .iter()
                .flatten()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 100000.)
        {
            return Err("Invalid face island boundaries".into());
        }
        if self.path.len() > 10000
            || self
                .path
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 100000.)
        {
            return Err("A contour needs at most 10,000 finite work-coordinate points".into());
        }
        for (name, value, min, max) in [
            ("Chamfer width", self.chamfer_width, 0.001, 100.),
            ("Pitch", self.pitch, 0.01, 100.),
            ("Thread depth", self.thread_depth, 0.001, 100.),
            ("Taper angle", self.taper_angle, -45., 45.),
        ] {
            if value.is_some_and(|v| !v.is_finite() || v < min || v > max) {
                return Err(format!("{name} is outside its supported range"));
            }
        }
        if self
            .start_depth
            .is_some_and(|v| !v.is_finite() || !(0.0..=10000.).contains(&v))
        {
            return Err(
                "Operation top depth must be between 0 and 10,000 mm below stock top".into(),
            );
        }
        if self
            .surface_step
            .is_some_and(|v| !v.is_finite() || !(0.005..=5.0).contains(&v))
        {
            return Err("3D surface grid step must be between 0.005 and 5 mm".into());
        }
        if self
            .path_tolerance
            .is_some_and(|v| !v.is_finite() || !(0.001..=1.0).contains(&v))
        {
            return Err("Path chord tolerance must be between 0.001 and 1 mm".into());
        }
        for (name, v, max) in [
            ("Cut feed", self.feed, 1e6),
            ("Plunge feed", self.plunge, 1e6),
            ("Stepdown / peck", self.step_down, 10000.0),
            ("Peck re-entry clearance", self.peck_clearance, 10000.0),
            ("Stepover percent", self.step_over_percent, 100.0),
            ("Clearance", self.clearance, 10000.0),
            ("Retract", self.retract, 10000.0),
            ("Finish feed", self.finish_feed, 1e6),
        ] {
            if v.is_some_and(|v| !v.is_finite() || v < 0.001 || v > max) {
                return Err(format!("{name} override is outside its supported range"));
            }
        }
        if self.spindle_rpm.is_some_and(|v| v == 0 || v > 1_000_000) || self.spring_passes > 20 {
            return Err("Invalid spindle override or spring-pass count".into());
        }
        if [self.radial_allowance, self.axial_allowance]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 10000.0)
        {
            return Err("Stock allowances must be finite and nonnegative".into());
        }
        Ok(())
    }
}
