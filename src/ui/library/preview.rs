//! Original CPU-rendered assembly preview; never used as collision geometry.
use super::*;

pub(super) fn icon(painter: &egui::Painter, kind: ToolKind, rect: Rect) {
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + rect.width() * x / 36.0,
            rect.top() + rect.height() * y / 36.0,
        )
    };
    let steel = Color32::from_rgb(147, 181, 202);
    let gold = Color32::from_rgb(216, 176, 74);
    if kind.rotates() {
        painter.rect_filled(Rect::from_min_max(p(14.0, 0.0), p(22.0, 16.0)), 1, steel);
        let width = if matches!(
            kind,
            ToolKind::FaceMill | ToolKind::SlotMill | ToolKind::LollipopMill
        ) {
            24.0
        } else {
            13.0
        };
        let bottom = if matches!(
            kind,
            ToolKind::Drill
                | ToolKind::CenterDrill
                | ToolKind::SpotDrill
                | ToolKind::ChamferMill
                | ToolKind::Countersink
        ) {
            28.0
        } else {
            35.0
        };
        painter.rect_filled(
            Rect::from_min_max(p(18.0 - width / 2.0, 16.0), p(18.0 + width / 2.0, bottom)),
            if matches!(
                kind,
                ToolKind::BallEndMill | ToolKind::BullNoseEndMill | ToolKind::LollipopMill
            ) {
                5
            } else {
                1
            },
            gold,
        );
        if bottom < 35.0 {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    p(18.0 - width / 2.0, 28.0),
                    p(18.0 + width / 2.0, 28.0),
                    p(18.0, 36.0),
                ],
                gold,
                Stroke::NONE,
            ));
        }
        for y in [19.0, 24.0, 29.0] {
            painter.line_segment(
                [p(18.0 - width / 2.0, y + 3.0), p(18.0 + width / 2.0, y)],
                Stroke::new(1.2_f32, Color32::from_rgb(100, 111, 109)),
            );
        }
    } else if kind.group() == "Turning" {
        painter.rect_filled(Rect::from_min_max(p(8.0, 5.0), p(19.0, 35.0)), 1, steel);
        painter.add(egui::Shape::convex_polygon(
            vec![p(9.0, 2.0), p(27.0, 10.0), p(12.0, 16.0)],
            gold,
            Stroke::NONE,
        ));
    } else {
        painter.rect_filled(Rect::from_min_max(p(7.0, 3.0), p(29.0, 21.0)), 2, steel);
        let color = match kind {
            ToolKind::Laser => Color32::from_rgb(238, 103, 91),
            ToolKind::PrintingHead => Color32::from_rgb(233, 164, 62),
            ToolKind::Probe => Color32::from_rgb(224, 84, 134),
            _ => ACCENT,
        };
        painter.add(egui::Shape::convex_polygon(
            vec![p(7.0, 21.0), p(29.0, 21.0), p(18.0, 32.0)],
            color,
            Stroke::NONE,
        ));
        if kind == ToolKind::Probe {
            painter.circle_filled(p(18.0, 32.0), 3.0, color);
        } else {
            painter.line_segment([p(18.0, 30.0), p(18.0, 36.0)], Stroke::new(1.8_f32, color));
        }
    }
}

pub(crate) fn draw(
    ui: &mut egui::Ui,
    tool: &LibraryTool,
    library: &ToolLibrary,
    camera: &mut Camera,
    zoom: &mut f32,
    size: Vec2,
) -> Rect {
    let (response, painter) =
        ui.allocate_painter(size.max(Vec2::splat(100.0)), egui::Sense::drag());
    let rect = response.rect;
    painter.rect_filled(rect, 4, BG);
    if response.dragged() {
        let d = response.drag_delta();
        camera.orbit(d.x as f64, d.y as f64);
    }
    if response.hovered() {
        *zoom = (*zoom * (ui.input(|i| i.smooth_scroll_delta.y) * 0.002).exp()).clamp(0.25, 3.0);
    }
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
    let mut faces = Vec::new();
    let mut bounds = Rect::NOTHING;
    let project = |point: [f64; 3]| {
        let v = camera.project(point);
        (Pos2::new(v[0] as f32, v[1] as f32), v[2])
    };
    for (vertices, cutting) in surface_triangles(&rings) {
        let normal = opensourcecam::import::normal(vertices);
        if camera.project(normal)[2] <= 0.0 {
            continue;
        }
        let transformed = vertices.map(project);
        let depth = transformed.iter().map(|(_, z)| z).sum::<f64>() / 3.0;
        let color = if cutting {
            Color32::from_rgb(190, 152, 64)
        } else {
            Color32::from_rgb(137, 163, 178)
        };
        let center = [
            vertices.iter().map(|v| v[0]).sum::<f64>(),
            vertices.iter().map(|v| v[1]).sum::<f64>(),
        ];
        let light = if normal[0].abs() + normal[1].abs() < 1e-9 {
            0.94_f32
        } else {
            (0.82 + 0.17 * (center[1].atan2(center[0]) - camera.yaw + 0.5).cos()) as f32
        };
        let color = Color32::from_rgb(
            (color.r() as f32 * light) as u8,
            (color.g() as f32 * light) as u8,
            (color.b() as f32 * light) as u8,
        );
        let points = transformed.map(|(p, _)| p);
        for point in points {
            bounds.extend_with(point);
        }
        faces.push((depth, points, color));
    }
    if bounds.is_finite() && bounds.width() > 0.0 && bounds.height() > 0.0 {
        let scale = ((rect.width() - 35.0) / bounds.width())
            .min((rect.height() - 55.0) / bounds.height())
            * 0.85
            * *zoom;
        let screen = |point: Pos2| rect.center() + (point - bounds.center()) * scale;
        let clipped = painter.with_clip_rect(rect);
        for i in 0..=10 {
            let x = rect.left() + rect.width() * i as f32 / 10.0;
            let y = rect.top() + rect.height() * i as f32 / 10.0;
            clipped.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.5_f32, LINE),
            );
            clipped.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(0.5_f32, LINE),
            );
        }
        faces.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, vertices, color) in faces {
            let mut mesh = egui::Mesh::default();
            for point in vertices.map(screen) {
                mesh.colored_vertex(point, color);
            }
            mesh.add_triangle(0, 1, 2);
            clipped.add(egui::Shape::mesh(mesh));
        }
        clipped.text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "ASSEMBLY PREVIEW",
            egui::FontId::proportional(10.0),
            MUTED,
        );
        clipped.text(
            rect.left_bottom() + Vec2::new(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            "Drag to orbit · wheel to zoom",
            egui::FontId::proportional(10.0),
            MUTED,
        );
    }
    ui.horizontal(|ui| {
        if ui.small_button("Fit preview").clicked() {
            *camera = Camera::default();
            *zoom = 1.0;
        }
        ui.label(
            RichText::new("Illustrative geometry")
                .size(10.0)
                .color(MUTED),
        );
    });
    rect
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
        opensourcecam::import::normal(*triangle)
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
