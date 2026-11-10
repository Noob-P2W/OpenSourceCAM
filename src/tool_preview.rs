//! Illustrative tool/holder surfaces shared by both UIs, never collision geometry.
use crate::tool_library::{LibraryTool, ToolKind, ToolLibrary};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Dimension {
    pub key: String,
    pub label: String,
    pub value_mm: f64,
    pub start: [f64; 3],
    pub end: [f64; 3],
}
/// Measurement anchors share the assembly's tip-at-zero, positive-Z convention.
/// They illustrate supplied dimensions and do not replace collision geometry.
pub fn dimensions(tool: &LibraryTool) -> Vec<Dimension> {
    let radius = tool.dimension("diameter") / 2.;
    let flute = tool.dimension("flute_length");
    let mut result = Vec::new();
    for key in [
        "diameter",
        "shaft_diameter",
        "overall_length",
        "stickout",
        "shoulder_length",
        "flute_length",
        "gauge_length",
        "tip_diameter",
        "neck_diameter",
    ] {
        let Some(value) = tool
            .geometry
            .get(key)
            .copied()
            .filter(|v| v.is_finite() && *v > 0.)
        else {
            continue;
        };
        let Some(spec) = tool.kind.geometry().into_iter().find(|p| p.key == key) else {
            continue;
        };
        let (start, end) = if key.contains("diameter") {
            let z = if key == "shaft_diameter" {
                flute + (tool.dimension("overall_length") - flute) / 2.
            } else if key == "tip_diameter" {
                0.
            } else {
                flute / 2.
            };
            ([-value / 2., 0., z], [value / 2., 0., z])
        } else {
            ([radius, 0., 0.], [radius, 0., value])
        };
        result.push(Dimension {
            key: key.into(),
            label: spec.label.into(),
            value_mm: value,
            start,
            end,
        });
    }
    result
}

pub fn assembly(tool: &LibraryTool, library: &ToolLibrary) -> Vec<([[f64; 3]; 3], bool)> {
    let finite = |v: f64, default: f64| {
        if v.is_finite() && (0.001..=100000.0).contains(&v) {
            v
        } else {
            default
        }
    };
    let length = finite(tool.dimension("overall_length"), 60.0);
    let diameter = finite(
        tool.dimension("diameter"),
        finite(
            tool.dimension("body_diameter"),
            finite(tool.dimension("body_width"), 20.0),
        ),
    );
    let flute = finite(tool.dimension("flute_length"), length * 0.4).min(length);
    let shaft = finite(tool.dimension("shaft_diameter"), diameter * 0.65);
    let mut rings = Vec::<(f64, f64, bool)>::new();
    use ToolKind::*;
    match tool.kind {
        BallEndMill => {
            for i in 0..=8 {
                let a = i as f64 / 8.0 * std::f64::consts::FRAC_PI_2;
                rings.push((
                    diameter / 2.0 * (1.0 - a.cos()),
                    diameter / 2.0 * a.sin(),
                    true,
                ));
            }
            rings.extend([
                (flute, diameter / 2.0, true),
                (flute, shaft / 2.0, false),
                (length, shaft / 2.0, false),
            ]);
        }
        BullNoseEndMill | RadiusMill => {
            let r = finite(tool.dimension("corner_radius"), diameter / 8.0).min(diameter / 2.0);
            for i in 0..=6 {
                let a = i as f64 / 6.0 * std::f64::consts::FRAC_PI_2;
                rings.push((r * (1.0 - a.cos()), diameter / 2.0 - r + r * a.sin(), true));
            }
            rings.extend([
                (flute, diameter / 2.0, true),
                (flute, shaft / 2.0, false),
                (length, shaft / 2.0, false),
            ]);
        }
        LollipopMill => {
            for i in 0..=12 {
                let a = i as f64 / 12.0 * std::f64::consts::PI;
                rings.push((
                    diameter / 2.0 * (1.0 - a.cos()),
                    (diameter / 2.0 * a.sin()).max(0.01),
                    true,
                ));
            }
            let neck = finite(tool.dimension("neck_diameter"), diameter * 0.4);
            rings.extend([
                (diameter, neck / 2.0, false),
                (flute.max(diameter), neck / 2.0, false),
                (length, shaft / 2.0, false),
            ]);
        }
        ChamferMill | Countersink | TaperedMill | DovetailMill => {
            let tip = finite(
                tool.dimension("tip_diameter"),
                if tool.kind == DovetailMill {
                    diameter
                } else {
                    0.1
                },
            );
            let top = if tool.kind == DovetailMill {
                finite(tool.dimension("neck_diameter"), shaft)
            } else {
                diameter
            };
            rings.extend([
                (0.0, tip / 2.0, true),
                (flute, top / 2.0, true),
                (flute, shaft / 2.0, false),
                (length, shaft / 2.0, false),
            ]);
        }
        Drill | SpotDrill | CenterDrill => rings.extend([
            (0.0, 0.01, true),
            (diameter * 0.3, diameter / 2.0, true),
            (flute, diameter / 2.0, true),
            (flute, shaft / 2.0, false),
            (length, shaft / 2.0, false),
        ]),
        BarrelMill | LensMill | OvalMill | TaperBarrelMill => {
            for i in 0..=10 {
                let t = i as f64 / 10.0;
                rings.push((
                    flute * t,
                    diameter / 2.0 * (0.55 + 0.45 * (t * std::f64::consts::PI).sin()),
                    true,
                ));
            }
            rings.extend([(flute, shaft / 2.0, false), (length, shaft / 2.0, false)]);
        }
        PrintingHead | Laser | Plasma | Waterjet => {
            let nozzle = finite(
                tool.dimension("nozzle_diameter"),
                finite(tool.dimension("orifice_diameter"), 1.0),
            );
            rings.extend([
                (0.0, nozzle / 2.0, true),
                (length * 0.18, diameter * 0.25, true),
                (length * 0.3, diameter / 2.0, false),
                (length * 0.85, diameter / 2.0, false),
                (length, diameter * 0.3, false),
            ]);
        }
        Probe => {
            let ball = finite(tool.dimension("stylus_diameter"), 3.0);
            for i in 0..=8 {
                let a = i as f64 / 8.0 * std::f64::consts::PI;
                rings.push((
                    ball / 2.0 * (1.0 - a.cos()),
                    (ball / 2.0 * a.sin()).max(0.01),
                    true,
                ));
            }
            rings.extend([
                (ball, ball * 0.15, false),
                (length * 0.6, ball * 0.15, false),
                (length * 0.6, diameter / 2.0, false),
                (length, diameter / 2.0, false),
            ]);
        }
        SlotMill => {
            let width = finite(tool.dimension("cutting_width"), 2.0);
            let neck = finite(tool.dimension("neck_diameter"), shaft / 2.0);
            rings.extend([
                (0.0, diameter / 2.0, true),
                (width, diameter / 2.0, true),
                (width, neck / 2.0, false),
                (flute, neck / 2.0, false),
                (length, shaft / 2.0, false),
            ]);
        }
        GrindingWheel => {
            let width = finite(tool.dimension("wheel_width"), 20.0);
            rings.extend([
                (0.0, diameter / 2.0, true),
                (width, diameter / 2.0, true),
                (width, diameter * 0.1, false),
                (length, diameter * 0.1, false),
            ]);
        }
        _ => rings.extend([
            (0.0, diameter / 2.0, true),
            (flute, diameter / 2.0, true),
            (flute, shaft / 2.0, false),
            (length, shaft / 2.0, false),
        ]),
    }
    if !tool.shaft.is_empty() {
        rings.retain(|(z, _, _)| *z <= flute);
        let mut z = flute;
        for section in &tool.shaft {
            rings.push((z, finite(section.lower_diameter, 6.0) / 2.0, false));
            z += finite(section.height, 5.0);
            rings.push((z, finite(section.upper_diameter, 6.0) / 2.0, false));
        }
    }
    if let Some(holder) = tool.holder.and_then(|id| library.get(id).ok()) {
        let z = finite(tool.dimension("stickout"), length * 0.7);
        let r = finite(holder.dimension("body_diameter"), 30.0) / 2.0;
        let h = finite(holder.dimension("overall_length"), 60.0);
        rings.extend([(z, shaft / 2.0, false), (z, r, false), (z + h, r, false)]);
    }
    surface_triangles(&rings)
}

