//! Conservative remaining-stock certificates. Unlike the display heightfield,
//! a cell is lowered only when an entire cell is covered by a cutter sweep.
use crate::{
    cam::{Motion, Move, Point3},
    model::Project,
    stock::Footprint,
};
#[derive(Clone, Debug)]
pub struct RemainingStock {
    pub nx: usize,
    pub ny: usize,
    pub dx: f64,
    pub dy: f64,
    pub bottom: f64,
    pub upper: Vec<f64>,
    pub exhausted: bool,
    work: usize,
}
impl RemainingStock {
    pub fn new(p: &Project) -> Self {
        let diameter = p
            .operations
            .iter()
            .filter(|o| o.enabled)
            .filter_map(|o| p.operation_tool(o).ok())
            .map(|t| t.diameter)
            .fold(4., f64::min);
        let mut step = (diameter / 8.)
            .min(0.5)
            .max((p.stock.width * p.stock.height / 262144.).sqrt());
        let (nx, ny) = loop {
            let nx = (p.stock.width / step).ceil().max(1.) as usize;
            let ny = (p.stock.height / step).ceil().max(1.) as usize;
            if nx.saturating_mul(ny) <= 262144 {
                break (nx, ny);
            }
            step *= 1.01;
        };
        Self::dimensions(p, nx, ny)
    }
    pub(crate) fn dimensions(p: &Project, nx: usize, ny: usize) -> Self {
        Self {
            nx,
            ny,
            dx: p.stock.width / nx as f64,
            dy: p.stock.height / ny as f64,
            bottom: -p.stock.thickness,
            upper: vec![0.; nx * ny],
            exhausted: false,
            work: 50_000_000,
        }
    }
    pub(crate) fn reset_budget(&mut self, limit: u64) {
        self.work = limit.min(usize::MAX as u64) as usize;
        self.exhausted = false;
    }
    fn tick(&mut self) -> bool {
        if self.work == 0 {
            self.exhausted = true;
            false
        } else {
            self.work -= 1;
            true
        }
    }
    pub(crate) fn cut(&mut self, a: Point3, b: Point3, tool: Footprint, error: f64) {
        if self.exhausted || tool.radius <= 0. {
            return;
        }
        let diag = self.dx.hypot(self.dy) / 2. + error;
        if tool.radius <= diag {
            return;
        }
        let radius = tool.radius - diag;
        let y0 = (((a.y.min(b.y) - radius) / self.dy - 0.5).ceil().max(0.) as usize).min(self.ny);
        let y1 = (((a.y.max(b.y) + radius) / self.dy - 0.5).floor() + 1.).max(0.) as usize;
        for y in y0..y1.min(self.ny) {
            let cy = (y as f64 + 0.5) * self.dy;
            let Some([lo, hi]) = crate::stock::capsule_row(a, b, radius, cy) else {
                continue;
            };
            let x0 = ((lo / self.dx - 0.5).ceil().max(0.) as usize).min(self.nx);
            let x1 = ((hi / self.dx - 0.5).floor() + 1.).max(0.) as usize;
            for x in x0..x1.min(self.nx) {
                if !self.tick() {
                    return;
                }
                let id = y * self.nx + x;
                if self.upper[id] <= a.z.min(b.z).max(self.bottom) {
                    continue;
                }
                if let Some(h) = certificate(
                    [(x as f64 + 0.5) * self.dx, (y as f64 + 0.5) * self.dy],
                    a,
                    b,
                    tool,
                    diag,
                ) {
                    self.upper[id] = self.upper[id].min(h.max(self.bottom));
                }
            }
        }
    }
    pub(crate) fn apply(&mut self, m: &Move, tool: Option<Footprint>, error: f64) {
        if m.motion != Motion::Rapid {
            if let Some(t) = tool {
                self.cut(m.from, m.to, t, error);
            }
        }
    }
    /// Upper height over every stock cell intersected by the swept XY disc.
    /// None means the calculation cannot certify a result, never that stock is empty.
    pub fn top(&mut self, a: [f64; 2], b: [f64; 2], radius: f64) -> Option<f64> {
        if self.exhausted {
            return None;
        }
        let pad = radius + self.dx.hypot(self.dy) / 2.;
        let mut top = self.bottom;
        let x0 = (((a[0].min(b[0]) - pad) / self.dx).floor().max(0.) as usize).min(self.nx);
        let x1 = (((a[0].max(b[0]) + pad) / self.dx).ceil().max(0.) as usize).min(self.nx);
        let y0 = (((a[1].min(b[1]) - pad) / self.dy).floor().max(0.) as usize).min(self.ny);
        let y1 = (((a[1].max(b[1]) + pad) / self.dy).ceil().max(0.) as usize).min(self.ny);
        for y in y0..y1 {
            for x in x0..x1 {
                if !self.tick() {
                    return None;
                }
                let q = [(x as f64 + 0.5) * self.dx, (y as f64 + 0.5) * self.dy];
                if crate::planar::distance(q, [a[0], a[1], 0.], [b[0], b[1], 0.]) <= pad {
                    top = top.max(self.upper[y * self.nx + x]);
                }
            }
        }
        Some(top)
    }
    pub fn is_clear(&mut self, a: Point3, b: Point3, r: f64, gap: f64) -> bool {
        self.top([a.x, a.y], [b.x, b.y], r)
            .is_some_and(|h| h + gap <= a.z.min(b.z) + 1e-8)
    }
    pub fn ring_clear(&mut self, c: [f64; 2], inner: f64, outer: f64, z: f64) -> bool {
        let pad = self.dx.hypot(self.dy) / 2.;
        for y in 0..self.ny {
            for x in 0..self.nx {
                if !self.tick() {
                    return false;
                }
                let d =
                    ((x as f64 + 0.5) * self.dx - c[0]).hypot((y as f64 + 0.5) * self.dy - c[1]);
                if d + pad >= inner && d - pad <= outer && self.upper[y * self.nx + x] > z + 1e-7 {
                    return false;
                }
            }
        }
        true
    }
    pub fn remaining_upper_mm3(&self) -> f64 {
        self.upper
            .iter()
            .map(|h| (h - self.bottom).max(0.) * self.dx * self.dy)
            .sum()
    }
}
fn certificate(p: [f64; 2], a: Point3, b: Point3, tool: Footprint, pad: f64) -> Option<f64> {
    let r = tool.radius - pad;
    let vx = b.x - a.x;
    let vy = b.y - a.y;
    let l2 = vx * vx + vy * vy;
    let eval = |t: f64| {
        let d = (p[0] - a.x - vx * t).hypot(p[1] - a.y - vy * t) + pad;
        a.z + (b.z - a.z) * t + crate::stock::cutter_height(tool.radius, tool.ball, tool.cone, d)
    };
    if l2 < 1e-20 {
        return ((p[0] - a.x).hypot(p[1] - a.y) <= r)
            .then(|| eval(if b.z < a.z { 1. } else { 0. }));
    }
    let t = ((p[0] - a.x) * vx + (p[1] - a.y) * vy) / l2;
    let d2 = (p[0] - a.x - vx * t).powi(2) + (p[1] - a.y - vy * t).powi(2);
    if d2 > r * r {
        return None;
    }
    let half = ((r * r - d2) / l2).sqrt();
    let lo = (t - half).max(0.);
    let hi = (t + half).min(1.);
    if lo > hi {
        return None;
    }
    Some(eval(lo).min(eval(hi)).min(eval(t.clamp(lo, hi))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whole_cell_certificates_bound_flat_ball_and_conical_ramp_sweeps() {
        let mut p = Project::default();
        p.stock.width = 8.;
        p.stock.height = 6.;
        p.stock.thickness = 10.;
        for tool in [
            Footprint {
                radius: 2.,
                ball: false,
                cone: None,
            },
            Footprint {
                radius: 2.,
                ball: true,
                cone: None,
            },
            Footprint {
                radius: 2.,
                ball: false,
                cone: Some((1., 0.2)),
            },
        ] {
            for (a, b) in [
                (
                    Point3 {
                        x: 1.,
                        y: 1.,
                        z: -1.,
                    },
                    Point3 {
                        x: 7.,
                        y: 5.,
                        z: -5.,
                    },
                ),
                (
                    Point3 {
                        x: 3.,
                        y: 3.,
                        z: 0.,
                    },
                    Point3 {
                        x: 3.,
                        y: 3.,
                        z: -4.,
                    },
                ),
                (
                    Point3 {
                        x: 1.,
                        y: 4.,
                        z: -5.,
                    },
                    Point3 {
                        x: 7.,
                        y: 2.,
                        z: -1.,
                    },
                ),
            ] {
                let mut state = RemainingStock::dimensions(&p, 20, 15);
                state.cut(a, b, tool, 0.001);
                for y in 0..state.ny {
                    for x in 0..state.nx {
                        for (u, v) in [(0., 0.), (1., 0.), (0., 1.), (1., 1.), (0.5, 0.5)] {
                            let q = [(x as f64 + u) * state.dx, (y as f64 + v) * state.dy];
                            let mut sampled = 0_f64;
                            for i in 0..=1000 {
                                let t = i as f64 / 1000.;
                                let d = (q[0] - a.x - (b.x - a.x) * t)
                                    .hypot(q[1] - a.y - (b.y - a.y) * t);
                                if d <= tool.radius {
                                    sampled = sampled.min(
                                        a.z + (b.z - a.z) * t
                                            + crate::stock::cutter_height(
                                                tool.radius,
                                                tool.ball,
                                                tool.cone,
                                                d,
                                            ),
                                    );
                                }
                            }
                            assert!(
                                state.upper[y * state.nx + x] + 0.005 >= sampled,
                                "Certificate removed material outside a complete cutter sweep"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn exhaustion_means_unknown_and_extreme_aspect_ratios_stay_bounded() {
        let mut p = Project::default();
        p.stock.width = 100000.;
        p.stock.height = 0.001;
        let bounded = RemainingStock::new(&p);
        assert!(bounded.upper.len() <= 262144);
        let mut state = RemainingStock::dimensions(&p, 2, 2);
        state.reset_budget(0);
        assert!(state.top([0., 0.], [1., 0.], 1.).is_none());
        assert!(state.exhausted);
        assert!(!state.is_clear(Point3::default(), Point3::default(), 1., 0.));
    }
}
