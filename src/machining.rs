//! Model-derived geometry links and a reviewable top-side machining plan.
use crate::{
    cam,
    features::{self, Report},
    import::Part,
    model::{Operation, Project, Stock, Strategy},
    process::Process,
    tool_library::ToolKind,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelLink {
    pub model: String,
    pub geometry: String,
}
struct Fingerprint(u64);
impl Fingerprint {
    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
    fn number(&mut self, v: f64) {
        self.bytes(&v.to_bits().to_le_bytes());
    }
    fn finish(self) -> String {
        format!("{:016x}", self.0)
    }
}
pub fn model_fingerprint(part: &Part) -> String {
    let mut hash = Fingerprint(0xcbf29ce484222325);
    hash.bytes(b"OpenSourceCAM model link 1");
    hash.number(part.units.factor());
    for v in part.offset {
        hash.number(v);
    }
    for vertex in &part.geometry.mesh.vertices {
        for v in vertex {
            hash.number(*v);
        }
    }
    for triangle in &part.geometry.mesh.triangles {
        for v in triangle {
            hash.bytes(&v.to_le_bytes());
        }
    }
    if let Some(source) = &part.geometry.cad_source {
        hash.bytes(source.as_bytes());
    }
    hash.finish()
}
pub fn geometry_fingerprint(op: &Operation) -> String {
    let mut hash = Fingerprint(0xcbf29ce484222325);
    hash.bytes(&[match op.strategy {
        Strategy::Pocket => 1,
        Strategy::OutsideProfile => 2,
        Strategy::InsideProfile => 3,
        Strategy::Drill => 4,
        Strategy::Facing => 5,
        Strategy::CircularPocket => 6,
        Strategy::StockRough => 7,
        Strategy::SurfaceRough => 8,
        Strategy::ParallelFinish => 9,
    }]);
    for v in [op.x, op.y, op.width, op.height, op.depth] {
        hash.number(v);
    }
    hash.finish()
}
pub fn link(op: &mut Operation, part: &Part) {
    op.model_link = Some(ModelLink {
        model: model_fingerprint(part),
        geometry: geometry_fingerprint(op),
    });
}
pub fn validate_links(project: &Project) -> Result<(), String> {
    if !project
        .operations
        .iter()
        .any(|op| op.enabled && op.model_link.is_some())
    {
        return Ok(());
    }
    let part = project
        .part
        .as_ref()
        .ok_or("Linked machining operations require their imported part")?;
    let hash = model_fingerprint(part);
    for op in project.operations.iter().filter(|op| op.enabled) {
        if let Some(link) = &op.model_link {
            if link.model != hash {
                return Err(format!("{}: imported part or placement changed. Rebuild model-derived toolpaths before export.",op.name));
            }
            if link.geometry != geometry_fingerprint(op) {
                return Err(format!("{}: linked geometry changed. Rebuild it from the feature or explicitly unlink it for manual programming.",op.name));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanOptions {
    pub xy_margin: f64,
    pub top_allowance: f64,
    pub backing: f64,
    pub face: bool,
    pub outside: bool,
    pub cavities: bool,
    pub replace_operations: bool,
}
impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            xy_margin: 5.0,
            top_allowance: 1.0,
            backing: 0.5,
            face: true,
            outside: true,
            cavities: true,
            replace_operations: false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub project: Project,
    pub report: Report,
    pub notes: Vec<String>,
}
pub fn from_stock(
    project: &Project,
    options: &PlanOptions,
    cancel: &AtomicBool,
) -> Result<Plan, String> {
    if project.process != Process::Milling {
        return Err("Machining from stock currently requires Milling".into());
    }
    if !project.operations.is_empty() && !options.replace_operations {
        return Err("This job already has operations. Select Replace operations or start an empty imported-part job.".into());
    }
    for v in [options.xy_margin, options.top_allowance, options.backing] {
        if !v.is_finite() || !(0.0..=10000.0).contains(&v) {
            return Err("Stock allowances must be finite and between 0 and 10,000 mm".into());
        }
    }
    if !options.face && !options.outside && !options.cavities {
        return Err("Select at least one machining stage".into());
    }
    let mut result = project.clone();
    result.ensure_tool_library();
    let tool = result
        .library_tool(None)?
        .ok_or("Select a flat end mill in the tool library")?;
    if tool.kind != ToolKind::FlatEndMill || !tool.center_cutting {
        return Err("The complete top-side plan requires a center-cutting flat end mill. Choose a tool and cutting preset in the library.".into());
    }
    let id = tool.id;
    let preset = result.active_preset.unwrap_or(tool.default_preset);
    let part = result
        .part
        .as_mut()
        .ok_or("Import the target part before machining from stock")?;
    part.align_to_top(options.xy_margin);
    part.offset[2] -= options.top_allowance;
    let bounds = part.bounds();
    let size = bounds.size();
    result.stock = Stock {
        width: size[0] + 2.0 * options.xy_margin,
        height: size[1] + 2.0 * options.xy_margin,
        thickness: size[2] + options.top_allowance + options.backing,
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("Machining plan cancelled".into());
    }
    let report = features::recognize_with_cancel(part, cancel)?;
    if let Some(error) = &report.machining_error {
        return Err(error.clone());
    }
    let exterior = report
        .exterior
        .as_ref()
        .ok_or("No supported prismatic outside profile was found")?;
    let mut operations = Vec::new();
    let make = |strategy, name: &str, x, y, width, height, depth| Operation {
        model_link: None,
        settings: Default::default(),
        tool_id: Some(id),
        preset_id: Some(preset),
        name: name.into(),
        enabled: true,
        strategy,
        x,
        y,
        width,
        height,
        depth,
    };
    if options.face && options.top_allowance >= 0.001 {
        operations.push(make(
            Strategy::Facing,
            "Face blank to model top",
            0.0,
            0.0,
            result.stock.width,
            result.stock.height,
            options.top_allowance,
        ));
    }
    if options.cavities {
        for (index, feature) in report.features.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Machining plan cancelled".into());
            }
            let mut op = features::operation(&result, feature)?;
            op.enabled = true;
            op.name = format!("{} {}", feature.kind.label(), index + 1);
            operations.push(op);
        }
    }
    // Clear the exterior last; the backing remains as explicitly configured.
    if options.outside {
        operations.push(make(
            Strategy::StockRough,
            "Clear blank outside model",
            exterior.min[0],
            exterior.min[1],
            exterior.max[0] - exterior.min[0],
            exterior.max[1] - exterior.min[1],
            -exterior.bottom_z,
        ));
    }
    if operations.is_empty() {
        return Err("The selected stages do not produce a machining operation".into());
    }
    let part = result.part.as_ref().unwrap();
    for op in &mut operations {
        link(op, part);
    }
    result.operations = operations;
    cam::generate(&result)?;
    let mut notes=vec!["This is a top-side 2.5D plan. Review tool engagement, workholding, cutting data and the NC program before running a machine.".into()];
    if options.backing > 0.0 {
        notes.push(format!("{:.3} mm backing remains below the target part, including beneath through holes. A second setup is needed to remove it.",options.backing));
    } else if options.outside {
        notes.push("Full-depth outside clearing can release the part; use workholding appropriate to that motion.".into());
    }
    if report
        .features
        .iter()
        .any(|f| f.kind == features::FeatureKind::RectangularPocket)
    {
        notes.push("Internal pocket corners retain the cutter radius. Stock verification reports the corner material that cannot be removed by this end mill.".into());
    }
    if !options.face || !options.outside || !options.cavities {
        notes.push("Some machining stages are omitted; the stock preview shows the resulting unfinished material.".into());
    }
    Ok(Plan {
        project: result,
        report,
        notes,
    })
}
