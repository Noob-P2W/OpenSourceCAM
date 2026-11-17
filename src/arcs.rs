//! NC uses only arcs retained by analytical generators, never inferred polygon fits.
use crate::{
    cam::{ArcSpan, Motion, Point3, Toolpath},
    model::Project,
};
pub fn preflight(project: &Project, path: &Toolpath) -> Result<bool, String> {
    let mut end = 0;
    let mut curved = Toolpath {
        arcs: Vec::new(),
        moves: Vec::new(),
        cut_length: 0.,
        rapid_length: 0.,
        estimated_seconds: 0.,
    };
    let mut margin = 0.0002_f64;
    for arc in &path.arcs {
        if arc.start < end
            || arc.start >= arc.end
            || arc.end > path.moves.len()
            || !arc.sweep.is_finite()
            || arc.sweep.abs() > std::f64::consts::PI + 1e-7
            || arc.sweep.abs() < 1e-12
        {
            return Err("Invalid analytical arc span".into());
        }
        end = arc.end;
        let moves = &path.moves[arc.start..arc.end];
        let first = &moves[0];
        let last = &moves[moves.len() - 1];
        let radius = (first.from.x - arc.center[0]).hypot(first.from.y - arc.center[1]);
        let phase = (first.from.y - arc.center[1]).atan2(first.from.x - arc.center[0]);
        if radius <= 1e-6 || !radius.is_finite() {
            return Err("Invalid analytical arc radius".into());
        }
        for (i, m) in moves.iter().enumerate() {
            if m.motion != Motion::Cut
                || m.operation != first.operation
                || (m.feed - first.feed).abs() > 1e-7 * first.feed.max(1.)
                || i > 0 && m.from.distance(moves[i - 1].to) > 1e-7
            {
                return Err("Arc crosses a feed, operation or continuity boundary".into());
            }
            let angle = phase + arc.sweep * (i + 1) as f64 / moves.len() as f64;
            let expected = Point3 {
                x: arc.center[0] + radius * angle.cos(),
                y: arc.center[1] + radius * angle.sin(),
                z: first.from.z + (last.to.z - first.from.z) * (i + 1) as f64 / moves.len() as f64,
            };
            if expected.distance(m.to) > 1e-5 {
                return Err("Arc samples disagree with the analytical curve".into());
            }
        }
        // The inflated segment tubes contain the complete true arc and posted rounding.
        let sag = radius * (1. - (arc.sweep / moves.len() as f64 / 2.).cos());
        margin = margin.max(sag + 0.0002);
        curved.moves.extend_from_slice(moves);
        if let Some(machine) = &project.machine {
            let mut extrema = vec![(phase, 0.), (phase + arc.sweep, arc.sweep.abs())];
            for k in 0..4 {
                let theta = k as f64 * std::f64::consts::FRAC_PI_2;
                let delta = if arc.sweep > 0. {
                    (theta - phase).rem_euclid(std::f64::consts::TAU)
                } else {
                    (phase - theta).rem_euclid(std::f64::consts::TAU)
                };
                extrema.push((theta, delta));
            }
            for (theta, delta) in extrema {
                if delta <= arc.sweep.abs() + 1e-8 {
                    let z = first.from.z + (last.to.z - first.from.z) * delta / arc.sweep.abs();
                    let point = Point3 {
                        x: arc.center[0] + radius * theta.cos(),
                        y: arc.center[1] + radius * theta.sin(),
                        z,
                    };
                    for dx in [-0.0002, 0.0002] {
                        for dy in [-0.0002, 0.0002] {
                            if machine
                                .check_point(Point3 {
                                    x: point.x + dx,
                                    y: point.y + dy,
                                    z: point.z,
                                })
                                .is_err()
                            {
                                return Ok(false);
                            }
                        }
                    }
                    for dz in [-0.000002, 0.000002] {
                        if machine
                            .check_point(Point3 {
                                z: point.z + dz,
                                ..point
                            })
                            .is_err()
                        {
                            return Ok(false);
                        }
                    }
                    let direction = arc.sweep.signum();
                    let tangent = Point3 {
                        x: point.x - direction * theta.sin(),
                        y: point.y + direction * theta.cos(),
                        z: point.z + (last.to.z - first.from.z) / (radius * arc.sweep.abs()),
                    };
                    let movement = crate::cam::Move {
                        from: point,
                        to: tangent,
                        feed: first.feed,
                        motion: Motion::Cut,
                        operation: first.operation,
                    };
                    if machine.check_move(&movement).is_err() {
                        return Ok(false);
                    }
                }
            }
        }
    }
    if curved.moves.is_empty() {
        return Ok(true);
    }
    Ok(crate::collision::check_with_margin(project, &curved, margin).is_ok())
}
pub fn endpoint(path: &Toolpath, arc: &ArcSpan) -> Point3 {
    path.moves[arc.end - 1].to
}
