//! Direct planar offsets for clearing around a selected island.
use crate::{
    cam::Point3,
    model::{Operation, Project},
    operation::Direction,
    roughing::{follows_walk, CornerArc, Span},
    surface::Budget,
};
use clipper2_rust::{
    is_positive, poly_tree_to_faces64, ClipType, Clipper64, ClipperOffset, EndType, FillRule,
    JoinType, Path64, Paths64, Point64, PolyTree64, Rect64,
};

const SCALE: f64 = 100_000.;
pub(crate) const SAG: f64 = 0.002;
// Reserve integer clipping, offset tessellation, curve reconstruction and NC rounding.
// This is extra radial clearance; it does not consume the requested allowance.
const CLEANUP: f64 = 0.003;
const RESERVE: f64 = 0.012;
pub(crate) const MAX_EDGES: usize = 262_144;
const MAX_WORK: usize = 50_000_000;
const MAX_LEVELS: usize = 10_000;

fn vertices(paths: &Paths64) -> usize {
    paths.iter().map(Vec::len).sum()
}

fn polygon(points: &[[f64; 3]]) -> Path64 {
    let mut path: Path64 = points
        .iter()
        .map(|p| Point64::new((p[0] * SCALE).round() as i64, (p[1] * SCALE).round() as i64))
        .collect();
    path.dedup();
    if path.len() > 1 && path.first() == path.last() {
        path.pop();
    }
    path
}

fn offset(paths: &Paths64, delta: f64, join: JoinType, end: EndType) -> Result<Paths64, String> {
    let mut engine = ClipperOffset::new(2., SAG * SCALE, false, false);
    engine.add_paths(paths, join, end);
    let mut result = Paths64::new();
    engine.execute(delta, &mut result);
    if engine.error_code() != 0 || vertices(&result) > MAX_EDGES {
        return Err("Outside clearing offset exceeds its geometry limits".into());
    }
    Ok(result)
}

fn protect(obstacles: &[Vec<[f64; 3]>], budget: &mut Budget<'_>) -> Result<Paths64, String> {
    if obstacles.iter().map(Vec::len).sum::<usize>() > MAX_EDGES {
        return Err("Outside clearing exceeds its projected-edge limit".into());
    }
    let mut areas = Paths64::new();
    let mut lines = Paths64::new();
    for (i, points) in obstacles.iter().enumerate() {
        budget.tick()?;
        let mut path = polygon(points);
        // Integer shoelace sum, including very small triangles at large coordinates.
        let area: i128 = path
            .iter()
            .zip(path.iter().cycle().skip(1))
            .map(|(a, b)| a.x as i128 * b.y as i128 - b.x as i128 * a.y as i128)
            .sum();
        if area == 0 {
            if i == 0 {
                return Err("Select a closed outside boundary with nonzero area".into());
            }
            // A vertical projected face may be only a line. Keep it as a thin
            // obstacle rather than assuming adjacent triangles cover it.
            if let (Some(a), Some(b)) = (
                path.iter().min_by_key(|p| (p.x, p.y)),
                path.iter().max_by_key(|p| (p.x, p.y)),
            ) {
                lines.push(if a == b { vec![*a] } else { vec![*a, *b] });
            }
        } else {
            if area < 0 {
                path.reverse();
            }
            areas.push(path);
        }
    }
    if !lines.is_empty() {
        // Square joins give the thin projection a small conservative width.
        areas.extend(offset(&lines, 4., JoinType::Square, EndType::Square)?);
    }
    let mut engine = Clipper64::new();
    engine.add_subject(&areas);
    let mut union = Paths64::new();
    if !engine.execute(ClipType::Union, FillRule::NonZero, &mut union, None)
        || union.is_empty()
        || vertices(&union) > MAX_EDGES
    {
        return Err("Could not construct the protected Outside clearing region".into());
    }
    Ok(union)
}

