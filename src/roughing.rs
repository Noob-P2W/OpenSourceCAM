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
    origin: f64,
    spacing: usize,
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
            origin: -margin,
            spacing,
        })
    }
    fn point(&self, node: usize, z: f64) -> Point3 {
        Point3 {
            x: self.origin + (node % (self.nx + 1)) as f64 * self.dx,
            y: self.origin + (node / (self.nx + 1)) as f64 * self.dy,
            z,
        }
    }
    pub(crate) fn layer(
        &self,
        z: f64,
        reverse: bool,
        budget: &mut Budget,
    ) -> Result<Vec<Vec<Point3>>, String> {
        let free: Vec<_> = self.heights.iter().map(|h| *h <= z + 1e-7).collect();
        let mut distance = vec![usize::MAX; free.len()];
        let mut queue = VecDeque::new();
        // Eight-neighbour erosion bounds diagonal spacing as well as straight runs.
        for y in 0..self.ny {
            for x in 0..self.nx {
                let id = y * self.nx + x;
                if !free[id] {
                    distance[id] = 0;
                    queue.push_back(id)
                } else if x == 0 || y == 0 || x + 1 == self.nx || y + 1 == self.ny {
                    distance[id] = 1;
                    queue.push_back(id)
                }
            }
        }
        while let Some(id) = queue.pop_front() {
            budget.tick()?;
            let (x, y) = (id % self.nx, id / self.nx);
            for yy in y.saturating_sub(1)..=(y + 1).min(self.ny - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(self.nx - 1) {
                    let next = yy * self.nx + xx;
                    if distance[next] > distance[id] + 1 {
                        distance[next] = distance[id] + 1;
                        queue.push_back(next)
                    }
                }
            }
        }
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
                    nodes.rotate_left(nearest);
                    if let Some(link) = graph.route(last, nodes[0], budget)? {
                        let chain = chains.last_mut().unwrap();
                        chain.extend(link.into_iter().skip(1));
                        nodes.push(nodes[0]);
                        chain.extend(nodes.into_iter().skip(1));
                        continue;
                    }
                }
                nodes.push(nodes[0]);
                chains.push(nodes);
            }
        }
        Ok(chains
            .into_iter()
            .map(|nodes| {
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
                result
            })
            .collect())
    }
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
