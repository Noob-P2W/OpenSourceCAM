//! Recover exact circular edges from the retained ISO 10303-21 model.
//! Geometry is used only when the selected mesh boundary confirms the source
//! placement, trim, traversal and scale.
use crate::{
    cam::Point3,
    import::{Part, SourceFormat},
};
use std::{collections::BTreeMap, f64::consts::TAU};

const MATCH: f64 = 0.0001;
const MAX_ENTITIES: usize = 500_000;
const MAX_MATCHES: usize = 2_000_000;

#[derive(Clone, Copy)]
struct StepCircle {
    center: [f64; 3],
    radius: f64,
    start: f64,
    sweep: f64,
}

#[derive(Clone, Copy)]
struct CircularChain {
    pub center: [f64; 2],
    pub radius: f64,
    pub start: f64,
    pub sweep: f64,
}

impl CircularChain {
    fn point(self, z: f64) -> Point3 {
        Point3 {
            x: self.center[0] + self.radius * self.start.cos(),
            y: self.center[1] + self.radius * self.start.sin(),
            z,
        }
    }

    fn span(self, z: f64) -> crate::roughing::Span {
        let stop = Self {
            start: self.start + self.sweep,
            ..self
        };
        crate::roughing::Span {
            from: self.point(z),
            to: stop.point(z),
            arc: Some(crate::roughing::CornerArc {
                center: self.center,
                sweep: self.sweep,
                sag: 2. * self.radius * (self.sweep / 4.).sin().powi(2),
            }),
        }
    }

    fn compensated(mut self, closed: bool, inward: bool, distance: f64) -> Option<Self> {
        let side = if inward { 1. } else { -1. };
        let winding = if closed { self.sweep.signum() } else { 1. };
        self.radius -= self.sweep.signum() * winding * side * distance;
        (self.radius > 0.001).then_some(self)
    }
}

struct Entity<'a> {
    name: &'a str,
    parameters: &'a str,
}

impl<'a> Entity<'a> {
    fn fields(&self) -> impl Iterator<Item = &'a str> {
        let mut depth = 0usize;
        let mut quoted = false;
        self.parameters
            .split(move |c| {
                match c {
                    '\'' => quoted = !quoted,
                    '(' if !quoted => depth += 1,
                    ')' if !quoted => depth = depth.saturating_sub(1),
                    ',' if !quoted && depth == 0 => return true,
                    _ => {}
                }
                false
            })
            .map(str::trim)
    }
}

fn statements(source: &str) -> impl Iterator<Item = &str> {
    let mut quoted = false;
    let mut comment = false;
    let mut previous = '\0';
    source.split(move |c| {
        let was = previous;
        previous = c;
        if comment {
            if was == '*' && c == '/' {
                comment = false;
            }
        } else if !quoted && was == '/' && c == '*' {
            comment = true;
        } else if c == '\'' {
            quoted = !quoted;
        } else if !quoted && c == ';' {
            return true;
        }
        false
    })
}

fn reference(field: &str) -> Option<u32> {
    field.strip_prefix('#')?.parse().ok()
}

fn xyz(entity: &Entity<'_>, kind: &str) -> Option<[f64; 3]> {
    if entity.name != kind {
        return None;
    }
    let mut coordinates = entity
        .fields()
        .nth(1)?
        .strip_prefix('(')?
        .strip_suffix(')')?
        .split(',');
    let mut value = [0.; 3];
    for v in &mut value {
        *v = coordinates.next()?.trim().parse::<f64>().ok()?;
        if !v.is_finite() || v.abs() > 1_000_000. {
            return None;
        }
    }
    coordinates.next().is_none().then_some(value)
}

