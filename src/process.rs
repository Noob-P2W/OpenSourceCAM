//! Project-level process selection. Strategies are enabled only when implemented.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Process {
    #[default]
    Milling,
    Turning,
    #[serde(rename = "printing_3d")]
    Printing3d,
    LaserCutting,
    PlasmaCutting,
    Grinding,
    WireEdm,
    SinkerEdm,
    Mixed,
}

impl Process {
    /// Mixed deliberately stays last, including in the main menu.
    pub const ALL: [Self; 9] = [
        Self::Milling,
        Self::Turning,
        Self::Printing3d,
        Self::LaserCutting,
        Self::PlasmaCutting,
        Self::Grinding,
        Self::WireEdm,
        Self::SinkerEdm,
        Self::Mixed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Milling => "Milling",
            Self::Turning => "Turning",
            Self::Printing3d => "3D printing",
            Self::LaserCutting => "Laser cutting",
            Self::PlasmaCutting => "Plasma cutting",
            Self::Grinding => "Grinding",
            Self::WireEdm => "Wire EDM",
            Self::SinkerEdm => "Electrode EDM",
            Self::Mixed => "Mixed",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Milling => "Pockets, profiles and drilling",
            Self::Turning => "Rotational parts and lathe work",
            Self::Printing3d => "Slicing and additive manufacturing",
            Self::LaserCutting => "Cutting, engraving and raster work",
            Self::PlasmaCutting => "Sheet cutting and pierced contours",
            Self::Grinding => "Surface and cylindrical finishing",
            Self::WireEdm => "Wire-cut profiles and tapered parts",
            Self::SinkerEdm => "Electrodes, cavities and sinker EDM",
            Self::Mixed => "DIY and special-purpose machines",
        }
    }

    pub fn supports_toolpaths(self) -> bool {
        self == Self::Milling
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Milling => "2.5D operations ready",
            Self::Mixed => "Developed last",
            _ => "Strategies planned",
        }
    }

    pub fn planned_operations(self) -> &'static [&'static str] {
        match self {
            Self::Milling => &[],
            Self::Turning => &["Facing", "Rough turn", "Finish turn", "Groove", "Thread"],
            Self::Printing3d => &["Slice model", "Infill", "Supports"],
            Self::LaserCutting => &["Cut contour", "Engrave", "Raster"],
            Self::PlasmaCutting => &["Cut contour", "Pierce", "Lead-in"],
            Self::Grinding => &["Surface grind", "Cylindrical grind", "Dress wheel"],
            Self::WireEdm => &["Wire contour", "Taper cut", "Thread wire"],
            Self::SinkerEdm => &["Cavity", "Electrode", "Rough / finish"],
            Self::Mixed => &["Custom operation", "Process chain"],
        }
    }
}
