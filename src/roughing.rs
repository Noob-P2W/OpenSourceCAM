//! Constant-Z offset clearing on a conservatively dilated target heightfield.
//! Links stay inside the machinable region; disconnected regions retract.
use crate::{
    cam::Point3,
    model::{Operation, Project},
    surface::{Budget, Field},
};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

pub(crate) struct Clearing {
    heights: Vec<f64>,
    nx: usize,
    ny: usize,
    dx: f64,
    dy: f64,
    origin: [f64; 2],
    spacing: usize,
    weights: [usize; 3],
    smooth_steps: bool,
    blend_deviation: f64,
    clearance_halo: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct Span {
    pub from: Point3,
    pub to: Point3,
    pub arc: Option<CornerArc>,
}

#[derive(Clone, Copy)]
pub(crate) struct CornerArc {
    pub center: [f64; 2],
    pub sweep: f64,
    pub sag: f64,
}

impl Span {
    pub fn line(from: Point3, to: Point3) -> Self {
        Self {
            from,
            to,
            arc: None,
        }
    }
    pub fn at_z(self, z: f64) -> Self {
        Self {
            from: Point3 { z, ..self.from },
            to: Point3 { z, ..self.to },
            ..self
        }
    }
}
impl Clearing {
    pub(crate) fn build(
        field: &Field,
        project: &Project,
        op: &Operation,
        budget: &mut Budget,
    ) -> Result<Self, String> {
        let tool = project.operation_tool(op)?;
        let step_over = tool.diameter * tool.step_over.min(0.5);
        let step = op.settings.surface_step.unwrap_or(0.5).min(step_over / 2.);
        let margin = (tool.diameter / 2. - step / 2.).max(step);
        let nx = ((project.stock.width + 2. * margin) / step).ceil() as usize;
        let ny = ((project.stock.height + 2. * margin) / step).ceil() as usize;
        if nx > 2048 || ny > 2048 || nx.checked_mul(ny).is_none_or(|n| n > 262_144) {
            return Err("3D offset grid exceeds 262,144 cells; increase grid step or stepover, or split the setup".into());
        }
        let dx = (project.stock.width + 2. * margin) / nx as f64;
        let dy = (project.stock.height + 2. * margin) / ny as f64;
        let mut heights = Vec::with_capacity(nx * ny);
        // Protect the entire cell, including its corners and every linking edge.
        // Treat a ball as a full cylinder for roughing: conservative stock remains.
        let radius = tool.diameter / 2. + dx.hypot(dy) / 2.;
        for y in 0..ny {
            for x in 0..nx {
                let cx = -margin + (x as f64 + 0.5) * dx;
                let cy = -margin + (y as f64 + 0.5) * dy;
                heights.push(field.height([cx, cx], cy, radius, false, op, budget)?);
            }
        }
        let spacing = (step_over / (dx.max(dy) * 2_f64.sqrt())).floor().max(1.) as usize;
        Ok(Self {
            heights,
            nx,
            ny,
            dx,
            dy,
            origin: [-margin; 2],
            spacing,
            weights: [1; 3],
            smooth_steps: false,
            blend_deviation: 0.,
            clearance_halo: 0.,
        })
    }
    /// Reuse bounded offset loops and safe within-region links for planar clearing.
    pub(crate) fn planar(
        rings: &[Vec<[f64; 3]>],
        obstacles: &[Vec<[f64; 3]>],
        radius: f64,
        stepover: f64,
        resolution: f64,
        smooth_steps: bool,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, String> {
        let outer = rings.first().ok_or("Select a machining boundary")?;
        let smooth_steps = smooth_steps
            && rings.iter().chain(obstacles).any(|ring| {
                ring.iter()
                    .zip(ring.iter().cycle().skip(1))
                    .any(|(a, b)| (a[0] - b[0]).abs() > 1e-7 && (a[1] - b[1]).abs() > 1e-7)
            });
        let lo: [f64; 2] =
            std::array::from_fn(|a| outer.iter().map(|p| p[a]).fold(f64::INFINITY, f64::min));
        let hi: [f64; 2] =
            std::array::from_fn(|a| outer.iter().map(|p| p[a]).fold(f64::NEG_INFINITY, f64::max));
        let step = resolution.min(stepover / (2. * 2_f64.sqrt()));
        if !step.is_finite() || step <= 0. {
            return Err("Choose a positive roughing resolution".into());
        }
        let nx = ((hi[0] - lo[0]) / step).ceil() as usize;
        let ny = ((hi[1] - lo[1]) / step).ceil() as usize;
        if nx == 0
            || ny == 0
            || nx > 2048
            || ny > 2048
            || nx.checked_mul(ny).is_none_or(|n| n > 262144)
        {
            return Err(
                "2D offset grid exceeds 262,144 cells; increase toolpath resolution or stepover"
                    .into(),
            );
        }
        let dx = (hi[0] - lo[0]) / nx as f64;
        let dy = (hi[1] - lo[1]) / ny as f64;
        // Extra cell diagonal protects corners and the distance from a raster sample
        // to the nearest grid-row centre. Every graph edge stays in free cells.
        let spans = crate::planar::raster_obstacles(rings, radius + dx.hypot(dy), dy, obstacles)?;
        let mut heights = vec![f64::INFINITY; nx * ny];
        for [a, b] in spans {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Machining cancelled".into());
            }
            let y = (((a[1] - lo[1]) / dy).floor() as usize).min(ny - 1);
            let left = a[0].min(b[0]);
            let right = a[0].max(b[0]);
            for x in 0..nx {
                let cx = lo[0] + (x as f64 + 0.5) * dx;
                if cx >= left && cx <= right {
                    heights[y * nx + x] = f64::NEG_INFINITY;
                }
            }
        }
        if heights.iter().all(|v| *v == f64::INFINITY) {
            return Err("No cutter-sized clearing region remains at this depth".into());
        }
        let diagonal = dx.hypot(dy);
        // Each accepted cell has a raster witness at its exact X centre and
        // somewhere within its Y interval. The furthest cell point is within
        // (dx/2, dy) of that witness. Any remaining dilation is clearance
        // outside the cell, after reserving posted coordinate precision.
        let clearance_halo = (diagonal - (dx / 2.).hypot(dy) - 0.0002).max(0.);
        // At very small stepovers, keep the exact route instead of spending
        // the whole spacing budget on smoothing and generating excess laps.
        let spare = stepover - 2. * diagonal - dx.min(dy);
        let smooth_steps = smooth_steps && spare > 0.;
        let blend_deviation = if smooth_steps {
            (diagonal * 0.15).min(spare / 2.)
        } else {
            0.
        };
        Ok(Self {
            heights,
            nx,
            ny,
            dx,
            dy,
            origin: lo,
            // Reserve one diagonal for grid rounding and the combined straightening
            // and fillet deviations on adjacent passes.
            spacing: ((stepover
                - if smooth_steps { 2. } else { 1. } * diagonal
                - 2. * blend_deviation)
                / (dx.min(dy) / 1024.))
                .floor()
                .max(1.) as usize,
            weights: {
                let unit = dx.min(dy) / 1024.;
                [
                    (dx / unit).ceil() as usize,
                    (dy / unit).ceil() as usize,
                    (dx.hypot(dy) / unit).ceil() as usize,
                ]
            },
            smooth_steps,
            blend_deviation,
            clearance_halo,
        })
    }
    pub(crate) fn exterior_only(&mut self, width: f64, height: f64) {
        let mut seen = vec![false; self.heights.len()];
        let mut queue = VecDeque::new();
        for y in 0..self.ny {
            for x in 0..self.nx {
                let cx = self.origin[0] + (x as f64 + 0.5) * self.dx;
                let cy = self.origin[1] + (y as f64 + 0.5) * self.dy;
                let i = y * self.nx + x;
                if self.heights[i] == f64::NEG_INFINITY
                    && (cx < 0. || cy < 0. || cx > width || cy > height)
                {
                    seen[i] = true;
                    queue.push_back(i);
                }
            }
        }
        while let Some(i) = queue.pop_front() {
            let x = i % self.nx;
            let y = i / self.nx;
            for j in [
                x.checked_sub(1).map(|_| i - 1),
                (x + 1 < self.nx).then_some(i + 1),
                y.checked_sub(1).map(|_| i - self.nx),
                (y + 1 < self.ny).then_some(i + self.nx),
            ]
            .into_iter()
            .flatten()
            {
                if !seen[j] && self.heights[j] == f64::NEG_INFINITY {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
        }
        for (i, yes) in seen.into_iter().enumerate() {
            if !yes {
                self.heights[i] = f64::INFINITY;
            }
        }
    }
    fn point(&self, node: usize, z: f64) -> Point3 {
        Point3 {
            x: self.origin[0] + (node % (self.nx + 1)) as f64 * self.dx,
            y: self.origin[1] + (node / (self.nx + 1)) as f64 * self.dy,
            z,
        }
    }
    pub(crate) fn layer_from(
        &self,
        z: f64,
        reverse: bool,
        budget: &mut Budget,
        entry: Option<Point3>,
    ) -> Result<Vec<Vec<Span>>, String> {
        let free: Vec<_> = self.heights.iter().map(|h| *h <= z + 1e-7).collect();
        let mut distance = vec![usize::MAX; free.len()];
        let mut heap = BinaryHeap::new();
        // Planar offsets use physical axis/diagonal distances, avoiding the
        // unnecessary sqrt(2) reduction of straight-wall stepover. The 3D
        // contact field retains its conservative square-cell erosion metric.
        let [wx, wy, diagonal] = self.weights;
        for y in 0..self.ny {
            for x in 0..self.nx {
                let id = y * self.nx + x;
                if !free[id] {
                    distance[id] = 0;
                    heap.push(Reverse((0, id)));
                } else if x == 0 || y == 0 || x + 1 == self.nx || y + 1 == self.ny {
                    distance[id] = wx.min(wy);
                    heap.push(Reverse((distance[id], id)));
                }
            }
        }
        while let Some(Reverse((d, id))) = heap.pop() {
            budget.tick()?;
            if d != distance[id] {
                continue;
            }
            let (x, y) = (id % self.nx, id / self.nx);
            for yy in y.saturating_sub(1)..=(y + 1).min(self.ny - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(self.nx - 1) {
                    let weight = if xx == x {
                        wy
                    } else if yy == y {
                        wx
                    } else {
                        diagonal
                    };
                    let next = yy * self.nx + xx;
                    let candidate = d
                        .checked_add(weight)
                        .ok_or("Offset distance field exceeds its numeric range")?;
                    if distance[next] > candidate {
                        distance[next] = candidate;
                        heap.push(Reverse((distance[next], next)));
                    }
                }
            }
        }
        let mut queue = VecDeque::new();
        let mut components = vec![usize::MAX; free.len()];
        let mut component = 0;
        for seed in 0..free.len() {
            if !free[seed] || components[seed] != usize::MAX {
                continue;
            }
            components[seed] = component;
            queue.push_back(seed);
            while let Some(id) = queue.pop_front() {
                budget.tick()?;
                let (x, y) = (id % self.nx, id / self.nx);
                for yy in y.saturating_sub(1)..=(y + 1).min(self.ny - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(self.nx - 1) {
                        let next = yy * self.nx + xx;
                        if free[next] && components[next] == usize::MAX {
                            components[next] = component;
                            queue.push_back(next)
                        }
                    }
                }
            }
            component += 1;
        }
        let graph = Graph {
            nx: self.nx,
            ny: self.ny,
            free: &free,
            components: &components,
        };
        let maximum = distance
            .iter()
            .copied()
            .filter(|d| *d != usize::MAX)
            .max()
            .unwrap_or(0);
        let mut chains: Vec<Vec<usize>> = Vec::new();
        for level in (1..=maximum).step_by(self.spacing) {
            let mut edges: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            let inside =
                |x: usize, y: usize| free[y * self.nx + x] && distance[y * self.nx + x] >= level;
            for y in 0..self.ny {
                for x in 0..self.nx {
                    budget.tick()?;
                    if !inside(x, y) {
                        continue;
                    }
                    let a = y * (self.nx + 1) + x;
                    let b = a + 1;
                    let d = a + self.nx + 1;
                    let c = d + 1;
                    for (boundary, start, end) in [
                        (y == 0 || !inside(x, y - 1), a, b),
                        (x + 1 == self.nx || !inside(x + 1, y), b, c),
                        (y + 1 == self.ny || !inside(x, y + 1), c, d),
                        (x == 0 || !inside(x - 1, y), d, a),
                    ] {
                        if boundary {
                            edges.entry(start).or_default().push(end)
                        }
                    }
                }
            }
            let mut loops = Vec::new();
            while let Some((&start, _)) = edges.first_key_value() {
                let mut nodes = vec![start];
                let mut current = start;
                let mut previous = None;
                loop {
                    budget.tick()?;
                    let options = edges
                        .get_mut(&current)
                        .ok_or("Offset boundary could not be closed")?;
                    let choice = if let Some(prev) = previous {
                        let (x, y) = (current % (self.nx + 1), current / (self.nx + 1));
                        let incoming = [
                            x as isize - (prev % (self.nx + 1)) as isize,
                            y as isize - (prev / (self.nx + 1)) as isize,
                        ];
                        options
                            .iter()
                            .enumerate()
                            .max_by_key(|(_, next)| {
                                let outgoing = [
                                    (**next % (self.nx + 1)) as isize - x as isize,
                                    (**next / (self.nx + 1)) as isize - y as isize,
                                ];
                                let cross = incoming[0] * outgoing[1] - incoming[1] * outgoing[0];
                                let dot = incoming[0] * outgoing[0] + incoming[1] * outgoing[1];
                                if cross > 0 {
                                    3
                                } else if dot > 0 {
                                    2
                                } else if cross < 0 {
                                    1
                                } else {
                                    0
                                }
                            })
                            .map(|(i, _)| i)
                            .unwrap()
                    } else {
                        0
                    };
                    let next = options.swap_remove(choice);
                    if options.is_empty() {
                        edges.remove(&current);
                    }
                    previous = Some(current);
                    current = next;
                    nodes.push(current);
                    if current == start {
                        break;
                    }
                }
                loops.push(nodes);
            }
            // Clear the stock perimeter first, then the nested island boundaries.
            loops.sort_by(|a, b| {
                area(b, self.nx + 1)
                    .abs()
                    .total_cmp(&area(a, self.nx + 1).abs())
            });
            for mut nodes in loops {
                nodes.pop();
                if reverse {
                    nodes.reverse()
                }
                if let Some(last) = chains.last().and_then(|c| c.last()).copied() {
                    let nearest = nodes
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, n)| graph.metric(last, **n))
                        .unwrap()
                        .0;
                    let best = if self.smooth_steps {
                        graph.forward_link(chains.last().unwrap(), &nodes, nearest, budget)?
                    } else {
                        None
                    };
                    let (start, link) = if let Some((start, link)) = best {
                        (start, Some(link))
                    } else {
                        (nearest, graph.route(last, nodes[nearest], budget)?)
                    };
                    nodes.rotate_left(start);
                    if let Some(link) = link {
                        let chain = chains.last_mut().unwrap();
                        chain.extend(link.into_iter().skip(1));
                        nodes.push(nodes[0]);
                        chain.extend(nodes.into_iter().skip(1));
                        continue;
                    }
                }
                if let Some(entry) = entry {
                    let nearest = nodes
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            self.point(**a, z)
                                .distance(entry)
                                .total_cmp(&self.point(**b, z).distance(entry))
                        })
                        .unwrap()
                        .0;
                    nodes.rotate_left(nearest);
                }
                nodes.push(nodes[0]);
                chains.push(nodes);
            }
        }
        chains
            .into_iter()
            .map(|mut nodes| {
                if self.smooth_steps {
                    graph.straighten_steps(&mut nodes, [self.dx, self.dy], budget)?;
                }
                let mut result: Vec<Point3> = Vec::new();
                for node in nodes {
                    let p = self.point(node, z);
                    if result.len() >= 2 {
                        let a = result[result.len() - 2];
                        let b = result[result.len() - 1];
                        let same = (b.x - a.x) * (p.y - b.y) - (b.y - a.y) * (p.x - b.x);
                        let forward = (b.x - a.x) * (p.x - b.x) + (b.y - a.y) * (p.y - b.y);
                        if same.abs() < 1e-10 && forward > 0. {
                            result.pop();
                        }
                    }
                    result.push(p)
                }
                self.fillet(&result, &graph, budget)
            })
            .collect()
    }

    fn fillet(
        &self,
        points: &[Point3],
        graph: &Graph<'_>,
        budget: &mut Budget,
    ) -> Result<Vec<Span>, String> {
        let Some(&first) = points.first() else {
            return Ok(Vec::new());
        };
        let mut cursor = first;
        let mut spans = Vec::new();
        for triple in points.windows(3) {
            budget.tick()?;
            let [a, vertex, b] = [triple[0], triple[1], triple[2]];
            if let Some(arc) = self.corner(a, vertex, b, graph) {
                spans.push(Span::line(cursor, arc.from));
                spans.push(arc);
                cursor = arc.to;
            } else {
                spans.push(Span::line(cursor, vertex));
                cursor = vertex;
            }
        }
        if points.len() > 1 {
            spans.push(Span::line(cursor, *points.last().unwrap()));
        }
        Ok(spans)
    }

    fn corner(&self, a: Point3, v: Point3, b: Point3, graph: &Graph<'_>) -> Option<Span> {
        if self.blend_deviation <= 0. {
            return None;
        }
        let l1 = a.distance(v);
        let l2 = v.distance(b);
        if l1.min(l2) <= 1e-7 {
            return None;
        }
        let u = [(v.x - a.x) / l1, (v.y - a.y) / l1];
        let w = [(b.x - v.x) / l2, (b.y - v.y) / l2];
        let turn = cross(u, w).atan2(u[0] * w[0] + u[1] * w[1]);
        let angle = turn.abs();
        if !(0.05..=2.62).contains(&angle) {
            return None;
        }

        // Two neighboring blends can each use at most 48% of a shared leg.
        // Limit the arc's distance from the corner and its grid search size.
        let mut trim = (0.48 * l1.min(l2))
            .min(self.blend_deviation / (angle / 4.).tan())
            .min(2.8 * self.dx.min(self.dy));
        for _ in 0..5 {
            let r = trim / (angle / 2.).tan();
            // Very small arcs offer little benefit and may be rejected by posts.
            if r < 0.03 || trim < 0.005 {
                break;
            }
            let from = Point3 {
                x: v.x - trim * u[0],
                y: v.y - trim * u[1],
                z: v.z,
            };
            let to = Point3 {
                x: v.x + trim * w[0],
                y: v.y + trim * w[1],
                z: v.z,
            };
            let grid = |p: Point3| {
                [
                    (p.x - self.origin[0]) / self.dx,
                    (p.y - self.origin[1]) / self.dy,
                ]
            };
            // The complete minor arc lies in the triangle formed by the
            // tangent points and the original vertex, as do its G1 samples.
            let deviation = trim * (angle / 4.).tan();
            if deviation <= self.clearance_halo
                || graph.triangle_clear([grid(from), grid(v), grid(to)], turn.signum())
            {
                return Some(Span {
                    from,
                    to,
                    arc: Some(CornerArc {
                        center: [
                            from.x - turn.signum() * r * u[1],
                            from.y + turn.signum() * r * u[0],
                        ],
                        sweep: turn,
                        sag: r * (1. - (angle / 2.).cos()),
                    }),
                });
            }
            trim *= 0.5;
        }
        None
    }
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn area(nodes: &[usize], width: usize) -> f64 {
    nodes
        .windows(2)
        .map(|p| {
            (p[0] % width) as f64 * (p[1] / width) as f64
                - (p[1] % width) as f64 * (p[0] / width) as f64
        })
        .sum::<f64>()
        / 2.
}
struct Graph<'a> {
    nx: usize,
    ny: usize,
    free: &'a [bool],
    components: &'a [usize],
}
impl Graph<'_> {
    fn forward_link(
        &self,
        chain: &[usize],
        ring: &[usize],
        nearest: usize,
        budget: &mut Budget<'_>,
    ) -> Result<Option<(usize, Vec<usize>)>, String> {
        if chain.len() < 2 || ring.len() < 3 {
            return Ok(None);
        }
        let last = chain[chain.len() - 1];
        let previous = chain[chain.len() - 2];
        let dot = |a, b, c| {
            let [a, b, c] = [a, b, c].map(|n| self.coordinates(n));
            (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1])
        };
        let max_distance = self.metric(last, ring[nearest]) + 12;
        let mut best = None;
        let mut best_cost = usize::MAX;
        for offset in [0, 1, -1, 2, -2, 4, -4, 8, -8, 12, -12] {
            let i = (nearest as isize + offset).rem_euclid(ring.len() as isize) as usize;
            let target = ring[i];
            let next = ring[(i + 1) % ring.len()];
            let distance = self.metric(last, target);
            if distance > max_distance || distance >= best_cost {
                continue;
            }
            for horizontal in [true, false] {
                budget.tick()?;
                let Some(link) = self.elbow(last, target, horizontal) else {
                    continue;
                };
                let (out, incoming) = if link.len() == 1 {
                    (next, previous)
                } else {
                    (link[1], link[link.len() - 2])
                };
                let turns = [dot(previous, last, out), dot(incoming, target, next)];
                if turns.iter().any(|&t| t < 0) {
                    continue;
                }
                let cost = distance + 2 * turns.iter().filter(|&&t| t == 0).count();
                if cost < best_cost {
                    best_cost = cost;
                    best = Some((i, link));
                }
            }
        }
        Ok(best)
    }
    fn triangle_clear(&self, triangle: [[f64; 2]; 3], winding: f64) -> bool {
        let bounds = |axis: usize| {
            let values = triangle.map(|p| p[axis]);
            let lo = values.into_iter().fold(f64::INFINITY, f64::min);
            let hi = values.into_iter().fold(f64::NEG_INFINITY, f64::max);
            (lo.floor() as isize)..(hi.ceil() as isize)
        };
        for y in bounds(1) {
            for x in bounds(0) {
                if self.cell(x, y) {
                    continue;
                }
                let (x, y) = (x as f64, y as f64);
                let square = [[x, y], [x + 1., y], [x + 1., y + 1.], [x, y + 1.]];
                // The bounding ranges supply the square's two separating axes.
                // Test the triangle's three remaining axes. Boundary contact
                // is allowed as with the original cell-edge routes.
                let separated = (0..3).any(|i| {
                    let a = triangle[i];
                    let b = triangle[(i + 1) % 3];
                    square.iter().all(|q| {
                        winding * cross([b[0] - a[0], b[1] - a[1]], [q[0] - a[0], q[1] - a[1]])
                            <= 1e-10
                    })
                });
                if !separated {
                    return false;
                }
            }
        }
        true
    }
    fn coordinates(&self, node: usize) -> [isize; 2] {
        [
            (node % (self.nx + 1)) as isize,
            (node / (self.nx + 1)) as isize,
        ]
    }

    /// Walk every cell interior hit by a chord between two grid vertices.
    /// Integer crossing comparisons handle exact corner hits and keep the
    /// same closed-cell rules as the original graph edges.
    fn chord_clear(&self, start: usize, end: usize) -> bool {
        let [ax, ay] = self.coordinates(start);
        let [bx, by] = self.coordinates(end);
        let [dx, dy] = [bx - ax, by - ay];
        let [nx, ny] = [dx.abs(), dy.abs()];
        if nx == 0 {
            return (ay.min(by)..ay.max(by)).all(|y| self.cell(ax, y) || self.cell(ax - 1, y));
        }
        if ny == 0 {
            return (ax.min(bx)..ax.max(bx)).all(|x| self.cell(x, ay) || self.cell(x, ay - 1));
        }
        let mut x = ax - isize::from(dx < 0);
        let mut y = ay - isize::from(dy < 0);
        let mut next_x = 1;
        let mut next_y = 1;
        loop {
            if !self.cell(x, y) {
                return false;
            }
            if next_x == nx && next_y == ny {
                return true;
            }
            let px = next_x * ny;
            let py = next_y * nx;
            if px <= py {
                x += dx.signum();
                next_x += 1;
            }
            if py <= px {
                y += dy.signum();
                next_y += 1;
            }
        }
    }

    /// Fit several staircase treads at once. Ordered projections give a
    /// two-sided distance bound to the source walk, so the existing half-cell
    /// diagonal allowance still limits the displacement of each pass.
    fn straighten_steps(
        &self,
        nodes: &mut Vec<usize>,
        step: [f64; 2],
        budget: &mut Budget<'_>,
    ) -> Result<(), String> {
        const WINDOW: usize = 20;
        let point = |node| {
            let [x, y] = self.coordinates(node);
            [x as f64 * step[0], y as f64 * step[1]]
        };
        let deviation = step[0].hypot(step[1]) / 2.;
        let mut start = 0;
        let mut output = 1;
        while start + 1 < nodes.len() {
            let a = point(nodes[start]);
            let mut end = start + 1;
            for candidate in (start + 2..=(start + WINDOW).min(nodes.len() - 1)).rev() {
                budget.tick()?;
                let b = point(nodes[candidate]);
                let d = [b[0] - a[0], b[1] - a[1]];
                let length = d[0].hypot(d[1]);
                if length <= 1e-9 {
                    continue;
                }
                let mut previous = 0.;
                let fits = nodes[start + 1..candidate].iter().all(|&node| {
                    let p = point(node);
                    let offset = [p[0] - a[0], p[1] - a[1]];
                    let along = (offset[0] * d[0] + offset[1] * d[1]) / length;
                    let in_order = along >= previous - 1e-9 && along <= length + 1e-9;
                    previous = along;
                    in_order && cross(d, offset).abs() <= (deviation + 1e-9) * length
                });
                if fits && self.chord_clear(nodes[start], nodes[candidate]) {
                    end = candidate;
                    break;
                }
            }
            nodes[output] = nodes[end];
            output += 1;
            start = end;
        }
        nodes.truncate(output);
        Ok(())
    }
    fn metric(&self, a: usize, b: usize) -> usize {
        (a % (self.nx + 1)).abs_diff(b % (self.nx + 1))
            + (a / (self.nx + 1)).abs_diff(b / (self.nx + 1))
    }
    fn cell(&self, x: isize, y: isize) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.nx
            && (y as usize) < self.ny
            && self.free[y as usize * self.nx + x as usize]
    }
    fn component(&self, n: usize) -> Option<usize> {
        let (x, y) = ((n % (self.nx + 1)) as isize, (n / (self.nx + 1)) as isize);
        for yy in [y - 1, y] {
            for xx in [x - 1, x] {
                if self.cell(xx, yy) {
                    return Some(self.components[yy as usize * self.nx + xx as usize]);
                }
            }
        }
        None
    }
    fn edge(&self, a: usize, b: usize) -> bool {
        let (x, y) = (
            (a % (self.nx + 1)).min(b % (self.nx + 1)) as isize,
            (a / (self.nx + 1)).min(b / (self.nx + 1)) as isize,
        );
        if a / (self.nx + 1) == b / (self.nx + 1) {
            self.cell(x, y) || self.cell(x, y - 1)
        } else {
            self.cell(x, y) || self.cell(x - 1, y)
        }
    }
    fn elbow(&self, start: usize, goal: usize, horizontal: bool) -> Option<Vec<usize>> {
        let mut out = vec![start];
        let mut current = start;
        for axis in if horizontal { [0, 1] } else { [1, 0] } {
            while if axis == 0 {
                current % (self.nx + 1) != goal % (self.nx + 1)
            } else {
                current / (self.nx + 1) != goal / (self.nx + 1)
            } {
                let delta = if axis == 0 { 1 } else { self.nx + 1 };
                let forward = if axis == 0 {
                    current % (self.nx + 1) < goal % (self.nx + 1)
                } else {
                    current / (self.nx + 1) < goal / (self.nx + 1)
                };
                let next = if forward {
                    current + delta
                } else {
                    current - delta
                };
                if !self.edge(current, next) {
                    return None;
                }
                out.push(next);
                current = next;
            }
        }
        Some(out)
    }
    fn route(
        &self,
        start: usize,
        goal: usize,
        budget: &mut Budget,
    ) -> Result<Option<Vec<usize>>, String> {
        if self.component(start) != self.component(goal) {
            return Ok(None);
        }
        if let Some(route) = self
            .elbow(start, goal, true)
            .or_else(|| self.elbow(start, goal, false))
        {
            return Ok(Some(route));
        }
        let size = (self.nx + 1) * (self.ny + 1);
        let mut cost = vec![usize::MAX; size];
        let mut previous = vec![usize::MAX; size];
        let mut open = BinaryHeap::new();
        cost[start] = 0;
        open.push(Reverse((self.metric(start, goal), 0, start)));
        while let Some(Reverse((_, g, node))) = open.pop() {
            budget.tick()?;
            if g != cost[node] {
                continue;
            }
            if node == goal {
                let mut path = vec![goal];
                let mut n = goal;
                while n != start {
                    n = previous[n];
                    path.push(n)
                }
                path.reverse();
                return Ok(Some(path));
            }
            let (x, y) = (node % (self.nx + 1), node / (self.nx + 1));
            for next in [
                if x > 0 { Some(node - 1) } else { None },
                if x < self.nx { Some(node + 1) } else { None },
                if y > 0 {
                    Some(node - self.nx - 1)
                } else {
                    None
                },
                if y < self.ny {
                    Some(node + self.nx + 1)
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            {
                if g + 1 < cost[next] && self.edge(node, next) {
                    cost[next] = g + 1;
                    previous[next] = node;
                    open.push(Reverse((g + 1 + self.metric(next, goal), g + 1, next)))
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod smoothing_tests {
    use super::*;

    #[test]
    fn straightening_only_crosses_free_cells() {
        let graph = Graph {
            nx: 2,
            ny: 2,
            free: &[true, true, true, false],
            components: &[0; 4],
        };
        for (mut path, expected) in [
            (vec![0, 1, 4], vec![0, 4]),
            (vec![4, 1, 0], vec![4, 0]),
            (vec![4, 5, 8], vec![4, 5, 8]),
            (vec![8, 5, 4], vec![8, 5, 4]),
        ] {
            graph
                .straighten_steps(
                    &mut path,
                    [1.; 2],
                    &mut Budget::new(&std::sync::atomic::AtomicBool::new(false)),
                )
                .unwrap();
            assert_eq!(path, expected);
        }
    }

    #[test]
    fn several_stair_treads_fit_one_chord_within_the_grid_allowance() {
        let mut free = [true; 16];
        let stair = [0, 1, 6, 7, 12, 13, 18];
        let cancel = std::sync::atomic::AtomicBool::new(false);
        for obstructed in [false, true] {
            free[5] = !obstructed;
            let graph = Graph {
                nx: 4,
                ny: 4,
                free: &free,
                components: &[0; 16],
            };
            let mut path = stair.to_vec();
            graph
                .straighten_steps(&mut path, [1.; 2], &mut Budget::new(&cancel))
                .unwrap();
            assert_eq!(path == [0, 18], !obstructed);
            assert!(path
                .windows(2)
                .all(|pair| graph.chord_clear(pair[0], pair[1])));
        }
    }

    #[test]
    fn a_fillet_is_tangent_and_stays_out_of_the_blocked_cell() {
        let clearing = Clearing {
            heights: vec![],
            nx: 2,
            ny: 2,
            dx: 1.,
            dy: 1.,
            origin: [0.; 2],
            spacing: 1,
            weights: [1; 3],
            smooth_steps: true,
            blend_deviation: 0.1,
            clearance_halo: 0.,
        };
        let graph = Graph {
            nx: 2,
            ny: 2,
            free: &[true, false, true, true],
            components: &[0; 4],
        };
        let point = |x, y| Point3 { x, y, z: -1. };
        let a = point(0., 1.);
        let vertex = point(1., 1.);
        for (b, direction) in [(point(1., 2.), 1.), (point(1., 0.), -1.)] {
            let fillet = clearing.corner(a, vertex, b, &graph);
            if direction < 0. {
                // This blend would enter the blocked lower-right cell when
                // traversed from the right. The lower-left cell is free.
                let blocked = clearing.corner(point(2., 1.), vertex, b, &graph);
                assert!(blocked.is_none());
            }
            let span = fillet.unwrap();
            let arc = span.arc.unwrap();
            let r = (span.from.x - arc.center[0]).hypot(span.from.y - arc.center[1]);
            assert!((span.from.x - arc.center[0]).abs() < 1e-10);
            assert!((span.to.y - arc.center[1]).abs() < 1e-10);
            assert!((span.to.x - arc.center[0] - r).abs() < 1e-10);
            assert!((arc.sweep - direction * std::f64::consts::FRAC_PI_2).abs() < 1e-10);
            let phase = (span.from.y - arc.center[1]).atan2(span.from.x - arc.center[0]);
            for i in 0..=20 {
                let theta = phase + arc.sweep * i as f64 / 20.;
                let x = arc.center[0] + r * theta.cos();
                let y = arc.center[1] + r * theta.sin();
                assert!(x <= 1. + 1e-10);
                assert!((y - 1.) * direction >= -1e-10);
            }
        }
    }
}