fn parse_edge(
    part: &Part,
    entities: &BTreeMap<u32, Entity<'_>>,
    edge: &Entity<'_>,
) -> Option<StepCircle> {
    let get = |field: &str| entities.get(&reference(field)?);
    let vertex = |field| {
        let entity = get(field)?;
        if entity.name != "VERTEX_POINT" {
            return None;
        }
        xyz(get(entity.fields().nth(1)?)?, "CARTESIAN_POINT")
    };
    let mut fields = edge.fields().skip(1);
    let a = part.point(vertex(fields.next()?)?);
    let b = part.point(vertex(fields.next()?)?);
    let mut curve = get(fields.next()?)?;
    let sense = match fields.next()? {
        ".T." => 1.,
        ".F." => -1.,
        _ => return None,
    };
    if matches!(curve.name, "SURFACE_CURVE" | "SEAM_CURVE") {
        curve = get(curve.fields().nth(1)?)?;
    }
    if curve.name != "CIRCLE" {
        return None;
    }
    let placement = get(curve.fields().nth(1)?)?;
    if placement.name != "AXIS2_PLACEMENT_3D" {
        return None;
    }
    let radius: f64 = curve.fields().nth(2)?.parse().ok()?;
    if !(0.001..=1_000_000.).contains(&radius) {
        return None;
    }
    let origin = xyz(get(placement.fields().nth(1)?)?, "CARTESIAN_POINT")?;
    let axis = match placement.fields().nth(2)? {
        "$" => [0., 0., 1.],
        field => xyz(get(field)?, "DIRECTION")?,
    };
    let center = part.point(origin);
    let end = part.point(std::array::from_fn(|i| origin[i] + axis[i]));
    let z = end[2] - center[2];
    if z.abs() < 1e-8 || (end[0] - center[0]).hypot(end[1] - center[1]) > 1e-8 * z.abs() {
        return None;
    }
    let angle = |point: [f64; 3]| {
        let [x, y] = [point[0] - center[0], point[1] - center[1]];
        ((point[2] - center[2]).abs() <= MATCH && (x.hypot(y) - radius).abs() <= MATCH)
            .then(|| y.atan2(x))
    };
    let start = angle(a)?;
    let stop = angle(b)?;
    let direction = sense * z.signum();
    let sweep = if a == b {
        TAU
    } else {
        ((stop - start) * direction).rem_euclid(TAU)
    };
    (sweep > 1e-8).then_some(StepCircle {
        center,
        radius,
        start,
        sweep: sweep * direction,
    })
}

fn circles(part: &Part, height: f64) -> Option<Vec<StepCircle>> {
    if part.geometry.format != SourceFormat::Step || part.geometry.bodies.len() > 1 {
        return None;
    }
    let source = part.geometry.cad_source.as_deref()?;
    let mut millimetres = false;
    let mut entities = BTreeMap::new();
    for (i, statement) in statements(source).enumerate() {
        if i >= MAX_ENTITIES {
            return None;
        }
        let Some((id, entity)) = statement.trim().split_once('=') else {
            continue;
        };
        let Some(id) = reference(id.trim()) else {
            continue;
        };
        let entity = entity.trim();
        let Some((name, parameters)) = entity.split_once('(') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() && entity.contains("LENGTH_UNIT") {
            let unit: String = entity
                .chars()
                .filter(|c| !c.is_ascii_whitespace())
                .collect();
            if !unit.contains("SI_UNIT(.MILLI.,.METRE.)") {
                return None;
            }
            millimetres = true;
        }
        if matches!(
            name,
            "EDGE_CURVE"
                | "VERTEX_POINT"
                | "CARTESIAN_POINT"
                | "SURFACE_CURVE"
                | "SEAM_CURVE"
                | "CIRCLE"
                | "AXIS2_PLACEMENT_3D"
                | "DIRECTION"
        ) {
            let parameters = parameters.trim_end().strip_suffix(')')?;
            if entities.insert(id, Entity { name, parameters }).is_some() {
                return None;
            }
        }
    }
    if !millimetres {
        return None;
    }
    Some(
        entities
            .values()
            .filter(|edge| edge.name == "EDGE_CURVE")
            .filter_map(|edge| parse_edge(part, &entities, edge))
            .filter(|circle| (circle.center[2] - height).abs() <= MATCH)
            .collect(),
    )
}

impl StepCircle {
    fn angle(self, p: [f64; 3]) -> Option<f64> {
        let x = p[0] - self.center[0];
        let y = p[1] - self.center[1];
        ((p[2] - self.center[2]).abs() <= MATCH && (x.hypot(y) - self.radius).abs() <= MATCH)
            .then(|| y.atan2(x))
    }
    fn contains(self, angle: f64) -> bool {
        let slack = MATCH / self.radius;
        let offset = ((angle - self.start) * self.sweep.signum()).rem_euclid(TAU);
        offset <= self.sweep.abs() + slack || offset >= TAU - slack
    }
    fn matches(self, a: [f64; 3], b: [f64; 3]) -> Option<(f64, f64)> {
        let start = self.angle(a)?;
        let stop = self.angle(b)?;
        let sweep = (stop - start).sin().atan2((stop - start).cos());
        if !(1e-7..=0.6).contains(&sweep.abs())
            || 2. * self.radius * (sweep / 4.).sin().powi(2) > 0.12
            || ![start, start + sweep / 2., stop]
                .into_iter()
                .all(|p| self.contains(p))
        {
            return None;
        }
        Some((start, sweep))
    }
}

