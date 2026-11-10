//! Bounded triangle hierarchy for conservative finishing contact; no whole-blank fine grid.
use crate::{
    import::Part,
    surface::{clip, Budget},
};
type Triangle = [[f64; 3]; 3];
pub(crate) struct Contact {
    triangles: Vec<Triangle>,
    nodes: Vec<Node>,
}
struct Node {
    min: [f64; 3],
    max: [f64; 3],
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}
impl Contact {
    pub fn build(part: &Part, budget: &mut Budget) -> Result<Self, String> {
        let mut this = Self {
            triangles: part
                .geometry
                .mesh
                .triangles
                .iter()
                .map(|t| t.map(|i| part.point(part.geometry.mesh.vertices[i as usize])))
                .collect(),
            nodes: Vec::new(),
        };
        this.node(0..this.triangles.len(), budget)?;
        Ok(this)
    }
    fn node(
        &mut self,
        range: std::ops::Range<usize>,
        budget: &mut Budget,
    ) -> Result<usize, String> {
        budget.tick()?;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for t in &self.triangles[range.clone()] {
            for p in t {
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            range: range.clone(),
            children: None,
        });
        if range.len() > 8 {
            let axis = usize::from(max[1] - min[1] > max[0] - min[0]);
            self.triangles[range.clone()].sort_unstable_by(|a, b| {
                a.iter()
                    .map(|p| p[axis])
                    .sum::<f64>()
                    .total_cmp(&b.iter().map(|p| p[axis]).sum::<f64>())
            });
            let mid = range.start + range.len() / 2;
            let a = self.node(range.start..mid, budget)?;
            let b = self.node(mid..range.end, budget)?;
            self.nodes[id].children = Some([a, b]);
        }
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn height(
        &self,
        segment: [f64; 2],
        y: f64,
        r: f64,
        ball: bool,
        extra: f64,
        step: f64,
        tolerance: f64,
        bottom: f64,
        budget: &mut Budget,
    ) -> Result<f64, String> {
        let query = Query {
            a: segment[0].min(segment[1]),
            b: segment[0].max(segment[1]),
            y,
            r,
            ball,
            extra,
            step,
            tolerance,
        };
        let mut best = bottom;
        self.visit(0, &query, &mut best, budget)?;
        Ok(best)
    }
    fn visit(
        &self,
        id: usize,
        q: &Query,
        best: &mut f64,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.tick()?;
        let n = &self.nodes[id];
        let upper = q.upper(n.min, n.max);
        if upper <= *best {
            return Ok(());
        }
        if let Some(children) = n.children {
            let mut children = children;
            if self.nodes[children[0]].max[2] < self.nodes[children[1]].max[2] {
                children.swap(0, 1);
            }
            for child in children {
                self.visit(child, q, best, budget)?;
            }
        } else {
            for t in &self.triangles[n.range.clone()] {
                let min =
                    std::array::from_fn(|i| t.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
                let max = std::array::from_fn(|i| {
                    t.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max)
                });
                if q.upper(min, max) <= *best {
                    continue;
                }
                if !q.ball {
                    q.flat(*t, best, budget)?;
                    continue;
                }
                let mut poly = t.to_vec();
                for (axis, bound, greater) in [
                    (0, q.a - q.r - q.extra, true),
                    (0, q.b + q.r + q.extra, false),
                    (1, q.y - q.r - q.extra, true),
                    (1, q.y + q.r + q.extra, false),
                ] {
                    poly = clip(poly, axis, bound, greater);
                }
                for i in 1..poly.len().saturating_sub(1) {
                    q.patch([poly[0], poly[i], poly[i + 1]], best, 0, budget)?;
                }
            }
        }
        Ok(())
    }
}
struct Query {
    a: f64,
    b: f64,
    y: f64,
    r: f64,
    ball: bool,
    extra: f64,
    step: f64,
    tolerance: f64,
}
impl Query {
    fn profile(&self, d: f64) -> f64 {
        if self.ball {
            let d = (d - self.extra).max(0.).min(self.r);
            self.r - (self.r * self.r - d * d).max(0.).sqrt()
        } else {
            0.
        }
    }
    fn distance(&self, p: [f64; 3]) -> f64 {
        (self.a - p[0])
            .max(p[0] - self.b)
            .max(0.)
            .hypot(p[1] - self.y)
    }
    fn upper(&self, min: [f64; 3], max: [f64; 3]) -> f64 {
        let d = (self.a - max[0])
            .max(min[0] - self.b)
            .max(0.)
            .hypot((self.y - max[1]).max(min[1] - self.y).max(0.));
        if d > self.r + self.extra {
            f64::NEG_INFINITY
        } else {
            max[2] - self.profile(d)
        }
    }
    /// Exact linear-triangle maximum over the swept flat cutter's XY capsule.
    /// Rectangle clipping plus circle/edge intersections avoids fine area sampling.
    fn flat(&self, t: Triangle, best: &mut f64, budget: &mut Budget) -> Result<(), String> {
        budget.tick()?;
        let reach = self.r + self.extra;
        let mut poly = t.to_vec();
        for (axis, bound, greater) in [
            (0, self.a, true),
            (0, self.b, false),
            (1, self.y - reach, true),
            (1, self.y + reach, false),
        ] {
            poly = clip(poly, axis, bound, greater);
        }
        for p in poly {
            *best = best.max(p[2]);
        }
        let u = [t[1][0] - t[0][0], t[1][1] - t[0][1]];
        let v = [t[2][0] - t[0][0], t[2][1] - t[0][1]];
        let det = u[0] * v[1] - u[1] * v[0];
        for x in [self.a, self.b] {
            for i in 0..3 {
                let a = t[i];
                let b = t[(i + 1) % 3];
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let ax = a[0] - x;
                let ay = a[1] - self.y;
                if ax * ax + ay * ay <= reach * reach + 1e-12 {
                    *best = best.max(a[2]);
                }
                let aa = dx * dx + dy * dy;
                if aa <= 1e-30 {
                    continue;
                }
                let bb = 2. * (ax * dx + ay * dy);
                let cc = ax * ax + ay * ay - reach * reach;
                let disc = bb * bb - 4. * aa * cc;
                if disc >= -1e-12 {
                    for root in [-1., 1.] {
                        let f = (-bb + root * disc.max(0.).sqrt()) / (2. * aa);
                        if (0. ..=1.).contains(&f) {
                            *best = best.max(a[2] + f * (b[2] - a[2]));
                        }
                    }
                }
            }
            if det.abs() > 1e-20 {
                let dz1 = t[1][2] - t[0][2];
                let dz2 = t[2][2] - t[0][2];
                let gradient = [
                    (dz1 * v[1] - dz2 * u[1]) / det,
                    (u[0] * dz2 - v[0] * dz1) / det,
                ];
                let length = gradient[0].hypot(gradient[1]);
                let p = if length > 1e-20 {
                    [
                        x + reach * gradient[0] / length,
                        self.y + reach * gradient[1] / length,
                    ]
                } else {
                    [x, self.y]
                };
                let px = p[0] - t[0][0];
                let py = p[1] - t[0][1];
                let a = (px * v[1] - py * v[0]) / det;
                let b = (u[0] * py - u[1] * px) / det;
                if a >= -1e-12 && b >= -1e-12 && a + b <= 1. + 1e-12 {
                    *best = best.max(t[0][2] + a * dz1 + b * dz2);
                }
            }
        }
        Ok(())
    }
    fn patch(
        &self,
        t: Triangle,
        best: &mut f64,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.tick()?;
        let min = std::array::from_fn(|i| t.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
        let max = std::array::from_fn(|i| t.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max));
        let upper = self.upper(min, max);
        if upper <= *best {
            return Ok(());
        }
        let mut lower = t
            .iter()
            .filter(|p| self.distance(**p) <= self.r + self.extra)
            .map(|p| p[2] - self.profile(self.distance(*p)))
            .fold(*best, f64::max);
        // Interior samples tighten the lower bound on broad planar triangles.
        // Bounds remain conservative; these samples only avoid unnecessary subdivision.
        let center: [f64; 3] = std::array::from_fn(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.);
        if self.distance(center) <= self.r + self.extra {
            lower = lower.max(center[2] - self.profile(self.distance(center)));
        }
        let u = [t[1][0] - t[0][0], t[1][1] - t[0][1]];
        let v = [t[2][0] - t[0][0], t[2][1] - t[0][1]];
        let det = u[0] * v[1] - u[1] * v[0];
        if det.abs() > 1e-20 {
            for x in [self.a, (self.a + self.b) / 2., self.b] {
                let p = [x - t[0][0], self.y - t[0][1]];
                let a = (p[0] * v[1] - p[1] * v[0]) / det;
                let b = (u[0] * p[1] - u[1] * p[0]) / det;
                if a >= 0. && b >= 0. && a + b <= 1. {
                    lower = lower.max(t[0][2] + a * (t[1][2] - t[0][2]) + b * (t[2][2] - t[0][2]));
                }
            }
        }
        *best = best.max(lower);
        if upper - lower <= self.tolerance
            || (max[0] - min[0]).hypot(max[1] - min[1]) <= self.step
            || depth >= 32
        {
            *best = best.max(upper);
            return Ok(());
        }
        let edge = (0..3)
            .max_by(|&a, &b| {
                let length =
                    |i: usize| (t[i][0] - t[(i + 1) % 3][0]).hypot(t[i][1] - t[(i + 1) % 3][1]);
                length(a).total_cmp(&length(b))
            })
            .unwrap();
        let a = t[edge];
        let b = t[(edge + 1) % 3];
        let c = t[(edge + 2) % 3];
        let mid = std::array::from_fn(|i| (a[i] + b[i]) / 2.);
        self.patch([a, mid, c], best, depth + 1, budget)?;
        self.patch([mid, b, c], best, depth + 1, budget)
    }
}
