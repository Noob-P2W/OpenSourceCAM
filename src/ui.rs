use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Vec2};
mod editors;
mod explorer;
mod features;
mod history;
mod icons;
mod inspector;
mod library;
mod machining;
mod menu;
mod setup;
mod toolbar;
mod workspace;
use opensourcecam::{
    cam::{self, Motion, Point3, Toolpath},
    import::{self, Part, SourceFormat, Units},
    model::{Operation, Project, Strategy},
    post,
    process::Process,
    view::Camera,
};
pub use setup::SetupSection;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};

const BG: Color32 = Color32::from_rgb(17, 22, 30);
const PANEL: Color32 = Color32::from_rgb(25, 31, 41);
const SURFACE: Color32 = Color32::from_rgb(34, 42, 55);
const LINE: Color32 = Color32::from_rgb(43, 53, 68);
const MUTED: Color32 = Color32::from_rgb(140, 156, 175);
const ACCENT: Color32 = Color32::from_rgb(64, 213, 177);
const CUT: Color32 = Color32::from_rgb(80, 179, 238);
const RAPID: Color32 = Color32::from_rgb(210, 158, 78);

#[derive(Default)]
pub struct LaunchOptions {
    pub editor_view: Option<&'static str>,
    pub feature_view: bool,
    pub stock_view: bool,
    pub stock_progress: Option<f32>,
    pub library_view: Option<&'static str>,
    pub screenshot: Option<PathBuf>,
    pub project: Option<(Project, PathBuf)>,
    pub setup_section: Option<SetupSection>,
}