pub(crate) fn finishing_chain(
    part: &Part,
    points: &[[f64; 3]],
    compensated: &[[f64; 3]],
    closed: bool,
    inward: bool,
    distance: f64,
) -> Result<Option<Vec<crate::roughing::Span>>, String> {
    let Some(z) = points.first().map(|p| p[2]) else {
        return Ok(None);
    };
    let Some(circles) = circles(part, z) else {
        return Ok(None);
    };
    if circles.is_empty() || circles.len().saturating_mul(points.len()) > MAX_MATCHES {
        return Ok(None);
    }
    if let Some(circle) = circular_chain(&circles, points, closed) {
        let arc = circle
            .compensated(closed, inward, distance)
            .ok_or("Cutter compensation collapses the circular STEP edge")?;
        return Ok(Some(vec![arc.span(z)]));
    }
    Ok(tangent_chain(
        &circles,
        points,
        compensated,
        closed,
        inward,
        distance,
    ))
}

fn circular_chain(
    circles: &[StepCircle],
    points: &[[f64; 3]],
    closed: bool,
) -> Option<CircularChain> {
    if points.len() < if closed { 3 } else { 2 } {
        return None;
    }
    let mut chain: Option<CircularChain> = None;
    let count = points.len() - usize::from(!closed);
    for (a, b) in points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(count)
    {
        let mut matches = circles.iter().filter_map(|circle| {
            let (start, sweep) = circle.matches(a, b)?;
            Some(CircularChain {
                center: [circle.center[0], circle.center[1]],
                radius: circle.radius,
                start,
                sweep,
            })
        });
        let piece = matches.next()?;
        let same = |c: CircularChain| {
            (c.center[0] - piece.center[0]).hypot(c.center[1] - piece.center[1]) <= 1e-8
                && (c.radius - piece.radius).abs() <= 1e-8
                && c.sweep.signum() == piece.sweep.signum()
        };
        if !matches.all(same) {
            return None;
        }
        if let Some(previous) = chain.as_mut() {
            if !same(*previous) {
                return None;
            }
            previous.sweep += piece.sweep;
        } else {
            chain = Some(piece);
        }
    }
    let mut chain = chain?;
    if closed {
        if (chain.sweep.abs() - TAU).abs() > 1e-5 {
            return None;
        }
        chain.sweep = TAU * chain.sweep.signum();
    } else if chain.sweep.abs() >= TAU {
        return None;
    }
    Some(chain)
}

