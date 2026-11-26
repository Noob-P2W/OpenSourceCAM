//! Bounded cutter-centre regions on planar faces, including islands.
pub fn inside(p: [f64; 2], ring: &[[f64; 3]]) -> bool {
    let mut yes = false;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes;
        }
    }
    yes
}
pub fn distance(p: [f64; 2], a: [f64; 3], b: [f64; 3]) -> f64 {
    let v = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1])
        / (v[0] * v[0] + v[1] * v[1]).max(1e-20))
    .clamp(0., 1.);
    (p[0] - a[0] - t * v[0]).hypot(p[1] - a[1] - t * v[1])
}
fn restrict(range: &mut [f64; 2], a: f64, b: f64, lo: f64, hi: f64) -> bool {
    if a.abs() < 1e-12 {
        return b >= lo && b <= hi;
    }
    let x = (lo - b) / a;
    let y = (hi - b) / a;
    range[0] = range[0].max(x.min(y));
    range[1] = range[1].min(x.max(y));
    range[0] <= range[1]
}
pub(crate) fn capsule(y: f64, a: [f64; 3], b: [f64; 3], r: f64) -> Option<[f64; 2]> {
    let mut limits = [f64::INFINITY, f64::NEG_INFINITY];
    let mut include = |[lo, hi]: [f64; 2]| {
        limits[0] = limits[0].min(lo);
        limits[1] = limits[1].max(hi);
    };
    for p in [a, b] {
        let d = (y - p[1]).abs();
        if d <= r {
            let h = (r * r - d * d).max(0.).sqrt();
            include([p[0] - h, p[0] + h]);
        }
    }
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let l = dx.hypot(dy);
    if l > 1e-10 {
        let mut span = [f64::NEG_INFINITY, f64::INFINITY];
        if restrict(&mut span, dx, dy * (y - a[1]) - dx * a[0], 0., l * l)
            && restrict(&mut span, -dy, dy * a[0] + dx * (y - a[1]), -r * l, r * l)
        {
            include(span);
        }
    }
    (limits[0] <= limits[1]).then_some(limits)
}
pub fn raster(
    rings: &[Vec<[f64; 3]>],
    radius: f64,
    step: f64,
) -> Result<Vec<[[f64; 2]; 2]>, String> {
    raster_obstacles(rings, radius, step, &[])
}
pub(crate) fn validate_rings(rings: &[Vec<[f64; 3]>]) -> Result<usize, String> {
    let n = rings.iter().map(Vec::len).sum::<usize>();
    if rings.is_empty()
        || rings.iter().any(|r| r.len() < 3)
        || n > 2000
        || rings.iter().flatten().flatten().any(|v| !v.is_finite())
    {
        return Err("Select finite bounded face loops with at most 2,000 edges".into());
    }
    for (ri, ring) in rings.iter().enumerate() {
        if ri > 0
            && (!inside([ring[0][0], ring[0][1]], &rings[0])
                || rings
                    .iter()
                    .enumerate()
                    .any(|(j, r)| j > 0 && j != ri && inside([ring[0][0], ring[0][1]], r)))
        {
            return Err("Face islands must be disjoint and inside the outer boundary".into());
        }
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9 {
                return Err("Face boundary has a zero-length edge".into());
            }
            for (rj, other) in rings.iter().enumerate().skip(ri) {
                for j in 0..other.len() {
                    if ri == rj && (j <= i + 1 || (i == 0 && j == ring.len() - 1)) {
                        continue;
                    }
                    if segment_distance(a, b, other[j], other[(j + 1) % other.len()]) < 1e-9 {
                        return Err(
                            "Face boundaries cross or touch; select separate regions".into()
                        );
                    }
                }
            }
        }
    }
    Ok(n)
}

pub fn raster_obstacles(
    rings: &[Vec<[f64; 3]>],
    radius: f64,
    step: f64,
    obstacles: &[Vec<[f64; 3]>],
) -> Result<Vec<[[f64; 2]; 2]>, String> {
    let n = validate_rings(rings)?;
    if !radius.is_finite() || radius <= 0. || !step.is_finite() || step <= 0. {
        return Err("Choose a positive cutter diameter and stepover".into());
    }
    let lo = rings
        .iter()
        .flatten()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        + radius;
    let hi = rings
        .iter()
        .flatten()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        - radius;
    if hi < lo {
        return Err("The selected face is narrower than the cutter".into());
    }
    let rows = ((hi - lo) / step).ceil().max(1.) as usize;
    if rows > 10000 || rows * (n + obstacles.iter().map(Vec::len).sum::<usize>()) > 50_000_000 {
        return Err("Planar machining exceeds its calculation budget".into());
    }
    let mut paths = Vec::new();
    for row in 0..=rows {
        let y = lo + (hi - lo) * row as f64 / rows as f64;
        let mut crossings = Vec::new();
        let mut blocked = Vec::new();
        for ring in rings {
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                if (a[1] > y) != (b[1] > y) {
                    crossings.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
                if let Some(span) = capsule(y, a, b, (radius - 1e-8).max(0.)) {
                    blocked.push(span);
                }
            }
        }
        for ring in obstacles {
            let mut hits = Vec::new();
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                if (a[1] > y) != (b[1] > y) {
                    hits.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
                if let Some(span) = capsule(y, a, b, radius) {
                    blocked.push(span);
                }
            }
            if hits.len() >= 2 {
                blocked.push([
                    hits.iter().copied().fold(f64::INFINITY, f64::min),
                    hits.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ]);
            }
        }
        crossings.sort_by(f64::total_cmp);
        let mut spans: Vec<_> = crossings
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| [c[0], c[1]])
            .collect();
        blocked.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut merged: Vec<[f64; 2]> = Vec::new();
        for span in blocked {
            if let Some(last) = merged.last_mut() {
                if span[0] <= last[1] {
                    last[1] = last[1].max(span[1]);
                    continue;
                }
            }
            merged.push(span);
        }
        for cut in merged {
            let mut next = Vec::new();
            for [a, b] in spans {
                if cut[1] <= a || cut[0] >= b {
                    next.push([a, b]);
                } else {
                    if cut[0] > a {
                        next.push([a, cut[0]]);
                    }
                    if cut[1] < b {
                        next.push([cut[1], b]);
                    }
                }
            }
            spans = next;
        }
        if row % 2 == 1 {
            spans.reverse();
        }
        for [a, b] in spans {
            if b - a > 1e-6 {
                paths.push(if row % 2 == 0 {
                    [[a, y], [b, y]]
                } else {
                    [[b, y], [a, y]]
                });
            }
        }
    }
    if paths.is_empty() {
        return Err("No cutter-sized area remains on the selected face".into());
    }
    Ok(paths)
}
pub fn segment_distance(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    let cross = |x: [f64; 2], y: [f64; 2]| x[0] * y[1] - x[1] * y[0];
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [d[0] - c[0], d[1] - c[1]];
    let w = [c[0] - a[0], c[1] - a[1]];
    let den = cross(u, v);
    if den.abs() > 1e-12 {
        let t = cross(w, v) / den;
        let s = cross(w, u) / den;
        if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s) {
            return 0.;
        }
    }
    distance([a[0], a[1]], c, d)
        .min(distance([b[0], b[1]], c, d))
        .min(distance([c[0], c[1]], a, b))
        .min(distance([d[0], d[1]], a, b))
}