pub(crate) fn paths(
    project: &Project,
    op: &Operation,
    work: &[[f64; 3]],
    obstacles: &[Vec<[f64; 3]>],
    z: f64,
    budget: &mut Budget<'_>,
) -> Result<Vec<Vec<Span>>, String> {
    let tool = project.operation_tool(op)?;
    let radius = tool.diameter / 2. + op.settings.radial_allowance;
    let stepover = tool.diameter * tool.step_over.min(0.5);
    let blend = (stepover * 0.05).min(0.045);
    let step = stepover * 0.9;
    let first = radius + RESERVE + blend;
    let ring = polygon(work);
    let [lo, hi] = [ring[0], ring[2]];
    let width = (hi.x - lo.x).min(hi.y - lo.y) as f64 / SCALE;
    let count = ((width / 2. - first) / step).ceil().max(0.) as usize;
    if count > MAX_LEVELS {
        return Err(
            "Outside clearing exceeds its pass limit; increase stepover or split the setup".into(),
        );
    }
    let protected = protect(obstacles, budget)?;
    if count.saturating_mul(vertices(&protected)) > MAX_WORK {
        return Err(
            "Outside clearing exceeds its calculation budget; increase stepover or split the setup"
                .into(),
        );
    }
    let stock_x = (project.stock.width * SCALE).ceil() as i64;
    let stock_y = (project.stock.height * SCALE).ceil() as i64;
    let reverse = op.settings.direction == Direction::Clockwise;
    let mut reachable = Paths64::new();
    let mut result = Vec::new();
    let mut total = 0;
    for level in 0..count {
        budget.tick()?;
        let delta = (first + level as f64 * step) * SCALE;
        let blocked = offset(&protected, delta, JoinType::Round, EndType::Polygon)?;
        if blocked.is_empty() {
            return Err("Outside clearing lost the protected region during offsetting".into());
        }
        let d = delta.ceil() as i64;
        let limits = Rect64::new(lo.x + d, lo.y + d, hi.x - d, hi.y - d);
        let mut engine = Clipper64::new();
        engine.add_subject(&vec![limits.as_path()]);
        engine.add_clip(&blocked);
        let mut free = PolyTree64::new();
        if !engine.execute_tree(
            ClipType::Difference,
            FillRule::NonZero,
            &mut free,
            &mut Vec::new(),
        ) {
            return Err(
                "Could not subtract protected geometry from the Outside clearing passes".into(),
            );
        }
        let mut rings = Paths64::new();
        for face in poly_tree_to_faces64(&free) {
            if level > 0
                || face
                    .outer
                    .iter()
                    .any(|p| p.x < 0 || p.y < 0 || p.x > stock_x || p.y > stock_y)
            {
                rings.push(face.outer);
                rings.extend(face.holes);
            }
        }
        if level == 0 {
            reachable.clone_from(&rings);
        } else {
            // An interior hole disconnected at cutter radius is not an
            // outside-clearing region, even after neighboring offsets merge.
            let mut engine = Clipper64::new();
            engine.add_subject(&rings);
            engine.add_clip(&reachable);
            if !engine.execute(ClipType::Intersection, FillRule::NonZero, &mut rings, None) {
                return Err("Could not retain exterior-connected Outside clearing passes".into());
            }
        }
        if rings.is_empty() {
            break;
        }
        total += vertices(&rings);
        if total > MAX_EDGES {
            return Err("Outside clearing exceeds its path point limit; increase stepover or split the setup".into());
        }
        // Cut the stock edge first at each offset, then the island boundaries.
        rings.sort_by(|a, b| {
            is_positive(b).cmp(&is_positive(a)).then_with(|| {
                clipper2_rust::area(b)
                    .abs()
                    .total_cmp(&clipper2_rust::area(a).abs())
            })
        });
        for mut ring in rings {
            budget.tick()?;
            if reverse {
                ring.reverse();
            }
            result.push(spans(&ring, z, blend, budget)?);
        }
    }
    if result.is_empty() {
        return Err("No cutter-sized region is reachable from the outside of the stock".into());
    }
    Ok(result)
}

fn spans(ring: &Path64, z: f64, blend: f64, budget: &mut Budget<'_>) -> Result<Vec<Span>, String> {
    let mut points: Vec<_> = ring
        .iter()
        .map(|p| Point3 {
            x: p.x as f64 / SCALE,
            y: p.y as f64 / SCALE,
            z,
        })
        .collect();
    let start = (0..points.len()).find(|&i| {
        points[i].distance(points[(i + 1) % points.len()]) > 4. * CLEANUP
            && points[i].distance(points[(i + points.len() - 1) % points.len()]) > 4. * CLEANUP
    });
    points.rotate_left(start.unwrap_or(0));
    points.push(points[0]);
    let mut clean = vec![points[0]];
    let mut begin = 0;
    while begin + 1 < points.len() {
        budget.tick()?;
        let end = (begin + 2..=(begin + 8).min(points.len() - 1))
            .rev()
            .find(|&end| {
                // Drop clipping slivers without approximating an ordinary curve.
                points[begin..=end]
                    .windows(2)
                    .any(|p| p[0].distance(p[1]) < CLEANUP)
                    && follows_walk(&points[begin..=end], points[begin], points[end], CLEANUP)
            })
            .unwrap_or(begin + 1);
        clean.push(points[end]);
        begin = end;
    }
    let points = clean;
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < points.len() {
        budget.tick()?;
        if let Some((count, arc)) = fit_round(&points[i..]) {
            result.push(Span {
                from: points[i],
                to: points[i + count - 1],
                arc: Some(arc),
            });
            i += count - 1;
        } else {
            result.push(Span::line(points[i], points[i + 1]));
            i += 1;
        }
    }
    blend_lines(&result, blend, budget)
}