/// Reconstruct circular segments within a mixed chain when the compensated
/// lines and arcs meet tangentially. The sampled separation certificate
/// rejects ambiguity near sharp joins and nearby walls.
fn tangent_chain(
    candidates: &[StepCircle],
    points: &[[f64; 3]],
    compensated: &[[f64; 3]],
    closed: bool,
    inward: bool,
    distance: f64,
) -> Option<Vec<crate::roughing::Span>> {
    use crate::roughing::Span;
    if points.len() < 3 || points.len() != compensated.len() {
        return None;
    }
    let z = points[0][2];
    let count = points.len() - usize::from(!closed);
    let mut arcs = Vec::with_capacity(count);
    for i in 0..count {
        let [a, b] = [points[i], points[(i + 1) % points.len()]];
        if (a[2] - z).abs() > 1e-8 || (b[2] - z).abs() > 1e-8 {
            return None;
        }
        let mut choices = candidates.iter().filter_map(|c| {
            let (start, sweep) = c.matches(a, b)?;
            Some(CircularChain {
                center: [c.center[0], c.center[1]],
                radius: c.radius,
                start,
                sweep,
            })
        });
        let first = choices.next();
        if let Some(arc) = first {
            if choices.any(|c| c.center != arc.center || c.radius != arc.radius) {
                return None;
            }
        }
        arcs.push(first);
    }
    if arcs.iter().all(Option::is_none) {
        return None;
    }
    let point3 = |p: [f64; 3]| Point3 {
        x: p[0],
        y: p[1],
        z: p[2],
    };
    let mut work: Vec<_> = compensated.iter().copied().map(point3).collect();
    let mut exact: Vec<_> = points.iter().copied().map(point3).collect();
    let clockwise = closed
        && points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
            .sum::<f64>()
            < 0.;
    let left = distance * if inward ^ clockwise { 1. } else { -1. };
    let mut centers = vec![None; points.len()];
    let mut tool_arcs = Vec::with_capacity(count);
    for (i, arc) in arcs.iter().copied().enumerate() {
        let j = (i + 1) % points.len();
        let Some(arc) = arc else {
            tool_arcs.push(None);
            continue;
        };
        let r = arc.radius - arc.sweep.signum() * left;
        if r <= 0.001 {
            return None;
        }
        let tool = CircularChain { radius: r, ..arc };
        for (k, angle) in [(i, arc.start), (j, arc.start + arc.sweep)] {
            let source = CircularChain {
                start: angle,
                ..arc
            }
            .point(z);
            let offset = CircularChain {
                start: angle,
                ..tool
            }
            .point(z);
            let direction = [
                -arc.sweep.signum() * angle.sin(),
                arc.sweep.signum() * angle.cos(),
            ];
            if let Some(previous) = centers[k] {
                if dot(previous, direction) < 1. - 1e-6 || work[k].distance(offset) > 1e-7 {
                    return None;
                }
            }
            if (!closed && k > 0 || closed)
                && arcs[(k + count - 1) % count].is_none()
                && !line_tangent(
                    points[(k + points.len() - 1) % points.len()],
                    points[k],
                    direction,
                )
            {
                return None;
            }
            if k < count
                && arcs[k].is_none()
                && !line_tangent(points[k], points[(k + 1) % points.len()], direction)
            {
                return None;
            }
            centers[k] = Some(direction);
            exact[k] = source;
            work[k] = offset;
        }
        tool_arcs.push(Some(tool));
    }
    let build = |points: &[Point3], arcs: &[Option<CircularChain>]| -> Vec<Span> {
        (0..count)
            .map(|i| {
                let [from, to] = [points[i], points[(i + 1) % points.len()]];
                Span {
                    from,
                    to,
                    arc: arcs[i].map(|c| crate::roughing::CornerArc {
                        center: c.center,
                        sweep: c.sweep,
                        sag: 2. * c.radius * (c.sweep / 4.).sin().powi(2),
                    }),
                }
            })
            .collect()
    };
    let source = build(&exact, &arcs);
    let result = build(&work, &tool_arcs);
    if distance > 0. {
        let target = proof_chords(&source, 0.00015)?;
        let cut = proof_chords(&result, 0.00015)?;
        if target.len().saturating_mul(cut.len()) > MAX_MATCHES {
            return None;
        }
        let margin = (distance - 0.00025).max(0.);
        for [a, b] in &cut {
            for [u, v] in &target {
                if (0..2).any(|i| {
                    a[i].max(b[i]) + margin < u[i].min(v[i])
                        || u[i].max(v[i]) + margin < a[i].min(b[i])
                }) {
                    continue;
                }
                if crate::planar::segment_distance(*a, *b, *u, *v) < margin {
                    return None;
                }
            }
        }
    }
    let mut output: Vec<Span> = Vec::new();
    for span in result {
        if let Some((a, b, previous)) = output.last_mut().and_then(|p| Some((p.arc?, span.arc?, p)))
        {
            if (a.center[0] - b.center[0]).hypot(a.center[1] - b.center[1]) < 1e-8
                && a.sweep.signum() == b.sweep.signum()
                && previous.to.distance(span.from) < 1e-8
                && (a.sweep + b.sweep).abs() <= std::f64::consts::PI
            {
                previous.to = span.to;
                let sweep = a.sweep + b.sweep;
                let radius = (previous.from.x - a.center[0]).hypot(previous.from.y - a.center[1]);
                previous.arc = Some(crate::roughing::CornerArc {
                    center: a.center,
                    sweep,
                    sag: 2. * radius * (sweep / 4.).sin().powi(2),
                });
                continue;
            }
        }
        output.push(span);
    }
    Some(output)
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn line_tangent(a: [f64; 3], b: [f64; 3], tangent: [f64; 2]) -> bool {
    let u = [b[0] - a[0], b[1] - a[1]];
    dot(u, tangent) >= (1. - 1e-6) * u[0].hypot(u[1])
}

pub(crate) fn proof_chords(
    spans: &[crate::roughing::Span],
    sag: f64,
) -> Option<Vec<[[f64; 3]; 2]>> {
    let xyz = |p: Point3| [p.x, p.y, p.z];
    let mut chords = Vec::new();
    for span in spans {
        let mut cursor = xyz(span.from);
        if let Some(arc) = span.arc {
            let [x, y] = [span.from.x - arc.center[0], span.from.y - arc.center[1]];
            let r = x.hypot(y);
            let phase = y.atan2(x);
            let n = (arc.sweep.abs() / (2. * (1. - (sag / r).min(1.)).acos())).ceil() as usize;
            if chords.len().saturating_add(n) > 4096 || n == 0 {
                return None;
            }
            for i in 1..n {
                let angle = phase + arc.sweep * i as f64 / n as f64;
                let next = [
                    arc.center[0] + r * angle.cos(),
                    arc.center[1] + r * angle.sin(),
                    span.from.z,
                ];
                chords.push([cursor, next]);
                cursor = next;
            }
        }
        chords.push([cursor, xyz(span.to)]);
        if chords.len() > 4096 {
            return None;
        }
    }
    Some(chords)
}