pub fn launch(options: LaunchOptions) -> eframe::Result {
    eframe::run_native(
        "OpenSourceCAM",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 940.0])
                .with_min_inner_size([1080.0, 720.0]),
            renderer: eframe::Renderer::Glow,
            depth_buffer: 24,
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = PANEL;
            visuals.window_fill = PANEL;
            visuals.extreme_bg_color = BG;
            visuals.faint_bg_color = Color32::from_rgb(29, 37, 49);
            visuals.selection.bg_fill = Color32::from_rgb(32, 91, 81);
            visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 44, 57);
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
            cc.egui_ctx.set_visuals(visuals);
            cc.egui_ctx.style_mut(|style| {
                style.spacing.item_spacing = Vec2::new(8.0, 8.0);
                style.spacing.button_padding = Vec2::new(10.0, 7.0);
            });
            let mut app = Workbench::new();
            let gl = cc
                .gl
                .as_ref()
                .ok_or_else(|| std::io::Error::other("OpenGL renderer is unavailable"))?;
            app.mesh_view = Some(Arc::new(Mutex::new(
                crate::mesh_view::MeshView::new(gl).map_err(std::io::Error::other)?,
            )));
            app.screenshot = options.screenshot;
            if let Some((project, file)) = options.project {
                app.install_project(project, Some(file));
                app.notice = "Project opened".into();
            }
            if let Some(section) = options.setup_section {
                app.inspector_tab = InspectorTab::Setup;
                app.setup_section = section;
            }
            if let Some(mode) = options.library_view {
                app.library_ui.capture(mode, &app.project);
            }
            if options.stock_view {
                app.inspector_tab = InspectorTab::Machining;
                app.show_paths = false;
                app.progress = options.stock_progress.unwrap_or(1.0);
                if let Some(path) = &app.path {
                    app.stock_ui
                        .capture(&app.project, path, app.progress as f64);
                }
            }
            if options.feature_view {
                app.features_ui.capture(&app.project);
                app.open_features();
            }
            if let Some(view) = options.editor_view {
                match view {
                    "machine" => app.editors.open_machine(&app.project),
                    "operation" => app.editors.open_operation(0, &app.project),
                    _ => app.inspector_tab = InspectorTab::Workspace,
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Clone, Copy)]
enum Pending {
    New(Process),
    Demo,
    Open,
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum InspectorTab {
    Machining,
    Workspace,
    Features,
    Operation,
    Setup,
    Model,
    Post,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RibbonTab {
    Process,
    Inspect,
    Utilities,
}

struct Workbench {
    editors: editors::Editors,
    features_ui: features::FeatureState,
    machining_ui: machining::MachiningState,
    stock_ui: machining::StockState,
    library_ui: library::LibraryState,
    main_menu: bool,
    has_project: bool,
    inspector_tab: InspectorTab,
    ribbon_tab: RibbonTab,
    setup_section: SetupSection,
    project: Project,
    saved: Project,
    file: Option<PathBuf>,
    path: Option<Toolpath>,
    error: Option<String>,
    notice: String,
    selected: usize,
    history: history::History,
    history_epoch: u64,
    history_restored: bool,
    selected_path_only: bool,
    reveal_selection: bool,
    iso: bool,
    camera: Camera,
    grid: bool,
    rapids: bool,
    show_paths: bool,
    show_stock: bool,
    wireframe: bool,
    zoom: f32,
    pan: Vec2,
    playing: bool,
    progress: f32,
    speed: f32,
    code: String,
    code_tab: bool,
    analysis_open: bool,
    pending: Option<Pending>,
    allow_close: bool,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    import_choice: Option<PathBuf>,
    import_target: Option<opensourcecam::workspace::ModelKind>,
    workspace_selected: Option<u64>,
    show_workspace: bool,
    import_units: Units,
    import_receiver: Option<mpsc::Receiver<Result<Part, String>>>,
    import_cancel: Arc<AtomicBool>,
    mesh_view: Option<Arc<Mutex<crate::mesh_view::MeshView>>>,
    #[cfg(test)]
    ui_hits: Vec<(String, Rect)>,
    #[cfg(test)]
    inspector_scroll: Option<(egui::Id, Vec2)>,
}

impl Workbench {
    fn new() -> Self {
        let mut project = Project::default();
        project.ensure_tool_library();
        let mut app = Self {
            editors: Default::default(),
            features_ui: Default::default(),
            machining_ui: Default::default(),
            stock_ui: Default::default(),
            library_ui: Default::default(),
            main_menu: true,
            has_project: false,
            inspector_tab: InspectorTab::Setup,
            ribbon_tab: RibbonTab::Process,
            setup_section: SetupSection::Stock,
            saved: project.clone(),
            project,
            file: None,
            path: None,
            error: None,
            notice: "Example project loaded. All dimensions in millimetres.".into(),
            selected: 0,
            history: Default::default(),
            history_epoch: 0,
            history_restored: false,
            selected_path_only: false,
            reveal_selection: false,
            iso: true,
            camera: Camera::default(),
            grid: true,
            rapids: true,
            show_paths: true,
            show_stock: true,
            wireframe: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            playing: false,
            progress: 1.0,
            speed: 1.0,
            code: String::new(),
            code_tab: false,
            analysis_open: false,
            pending: None,
            allow_close: false,
            screenshot: None,
            screenshot_requested: false,
            import_choice: None,
            import_target: None,
            workspace_selected: None,
            show_workspace: true,
            import_units: Units::Millimetres,
            import_receiver: None,
            import_cancel: Arc::new(AtomicBool::new(false)),
            mesh_view: None,
            #[cfg(test)]
            ui_hits: Vec::new(),
            #[cfg(test)]
            inspector_scroll: None,
        };
        app.regenerate();
        app
    }

    fn regenerate(&mut self) {
        self.playing = false;
        self.progress = 1.0;
        if !self.project.operations.iter().any(|op| op.enabled) {
            self.path = None;
            self.code.clear();
            self.error = cam::validate_setup(&self.project).err();
            return;
        }
        match cam::generate(&self.project) {
            Ok(path) => {
                self.path = Some(path);
                self.error = None;
                match post::gcode(&self.project) {
                    Ok(code) => self.code = code,
                    Err(error) => {
                        self.code.clear();
                        self.error = Some(error);
                    }
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.path = None;
                self.code.clear();
            }
        }
    }

    fn dirty(&self) -> bool {
        self.project != self.saved
    }

    fn screen(&mut self, ctx: &egui::Context) {
        #[cfg(test)]
        self.ui_hits.clear();
        let before = self.project.clone();
        let selected_before = self.selected;
        let epoch = self.history_epoch;
        self.operation_shortcuts(ctx);
        if self.editors.is_open() {
            self.editor_screen(ctx);
            self.finish_edit(before, selected_before, epoch, ctx);
            return;
        }
        if self.library_ui.open {
            self.library_screen(ctx);
            self.finish_edit(before, selected_before, epoch, ctx);
            return;
        }
        self.header(ctx);
        if self.main_menu {
            self.main_menu_ui(ctx);
        } else {
            self.operations_toolbar(ctx);
            self.tree(ctx);
            self.inspector(ctx);
            self.finish_edit(before, selected_before, epoch, ctx);
            self.bottom(ctx);
            self.viewport(ctx);
            self.import_dialog(ctx);
            return;
        }
        self.finish_edit(before, selected_before, epoch, ctx);
    }

    fn install_project(&mut self, mut project: Project, file: Option<PathBuf>) {
        project.ensure_tool_library();
        self.history = Default::default();
        self.history_restored = false;
        self.history_epoch = self.history_epoch.wrapping_add(1);
        self.reveal_selection = true;
        self.library_ui = Default::default();
        self.editors = Default::default();
        self.workspace_selected = None;
        self.features_ui = Default::default();
        self.machining_ui = machining::MachiningState::for_project(&project);
        self.stock_ui = Default::default();
        self.project = project;
        self.workspace_selected = self
            .project
            .workspace
            .iter()
            .find(|m| m.kind == opensourcecam::workspace::ModelKind::Vise)
            .or_else(|| self.project.workspace.first())
            .map(|m| m.id);
        self.saved = self.project.clone();
        self.file = file;
        self.selected = 0;
        self.main_menu = false;
        self.has_project = true;
        self.ribbon_tab = RibbonTab::Process;
        self.setup_section = SetupSection::Stock;
        self.inspector_tab = if !self.project.operations.is_empty() {
            InspectorTab::Operation
        } else if self.project.part.is_some() {
            InspectorTab::Model
        } else {
            InspectorTab::Setup
        };
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.iso = true;
        self.camera = Camera::default();
        self.show_paths = true;
        self.selected_path_only = true;
        self.regenerate();
    }

    fn add_operation(&mut self, strategy: Strategy) {
        if !self.project.process.supports_toolpaths() || self.project.operations.len() >= 1000 {
            return;
        }
        let stock = &self.project.stock;
        let label = match strategy {
            Strategy::Pocket => "Pocket",
            Strategy::InsideProfile => "Inside profile",
            Strategy::OutsideProfile => "Outside profile",
            Strategy::Drill => "Peck drill",
            Strategy::DrillOnce => "Drill",
            Strategy::Facing => "Facing",
            Strategy::CircularPocket => "Circular pocket",
            Strategy::StockRough => "Outside clearing",
            Strategy::SurfaceRough => "3D roughing",
            Strategy::ParallelFinish => "3D finishing",
        };
        self.project.operations.push(Operation {
            model_link: None,
            settings: Default::default(),
            name: format!("{} {:02}", label, self.project.operations.len() + 1),
            strategy,
            x: stock.width * if strategy.is_drilling() { 0.5 } else { 0.15 },
            y: stock.height * if strategy.is_drilling() { 0.5 } else { 0.15 },
            width: stock.width * 0.7,
            height: stock.height * 0.7,
            depth: 3.0_f64.min(stock.thickness),
            enabled: true,
            tool_id: self.project.active_tool,
            preset_id: self.project.active_preset,
        });
        self.selected = self.project.operations.len() - 1;
        self.reveal_selection = true;
        if strategy == Strategy::Facing || strategy.is_surface() {
            let op = &mut self.project.operations[self.selected];
            op.x = 0.0;
            op.y = 0.0;
            op.width = self.project.stock.width;
            op.height = self.project.stock.height;
            if strategy.is_surface() {
                if let Some(part) = &self.project.part {
                    op.depth = (-part.bounds().min[2]).clamp(0.001, self.project.stock.thickness);
                    opensourcecam::machining::link(op, part);
                }
                op.settings.surface_step = Some(0.5);
                if strategy == Strategy::SurfaceRough {
                    op.settings.axial_allowance = 0.3_f64.min(op.depth * 0.1);
                } else {
                    op.settings.step_over_percent = Some(12.5);
                }
            }
        } else if strategy == Strategy::CircularPocket {
            let op = &mut self.project.operations[self.selected];
            op.x = self.project.stock.width / 2.0;
            op.y = self.project.stock.height / 2.0;
            op.width = self.project.stock.width.min(self.project.stock.height) * 0.3;
            op.height = op.width;
        }
        self.inspector_tab = InspectorTab::Operation;
        self.show_paths = true;
    }

    fn duplicate_selected(&mut self) {
        if self.project.operations.len() >= 1000 {
            return;
        }
        if let Some(mut op) = self.project.operations.get(self.selected).cloned() {
            op.name.push_str(" copy");
            self.selected += 1;
            self.project.operations.insert(self.selected, op);
            self.reveal_selection = true;
            self.inspector_tab = InspectorTab::Operation;
            self.notice = "Operation duplicated. Ctrl+Z to undo.".into();
        }
    }

    fn delete_selected(&mut self) {
        if self.selected < self.project.operations.len() {
            let name = self.project.operations.remove(self.selected).name;
            self.notice = format!("Deleted {name}. Ctrl+Z to undo.");
            self.selected = self
                .selected
                .min(self.project.operations.len().saturating_sub(1));
        }
    }

    fn move_selected(&mut self, up: bool) {
        let count = self.project.operations.len();
        if self.selected >= count {
            return;
        }
        let target = if up {
            self.selected.saturating_sub(1)
        } else {
            (self.selected + 1).min(count - 1)
        };
        self.project.operations.swap(self.selected, target);
        self.selected = target;
        self.reveal_selection = true;
    }

    fn request(&mut self, action: Pending, ctx: &egui::Context) {
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    fn perform(&mut self, action: Pending, ctx: &egui::Context) {
        self.import_cancel.store(true, Ordering::Relaxed);
        self.import_receiver = None;
        self.import_choice = None;
        match action {
            Pending::New(process) => {
                self.install_project(Project::new_for_process(process), None);
                self.notice = format!(
                    "{} workspace created. {}.",
                    process.label(),
                    process.status()
                );
            }
            Pending::Demo => {
                self.install_project(Project::default(), None);
                self.notice = "Milling demo loaded. All dimensions in millimetres.".into();
            }
            Pending::Open => {
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("CAM project (JSON)", &["json"])
                    .pick_file()
                {
                    match Project::load(&file).and_then(|p| {
                        cam::validate_setup(&p)?;
                        Ok(p)
                    }) {
                        Ok(project) => {
                            self.install_project(project, Some(file));
                            self.notice = "Project opened".into();
                        }
                        Err(error) => self.notice = format!("Open failed: {error}"),
                    }
                }
            }
            Pending::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn save(&mut self, save_as: bool) -> bool {
        if let Err(error) = cam::validate_setup(&self.project) {
            self.notice = format!("Fix project before saving: {error}");
            return false;
        }
        let file = if save_as || self.file.is_none() {
            rfd::FileDialog::new()
                .add_filter("CAM project", &["json"])
                .set_file_name("project.cam.json")
                .save_file()
        } else {
            self.file.clone()
        };
        if let Some(file) = file {
            match self.project.save(&file) {
                Ok(()) => {
                    self.notice = format!("Saved {}", file.display());
                    self.file = Some(file);
                    self.saved = self.project.clone();
                    return true;
                }
                Err(error) => self.notice = format!("Save failed: {error}"),
            }
        }
        false
    }

    fn export(&mut self) {
        match post::gcode(&self.project) {
            Ok(code) => {
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("G-code", &["nc", "gcode"])
                    .set_file_name(format!(
                        "toolpath-{}.{}",
                        self.project.post.processor.id(),
                        self.project.post.processor.extension()
                    ))
                    .save_file()
                {
                    if let Some(project_file) = &self.file {
                        if let Err(error) =
                            opensourcecam::files::ensure_distinct(project_file, &file)
                        {
                            self.notice = format!("Export failed: {error}");
                            return;
                        }
                    }
                    match opensourcecam::files::write_atomic(&file, code.as_bytes()) {
                        Ok(()) => self.notice = format!("G-code exported to {}", file.display()),
                        Err(error) => self.notice = format!("Export failed: {error}"),
                    }
                }
            }
            Err(error) => self.notice = format!("Export blocked: {error}"),
        }
    }

    fn choose_import(&mut self) {
        self.import_target = None;
        self.choose_model_file();
    }

    fn choose_model_file(&mut self) {
        if let Some(file) = rfd::FileDialog::new()
            .add_filter(
                "Machining models",
                &["stl", "obj", "step", "stp", "iges", "igs"],
            )
            .add_filter("Mesh (STL / OBJ)", &["stl", "obj"])
            .add_filter("CAD (STEP / IGES)", &["step", "stp", "iges", "igs"])
            .pick_file()
        {
            self.import_choice = Some(file);
            self.import_units = Units::Millimetres;
        }
    }

    fn begin_import(&mut self, file: PathBuf) {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.import_cancel = cancel.clone();
        self.import_receiver = Some(receiver);
        self.notice = "Importing model...".into();
        let units = self.import_units;
        std::thread::spawn(move || {
            let _ = sender.send(import::import_with_cancel(&file, units, &cancel));
        });
    }

    fn poll_import(&mut self, ctx: &egui::Context) {
        let result = self.import_receiver.as_ref().map(|r| r.try_recv());
        match result {
            Some(Ok(result)) => {
                self.import_receiver = None;
                match result {
                    Ok(mut part) => {
                        let before = self.project.clone();
                        let selected_before = self.selected;
                        if let Some(kind) = self.import_target.take() {
                            let id = self
                                .project
                                .workspace
                                .iter()
                                .map(|m| m.id)
                                .max()
                                .unwrap_or(0)
                                + 1;
                            let model =
                                opensourcecam::workspace::WorkspaceModel::new(id, kind, part);
                            let mut candidate = self.project.workspace.clone();
                            candidate.push(model);
                            if let Err(error) = opensourcecam::workspace::validate(
                                &candidate,
                                self.project.machine.as_ref(),
                            ) {
                                self.notice = format!("Workspace import: {error}");
                                return;
                            }
                            self.project.workspace = candidate;
                            self.history
                                .record(before, selected_before, &self.project, None);
                            self.workspace_selected = Some(id);
                            self.inspector_tab = InspectorTab::Workspace;
                            self.show_workspace = true;
                            self.notice=format!("Imported {} model. Review its coordinate frame, placement and rotation in Workspace.",kind.label());
                            self.zoom = 1.0;
                            self.pan = Vec2::ZERO;
                            self.regenerate();
                            return;
                        }
                        part.align_to_top(5.0);
                        if let Err(error) = part.validate() {
                            self.notice = format!("Import: {error}");
                            return;
                        }
                        self.notice = format!("Imported {} ({} triangles). Use Features to review candidate holes and pockets, or define operations manually.", part.geometry.source_name, part.geometry.mesh.triangles.len());
                        self.project.part = Some(part);
                        self.history
                            .record(before, selected_before, &self.project, None);
                        self.inspector_tab = InspectorTab::Model;
                        self.zoom = 1.0;
                        self.pan = Vec2::ZERO;
                        self.show_paths = false;
                        self.regenerate();
                    }
                    Err(error) => self.notice = format!("Import: {error}"),
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.import_receiver = None;
                self.notice = "Importer stopped unexpectedly; current project was preserved".into();
            }
            Some(Err(mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
            }
            None => {}
        }
    }

    fn import_dialog(&mut self, ctx: &egui::Context) {
        if let Some(file) = self.import_choice.clone() {
            let format = SourceFormat::from_path(&file).ok();
            egui::Window::new("Import model")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(file.file_name().unwrap_or_default().to_string_lossy());
                    let cad = format.is_some_and(SourceFormat::is_cad);
                    if cad {
                        ui.label(
                            "STEP / IGES file units are converted to millimetres automatically.",
                        );
                        ui.label(if import::cad_backend().is_some() {
                            "Bundled OpenCascade importer is ready."
                        } else {
                            "Bundled CAD runtime is missing from this build."
                        });
                    } else {
                        ui.label("Mesh files have no reliable units. Choose the source units:");
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut self.import_units,
                                Units::Millimetres,
                                "Millimetres",
                            );
                            ui.selectable_value(&mut self.import_units, Units::Inches, "Inches");
                        });
                    }
                    if let Some(kind)=self.import_target {ui.label(format!("Add a {} reference model; source origin is preserved. Position it in Workspace after import.",kind.label()));} else {ui.label("The model is positioned at stock top with a 5 mm XY margin.");}
                    ui.label(
                        if self.import_target.is_none() {"This replaces the workpiece reference model. Stock and operations are preserved."} else {"Workspace imports preserve the workpiece, stock and operations."},
                    );
                    ui.label(
                        "Import preserves the model. Use Features to review supported holes and pockets; 3D surface machining remains planned.",
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !cad || import::cad_backend().is_some(),
                                egui::Button::new("Import model"),
                            )
                            .clicked()
                        {
                            self.import_choice = None;
                            self.begin_import(file);
                        }
                        if ui.button("Cancel").clicked() {
                            self.import_choice = None;
                        }
                    });
                });
        }
        if self.import_receiver.is_some() {
            egui::Window::new("Importing model")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Reading geometry and building the surface mesh...");
                    });
                    if ui.button("Cancel import").clicked() {
                        self.import_cancel.store(true, Ordering::Relaxed);
                    }
                });
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(60.0)
            .show(ctx, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("[ OC ]").color(ACCENT).size(22.0).strong());
                    ui.vertical(|ui| {
                        ui.label(RichText::new("OpenSourceCAM").size(19.0).strong());
                        ui.label(
                            RichText::new(if self.main_menu {
                                "LOCAL CAM WORKSPACE"
                            } else {
                                "MANUFACTURING · MILLIMETRES"
                            })
                            .color(MUTED)
                            .size(10.0),
                        );
                    });
                    ui.add_space(24.0);
                    if self.main_menu {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                    .color(MUTED)
                                    .monospace(),
                            );
                        });
                        return;
                    }
                    if ui
                        .add_enabled(self.import_receiver.is_none(), egui::Button::new("Open"))
                        .clicked()
                    {
                        self.request(Pending::Open, ctx);
                    }
                    if ui
                        .add_enabled(
                            self.import_receiver.is_none(),
                            egui::Button::new("Import model"),
                        )
                        .clicked()
                    {
                        self.choose_import();
                    }
                    if ui.button("Save").on_hover_text("Ctrl+S").clicked() {
                        self.save(false);
                    }
                    if ui.button("Save as").clicked() {
                        self.save(true);
                    }
                    for (label, redo, enabled, id) in [
                        ("Undo", false, self.history.can_undo(), "undo"),
                        ("Redo", true, self.history.can_redo(), "redo"),
                    ] {
                        let r = ui.add_enabled(enabled, egui::Button::new(label));
                        #[cfg(test)]
                        self.ui_hits.push((id.into(), r.rect));
                        let _ = id;
                        if r.clicked() {
                            self.undo_edit(redo);
                        }
                        r.on_hover_text(if redo {
                            "Redo · Ctrl+Shift+Z / Ctrl+Y"
                        } else {
                            "Undo · Ctrl+Z"
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(12.0);
                        if ui
                            .add_enabled(
                                self.project.process.supports_toolpaths()
                                    && self.path.is_some()
                                    && !self.code.is_empty(),
                                egui::Button::new(
                                    RichText::new("Export G-code").color(BG).strong(),
                                )
                                .fill(ACCENT),
                            )
                            .clicked()
                        {
                            self.export();
                        }
                        ui.label(
                            RichText::new(if self.project.process.supports_toolpaths() {
                                self.project.post.work_offset.code()
                            } else {
                                "MM / REFERENCE"
                            })
                            .color(MUTED)
                            .monospace(),
                        );
                    });
                });
            });
    }

    fn model_inspector(&mut self, ui: &mut egui::Ui) {
        let mut remove = false;
        if let Some(part) = &mut self.project.part {
            egui::CollapsingHeader::new("Imported model")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(RichText::new(&part.geometry.source_name).color(ACCENT));
                    let size = part.bounds().size();
                    ui.label(format!(
                        "{:.2} x {:.2} x {:.2} mm",
                        size[0], size[1], size[2]
                    ));
                    ui.label(
                        RichText::new("Target geometry · model-linked 2.5D machining")
                            .size(10.0)
                            .color(MUTED),
                    );
                    if !part.geometry.format.is_cad() {
                        let before = part.units;
                        egui::ComboBox::from_id_salt("model_units")
                            .selected_text(match part.units {
                                Units::Millimetres => "Source: millimetres",
                                Units::Inches => "Source: inches",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut part.units,
                                    Units::Millimetres,
                                    "Millimetres",
                                );
                                ui.selectable_value(&mut part.units, Units::Inches, "Inches");
                            });
                        if before != part.units {
                            part.align_to_top(5.0);
                        }
                    } else {
                        ui.label(
                            RichText::new("CAD file units converted to mm")
                                .size(11.0)
                                .color(MUTED),
                        );
                    }
                    numeric(ui, "Offset X", &mut part.offset[0], "mm");
                    numeric(ui, "Offset Y", &mut part.offset[1], "mm");
                    numeric(ui, "Offset Z", &mut part.offset[2], "mm");
                    ui.horizontal(|ui| {
                        if ui.button("Align to top").clicked() {
                            part.align_to_top(5.0);
                        }
                        if ui.button("Fit stock").clicked() {
                            let size = part.bounds().size();
                            if size[0] + 10.0 <= 100_000.0
                                && size[1] + 10.0 <= 100_000.0
                                && size[2] <= 10_000.0
                            {
                                part.align_to_top(5.0);
                                self.project.stock = opensourcecam::model::Stock {
                                    profile: Default::default(),
                                    width: size[0] + 10.0,
                                    height: size[1] + 10.0,
                                    thickness: size[2].max(0.001),
                                };
                                self.zoom = 1.0;
                                self.pan = Vec2::ZERO;
                                self.notice =
                                    "Stock fitted to model; review any existing manual operations"
                                        .into();
                            } else {
                                self.notice =
                                    "Model is too large for the supported stock limits".into();
                            }
                        }
                    });
                    if ui.small_button("Remove reference model").clicked() {
                        remove = true;
                    }
                });
            ui.separator();
        }
        if remove {
            self.project.part = None;
        }
    }

    fn bottom(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(30.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(error) = &self.error {
                        ui.colored_label(Color32::from_rgb(255, 133, 126), error);
                    } else {
                        ui.label(RichText::new(&self.notice).color(MUTED).size(11.0));
                    }
                });
            });
        if !self.project.process.supports_toolpaths() {
            return;
        }
        if !self.analysis_open {
            egui::TopBottomPanel::bottom("analysis_collapsed")
                .exact_height(38.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Toolpath playback").clicked() {
                            self.code_tab = false;
                            self.analysis_open = true;
                        }
                        if ui.button("NC program").clicked() {
                            self.code_tab = true;
                            self.analysis_open = true;
                        }
                        if let Some(path) = &self.path {
                            ui.label(
                                RichText::new(format!(
                                    "{:.2} m cut  /  {} path segments",
                                    path.cut_length / 1000.0,
                                    path.moves.len()
                                ))
                                .color(MUTED)
                                .size(11.0),
                            );
                        }
                    });
                });
            return;
        }
        egui::TopBottomPanel::bottom("analysis").resizable(true).default_height(if self.path.is_some() {195.0} else {90.0}).height_range(if self.path.is_some() {140.0..=360.0} else {65.0..=140.0}).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.code_tab, false, "Toolpath playback");
                ui.selectable_value(&mut self.code_tab, true, "NC program");
                if ui.button("Collapse").clicked() { self.analysis_open = false; }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(self.project.post.processor.label().to_uppercase()).color(MUTED).size(10.0));
                });
            });
            ui.separator();
            if self.code_tab {
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.code.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                });
            } else if let Some(path) = &self.path {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    metric(ui, "CUT DISTANCE", &format!("{:.2} m", path.cut_length / 1000.0));
                    ui.separator();
                    metric(ui, "ESTIMATED TIME", &format!("{}m {:02}s", (path.estimated_seconds / 60.0) as u64, path.estimated_seconds as u64 % 60));
                    ui.separator();
                    metric(ui, "PATH SEGMENTS", &path.moves.len().to_string());
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                        if self.progress >= 1.0 { self.progress = 0.0; }
                        self.playing = !self.playing;
                    }
                    if ui.button("Reset").clicked() { self.progress = 0.0; self.playing = false; }
                    let slider = ui.add(egui::Slider::new(&mut self.progress, 0.0..=1.0).show_value(false));
                    if slider.dragged() { self.playing = false; }
                    ui.label(format!("{:.0}%", self.progress * 100.0));
                    egui::ComboBox::from_id_salt("playback_speed").selected_text(format!("{}x", self.speed)).show_ui(ui, |ui| {
                        for speed in [1.0, 5.0, 20.0, 100.0] {
                            ui.selectable_value(&mut self.speed, speed, format!("{speed}x"));
                        }
                    });
                });
                ui.label(RichText::new("Playback uses the setup rapid feed; estimates exclude acceleration. Cut stock shows sampled removal; fixture contact is checked before NC export.").color(MUTED).size(11.0));
            } else {
                ui.label(if self.error.is_some() { "Correct the highlighted parameters to generate a toolpath." } else { "Use From stock for supported imported parts, or add a manual operation." });
            }
        });
    }

    fn viewport(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(0.0))
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    section(ui, "MANUFACTURING VIEW");
                    ui.add_space(10.0);
                    if ui.selectable_label(self.iso, "Isometric").clicked() {
                        self.iso = true;
                        self.camera = Camera::default();
                    }
                    if ui.selectable_label(!self.iso, "Top").clicked() {
                        self.iso = false;
                        self.camera = Camera::top();
                    }
                    if ui.button("Fit").clicked() {
                        self.zoom = 1.0;
                        self.pan = Vec2::ZERO;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(14.0);
                    ui.checkbox(&mut self.grid, "Grid");
                    if self.project.process.supports_toolpaths() {
                        ui.checkbox(&mut self.rapids, "Rapids");
                        ui.checkbox(&mut self.show_paths, "Paths");
                        if self.show_paths {
                            ui.checkbox(&mut self.selected_path_only, "Selected only");
                        }
                    }
                    ui.checkbox(&mut self.show_stock, "Blank");
                    if self.path.is_some()
                        && ui.checkbox(&mut self.stock_ui.show, "Cut stock").changed()
                        && self.stock_ui.show
                    {
                        self.show_paths = false;
                    }
                    ui.checkbox(&mut self.wireframe, "Wireframe");
                    ui.checkbox(&mut self.show_workspace, "Machine / fixtures");
                });
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(
                            "Left drag: orbit   Right / middle / Shift drag: pan   Wheel: zoom",
                        )
                        .size(11.0)
                        .color(MUTED),
                    );
                });
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::drag());
                let rect = response.rect;
                #[cfg(test)]
                self.ui_hits.push(("viewport".into(), rect));
                if response.dragged_by(egui::PointerButton::Primary)
                    && !ui.input(|i| i.modifiers.shift)
                {
                    let delta = response.drag_delta();
                    self.camera.orbit(delta.x as f64, delta.y as f64);
                    self.iso = true;
                } else if response.dragged() {
                    self.pan += response.drag_delta();
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.25, 8.0);
                }
                self.draw(&painter, rect);
            });
    }

    fn draw(&self, painter: &egui::Painter, rect: Rect) {
        let s = &self.project.stock;
        if !s.width.is_finite()
            || !s.height.is_finite()
            || !s.thickness.is_finite()
            || s.width <= 0.0
            || s.height <= 0.0
            || s.thickness <= 0.0
            || s.width > 100_000.0
            || s.height > 100_000.0
            || s.thickness > 10_000.0
        {
            return;
        }
        let raw = |p: Point3| -> Vec2 {
            let screen = self.camera.project([p.x, p.y, p.z]);
            Vec2::new(screen[0] as f32, screen[1] as f32)
        };
        let p = |x, y, z| Point3 { x, y, z };
        let corners = [
            p(0.0, 0.0, 0.0),
            p(s.width, 0.0, 0.0),
            p(s.width, s.height, 0.0),
            p(0.0, s.height, 0.0),
        ];
        let mut bounds = Rect::NOTHING;
        let stock_result = self
            .stock_ui
            .current(&self.project, self.progress as f64)
            .filter(|_| !self.wireframe);
        let part = self.project.part.as_ref().filter(|part| {
            stock_result.is_none()
                && part.visible
                && part
                    .offset
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= import::MAX_COORDINATE)
        });
        if self.show_stock || part.is_none() {
            for point in (import::Bounds {
                min: [0.0, 0.0, -s.thickness],
                max: [
                    s.width,
                    s.height,
                    if self.project.clearance.is_finite() {
                        self.project.clearance.clamp(0.0, 10_000.0)
                    } else {
                        0.0
                    },
                ],
            })
            .corners()
            {
                bounds.extend_with(Pos2::ZERO + raw(p(point[0], point[1], point[2])));
            }
        }
        if let Some(part) = part {
            for point in part.bounds().corners() {
                bounds.extend_with(Pos2::ZERO + raw(p(point[0], point[1], point[2])));
            }
        }
        let workspace: Vec<_> = self
            .project
            .workspace
            .iter()
            .filter(|m| {
                self.show_workspace
                    && m.part.visible
                    && m.validate_placement(self.project.machine.as_ref()).is_ok()
            })
            .collect();
        for model in &workspace {
            for point in model.bounds(self.project.machine.as_ref()).corners() {
                bounds.extend_with(Pos2::ZERO + raw(p(point[0], point[1], point[2])));
            }
        }
        let scale = ((rect.width() - 110.0) / bounds.width().max(1.0))
            .min((rect.height() - 110.0) / bounds.height().max(1.0))
            .max(0.01)
            * self.zoom;
        let project = |point: Point3| {
            rect.center() + self.pan + (raw(point) - bounds.center().to_vec2()) * scale
        };
        if self.grid {
            let step = 10_f64.powf((s.width.max(s.height) / 12.0).log10().floor());
            let count_x = (s.width / step).ceil().min(150.0) as usize;
            let count_y = (s.height / step).ceil().min(150.0) as usize;
            for i in 0..=count_x {
                let x = i as f64 * step;
                painter.line_segment(
                    [project(p(x, 0.0, 0.0)), project(p(x, s.height, 0.0))],
                    Stroke::new(0.5_f32, LINE),
                );
            }
            for i in 0..=count_y {
                let y = i as f64 * step;
                painter.line_segment(
                    [project(p(0.0, y, 0.0)), project(p(s.width, y, 0.0))],
                    Stroke::new(0.5_f32, LINE),
                );
            }
        }
        if self.show_stock && !matches!(s.profile, opensourcecam::blank::StockProfile::Rectangular)
        {
            for segment in s.outline().as_chunks::<2>().0 {
                let a = segment[0];
                let b = segment[1];
                painter.line_segment(
                    [
                        project(Point3 {
                            x: a[0],
                            y: a[1],
                            z: a[2],
                        }),
                        project(Point3 {
                            x: b[0],
                            y: b[1],
                            z: b[2],
                        }),
                    ],
                    Stroke::new(1.0_f32, Color32::from_rgb(180, 140, 65)),
                );
            }
        }
        if self.show_stock && matches!(s.profile, opensourcecam::blank::StockProfile::Rectangular) {
            let bottom = corners.map(|point| Point3 {
                z: -s.thickness,
                ..point
            });
            let mut faces = vec![corners, bottom];
            for i in 0..4 {
                faces.push([
                    corners[i],
                    corners[(i + 1) % 4],
                    bottom[(i + 1) % 4],
                    bottom[i],
                ]);
            }
            faces.sort_by(|a, b| {
                let depth = |face: &[Point3; 4]| {
                    face.iter()
                        .map(|v| self.camera.project([v.x, v.y, v.z])[2])
                        .sum::<f64>()
                };
                depth(a).total_cmp(&depth(b))
            });
            for face in faces {
                let positions = face.map(project);
                if part.is_none() && stock_result.is_none() {
                    let mut mesh = egui::Mesh::default();
                    for pos in positions {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos,
                            uv: egui::epaint::WHITE_UV,
                            color: Color32::from_rgba_unmultiplied(49, 65, 83, 100),
                        });
                    }
                    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
                    painter.add(egui::Shape::mesh(mesh));
                }
                for i in 0..4 {
                    painter.line_segment(
                        [positions[i], positions[(i + 1) % 4]],
                        Stroke::new(1.0_f32, Color32::from_rgb(72, 88, 106)),
                    );
                }
            }
        }
        if let Some(part) = part {
            if self.wireframe {
                let mesh = &part.geometry.mesh;
                let points: Vec<_> = mesh
                    .vertices
                    .iter()
                    .map(|point| {
                        let point = part.point(*point);
                        project(p(point[0], point[1], point[2]))
                    })
                    .collect();
                for face in &mesh.triangles {
                    let projected = face.map(|i| points[i as usize]);
                    for i in 0..3 {
                        painter.line_segment(
                            [projected[i], projected[(i + 1) % 3]],
                            Stroke::new(0.6_f32, ACCENT),
                        );
                    }
                }
            }
        }
        if self.wireframe {
            for model in &workspace {
                let points: Vec<_> = model
                    .part
                    .geometry
                    .mesh
                    .vertices
                    .iter()
                    .map(|v| {
                        let v = model.point(*v, self.project.machine.as_ref());
                        project(p(v[0], v[1], v[2]))
                    })
                    .collect();
                let stride = model
                    .part
                    .geometry
                    .mesh
                    .triangles
                    .len()
                    .div_ceil(25000)
                    .max(1);
                for triangle in model.part.geometry.mesh.triangles.iter().step_by(stride) {
                    let q = triangle.map(|i| points[i as usize]);
                    for i in 0..3 {
                        painter.line_segment([q[i], q[(i + 1) % 3]], Stroke::new(0.6_f32, MUTED));
                    }
                }
            }
        } else if let Some(renderer) = &self.mesh_view {
            let mut surfaces = Vec::new();
            let mut near = f64::NEG_INFINITY;
            let mut far = f64::INFINITY;
            let mut include = |b: import::Bounds| {
                for point in b.corners() {
                    let depth = self.camera.project(point)[2];
                    near = near.max(depth);
                    far = far.min(depth);
                }
            };
            if let Some(s) = stock_result {
                include(import::Bounds {
                    min: [0.0, 0.0, -self.project.stock.thickness],
                    max: [self.project.stock.width, self.project.stock.height, 0.0],
                });
                surfaces.push(crate::mesh_view::Surface {
                    geometry: s.geometry.clone(),
                    units: 1.0,
                    offset: [0.0; 3],
                    rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                    color: [0.72, 0.59, 0.32],
                });
            }
            if let Some(part) = part {
                include(part.bounds());
                surfaces.push(crate::mesh_view::Surface {
                    geometry: part.geometry.clone(),
                    units: part.units.factor(),
                    offset: part.offset,
                    rotation: part.rotation_matrix(),
                    color: [0.35, 0.68, 0.8],
                });
            }
            for model in &workspace {
                include(model.bounds(self.project.machine.as_ref()));
                surfaces.push(crate::mesh_view::Surface {
                    geometry: model.part.geometry.clone(),
                    units: model.part.units.factor(),
                    offset: model.offset(self.project.machine.as_ref()),
                    rotation: model.rotation_matrix(),
                    color: match model.kind {
                        opensourcecam::workspace::ModelKind::Machine => [0.45, 0.5, 0.55],
                        opensourcecam::workspace::ModelKind::Vise => [0.7, 0.62, 0.4],
                        _ => [0.55, 0.58, 0.65],
                    },
                });
            }
            if !surfaces.is_empty() {
                let renderer = renderer.clone();
                let view = crate::mesh_view::View {
                    camera: self.camera.matrix(),
                    center: [bounds.center().x, bounds.center().y],
                    scale: [2.0 * scale / rect.width(), 2.0 * scale / rect.height()],
                    pan: [
                        2.0 * self.pan.x / rect.width(),
                        2.0 * self.pan.y / rect.height(),
                    ],
                    depth: [((near + far) * 0.5) as f32, (near - far).max(1.0) as f32],
                };
                painter.add(egui::PaintCallback {
                    rect,
                    callback: Arc::new(eframe::egui_glow::CallbackFn::new(
                        move |_info, painter| {
                            if let Ok(mut renderer) = renderer.lock() {
                                renderer.paint(painter.gl(), surfaces.clone(), &view);
                            }
                        },
                    )),
                });
            }
        }
        // Geometry remains visible even when parameters make toolpaths invalid.
        if self.inspector_tab == InspectorTab::Features {
            if let Some(feature) = self.features_ui.selected(&self.project) {
                let color = Color32::from_rgb(255, 185, 74);
                for z in [feature.opening_z, feature.bottom_z] {
                    let mut outline: Vec<_> = feature
                        .outline
                        .iter()
                        .map(|v| project(p(v[0], v[1], z)))
                        .collect();
                    if let Some(first) = outline.first().copied() {
                        outline.push(first);
                    }
                    painter.add(egui::Shape::line(outline, Stroke::new(2.0_f32, color)));
                }
                let stride = (feature.outline.len() / 8).max(1);
                for v in feature.outline.iter().step_by(stride) {
                    painter.line_segment(
                        [
                            project(p(v[0], v[1], feature.opening_z)),
                            project(p(v[0], v[1], feature.bottom_z)),
                        ],
                        Stroke::new(1.0_f32, color),
                    );
                }
                painter.text(
                    rect.left_top() + Vec2::new(18.0, 20.0),
                    egui::Align2::LEFT_TOP,
                    "AMBER · Selected feature candidate",
                    egui::FontId::proportional(12.0),
                    color,
                );
            }
        }
        for (index, op) in self
            .project
            .operations
            .iter()
            .enumerate()
            .filter(|(index, op)| {
                op.enabled
                    && self.show_paths
                    && (!self.selected_path_only || *index == self.selected)
            })
        {
            if ![op.x, op.y, op.width, op.height]
                .iter()
                .all(|v| v.is_finite() && v.abs() < 200_000.0)
            {
                continue;
            }
            let stroke = Stroke::new(
                1.2_f32,
                if index == self.selected {
                    ACCENT
                } else {
                    MUTED
                },
            );
            if matches!(
                op.strategy,
                Strategy::Drill | Strategy::DrillOnce | Strategy::CircularPocket
            ) {
                painter.circle_stroke(
                    project(p(op.x, op.y, 0.0)),
                    ((if op.strategy == Strategy::CircularPocket {
                        op.width
                    } else {
                        self.project
                            .operation_tool(op)
                            .map_or(self.project.tool.diameter, |t| t.diameter)
                    }) as f32
                        * scale
                        / 2.0)
                        .clamp(2.0, 1000.0),
                    stroke,
                );
            } else {
                let outline = [
                    p(op.x, op.y, 0.0),
                    p(op.x + op.width, op.y, 0.0),
                    p(op.x + op.width, op.y + op.height, 0.0),
                    p(op.x, op.y + op.height, 0.0),
                    p(op.x, op.y, 0.0),
                ];
                painter.add(egui::Shape::line(
                    outline.iter().map(|point| project(*point)).collect(),
                    stroke,
                ));
            }
        }
        if let Some(path) = self.path.as_ref().filter(|_| self.show_paths) {
            let target_seconds = path.estimated_seconds * self.progress as f64;
            let mut elapsed = 0.0;
            let mut tip = path.moves.first().map(|m| m.from).unwrap_or_default();
            let mut tip_diameter = self.project.tool.diameter;
            let mut tool_operation = None;
            // Cap preview tessellation while preserving the full path for export.
            let stride = (path.moves.len() / 25_000).max(1);
            for (index, movement) in path.moves.iter().enumerate() {
                if tool_operation != Some(movement.operation) {
                    tip_diameter = self
                        .project
                        .operations
                        .get(movement.operation)
                        .and_then(|op| self.project.operation_tool(op).ok())
                        .map_or(self.project.tool.diameter, |t| t.diameter);
                    tool_operation = Some(movement.operation);
                }
                let rate = movement.feed;
                let duration = movement.from.distance(movement.to) / rate * 60.0;
                let fraction = ((target_seconds - elapsed) / duration).clamp(0.0, 1.0);
                let end = p(
                    movement.from.x + (movement.to.x - movement.from.x) * fraction,
                    movement.from.y + (movement.to.y - movement.from.y) * fraction,
                    movement.from.z + (movement.to.z - movement.from.z) * fraction,
                );
                if index % stride == 0
                    && (movement.motion != Motion::Rapid || self.rapids)
                    && (!self.selected_path_only || movement.operation == self.selected)
                {
                    let color = match movement.motion {
                        Motion::Rapid => RAPID,
                        Motion::Plunge => ACCENT,
                        Motion::Cut => CUT,
                    };
                    painter.line_segment(
                        [project(movement.from), project(end)],
                        Stroke::new(
                            if movement.operation == self.selected {
                                1.6_f32
                            } else {
                                0.8_f32
                            },
                            color,
                        ),
                    );
                }
                tip = end;
                elapsed += duration;
                if elapsed >= target_seconds {
                    break;
                }
            }
            if !self.selected_path_only || tool_operation == Some(self.selected) {
                let center = project(tip);
                painter.circle_filled(center, 4.0, ACCENT);
                painter.circle_stroke(
                    center,
                    (tip_diameter as f32 * scale / 2.0).clamp(3.0, 1000.0),
                    Stroke::new(1.5_f32, ACCENT),
                );
                if self.progress < 1.0 {
                    painter.text(
                        rect.left_top() + Vec2::new(18.0, 20.0),
                        egui::Align2::LEFT_TOP,
                        format!("X {:.2}   Y {:.2}   Z {:.2}", tip.x, tip.y, tip.z),
                        egui::FontId::monospace(12.0),
                        ACCENT,
                    );
                }
            }
        }
        let origin = project(Point3::default());
        painter.circle_filled(origin, 3.0, Color32::WHITE);
        for (point, label, color) in [
            (
                p(s.width * 0.18, 0.0, 0.0),
                "X",
                Color32::from_rgb(239, 112, 116),
            ),
            (p(0.0, s.height * 0.22, 0.0), "Y", ACCENT),
        ] {
            painter.line_segment([origin, project(point)], Stroke::new(2.0_f32, color));
            painter.text(
                project(point),
                egui::Align2::LEFT_BOTTOM,
                label,
                egui::FontId::monospace(12.0),
                color,
            );
        }
        painter.text(
            rect.left_bottom() + Vec2::new(18.0, -15.0),
            egui::Align2::LEFT_BOTTOM,
            if self.project.process.supports_toolpaths() {
                "BLUE  Cutting     AMBER  Rapid     GREEN  Plunge / geometry"
            } else {
                "REFERENCE MODEL  /  MILLIMETRES"
            },
            egui::FontId::proportional(11.0),
            MUTED,
        );
    }
}

