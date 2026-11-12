//! Reusable imported parallel-jaw geometry profiles in work or machine coordinates.
use crate::{
    faces,
    import::{Geometry, Mesh, Part, Units},
    model::Project,
    workspace::{Frame, ModelKind, WorkspaceModel},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub fixed: usize,
    pub moving: usize,
    pub max_closure: f64,
    pub closed_by: f64,
    pub moving_vertices: Vec<usize>,
    pub grip: f64,
}
impl Profile {
    pub fn validate_geometry(&self, geometry: &Geometry) -> Result<(), String> {
        self.validate(&geometry.mesh)?;
        if geometry.bodies.is_empty() {
            return Ok(());
        }
        let mut selected = vec![false; geometry.mesh.vertices.len()];
        for i in &self.moving_vertices {
            selected[*i] = true;
        }
        for body in &geometry.bodies {
            let mut moving = false;
            let mut fixed = false;
            for triangle in &geometry.mesh.triangles
                [body.first_triangle..body.first_triangle + body.triangle_count]
            {
                for v in triangle {
                    if selected[*v as usize] {
                        moving = true;
                    } else {
                        fixed = true;
                    }
                }
            }
            if moving && fixed {
                return Err("A CAD component must move as a whole rigid body".into());
            }
        }
        Ok(())
    }
    pub fn validate(&self, m: &Mesh) -> Result<(), String> {
        if !self.grip.is_finite() || !(0.1..=1000.).contains(&self.grip) {
            return Err("Invalid saved gripping depth".into());
        }
        if self.fixed >= m.triangles.len()
            || self.moving >= m.triangles.len()
            || self.fixed == self.moving
            || !self.max_closure.is_finite()
            || !(0.1..=1000.).contains(&self.max_closure)
            || !self.closed_by.is_finite()
            || self.closed_by < 0.
            || self.closed_by > self.max_closure + 1e-7
        {
            return Err("Invalid jaw profile or travel limit".into());
        }
        let mut selected = vec![false; m.vertices.len()];
        for i in &self.moving_vertices {
            if *i >= selected.len() || selected[*i] {
                return Err("Invalid moving-body vertex selection".into());
            }
            selected[*i] = true;
        }
        if !m.triangles[self.moving]
            .iter()
            .all(|i| selected[*i as usize])
            || m.triangles[self.fixed]
                .iter()
                .any(|i| selected[*i as usize])
            || m.triangles.iter().any(|t| {
                let n = t.iter().filter(|i| selected[**i as usize]).count();
                n != 0 && n != 3
            })
        {
            return Err(
                "Moving-body selection would split a mesh face or move the fixed jaw".into(),
            );
        }
        Ok(())
    }
}
fn model(p: &Project, id: u64) -> Result<&WorkspaceModel, String> {
    let m = p
        .workspace
        .iter()
        .find(|m| m.id == id)
        .ok_or("Vise not found")?;
    if m.kind != ModelKind::Vise {
        return Err("Choose a vise".into());
    }
    m.validate(p.machine.as_ref())?;
    Ok(m)
}
fn world(m: &WorkspaceModel, p: &Project) -> Part {
    let mut g = (*m.part.geometry).clone();
    g.mesh.vertices = g
        .mesh
        .vertices
        .iter()
        .map(|v| m.point(*v, p.machine.as_ref()))
        .collect();
    Part {
        geometry: Arc::new(g),
        rotation: [0.; 3],
        offset: [0.; 3],
        units: Units::Millimetres,
        visible: true,
    }
}
pub fn pick(
    p: &Project,
    id: u64,
    origin: [f64; 3],
    direction: [f64; 3],
) -> Result<faces::Face, String> {
    faces::pick(&world(model(p, id)?, p), origin, direction)
}
fn root(a: &mut [usize], mut i: usize) -> usize {
    while a[i] != i {
        a[i] = a[a[i]];
        i = a[i]
    }
    i
}
fn component(m: &Mesh, f: usize, g: usize) -> Result<Vec<usize>, String> {
    let mut a: Vec<_> = (0..m.vertices.len()).collect();
    let mut map = HashMap::new();
    for (i, v) in m.vertices.iter().enumerate() {
        let key = v.map(|c| if c == 0. { 0 } else { c.to_bits() });
        if let Some(j) = map.insert(key, i) {
            a[i] = root(&mut a, j)
        }
    }
    for t in &m.triangles {
        let r = root(&mut a, t[0] as usize);
        for v in &t[1..] {
            let s = root(&mut a, *v as usize);
            a[s] = r
        }
    }
    let r = root(&mut a, m.triangles[g][0] as usize);
    if root(&mut a, m.triangles[f][0] as usize) == r {
        return Err("Jaw is fused or connected to the fixed body. Use separate closed jaw bodies; the wizard does not cut a mesh apart.".into());
    }
    Ok((0..m.vertices.len())
        .filter(|i| root(&mut a, *i) == r)
        .collect())
}
fn assembly_group(
    geometry: &Geometry,
    fixed: usize,
    moving: usize,
    bodies: &[usize],
) -> Result<Vec<usize>, String> {
    if geometry.bodies.is_empty() {
        if !bodies.is_empty() {
            return Err("This model has no imported CAD component identities".into());
        }
        return component(&geometry.mesh, fixed, moving);
    }
    let fixed_body = geometry
        .body_at(fixed)
        .ok_or("Fixed face has no CAD component")?;
    let moving_body = geometry
        .body_at(moving)
        .ok_or("Movable face has no CAD component")?;
    if fixed_body == moving_body {
        return Err(
            "Both gripping faces belong to the same CAD body. Re-export the vise as an assembly with separate fixed and movable jaw bodies; selecting faces cannot separate fused geometry.".into(),
        );
    }
    let mut selected = vec![false; geometry.bodies.len()];
    selected[moving_body] = true;
    for id in bodies {
        *selected
            .get_mut(*id)
            .ok_or("Selected CAD component does not exist")? = true;
    }
    if selected[fixed_body] {
        return Err("The fixed gripping component cannot move with the jaw".into());
    }
    let owners = geometry.body_owners()?;
    Ok(owners
        .iter()
        .enumerate()
        .filter_map(|(i, owner)| selected.get(*owner).copied().unwrap_or(false).then_some(i))
        .collect())
}

