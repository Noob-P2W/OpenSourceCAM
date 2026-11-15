//! Bounded planar cutter compensation. Ambiguous/self-intersecting offsets are rejected.
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let v = sub(b, a);
    let t = ((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1]) / (v[0] * v[0] + v[1] * v[1]).max(1e-20);
    let t = t.clamp(0., 1.);
    (p[0] - a[0] - t * v[0]).hypot(p[1] - a[1] - t * v[1])
}
fn separation(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    let u = sub(b, a);
    let v = sub(d, c);
    let den = cross(u, v);
    if den.abs() > 1e-12 {
        let t = cross(sub(c, a), v) / den;
        let s = cross(sub(c, a), u) / den;
        if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s) {
            return 0.;
        }
    }
    distance(a, c, d)
        .min(distance(b, c, d))
        .min(distance(c, a, b))
        .min(distance(d, a, b))
}
fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut yes = false;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes
        }
    }
    yes
}
pub fn offset(
    points: &[[f64; 3]],
    closed: bool,
    radius: f64,
    inward: bool,
) -> Result<Vec<[f64; 3]>, String> {
    let mut points = points.to_vec();
    if closed && points.first() == points.last() {
        points.pop();
    }
    if points.len() < if closed { 3 } else { 2 } || points.len() > 2000 {
        return Err("Compensated chains need 2–2,000 vertices (at least three when closed)".into());
    }
    if points.iter().flatten().any(|v| !v.is_finite()) || !radius.is_finite() || radius <= 0. {
        return Err("Invalid compensated chain or cutter radius".into());
    }
    if points.iter().any(|p| (p[2] - points[0][2]).abs() > 1e-6) {
        return Err("2D contour chains must be flat; use Trace for varying Z".into());
    }
    let poly: Vec<[f64; 2]> = points.iter().map(|p| [p[0], p[1]]).collect();
    let n = poly.len();
    let count = if closed { n } else { n - 1 };
    for i in 0..count {
        if distance(poly[i], poly[(i + 1) % n], poly[(i + 1) % n]) < 1e-8 {
            return Err("Contour contains a zero-length segment".into());
        }
        for j in i + 1..count {
            if j == i + 1 || (closed && i == 0 && j == count - 1) {
                continue;
            }
            if separation(poly[i], poly[(i + 1) % n], poly[j], poly[(j + 1) % n]) < 1e-8 {
                return Err("Contour self-intersects or touches itself".into());
            }
        }
    }
    let area = (0..n)
        .map(|i| cross(poly[i], poly[(i + 1) % n]))
        .sum::<f64>();
    if closed && area.abs() < 1e-10 {
        return Err("Closed contour has no area".into());
    }
    let offset =
        radius * if inward { 1. } else { -1. } * if closed && area < 0. { -1. } else { 1. };
    let unit = |i: usize| {
        let d = sub(poly[(i + 1) % n], poly[i]);
        let len = d[0].hypot(d[1]);
        [d[0] / len, d[1] / len]
    };
    let shifted =
        |i: usize, dir: [f64; 2]| [poly[i][0] - dir[1] * offset, poly[i][1] + dir[0] * offset];
    let mut result = Vec::with_capacity(n);
    for (i, original) in poly.iter().enumerate() {
        let p = if !closed && i == 0 {
            shifted(i, unit(0))
        } else if !closed && i == n - 1 {
            shifted(i, unit(n - 2))
        } else {
            let a = unit((i + n - 1) % n);
            let b = unit(i);
            let x = shifted(i, a);
            let y = shifted(i, b);
            let den = cross(a, b);
            if den.abs() < 1e-10 {
                if a[0] * b[0] + a[1] * b[1] < 0. {
                    return Err("Contour reverses on itself".into());
                }
                x
            } else {
                let t = cross(sub(y, x), b) / den;
                [x[0] + t * a[0], x[1] + t * a[1]]
            }
        };
        if (p[0] - original[0]).hypot(p[1] - original[1]) > radius * 10. + 1e-6 {
            return Err("Contour corner is too sharp for bounded cutter compensation".into());
        }
        result.push(p);
    }
    for i in 0..count {
        let a = result[i];
        let b = result[(i + 1) % n];
        if closed {
            for q in [a, [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.]] {
                if inside(q, &poly) != inward {
                    return Err(
                        "Cutter offset collapses the contour; choose a smaller cutter".into(),
                    );
                }
            }
        }
        for j in 0..count {
            if separation(a, b, poly[j], poly[(j + 1) % n]) < radius - 1e-6 {
                return Err("Cutter compensation crosses another edge; choose a smaller cutter or simplify the chain".into());
            }
        }
        for j in i + 2..count {
            if closed && i == 0 && j == count - 1 {
                continue;
            }
            if separation(a, b, result[j], result[(j + 1) % n]) < 1e-8 {
                return Err("Compensated contour self-intersects; choose a smaller cutter".into());
            }
        }
    }
    Ok(result
        .into_iter()
        .map(|p| [p[0], p[1], points[0][2]])
        .collect())
}
