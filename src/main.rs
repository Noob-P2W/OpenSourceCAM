#[cfg(feature = "desktop")]
mod mesh_view;
#[cfg(feature = "desktop")]
mod ui;

use opensourcecam::{
    cam,
    import::{self, Units},
    model::Project,
    post,
};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("OpenSourceCAM: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, input, output] if flag == "--plan-stock" || flag == "--plan-surface" => {
            opensourcecam::files::ensure_distinct(Path::new(input), Path::new(output))?;
            let project = Project::load(Path::new(input))?;
            let planner = if flag == "--plan-surface" {
                opensourcecam::surface::from_stock
            } else {
                opensourcecam::machining::from_stock
            };
            let plan = planner(
                &project,
                &Default::default(),
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            plan.project.save(Path::new(output))?;
            println!(
                "Created {} enabled model-linked machining operations",
                plan.project.operations.len()
            );
            for note in plan.notes {
                println!("{note}");
            }
            Ok(())
        }
        [flag, input, output] if flag == "--simulate-stock" => {
            opensourcecam::files::ensure_distinct(Path::new(input), Path::new(output))?;
            let project = Project::load(Path::new(input))?;
            let path = cam::generate(&project)?;
            let result = opensourcecam::stock::simulate(
                &project,
                &path,
                1.0,
                if project.operations.iter().any(|op| op.strategy.is_surface()) {
                    128
                } else {
                    192
                },
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            opensourcecam::stock::export_obj(&result, Path::new(output))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result.summary).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        #[cfg(feature = "desktop")]
        [flag, input, output]
            if flag == "--screenshot-stock" || flag == "--screenshot-stock-half" =>
        {
            let project = Project::load(Path::new(input))?;
            ui::launch(ui::LaunchOptions {
                stock_view: true,
                stock_progress: Some(if flag == "--screenshot-stock-half" {
                    0.5
                } else {
                    1.0
                }),
                screenshot: Some(output.into()),
                project: Some((project, input.into())),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        #[cfg(feature = "desktop")]
        [flag, input, output]
            if matches!(
                flag.as_str(),
                "--screenshot-machine-config"
                    | "--screenshot-operation-editor"
                    | "--screenshot-workspace"
            ) =>
        {
            let project = Project::load(Path::new(input))?;
            ui::launch(ui::LaunchOptions {
                editor_view: Some(if flag == "--screenshot-machine-config" {
                    "machine"
                } else if flag == "--screenshot-operation-editor" {
                    "operation"
                } else {
                    "workspace"
                }),
                screenshot: Some(output.into()),
                project: Some((project, input.into())),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        #[cfg(feature = "desktop")]
        [flag, input, output] if flag == "--screenshot-features" => {
            let project = Project::load(Path::new(input))?;
            ui::launch(ui::LaunchOptions {
                feature_view: true,
                screenshot: Some(output.into()),
                project: Some((project, input.into())),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        [flag, file] if flag == "--features" => {
            let project = Project::load(Path::new(file))?;
            let part = project
                .part
                .as_ref()
                .ok_or("Project has no imported model")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&opensourcecam::features::recognize(part)?)
                    .map_err(|e| e.to_string())?
            );
            Ok(())
        }
        [flag, file] if flag == "--sample-library" => {
            let mut library = opensourcecam::tool_library::ToolLibrary::default();
            for kind in opensourcecam::tool_library::ToolKind::ALL {
                let tool = opensourcecam::tool_library::LibraryTool::new(
                    library.next_id(),
                    library.next_number(kind.process()),
                    kind,
                );
                library.tools.push(tool);
            }
            library.save(Path::new(file))
        }
        [flag, file] if flag == "--inspect-library" => {
            let library = opensourcecam::tool_library::ToolLibrary::load(Path::new(file))?;
            println!("Valid local library: {} tools", library.tools.len());
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            println!("OpenSourceCAM {}\n\n  opensourcecam                          Open desktop workbench\n  opensourcecam --open project.cam.json   Open a project (desktop)\n  opensourcecam --sample project.cam.json Write example project\n  opensourcecam --sample-library tools.cam-tools.json\n  opensourcecam --inspect-library tools.cam-tools.json\n  opensourcecam --import model.stl project.cam.json\n  opensourcecam --import-inches model.obj project.cam.json\n  opensourcecam --inspect model.step      Report model bounds and triangle count\n  opensourcecam --features project.cam.json   Report candidate holes and pockets\n  opensourcecam --plan-stock input.cam.json output.cam.json   Build a top-side stock plan\n  opensourcecam --plan-surface input.cam.json output.cam.json   Build 3D roughing and finishing\n  opensourcecam --simulate-stock project.cam.json stock.obj   Export cut-stock mesh\n  opensourcecam --validate project.cam.json\n  opensourcecam --posts                   List controller postprocessors\n  opensourcecam --export project.cam.json output.nc\n  opensourcecam --export-post mach3 project.cam.json output.nc\n  opensourcecam --screenshot output.png   Capture the main menu\n  opensourcecam --screenshot project.cam.json output.png\n  opensourcecam --screenshot-setup project.cam.json output.png\n  opensourcecam --screenshot-tool project.cam.json output.png",env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        [flag] if flag == "--posts" => {
            for processor in post::PostProcessor::ALL {
                println!(
                    "{}: {}{}",
                    processor.id(),
                    processor.label(),
                    if processor == post::PostProcessor::Klipper {
                        " - printing pipeline planned"
                    } else {
                        " - milling"
                    }
                );
            }
            Ok(())
        }
        [flag, file] if flag == "--sample" => Project::default().save(Path::new(file)),
        [flag, file] if flag == "--validate" => {
            let project = Project::load(Path::new(file))?;
            if !project.operations.iter().any(|op| op.enabled) {
                println!(
                    "Valid {} reference project; no enabled machining operations",
                    project.process.label()
                );
                return Ok(());
            }
            let path = post::checked_path(&project)?;
            println!(
                "Valid: {} moves, {:.1} mm cut, ~{:.0} s",
                path.moves.len(),
                path.cut_length,
                path.estimated_seconds
            );
            Ok(())
        }
        [flag, input, output] if flag == "--import" || flag == "--import-inches" => {
            opensourcecam::files::ensure_distinct(Path::new(input), Path::new(output))?;
            let units = if flag == "--import-inches" {
                Units::Inches
            } else {
                Units::Millimetres
            };
            let part = import::import_file(Path::new(input), units)?;
            let count = part.geometry.mesh.triangles.len();
            Project::from_part(part)?.save(Path::new(output))?;
            println!("Imported {count} triangles into {output}; no operations yet; use From stock or --plan-stock to generate supported machining");
            Ok(())
        }
        [flag, input] if flag == "--inspect" => {
            let part = import::import_file(Path::new(input), Units::Millimetres)?;
            println!(
                "{}: {} triangles, bounds {:?} to {:?} mm",
                part.geometry.format.label(),
                part.geometry.mesh.triangles.len(),
                part.bounds().min,
                part.bounds().max
            );
            Ok(())
        }
        [flag, input, output] if flag == "--export" => {
            opensourcecam::files::ensure_distinct(Path::new(input), Path::new(output))?;
            let project = Project::load(Path::new(input))?;
            let code = post::gcode(&project)?;
            opensourcecam::files::write_atomic(Path::new(output), code.as_bytes())
                .map_err(|e| e.to_string())?;
            println!("Exported {output}");
            Ok(())
        }
        [flag, id, input, output] if flag == "--export-post" => {
            opensourcecam::files::ensure_distinct(Path::new(input), Path::new(output))?;
            let mut project = Project::load(Path::new(input))?;
            project.post.processor = post::PostProcessor::parse(id)?;
            let code = post::gcode(&project)?;
            opensourcecam::files::write_atomic(Path::new(output), code.as_bytes())
                .map_err(|e| e.to_string())?;
            println!("Exported {output} using {}", project.post.processor.label());
            Ok(())
        }
        [] => {
            #[cfg(feature = "desktop")]
            {
                ui::launch(ui::LaunchOptions::default()).map_err(|e| e.to_string())
            }
            #[cfg(not(feature = "desktop"))]
            Err("Desktop feature disabled; use --help for CLI commands".into())
        }
        #[cfg(feature = "desktop")]
        [flag, output] if flag == "--screenshot" => ui::launch(ui::LaunchOptions {
            screenshot: Some(output.into()),
            ..Default::default()
        })
        .map_err(|e| e.to_string()),
        #[cfg(feature = "desktop")]
        [flag, input] if flag == "--open" => {
            let project = Project::load(Path::new(input))?;
            ui::launch(ui::LaunchOptions {
                project: Some((project, input.into())),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        #[cfg(feature = "desktop")]
        [flag, input, output] if flag == "--screenshot" => {
            let project = Project::load(Path::new(input))?;
            ui::launch(ui::LaunchOptions {
                project: Some((project, input.into())),
                screenshot: Some(output.into()),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        #[cfg(feature = "desktop")]
        [flag, input, output] if flag == "--screenshot-setup" || flag == "--screenshot-tool" => {
            let project = Project::load(Path::new(input))?;
            ui::launch(ui::LaunchOptions {
                project: Some((project, input.into())),
                screenshot: Some(output.into()),
                setup_section: Some(if flag == "--screenshot-tool" {
                    ui::SetupSection::Tool
                } else {
                    ui::SetupSection::Machine
                }),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        #[cfg(feature = "desktop")]
        [flag, input, output]
            if matches!(
                flag.as_str(),
                "--screenshot-library"
                    | "--screenshot-tool-types"
                    | "--screenshot-tool-editor"
                    | "--screenshot-tool-bottom"
                    | "--screenshot-tool-top"
                    | "--screenshot-cutting-data"
                    | "--screenshot-tool-post"
            ) =>
        {
            let mut project = Project::load(Path::new(input))?;
            project.ensure_tool_library();
            ui::launch(ui::LaunchOptions {
                project: Some((project, input.into())),
                screenshot: Some(output.into()),
                library_view: Some(match flag.as_str() {
                    "--screenshot-tool-types" => "types",
                    "--screenshot-tool-editor" => "edit",
                    "--screenshot-tool-bottom" => "edit-bottom",
                    "--screenshot-tool-top" => "edit-top",
                    "--screenshot-cutting-data" => "cutting",
                    "--screenshot-tool-post" => "post",
                    _ => "list",
                }),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        }
        _ => Err("Unknown arguments; use --help".into()),
    }
}