fn bounds(f: &faces::Face) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in f.fill.iter().flatten() {
        for i in 0..3 {
            lo[i] = lo[i].min(v[i]);
            hi[i] = hi[i].max(v[i])
        }
    }
    (lo, hi)
}
type Details = (faces::Face, faces::Face, usize, f64, Vec<usize>);
fn details(p: &Project, id: u64) -> Result<Details, String> {
    let m = model(p, id)?;
    let c = m.clamp.as_ref().ok_or("Pick both gripping faces first")?;
    c.validate(&m.part.geometry.mesh)?;
    let part = world(m, p);
    let f = faces::face(&part, c.fixed)?;
    let g = faces::face(&part, c.moving)?;
    let axis = (0..2)
        .find(|i| f.normal[*i].abs() > 0.999999)
        .ok_or("Orient the vise so its gripping faces close along work X or Y")?;
    let sign = f.normal[axis].signum();
    if (0..3).map(|i| f.normal[i] * g.normal[i]).sum::<f64>() > -0.999999
        || (g.center[axis] - f.center[axis]) * sign <= 1e-8
    {
        return Err("Select opposing parallel faces that face into the jaw opening".into());
    }
    for face in [&f, &g] {
        let (lo, hi) = bounds(face);
        let area = (hi[1 - axis] - lo[1 - axis]) * (hi[2] - lo[2]);
        if area <= 0. || face.area < area * 0.999 {
            return Err(
                "This profile needs rectangular flat contact faces without holes or steps".into(),
            );
        }
    }
    let indices = c.moving_vertices.clone();
    if indices
        .iter()
        .any(|i| (part.geometry.mesh.vertices[*i][axis] - g.center[axis]) * sign < -1e-5)
    {
        return Err("The moving body extends ahead of its gripping plane. This profile supports straight jaws with the body behind the selected contact face.".into());
    }
    Ok((f, g, axis, sign, indices))
}
pub fn configure(
    p: &mut Project,
    id: u64,
    fixed: usize,
    moving: usize,
    max_closure: f64,
) -> Result<(), String> {
    configure_group(p, id, fixed, moving, max_closure, &[])
}

