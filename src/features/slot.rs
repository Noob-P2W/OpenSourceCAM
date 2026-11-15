//! Separate a straight capsule slot's vertical walls from its entry chamfer.
use super::*;
#[derive(Clone, Debug)]
pub struct SlotGeometry {
    pub centerline: Vec<[f64; 3]>,
    pub boundary: Vec<[f64; 3]>,
    pub width: f64,
    pub wall_top: f64,
    pub bottom: f64,
    pub opening: f64,
    pub chamfer_width: f64,
    pub chamfer_angle: Option<f64>,
}
fn capsule(ring: &[[f64; 3]]) -> Option<(Vec<[f64; 3]>, f64)> {
    if ring.len() < 8 {
        return None;
    }
    let lo: [f64; 2] =
        std::array::from_fn(|i| ring.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
    let hi: [f64; 2] =
        std::array::from_fn(|i| ring.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max));
    let dx = hi[0] - lo[0];
    let dy = hi[1] - lo[1];
    let r = dx.min(dy) / 2.;
    if r < 0.01 || (dx - dy).abs() < r * 0.1 {
        return None;
    }
    let c = [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.];
    let mut ends = if dx > dy {
        vec![[lo[0] + r, c[1], 0.], [hi[0] - r, c[1], 0.]]
    } else {
        vec![[c[0], lo[1] + r, 0.], [c[0], hi[1] - r, 0.]]
    };
    // Tessellation need not include an arc's longitudinal extremum. Recover
    // cap centres from their circular samples rather than shortening the slot.
    let axis = if dx > dy { 0 } else { 1 };
    let lateral = 1 - axis;
    for (index, sign) in [(0, 1.), (1, -1.)] {
        let samples: Vec<_> = ring
            .iter()
            .filter(|p| sign * (ends[index][axis] - p[axis]) > r * 0.2)
            .map(|p| p[axis] + sign * (r * r - (p[lateral] - c[lateral]).powi(2)).max(0.).sqrt())
            .collect();
        if samples.len() < 2 {
            return None;
        }
        ends[index][axis] = samples.iter().sum::<f64>() / samples.len() as f64;
    }
    if ring
        .iter()
        .any(|p| (crate::planar::distance([p[0], p[1]], ends[0], ends[1]) - r).abs() > 0.025)
    {
        return None;
    }
    Some((ends, 2. * r))
}
pub fn geometry(feature: &Feature) -> Result<SlotGeometry, String> {
    let rings = &feature.detail.profiles;
    if rings.len() < 2 {
        return Err("Select a straight slot with vertical walls and optional entry chamfer".into());
    }
    let mut found = None;
    for i in 0..rings.len() - 1 {
        if let (Some((a, w)), Some((b, v))) = (capsule(&rings[i]), capsule(&rings[i + 1])) {
            if (w - v).abs() > TOLERANCE
                || a.iter()
                    .zip(&b)
                    .any(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) > TOLERANCE)
            {
                continue;
            }
            let top = rings[i][0][2];
            let bottom = rings[i + 1][0][2];
            if top <= bottom + TOLERANCE {
                continue;
            }
            if found
                .as_ref()
                .is_some_and(|s: &SlotGeometry| s.wall_top - s.bottom >= top - bottom)
            {
                continue;
            }
            let (_, opening_width) =
                capsule(&rings[0]).ok_or("Slot entry is not a straight capsule")?;
            let chamfer_width = (opening_width - w) / 2.;
            let dz = feature.opening_z - top;
            if chamfer_width < -TOLERANCE {
                return Err("Slot narrows toward its opening".into());
            }
            let angle = if chamfer_width > TOLERANCE && dz > TOLERANCE {
                Some((dz / chamfer_width).atan().to_degrees())
            } else {
                None
            };
            // Entry sections must be concentric and linear, not a rounded or undercut transition.
            for ring in &rings[..=i] {
                let (ends, width) =
                    capsule(ring).ok_or("Slot entry profile is not a straight capsule")?;
                let ratio = if dz > TOLERANCE {
                    (ring[0][2] - top) / dz
                } else {
                    0.
                };
                let expected = w + 2. * chamfer_width * ratio;
                if (width - expected).abs() > 0.025
                    || ends
                        .iter()
                        .zip(&a)
                        .any(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) > 0.025)
                {
                    return Err("Slot entry is curved; select the vertical slot separately".into());
                }
            }
            found = Some(SlotGeometry {
                centerline: a,
                boundary: rings[i].clone(),
                width: w,
                wall_top: top,
                bottom,
                opening: feature.opening_z,
                chamfer_width,
                chamfer_angle: angle,
            });
        }
    }
    found.ok_or("This selection is not a straight slot with vertical walls".into())
}
pub fn draft(
    project: &Project,
    feature: &Feature,
    strategy: Strategy,
) -> Result<Operation, String> {
    let slot = geometry(feature)?;
    if let Some(part) = &project.part {
        for triangle in &part.geometry.mesh.triangles {
            let poly = crate::surface::clip(
                triangle
                    .iter()
                    .map(|i| part.point(part.geometry.mesh.vertices[*i as usize]))
                    .collect(),
                2,
                slot.opening + TOLERANCE,
                true,
            );
            if poly.len() < 2 {
                continue;
            }
            let distance = (0..poly.len())
                .map(|i| {
                    crate::planar::segment_distance(
                        slot.centerline[0],
                        slot.centerline[1],
                        poly[i],
                        poly[(i + 1) % poly.len()],
                    )
                })
                .fold(f64::INFINITY, f64::min);
            let contains = poly.len() >= 3
                && (crate::planar::inside([slot.centerline[0][0], slot.centerline[0][1]], &poly)
                    || crate::planar::inside(
                        [slot.centerline[1][0], slot.centerline[1][1]],
                        &poly,
                    ));
            if contains || distance < slot.width / 2. - TOLERANCE {
                return Err("The selected slot is obstructed above its opening".into());
            }
        }
    }
    let mut op = crate::commands::operation_template(project, strategy);
    op.enabled = true;
    op.x = slot.centerline[0][0];
    op.y = slot.centerline[0][1];
    op.height = slot.width;
    op.width = (slot.centerline[1][0] - op.x).hypot(slot.centerline[1][1] - op.y);
    if strategy == Strategy::Slot {
        op.settings.path = slot.centerline;
        op.depth = -slot.bottom;
        op.settings.start_depth = Some((-slot.opening).max(0.));
    } else {
        if slot.chamfer_width <= TOLERANCE {
            return Err("No planar entry chamfer was measured on this slot".into());
        }
        op.settings.path = slot
            .boundary
            .iter()
            .map(|p| [p[0], p[1], slot.opening])
            .collect();
        op.settings.closed = true;
        op.settings.outside = false;
        op.settings.chamfer_width = Some(slot.chamfer_width);
        op.settings.chamfer_angle = slot.chamfer_angle;
        op.depth = (slot.opening - slot.wall_top) + 0.5;
        op.settings.start_depth = None;
    }
    if let Some(part) = &project.part {
        crate::machining::link(&mut op, part);
    }
    Ok(op)
}
