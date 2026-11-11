//! Configurable example workholding; every generated jaw/base is a closed shell.
use crate::{
    blank,
    import::{Geometry, Part, SourceFormat, Units},
    model::Project,
    workspace::{ModelKind, WorkspaceModel},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViseSpec {
    pub jaw_width: f64,
    pub jaw_depth: f64,
    pub grip: f64,
    pub clearance: f64,
    pub reserve_backing: bool,
}
impl Default for ViseSpec {
    fn default() -> Self {
        Self {
            jaw_width: 100.,
            jaw_depth: 12.,
            grip: 3.,
            clearance: 1.,
            reserve_backing: true,
        }
    }
}
pub fn place_example(project: &mut Project, id: Option<u64>, spec: ViseSpec) -> Result<(), String> {
    if [spec.jaw_width, spec.jaw_depth, spec.grip, spec.clearance]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.1 || *v > 1000.)
    {
        return Err("Vise dimensions must be 0.1–1000 mm".into());
    }
    if let Some(id) = id {
        let model = project
            .workspace
            .iter()
            .find(|m| m.id == id)
            .ok_or("Vise not found")?;
        if model.vise.is_none() {
            return Err("Choose the configurable example vise; position imported fixtures using their measured offsets".into());
        }
    }
    if spec.reserve_backing {
        if let Some(part) = &project.part {
            let backing = project.stock.thickness + part.bounds().min[2];
            let extra = (spec.grip + spec.clearance - backing).max(0.);
            if extra > 0. {
                let mut definition = project.stock.definition();
                if blank::rotation(definition.rotation)[2][2].abs() < 1. - 1e-9 {
                    return Err("For a tilted blank, set backing and fit the stock before seating it in the vise".into());
                }
                definition.size[2] += extra;
                project.stock = definition.build()?;
            }
        }
    }
    if spec.grip >= project.stock.thickness {
        return Err("Jaw grip must be less than stock thickness".into());
    }
    let stock = &project.stock;
    let width = spec.jaw_width.max(stock.width + 20.);
    let center = stock.width / 2.;
    let bottom = -stock.thickness;
    let jaw_top = bottom + spec.grip;
    let y0 = -0.25;
    let y1 = stock.height + 0.25;
    let boxes = [
        (
            [center - width / 2., y0 - spec.jaw_depth - 8., bottom - 25.],
            [center + width / 2., y1 + spec.jaw_depth + 8., bottom - 15.],
        ),
        (
            [center - width / 2., y0 - spec.jaw_depth, bottom - 15.],
            [center + width / 2., y0, jaw_top],
        ),
        (
            [center - width / 2., y1, bottom - 15.],
            [center + width / 2., y1 + spec.jaw_depth, jaw_top],
        ),
    ];
    let part = Part {
        rotation: [0.; 3],
        geometry: Arc::new(Geometry {
            removed_degenerate_triangles: 0,
            source_name: "OpenSourceCAM example vise".into(),
            format: SourceFormat::Stl,
            mesh: blank::mesh_from_boxes(&boxes),
            cad_source: None,
        }),
        units: Units::Millimetres,
        offset: [0.; 3],
        visible: true,
    };
    part.validate()?;
    if let Some(id) = id {
        let model = project
            .workspace
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or("Vise not found")?;
        model.clamp = None;
        model.part = part;
        model.rotation = [0.; 3];
        model.vise = Some(spec)
    } else {
        let id = project.workspace.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let mut model = WorkspaceModel::new(id, ModelKind::Vise, part);
        model.name = "Example milling vise".into();
        model.vise = Some(spec);
        project.workspace.push(model)
    }
    crate::workspace::validate(&project.workspace, project.machine.as_ref())
}