pub fn configure_group(
    p: &mut Project,
    id: u64,
    fixed: usize,
    moving: usize,
    max_closure: f64,
    bodies: &[usize],
) -> Result<(), String> {
    if model(p, id)?
        .clamp
        .as_ref()
        .is_some_and(|c| c.closed_by > 1e-7)
    {
        return Err("Open the jaw before reconfiguring it".into());
    }
    let mesh = &model(p, id)?.part.geometry.mesh;
    if fixed >= mesh.triangles.len() || moving >= mesh.triangles.len() || fixed == moving {
        return Err("Pick two distinct valid gripping faces".into());
    }
    let moving_vertices = assembly_group(&model(p, id)?.part.geometry, fixed, moving, bodies)?;
    p.workspace.iter_mut().find(|m| m.id == id).unwrap().clamp = Some(Profile {
        fixed,
        moving,
        max_closure,
        closed_by: 0.,
        moving_vertices,
        grip: 3.,
    });
    details(p, id)?;
    Ok(())
}
fn translate(m: &mut WorkspaceModel, indices: &[usize], delta: [f64; 3]) {
    let zero = m.point([0.; 3], None);
    let basis: [[f64; 3]; 3] = std::array::from_fn(|i| {
        let mut v = [0.; 3];
        v[i] = 1.;
        let q = m.point(v, None);
        std::array::from_fn(|j| q[j] - zero[j])
    });
    let factor = m.part.units.factor();
    let local: [f64; 3] = std::array::from_fn(|i| {
        (0..3).map(|j| basis[i][j] * delta[j]).sum::<f64>() / (factor * factor)
    });
    let mesh = &mut Arc::make_mut(&mut m.part.geometry).mesh;
    for i in indices {
        for (a, d) in local.iter().enumerate() {
            mesh.vertices[*i][a] += d
        }
    }
}
pub fn open(p: &mut Project, id: u64) -> Result<(), String> {
    let (_, _, axis, sign, indices) = details(p, id)?;
    let m = p.workspace.iter_mut().find(|m| m.id == id).unwrap();
    let mut d = [0.; 3];
    d[axis] = sign * m.clamp.as_ref().unwrap().closed_by;
    translate(m, &indices, d);
    m.clamp.as_mut().unwrap().closed_by = 0.;
    Ok(())
}
pub fn clamp(p: &mut Project, id: u64, grip: f64) -> Result<(), String> {
    if !grip.is_finite() || !(0.1..=1000.).contains(&grip) || grip >= p.stock.thickness {
        return Err("Grip must be positive and less than stock thickness".into());
    }
    let def = p.stock.definition();
    if def.rotation.iter().any(|r| r.abs() > 1e-8)
        || !matches!(p.stock.profile, crate::blank::StockProfile::Rectangular)
    {
        return Err("Clamping requires unrotated rectangular stock".into());
    }
    open(p, id)?;
    let (f, g, axis, sign, indices) = details(p, id)?;
    let size = [p.stock.width, p.stock.height];
    let closure = (g.center[axis] - f.center[axis]) * sign - size[axis];
    if closure < -1e-6 || closure > model(p, id)?.clamp.as_ref().unwrap().max_closure + 1e-6 {
        return Err(format!("Jaw opening {:.3} mm, stock {:.3} mm: required closing travel {:.3} mm, allowed {:.3} mm. Review the selected gripping faces and measured travel limit.", (g.center[axis]-f.center[axis])*sign, size[axis], closure, model(p,id)?.clamp.as_ref().unwrap().max_closure));
    }
    let other = 1 - axis;
    let (lo, hi) = bounds(&f);
    let mut offset = [0.; 3];
    offset[axis] = if sign > 0. { 0. } else { size[axis] } - f.center[axis];
    offset[other] = size[other] / 2. - (lo[other] + hi[other]) / 2.;
    offset[2] = -p.stock.thickness + grip - hi[2];
    let m = p.workspace.iter_mut().find(|m| m.id == id).unwrap();
    for (i, d) in offset.iter().enumerate() {
        m.part.offset[i] += d
    }
    let (f, g, _, _, _) = details(p, id)?;
    for face in [&f, &g] {
        let (lo, hi) = bounds(face);
        if lo[other] > 1e-5
            || hi[other] < size[other] - 1e-5
            || lo[2] > -p.stock.thickness + 1e-5
            || hi[2] < -p.stock.thickness + grip - 1e-5
        {
            return Err("Jaw faces cannot cover the stock width and gripping depth".into());
        }
    }
    let mut delta = [0.; 3];
    delta[axis] = -sign * closure.max(0.);
    let m = p.workspace.iter_mut().find(|m| m.id == id).unwrap();
    translate(m, &indices, delta);
    m.clamp.as_mut().unwrap().closed_by = closure.max(0.);
    m.clamp.as_mut().unwrap().grip = grip;
    crate::collision::check_stock_in_vise(p, id)?;
    Ok(())
}