impl eframe::App for Workbench {
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let (Some(gl), Some(renderer)) = (gl, self.mesh_view.take()) {
            if let Ok(mut renderer) = renderer.lock() {
                renderer.destroy(gl);
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_import(ctx);
        self.poll_features(ctx);
        self.poll_machining(ctx);
        if self.screenshot.is_some() {
            let captured = ctx.input(|i| {
                i.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = captured {
                let destination = self.screenshot.take().unwrap();
                if let Err(error) = save_screenshot(&destination, &image) {
                    eprintln!("Screenshot failed: {error}");
                    std::process::exit(1);
                }
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                if !self.screenshot_requested && ctx.cumulative_pass_nr() >= 10 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                    self.screenshot_requested = true;
                }
                ctx.request_repaint();
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Close);
        }
        if self.has_project
            && !self.library_ui.open
            && !self.editors.is_open()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S))
        {
            self.save(false);
        }
        if self.playing {
            if let Some(path) = &self.path {
                self.progress = (self.progress
                    + ctx.input(|i| i.stable_dt).min(0.1) * self.speed
                        / path.estimated_seconds.max(0.01) as f32)
                    .min(1.0);
                if self.progress >= 1.0 {
                    self.playing = false;
                }
                ctx.request_repaint();
            }
        }
        self.screen(ctx);
        if let Some(action) = self.pending {
            egui::Window::new("Unsaved project")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Save your changes before continuing?");
                    ui.horizontal(|ui| {
                        if ui.button("Save and continue").clicked() && self.save(false) {
                            self.pending = None;
                            self.perform(action, ctx);
                        }
                        if ui.button("Discard changes").clicked() {
                            self.pending = None;
                            self.perform(action, ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending = None;
                        }
                    });
                });
        }
    }
}