fn blend_lines(
    source: &[Span],
    deviation: f64,
    budget: &mut Budget<'_>,
) -> Result<Vec<Span>, String> {
    if source.len() < 3 {
        return Ok(source.to_vec());
    }
    let mut blends = Vec::with_capacity(source.len());
    for i in 0..source.len() {
        budget.tick()?;
        let next = source[(i + 1) % source.len()];
        blends.push(blend_corner(source[i], next, deviation));
    }
    let mut result = Vec::new();
    for (i, &span) in source.iter().enumerate() {
        let previous = blends[(i + blends.len() - 1) % blends.len()];
        result.push(Span {
            from: previous.map_or(span.from, |arc| arc.to),
            to: blends[i].map_or(span.to, |arc| arc.from),
            ..span
        });
        result.extend(blends[i]);
    }
    Ok(result)
}

fn blend_corner(a: Span, b: Span, deviation: f64) -> Option<Span> {
    if a.arc.is_some() || b.arc.is_some() {
        return None;
    }
    let v = a.to;
    let l = a.from.distance(v);
    let m = v.distance(b.to);
    if l.min(m) < 0.015 {
        return None;
    }
    let u = [(v.x - a.from.x) / l, (v.y - a.from.y) / l];
    let w = [(b.to.x - v.x) / m, (b.to.y - v.y) / m];
    let sweep = (u[0] * w[1] - u[1] * w[0]).atan2(u[0] * w[0] + u[1] * w[1]);
    let angle = sweep.abs();
    if !(0.035..2.85).contains(&angle) {
        return None;
    }
    let trim = (0.48 * l.min(m)).min(deviation / (angle / 4.).tan());
    let r = trim / (angle / 2.).tan();
    if r < 0.03 {
        return None;
    }
    let from = Point3 {
        x: v.x - trim * u[0],
        y: v.y - trim * u[1],
        z: v.z,
    };
    Some(Span {
        from,
        to: Point3 {
            x: v.x + trim * w[0],
            y: v.y + trim * w[1],
            z: v.z,
        },
        arc: Some(CornerArc {
            center: [
                from.x - sweep.signum() * r * u[1],
                from.y + sweep.signum() * r * u[0],
            ],
            sweep,
            sag: 2. * r * (sweep / 4.).sin().powi(2),
        }),
    })
}

fn circle(a: Point3, b: Point3, c: Point3) -> Option<[f64; 2]> {
    let [ux, uy] = [b.x - a.x, b.y - a.y];
    let [vx, vy] = [c.x - a.x, c.y - a.y];
    let d = 2. * (ux * vy - uy * vx);
    if d.abs() < 1e-10 {
        return None;
    }
    let u = ux * ux + uy * uy;
    let v = vx * vx + vy * vy;
    Some([a.x + (u * vy - v * uy) / d, a.y + (ux * v - vx * u) / d])
}

// Recognize the round parts of a clipped offset and preserve them in the
// shared arc stream. Accept only monotone minor arcs with bounded deviation
// from every source chord, including after segmenting a posted arc.
fn fit_round(points: &[Point3]) -> Option<(usize, CornerArc)> {
    let [a, b, c, _, ..] = points else {
        return None;
    };
    let radius_at = |p: Point3, center: [f64; 2]| (p.x - center[0]).hypot(p.y - center[1]);
    let seed = circle(*a, *b, *c)?;
    let r = radius_at(*a, seed);
    let count = points
        .iter()
        .take(128)
        .take_while(|&&p| (radius_at(p, seed) - r).abs() <= 0.001 && p.distance(*a) <= 1.9 * r)
        .count();
    if count < 4 {
        return None;
    }
    let center = circle(*a, points[count / 2], points[count - 1])?;
    let r = radius_at(*a, center);
    if !(0.03..=10_000.).contains(&r) {
        return None;
    }
    let mut sweep: f64 = 0.;
    for edge in points[..count].windows(2) {
        let u = [edge[0].x - center[0], edge[0].y - center[1]];
        let v = [edge[1].x - center[0], edge[1].y - center[1]];
        let angle = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
        let sag = 2. * r * (angle / 4.).sin().powi(2);
        if (radius_at(edge[1], center) - r).abs() > 0.0002
            || sag > SAG + 0.0001
            || angle.abs() < 1e-7
            || (sweep != 0. && angle.signum() != sweep.signum())
        {
            return None;
        }
        sweep += angle;
    }
    if !(0.02..=std::f64::consts::PI).contains(&sweep.abs()) {
        return None;
    }
    Some((
        count,
        CornerArc {
            center,
            sweep,
            sag: 2. * r * (sweep / 4.).sin().powi(2),
        },
    ))
}