/// Seat stock by relocating its work origin while keeping the vise fixed in the machine.
/// Use a Session transaction so any failure also rolls back geometry and the origin.
pub fn clamp_stock_in_vise(p: &mut Project, id: u64, grip: f64) -> Result<(), String> {
    let machine = p.machine.as_ref().ok_or("Machine setup is required")?;
    machine.validate(p.process)?;
    let m = model(p, id)?;
    let work_offset = m.offset(Some(machine));
    let machine_offset = if m.frame == Frame::Machine {
        m.part.offset
    } else {
        std::array::from_fn(|i| work_offset[i] + machine.work_origin[i])
    };
    // The existing clamp seats the vise around the work-origin stock. Its translation
    // gives the inverse movement needed to seat that same stock in the fixed vise.
    let m = p.workspace.iter_mut().find(|m| m.id == id).unwrap();
    m.frame = Frame::Work;
    m.part.offset = work_offset;
    clamp(p, id, grip)?;
    let seated_offset = model(p, id)?.part.offset;
    let machine = p.machine.as_mut().unwrap();
    for i in 0..3 {
        machine.work_origin[i] -= seated_offset[i] - work_offset[i];
    }
    machine.validate(p.process)?;
    let m = p.workspace.iter_mut().find(|m| m.id == id).unwrap();
    m.frame = Frame::Machine;
    m.part.offset = machine_offset;
    crate::workspace::validate(&p.workspace, p.machine.as_ref())?;
    // Machine-framed fixtures now have different work positions. Check the final
    // placement, including the closed vise, rather than relying on the temporary pose.
    for m in &p.workspace {
        if m.kind != ModelKind::Machine {
            crate::collision::check_stock_in_vise(p, m.id)?;
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryProfile {
    format_version: u32,
    model: WorkspaceModel,
}
pub fn export(p: &Project, id: u64) -> Result<String, String> {
    let mut copy = p.clone();
    open(&mut copy, id)?;
    let mut portable = model(&copy, id)?.clone();
    portable.part.offset = portable.offset(copy.machine.as_ref());
    portable.frame = Frame::Work;
    serde_json::to_string(&LibraryProfile {
        format_version: 1,
        model: portable,
    })
    .map_err(|e| e.to_string())
}
pub fn add(p: &mut Project, data: &str) -> Result<(), String> {
    if data.len() as u64 > crate::model::MAX_PROJECT_BYTES {
        return Err("Vise profile exceeds file limits".into());
    }
    let mut item: LibraryProfile = serde_json::from_str(data).map_err(|e| e.to_string())?;
    if item.format_version != 1
        || item.model.clamp.is_none()
        || item.model.kind != ModelKind::Vise
        || item.model.frame != Frame::Work
    {
        return Err("Unsupported vise profile".into());
    }
    item.model.id = p.workspace.iter().map(|m| m.id).max().unwrap_or(0) + 1;
    item.model.validate(p.machine.as_ref())?;
    let id = item.model.id;
    p.workspace.push(item.model);
    crate::workspace::validate(&p.workspace, p.machine.as_ref())?;
    details(p, id)?;
    Ok(())
}

pub fn info(p: &Project, id: u64) -> Result<serde_json::Value, String> {
    let (f, g, axis, sign, indices) = details(p, id)?;
    let m = model(p, id)?;
    Ok(
        serde_json::json!({"opening":(g.center[axis]-f.center[axis])*sign,"original_opening":(g.center[axis]-f.center[axis])*sign+m.clamp.as_ref().unwrap().closed_by,"axis":if axis==0{"X"}else{"Y"},"moving_vertices":indices.len(),"stock_span":if axis==0 {p.stock.width} else {p.stock.height},"required_closure":(g.center[axis]-f.center[axis])*sign-if axis==0 {p.stock.width} else {p.stock.height},"max_closure":m.clamp.as_ref().unwrap().max_closure}),
    )
}