// Close both ends, including the annular shoulders between adjacent rings.
fn surface_triangles(rings: &[(f64, f64, bool)]) -> Vec<([[f64; 3]; 3], bool)> {
    let mut result = Vec::new();
    let point = |ring: (f64, f64, bool), i: usize| {
        let angle = i as f64 / 32.0 * std::f64::consts::TAU;
        [ring.1 * angle.cos(), ring.1 * angle.sin(), ring.0]
    };
    for pair in rings.windows(2) {
        for i in 0..32 {
            let a = point(pair[0], i);
            let b = point(pair[0], i + 1);
            let c = point(pair[1], i + 1);
            let d = point(pair[1], i);
            result.push(([a, b, c], pair[1].2));
            result.push(([a, c, d], pair[1].2));
        }
    }
    for (index, ring) in [rings.first(), rings.last()].into_iter().enumerate() {
        if let Some(ring) = ring {
            for i in 0..32 {
                let center = [0.0, 0.0, ring.0];
                let a = point(*ring, i);
                let b = point(*ring, i + 1);
                result.push((
                    if index == 0 {
                        [center, b, a]
                    } else {
                        [center, a, b]
                    },
                    ring.2,
                ));
            }
        }
    }
    result.retain(|(triangle, _)| {
        crate::import::normal(*triangle)
            .iter()
            .map(|v| v * v)
            .sum::<f64>()
            > 1e-20
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_desktop_tool_kind_has_a_finite_shared_preview() {
        for kind in ToolKind::ALL {
            let tool = LibraryTool::new(1, 1, kind);
            let library = ToolLibrary {
                tools: vec![tool.clone()],
            };
            let triangles = assembly(&tool, &library);
            assert!(!triangles.is_empty(), "{kind:?}");
            assert!(triangles.len() < 5000);
            assert!(triangles
                .iter()
                .flat_map(|(t, _)| t.iter().flatten())
                .all(|v| v.is_finite()));
        }
    }
    #[test]
    fn revolved_tool_is_closed_at_both_ends_and_pointed_tips() {
        for rings in [
            vec![
                (0.0, 3.0, true),
                (20.0, 3.0, true),
                (20.0, 2.0, false),
                (60.0, 2.0, false),
            ],
            vec![(0.0, 0.0, true), (3.0, 3.0, true), (60.0, 3.0, false)],
        ] {
            let mut edges = std::collections::BTreeMap::new();
            for (triangle, _) in surface_triangles(&rings) {
                let v = triangle.map(|p| p.map(|x| (x * 1e8).round() as i64));
                for (a, b) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
                    *edges
                        .entry(if a < b { (a, b) } else { (b, a) })
                        .or_insert(0) += 1;
                }
            }
            assert!(
                edges.values().all(|count| *count == 2),
                "Tool surface has open boundary edges"
            );
        }
    }
}