impl Drop for Workbench {
    fn drop(&mut self) {
        self.import_cancel.store(true, Ordering::Relaxed);
    }
}

fn save_screenshot(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.width() as u32,
        image.height() as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|color| color.to_array())
        .collect();
    writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED).size(10.0).strong());
}

fn numeric(ui: &mut egui::Ui, label: &str, value: &mut f64, suffix: &str) {
    ui.horizontal(|ui| {
        ui.add_sized([126.0, 25.0], egui::Label::new(label));
        ui.add(
            egui::DragValue::new(value)
                .speed(0.1)
                .max_decimals(3)
                .suffix(format!(" {suffix}")),
        );
    });
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(140.0);
        section(ui, label);
        ui.label(RichText::new(value).size(21.0).strong());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_frame(
        ctx: &egui::Context,
        app: &mut Workbench,
        size: Vec2,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.screen(ctx),
        );
    }

    fn tap(ctx: &egui::Context, app: &mut Workbench, id: &str) {
        let size = Vec2::new(1440.0, 940.0);
        screen_frame(ctx, app, size, vec![]);
        let pos = app
            .ui_hits
            .iter()
            .find(|(name, _)| name == id)
            .unwrap_or_else(|| {
                panic!(
                    "Missing UI action {id}; available {:?}",
                    app.ui_hits.iter().map(|(name, _)| name).collect::<Vec<_>>()
                )
            })
            .1
            .center();
        screen_frame(
            ctx,
            app,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        screen_frame(
            ctx,
            app,
            size,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        screen_frame(ctx, app, size, vec![]);
    }

    #[test]
    fn main_menu_cards_fit_and_open_every_process() {
        for size in [Vec2::new(1080.0, 720.0), Vec2::new(1440.0, 940.0)] {
            let ctx = egui::Context::default();
            let mut app = Workbench::new();
            screen_frame(&ctx, &mut app, size, vec![]);
            screen_frame(&ctx, &mut app, size, vec![]);
            let cards: Vec<_> = app
                .ui_hits
                .iter()
                .filter(|(name, _)| name.starts_with("process:"))
                .collect();
            assert_eq!(cards.len(), 9);
            assert_eq!(cards.last().unwrap().0, "process:Mixed");
            for (name, rect) in cards {
                assert!(
                    rect.min.x >= 0.0
                        && rect.max.x <= size.x
                        && rect.min.y >= 68.0
                        && rect.max.y <= size.y,
                    "Card {name} overflows at {size:?}: {rect:?}"
                );
            }
        }
        for process in Process::ALL {
            let ctx = egui::Context::default();
            let mut app = Workbench::new();
            tap(&ctx, &mut app, &format!("process:{}", process.label()));
            assert!(
                !app.main_menu,
                "Process card did not open {}",
                process.label()
            );
            assert_eq!(app.project.process, process);
            assert!(app.project.operations.is_empty());
            assert!(app.path.is_none());
        }
    }

    #[test]
    fn operation_list_actions_delete_last_item_and_restore_valid_programs() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        app.install_project(Project::default(), None);
        let original = app.project.clone();
        tap(&ctx, &mut app, "list:delete");
        assert_eq!(app.project.operations.len(), 1);
        assert!(app.history.can_undo());
        tap(&ctx, &mut app, "undo");
        assert_eq!(app.project, original);
        assert!(!app.code.is_empty());
        tap(&ctx, &mut app, "redo");
        assert_eq!(app.project.operations.len(), 1);
        tap(&ctx, &mut app, "list:delete");
        assert!(app.project.operations.is_empty());
        assert!(app.code.is_empty());
        assert!(app.path.is_none());
        tap(&ctx, &mut app, "undo");
        assert_eq!(app.project.operations.len(), 1);
        assert!(app.path.is_some());
        tap(&ctx, &mut app, "list:duplicate");
        assert_eq!(app.project.operations.len(), 2);
        assert!(!app.history.can_redo());
        tap(&ctx, &mut app, "list:up");
        assert_eq!(app.selected, 0);
        tap(&ctx, &mut app, "list:down");
        assert_eq!(app.selected, 1);
    }

    fn press_key(
        ctx: &egui::Context,
        app: &mut Workbench,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) {
        screen_frame(
            ctx,
            app,
            Vec2::new(1440.0, 940.0),
            vec![
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers,
                },
            ],
        );
        screen_frame(ctx, app, Vec2::new(1440.0, 940.0), vec![]);
    }
    #[test]
    fn operation_keyboard_shortcuts_respect_text_focus() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        app.install_project(Project::default(), None);
        tap(&ctx, &mut app, "op:0");
        press_key(&ctx, &mut app, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(app.project.operations.len(), 1);
        press_key(&ctx, &mut app, egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(app.project.operations.len(), 2);
        press_key(&ctx, &mut app, egui::Key::D, egui::Modifiers::CTRL);
        assert_eq!(app.project.operations.len(), 3);
        tap(&ctx, &mut app, "operation:name");
        press_key(&ctx, &mut app, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(
            app.project.operations.len(),
            3,
            "Delete in a text field must not delete an operation"
        );
    }
    #[test]
    fn right_click_deletes_the_clicked_operation_and_undo_restores_it() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        app.install_project(Project::default(), None);
        let original = app.project.clone();
        tap(&ctx, &mut app, "op:1");
        let pos = app
            .ui_hits
            .iter()
            .find(|(id, _)| id == "op:0")
            .unwrap()
            .1
            .center();
        for pressed in [true, false] {
            screen_frame(
                &ctx,
                &mut app,
                Vec2::new(1440.0, 940.0),
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        tap(&ctx, &mut app, "op:menu:0:Delete operation");
        assert_eq!(app.project.operations.len(), 1);
        assert_eq!(app.project.operations[0], original.operations[1]);
        tap(&ctx, &mut app, "undo");
        assert_eq!(app.project, original);
    }
    #[test]
    fn operation_editor_delete_is_undoable_and_stale_drafts_cannot_overwrite() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        app.install_project(Project::default(), None);
        let original = app.project.clone();
        tap(&ctx, &mut app, "list:edit");
        tap(&ctx, &mut app, "editor:delete");
        assert!(!app.editors.is_open());
        assert_eq!(app.project.operations.len(), 1);
        tap(&ctx, &mut app, "undo");
        assert_eq!(app.project, original);
        app.editors.open_operation(0, &app.project);
        app.project.operations.remove(0);
        let changed = app.project.clone();
        tap(&ctx, &mut app, "editor:save");
        assert_eq!(app.project, changed);
        assert!(app.editors.is_open());
        tap(&ctx, &mut app, "editor:cancel");
    }
    #[test]
    fn list_controls_and_long_names_fit_at_supported_window_sizes() {
        for size in [Vec2::new(1080.0, 720.0), Vec2::new(1440.0, 940.0)] {
            let ctx = egui::Context::default();
            let mut app = Workbench::new();
            let mut project = Project {
                name: "Long project name ".repeat(20),
                ..Default::default()
            };
            project.operations[0].name = "Very long operation name ".repeat(30);
            app.install_project(project, None);
            for _ in 0..4 {
                screen_frame(&ctx, &mut app, size, vec![]);
            }
            let view = app
                .ui_hits
                .iter()
                .find(|(id, _)| id == "viewport")
                .unwrap()
                .1;
            assert!(
                view.min.x <= 401.0 && view.max.x >= size.x - 461.0,
                "Long names created unused space beside the viewport: {view:?}"
            );
            for name in ["list:delete", "list:edit", "list:duplicate", "op:0"] {
                let rect = app.ui_hits.iter().find(|(id, _)| id == name).unwrap().1;
                assert!(
                    rect.min.x >= 0.0
                        && rect.max.x <= 401.0
                        && rect.min.y >= 0.0
                        && rect.max.y <= size.y,
                    "{name} overflows {size:?}: {rect:?}"
                );
            }
        }
    }

    #[test]
    fn top_buttons_create_operations_and_list_selection_edits_them() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        tap(&ctx, &mut app, "process:Milling");
        tap(&ctx, &mut app, "add:Pocket");
        assert_eq!(app.project.operations[0].strategy, Strategy::Pocket);
        assert!(app.path.is_some());
        tap(&ctx, &mut app, "add:Drill");
        assert_eq!(app.project.operations[1].strategy, Strategy::Drill);
        tap(&ctx, &mut app, "post");
        assert!(app.inspector_tab == InspectorTab::Post);
        assert_eq!(app.selected, 1);
        tap(&ctx, &mut app, "op:0");
        assert_eq!(app.selected, 0);
        assert!(app.inspector_tab == InspectorTab::Operation);
        tap(&ctx, &mut app, "duplicate");
        assert_eq!(app.project.operations.len(), 3);
        assert_eq!(app.project.operations[1].strategy, Strategy::Pocket);
        tap(&ctx, &mut app, "delete");
        assert_eq!(app.project.operations.len(), 2);
        assert_eq!(app.project.operations[1].strategy, Strategy::Drill);
    }

    #[test]
    fn ribbon_opens_process_specific_setup_and_limits_block_stale_export() {
        for process in Process::ALL.into_iter().filter(|p| *p != Process::Mixed) {
            let ctx = egui::Context::default();
            let mut app = Workbench::new();
            tap(&ctx, &mut app, &format!("process:{}", process.label()));
            tap(&ctx, &mut app, "machine");
            assert!(app.setup_section == SetupSection::Machine);
            let tool_rect = app.ui_hits.iter().find(|(id, _)| id == "tool").unwrap().1;
            for (_, rect) in app
                .ui_hits
                .iter()
                .filter(|(id, _)| id.starts_with("planned:"))
            {
                assert!(
                    rect.min.x >= tool_rect.max.x,
                    "Planned buttons overlap setup: {process:?}, {rect:?}, {tool_rect:?}"
                );
            }
            assert_eq!(
                app.project.machine.as_ref().unwrap().tooling.process(),
                process
            );
            tap(&ctx, &mut app, "tool");
            assert!(app.library_ui.open);
        }
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        tap(&ctx, &mut app, "process:Milling");
        tap(&ctx, &mut app, "add:Pocket");
        assert!(!app.code.is_empty());
        app.project.machine.as_mut().unwrap().axis_max[2] = 4.0;
        tap(&ctx, &mut app, "machine");
        tap(&ctx, &mut app, "machine:limits");
        assert!(app.project.machine.as_ref().unwrap().enforce_limits);
        assert!(app.path.is_none());
        assert!(app.code.is_empty());
        assert!(app.error.as_ref().unwrap().contains("Z travel"));
    }

    #[test]
    fn inspector_sections_keep_independent_scroll_and_visible_navigation() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 7.0);
        });
        tap(&ctx, &mut app, "process:Milling");
        tap(&ctx, &mut app, "machine");
        let size = Vec2::new(1080.0, 720.0);
        for _ in 0..3 {
            screen_frame(&ctx, &mut app, size, vec![]);
        }
        let (tool_id, _) = app.inspector_scroll.unwrap();
        let mut state = egui::scroll_area::State::load(&ctx, tool_id).unwrap();
        state.offset.y = 100.0;
        state.store(&ctx, tool_id);
        screen_frame(&ctx, &mut app, size, vec![]);
        assert!(
            app.inspector_scroll.unwrap().1.y > 50.0,
            "Stored {state:?}; rendered {:?}; setup {}",
            app.inspector_scroll,
            app.setup_section == SetupSection::Machine
        );
        let navigation = app
            .ui_hits
            .iter()
            .find(|(id, _)| id == "setup:Stock")
            .unwrap()
            .1;
        assert!(navigation.top() > 280.0, "Setup navigation scrolled away");
        app.setup_section = SetupSection::Stock;
        screen_frame(&ctx, &mut app, size, vec![]);
        let (machine_id, offset) = app.inspector_scroll.unwrap();
        assert_ne!(
            tool_id, machine_id,
            "Tool and machine views share scroll memory"
        );
        assert_eq!(offset.y, 0.0);
    }

    #[test]
    fn selected_library_preset_is_used_by_new_operations() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        tap(&ctx, &mut app, "process:Milling");
        let mut preset = app.project.tool_library.tools[0].presets[0].clone();
        preset.id = 2;
        preset.name = "Finishing".into();
        preset.values.insert("cut_feed".into(), 200.0);
        app.project.tool_library.tools[0].presets.push(preset);
        tap(&ctx, &mut app, "library");
        tap(&ctx, &mut app, "library:preset:2");
        tap(&ctx, &mut app, "library:assign");
        assert_eq!(app.project.active_preset, Some(2));
        tap(&ctx, &mut app, "add:Pocket");
        assert_eq!(app.project.operations[0].preset_id, Some(2));
        assert!(app.code.contains("F200.0000"));
    }

    #[test]
    fn detailed_editors_commit_only_valid_drafts_and_keep_footer_visible() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 7.0);
        });
        app.main_menu = false;
        app.has_project = true;
        app.editors.open_operation(0, &app.project);
        app.editors.operation.as_mut().unwrap().1.settings.feed = Some(250.0);
        tap(&ctx, &mut app, "editor:cancel");
        assert!(app.project.operations[0].settings.feed.is_none());
        app.editors.open_operation(0, &app.project);
        app.editors.operation.as_mut().unwrap().1.settings.feed = Some(250.0);
        tap(&ctx, &mut app, "editor:save");
        assert_eq!(app.project.operations[0].settings.feed, Some(250.0));
        assert!(app.code.contains("F250.0000"));
        let original = app.project.machine.clone();
        app.editors.open_machine(&app.project);
        app.editors.machine.as_mut().unwrap().axis_min[0] = 1e6;
        tap(&ctx, &mut app, "editor:save");
        assert!(app.editors.is_open());
        assert_eq!(app.project.machine, original);
        tap(&ctx, &mut app, "editor:cancel");
        app.project.tool_library.tools[0].name =
            "Long tool description for layout testing ".repeat(20);
        app.project.machine.as_mut().unwrap().name =
            "Long machine description for layout testing ".repeat(20);
        for size in [Vec2::new(1080.0, 720.0), Vec2::new(1440.0, 940.0)] {
            for machine in [false, true] {
                if machine {
                    app.editors.open_machine(&app.project);
                } else {
                    app.editors.open_operation(0, &app.project);
                }
                for tab in 0..5 {
                    app.editors.tab = tab;
                    for _ in 0..3 {
                        screen_frame(&ctx, &mut app, size, vec![]);
                    }
                    for id in ["editor:save", "editor:cancel"] {
                        let rect = app.ui_hits.iter().find(|(key, _)| key == id).unwrap().1;
                        assert!(
                            rect.min.x >= 0.0
                                && rect.max.x <= size.x
                                && rect.min.y >= 0.0
                                && rect.max.y <= size.y,
                            "{id} overflows {size:?}: {rect:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fixture_import_preserves_the_workpiece_and_can_be_cancelled_independently() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        let part = import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap();
        app.project.part = Some(part.clone());
        let original = app.project.part.clone();
        let (sender, receiver) = mpsc::channel();
        app.import_receiver = Some(receiver);
        app.import_target = Some(opensourcecam::workspace::ModelKind::Vise);
        sender.send(Ok(part)).unwrap();
        app.poll_import(&ctx);
        assert_eq!(app.project.part, original);
        assert_eq!(app.project.workspace.len(), 1);
        assert!(app.code.is_empty());
        assert!(app.error.as_ref().unwrap().contains("contacts fixture"));
        assert_eq!(
            app.project.workspace[0].kind,
            opensourcecam::workspace::ModelKind::Vise
        );
        let before = app.project.clone();
        let (sender, receiver) = mpsc::channel();
        app.import_receiver = Some(receiver);
        app.import_target = Some(opensourcecam::workspace::ModelKind::Machine);
        sender.send(Err("Import cancelled".into())).unwrap();
        app.poll_import(&ctx);
        assert_eq!(app.project, before);
    }

    #[test]
    fn from_stock_buttons_generate_material_preview_and_reject_stale_worker_results() {
        let ctx = egui::Context::default();
        let part = import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap();
        let mut app = Workbench::new();
        app.install_project(Project::from_part(part).unwrap(), None);
        screen_frame(&ctx, &mut app, Vec2::new(1440.0, 940.0), vec![]);
        screen_frame(&ctx, &mut app, Vec2::new(1440.0, 940.0), vec![]);
        tap(&ctx, &mut app, "machining:open");
        assert!(app.inspector_tab == InspectorTab::Machining);
        tap(&ctx, &mut app, "machining:generate");
        let start = std::time::Instant::now();
        while app.stock_ui.result.is_none() && start.elapsed().as_secs_f64() < 5.0 {
            app.poll_machining(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.project.operations.len(), 7);
        assert!(app
            .project
            .operations
            .iter()
            .all(|op| op.enabled && op.model_link.is_some()));
        assert!(
            app.stock_ui
                .result
                .as_ref()
                .unwrap()
                .summary
                .removed_volume_mm3
                > 1000.0
        );
        assert!(!app.code.is_empty());
        let planned = app.project.clone();
        app.undo_edit(false);
        assert!(app.project.operations.is_empty());
        assert_eq!(app.project.stock.thickness, 12.0);
        assert_eq!(app.project.part.as_ref().unwrap().bounds().max[2], 0.0);
        app.undo_edit(true);
        assert_eq!(app.project, planned);
        assert!(!app.code.is_empty());
        let mut fresh = app.project.clone();
        fresh.operations.clear();
        app.install_project(fresh, None);
        app.plan_from_stock();
        app.project.name = "Edited while planning".into();
        for _ in 0..100 {
            app.poll_machining(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(app.project.operations.is_empty());
        assert_eq!(app.project.name, "Edited while planning");
    }

    #[test]
    fn recognized_feature_buttons_generate_linked_operations_and_stale_geometry_is_hidden() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        let mut part = import::import_file(
            std::path::Path::new("examples/models/mount.stl"),
            Units::Millimetres,
        )
        .unwrap();
        part.align_to_top(5.0);
        app.install_project(Project::from_part(part).unwrap(), None);
        app.open_features();
        tap(&ctx, &mut app, "features:scan");
        let start = std::time::Instant::now();
        while app.features_ui.selected(&app.project).is_none()
            && start.elapsed() < std::time::Duration::from_secs(5)
        {
            app.poll_features(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(app.features_ui.selected(&app.project).is_some());
        tap(&ctx, &mut app, "features:create");
        assert_eq!(app.project.operations.len(), 1);
        assert!(app.project.operations[0].enabled);
        assert!(app.project.operations[0].model_link.is_some());
        assert!(!app.code.is_empty());
        assert!(app.inspector_tab == InspectorTab::Operation);
        app.project.part.as_mut().unwrap().offset[0] += 1.0;
        assert!(app.features_ui.selected(&app.project).is_none());
        app.poll_features(&ctx);
        assert!(app.features_ui.source.is_none());
    }

    #[test]
    fn tool_library_creates_cancels_and_assigns_tools_from_real_buttons() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        tap(&ctx, &mut app, "process:Milling");
        tap(&ctx, &mut app, "add:Pocket");
        tap(&ctx, &mut app, "library");
        assert!(app.library_ui.open);
        tap(&ctx, &mut app, "library:New");
        tap(&ctx, &mut app, "library:new:Flat end mill");
        tap(&ctx, &mut app, "library:cancel");
        assert_eq!(app.project.tool_library.tools.len(), 1);
        tap(&ctx, &mut app, "library:New");
        tap(&ctx, &mut app, "library:new:Flat end mill");
        tap(&ctx, &mut app, "library:save");
        assert_eq!(app.project.tool_library.tools.len(), 2);
        tap(&ctx, &mut app, "library:tool:2");
        tap(&ctx, &mut app, "library:assign");
        assert!(!app.library_ui.open);
        assert_eq!(app.project.active_tool, Some(2));
        tap(&ctx, &mut app, "add:Inside profile");
        assert_eq!(app.project.operations[1].tool_id, Some(2));
        app.open_library(Some(0));
        tap(&ctx, &mut app, "library:tool:2");
        tap(&ctx, &mut app, "library:assign");
        assert_eq!(app.project.operations[0].tool_id, Some(2));
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(!app.code.is_empty());
    }

    #[test]
    fn every_library_editor_keeps_save_cancel_visible_at_supported_sizes() {
        use opensourcecam::tool_library::{LibraryTool, ToolKind, ToolLibrary};
        for size in [Vec2::new(1080.0, 720.0), Vec2::new(1440.0, 940.0)] {
            for kind in ToolKind::ALL {
                let ctx = egui::Context::default();
                let mut app = Workbench::new();
                ctx.style_mut(|s| {
                    s.spacing.item_spacing = Vec2::new(10.0, 9.0);
                    s.spacing.button_padding = Vec2::new(13.0, 8.0);
                });
                app.project.tool_library = ToolLibrary {
                    tools: vec![LibraryTool::new(1, 1, kind)],
                };
                app.project.active_tool = Some(1);
                app.project.operations.clear();
                for mode in ["general", "edit", "shaft", "holder", "cutting", "post"] {
                    app.library_ui.capture(mode, &app.project);
                    for _ in 0..3 {
                        screen_frame(&ctx, &mut app, size, vec![]);
                    }
                    for id in ["library:save", "library:cancel", "library:preview"] {
                        let rect = app.ui_hits.iter().find(|(name, _)| name == id).unwrap().1;
                        assert!(
                            rect.min.x >= 0.0
                                && rect.max.x <= size.x
                                && rect.min.y >= 0.0
                                && rect.max.y <= size.y,
                            "{kind:?}/{mode} {id} overflows at {size:?}: {rect:?}"
                        );
                        if id == "library:preview" {
                            assert!(
                                rect.left() > size.x * 0.5,
                                "Preview column collapsed into the editor"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn process_switch_preserves_unsaved_project_until_confirmed() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        tap(&ctx, &mut app, "process:Milling");
        tap(&ctx, &mut app, "add:Inside profile");
        let project = app.project.clone();
        tap(&ctx, &mut app, "menu");
        assert!(app.main_menu);
        tap(&ctx, &mut app, "process:Turning");
        assert!(matches!(app.pending, Some(Pending::New(Process::Turning))));
        assert_eq!(app.project, project);
        app.pending = None; // Cancel the choice; resume must keep the same work.
        tap(&ctx, &mut app, "resume");
        assert!(!app.main_menu);
        assert_eq!(app.project, project);
        app.perform(Pending::New(Process::Mixed), &ctx);
        assert_eq!(app.project.process, Process::Mixed);
        assert!(app.project.operations.is_empty());
        assert!(app.path.is_none());
    }

    fn frame(ctx: &egui::Context, app: &mut Workbench, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0))),
                events,
                ..Default::default()
            },
            |ctx| app.viewport(ctx),
        );
    }

    #[test]
    fn viewport_primary_drag_orbits_and_secondary_drag_pans() {
        let ctx = egui::Context::default();
        let mut app = Workbench::new();
        frame(&ctx, &mut app, vec![]);
        let start = Pos2::new(400.0, 300.0);
        let moved = Pos2::new(450.0, 330.0);
        let before = app.camera;
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(moved)]);
        assert_ne!(
            app.camera, before,
            "Viewport did not rotate on a real primary pointer drag"
        );
        assert_eq!(app.pan, Vec2::ZERO);
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        let camera = app.camera;
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(Pos2::new(480.0, 340.0))],
        );
        assert_eq!(app.camera, camera);
        assert_ne!(
            app.pan,
            Vec2::ZERO,
            "Secondary drag did not pan the viewport"
        );
    }
}
