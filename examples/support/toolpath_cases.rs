use opensourcecam::{
    commands,
    import::{self, Units},
    machining::PlanOptions,
    model::{Project, Strategy},
    operation::{MillingMode, PocketPattern},
    surface,
    tool_library::{LibraryTool, ToolKind},
};
use std::{path::Path, sync::atomic::AtomicBool};

pub struct Case {
    pub name: String,
    pub project: Project,
}

pub fn numeric(strategy: Strategy) -> Project {
    let mut p = Project {
        part: None,
        operations: vec![],
        ..Default::default()
    };
    p.workspace.clear();
    p.stock.width = 60.;
    p.stock.height = 90.;
    p.stock.thickness = 10.;
    let kind = match strategy {
        Strategy::Thread => ToolKind::ThreadMill,
        Strategy::Chamfer | Strategy::Engrave => ToolKind::ChamferMill,
        Strategy::Drill | Strategy::DrillOnce => ToolKind::Drill,
        _ => ToolKind::FlatEndMill,
    };
    p.tool_library.tools = vec![LibraryTool::new(1, 1, kind)];
    p.active_tool = Some(1);
    p.active_preset = Some(1);
    let mut op = commands::operation_template(&p, strategy);
    op.x = 15.;
    op.y = 20.;
    op.width = 30.;
    op.height = 40.;
    op.depth = 3.;
    op.settings.step_down = Some(1.);
    op.settings.precleared = true;
    op.settings.pocket_pattern = PocketPattern::Offset;
    if strategy == Strategy::Facing {
        op.x = 0.;
        op.y = 0.;
        op.width = p.stock.width;
        op.height = p.stock.height;
    }
    if strategy.is_round() || strategy.is_drilling() {
        op.x = 30.;
        op.y = 45.;
        op.width = 24.;
        op.height = 24.;
    }
    if strategy == Strategy::Thread {
        op.settings.pitch = Some(p.tool_library.tools[0].dimension("thread_pitch"));
    }
    if strategy == Strategy::SurfaceSmooth {
        op.settings.height_offset = Some(-3.);
    }
    if strategy.is_curve() {
        op.settings.path = vec![[15., 20., 0.], [45., 20., 0.], [45., 60., 0.]];
        op.height = p.operation_tool(&op).unwrap().diameter;
        if matches!(strategy, Strategy::Chamfer | Strategy::Engrave) {
            op.depth = 0.5;
        }
    }
    p.operations = vec![op];
    p
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for strategy in Strategy::ALL {
        let mut project = if strategy.is_surface() {
            let part =
                import::import_file(Path::new("examples/models/mount.stl"), Units::Millimetres)
                    .unwrap();
            let mut p = Project::from_part(part).unwrap();
            p.ensure_tool_library();
            let mut planned = surface::from_stock(
                &p,
                &PlanOptions {
                    xy_margin: 4.,
                    top_allowance: 1.,
                    backing: 1.,
                    ..Default::default()
                },
                &AtomicBool::new(false),
            )
            .unwrap()
            .project;
            if strategy == Strategy::SurfaceRough {
                planned.operations.truncate(1);
            }
            planned
        } else {
            numeric(strategy)
        };
        for op in &mut project.operations {
            op.settings.milling_mode = Some(MillingMode::Optimized);
        }
        cases.push(Case {
            name: format!("{strategy:?}"),
            project,
        });
    }
    for strategy in [Strategy::StockRough, Strategy::AreaRough] {
        let mut project = numeric(strategy);
        let op = &mut project.operations[0];
        op.settings.path = vec![
            [15., 20., -3.],
            [45., 20., -3.],
            [45., 60., -3.],
            [15., 60., -3.],
        ];
        op.settings.closed = true;
        op.settings.height_offset = Some(0.);
        cases.push(Case {
            name: format!("{strategy:?} planar"),
            project: project.clone(),
        });
        let mut next = project.operations[0].clone();
        next.depth = 4.;
        for q in &mut next.settings.path {
            q[2] = -4.;
        }
        project.operations.push(next);
        cases.push(Case {
            name: format!("{strategy:?} sequential"),
            project,
        });
    }
    cases
}
