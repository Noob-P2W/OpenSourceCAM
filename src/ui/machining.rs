use super::*;
use opensourcecam::{machining, stock};

type PlanResult = (Project, Result<machining::Plan, String>);
#[derive(Default)]
pub(super) struct MachiningState {
    surface: bool,
    options: machining::PlanOptions,
    receiver: Option<mpsc::Receiver<PlanResult>>,
    cancel: Arc<AtomicBool>,
    error: Option<String>,
    notes: Vec<String>,
}
impl Drop for MachiningState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl MachiningState {
    pub(super) fn for_project(project: &Project) -> Self {
        let mut state = Self::default();
        state.surface = project.operations.iter().any(|op| op.strategy.is_surface());
        state
    }
}
type StockResult = (Project, Result<stock::Simulation, String>);
#[derive(Default)]
pub(super) struct StockState {
    pub(super) show: bool,
    source: Option<Project>,
    requested: Option<(Project, u32, usize)>,
    resolution: usize,
    receiver: Option<mpsc::Receiver<StockResult>>,
    cancel: Arc<AtomicBool>,
    pub(super) result: Option<stock::Simulation>,
    error: Option<String>,
}
impl Drop for StockState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl StockState {
    pub(super) fn capture(&mut self, project: &Project, path: &Toolpath, progress: f64) {
        self.show = true;
        self.resolution = if project.operations.iter().any(|op| op.strategy.is_surface()) {
            128
        } else {
            192
        };
        self.source = Some(project.clone());
        self.requested = Some((project.clone(), (progress * 100.0) as u32, self.resolution));
        self.result = stock::simulate(
            project,
            path,
            progress,
            self.resolution,
            &AtomicBool::new(false),
        )
        .ok();
    }
    pub(super) fn current(&self, project: &Project, progress: f64) -> Option<&stock::Simulation> {
        self.result.as_ref().filter(|s| {
            self.show
                && self.source.as_ref() == Some(project)
                && s.summary.progress <= progress + 0.0001
        })
    }
}
impl Workbench {
    pub(super) fn plan_from_stock(&mut self) {
        let source = self.project.clone();
        let options = self.machining_ui.options.clone();
        let surface = self.machining_ui.surface;
        self.machining_ui.cancel.store(true, Ordering::Relaxed);
        self.machining_ui.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.machining_ui.cancel.clone();
        let (tx, rx) = mpsc::channel();
        self.machining_ui.receiver = Some(rx);
        self.machining_ui.error = None;
        std::thread::spawn(move || {
            let result = if surface {
                opensourcecam::surface::from_stock(&source, &options, &cancel)
            } else {
                machining::from_stock(&source, &options, &cancel)
            };
            let _ = tx.send((source, result));
        });
    }
    pub(super) fn poll_machining(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.machining_ui.receiver {
            match rx.try_recv() {
                Ok((source, result)) => {
                    self.machining_ui.receiver = None;
                    if source != self.project {
                        self.machining_ui.error = Some("Project changed during planning; generate again with the current settings.".into());
                    } else {
                        match result {
                            Ok(plan) => {
                                let selected = self.selected;
                                self.project = plan.project;
                                self.history.record(source, selected, &self.project, None);
                                self.machining_ui.notes = plan.notes;
                                self.features_ui = Default::default();
                                self.stock_ui = Default::default();
                                self.stock_ui.show = true;
                                self.selected = 0;
                                self.progress = 1.0;
                                self.playing = false;
                                self.show_paths = false;
                                self.regenerate();
                                self.notice = format!("{} model-linked machining operations generated. Review stock and cutting data before export.",self.project.operations.len());
                            }
                            Err(e) => self.machining_ui.error = Some(e),
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30))
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.machining_ui.receiver = None;
                    self.machining_ui.error =
                        Some("Planning worker stopped; project preserved".into());
                }
            }
        }
        if self
            .stock_ui
            .source
            .as_ref()
            .is_some_and(|p| p != &self.project)
            || self
                .stock_ui
                .requested
                .as_ref()
                .is_some_and(|(p, _, _)| p != &self.project)
        {
            let show = self.stock_ui.show;
            let resolution = self.stock_ui.resolution;
            self.stock_ui = Default::default();
            self.stock_ui.show = show;
            self.stock_ui.resolution = resolution;
        }
        if let Some(rx) = &self.stock_ui.receiver {
            match rx.try_recv() {
                Ok((source, result)) => {
                    self.stock_ui.receiver = None;
                    if source == self.project {
                        self.stock_ui.source = Some(source);
                        match result {
                            Ok(s) => {
                                self.stock_ui.result = Some(s);
                                self.stock_ui.error = None;
                            }
                            Err(e) => self.stock_ui.error = Some(e),
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30))
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.stock_ui.receiver = None;
                    self.stock_ui.error = Some("Stock worker stopped".into());
                }
            }
        }
        let key = (self.progress as f64 * 100.0).floor() as u32;
        let resolution = if self.stock_ui.resolution == 0 {
            128
        } else {
            self.stock_ui.resolution
        };
        if self.stock_ui.show
            && self.stock_ui.receiver.is_none()
            && self
                .stock_ui
                .requested
                .as_ref()
                .is_none_or(|(p, k, r)| p != &self.project || *k != key || *r != resolution)
        {
            if let Some(path) = self.path.clone() {
                let source = self.project.clone();
                self.stock_ui.requested = Some((source.clone(), key, resolution));
                let cancel = self.stock_ui.cancel.clone();
                let (tx, rx) = mpsc::channel();
                self.stock_ui.receiver = Some(rx);
                std::thread::spawn(move || {
                    let result =
                        stock::simulate(&source, &path, key as f64 / 100.0, resolution, &cancel);
                    let _ = tx.send((source, result));
                });
            }
        }
    }
    pub(super) fn machining_parameters(&mut self, ui: &mut egui::Ui) {
        ui.heading("Machine from stock");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.machining_ui.surface, false, "2.5D features");
            ui.selectable_value(&mut self.machining_ui.surface, true, "3D surfaces");
        });
        ui.label("Recognize the part, size the blank and generate enabled machining operations.");
        ui.separator();
        let planned = self
            .project
            .operations
            .iter()
            .any(|op| op.model_link.is_some());
        egui::CollapsingHeader::new("Stock and machining setup").default_open(!planned).show(ui,|ui| {
        section(ui, "CUTTING TOOL / PRESET");
        if let Ok(Some(tool)) = self.project.library_tool(None) {
            ui.label(format!("T{} · {}", tool.post.number, tool.name));
        }
        if ui.button("Choose tool / cutting data").clicked() {
            self.open_library(None);
        }
        section(ui, "BLANK ALLOWANCES");
        let o = &mut self.machining_ui.options;
        numeric(ui, "XY margin", &mut o.xy_margin, "mm");
        numeric(ui, "Top allowance", &mut o.top_allowance, "mm");
        numeric(ui, "Backing", &mut o.backing, "mm");
        if self.machining_ui.surface {ui.label("3D raster roughing + parallel XYZ finishing");}
        else {
        ui.checkbox(&mut o.face, "Face blank");
        ui.checkbox(&mut o.cavities, "Machine pockets and holes");
        ui.checkbox(&mut o.outside, "Clear outside part last");
        }
        if !self.project.operations.is_empty() {
            ui.checkbox(&mut o.replace_operations, "Replace existing operations");
        }
        ui.label(RichText::new("Blank top is Z0. Part is placed below the top allowance. Backing needs a second setup; zero backing can release the part.").color(MUTED).size(11.0));
        let busy = self.machining_ui.receiver.is_some();
        let generate = ui.add_enabled(
            !busy && self.project.part.is_some() && self.project.process == Process::Milling,
            egui::Button::new("Create machining toolpaths").fill(Color32::from_rgb(32, 91, 81)),
        );
        #[cfg(test)]
        self.ui_hits
            .push(("machining:generate".into(), generate.rect));
        if generate.clicked() {
            self.plan_from_stock();
        }
        if busy {
            ui.spinner();
            if ui.button("Cancel planning").clicked() {
                self.machining_ui.cancel.store(true, Ordering::Relaxed);
                self.machining_ui.receiver = None;
            }
        }
        });
        if let Some(e) = &self.machining_ui.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
        for note in &self.machining_ui.notes {
            ui.label(RichText::new(note).size(11.0).color(MUTED));
        }
        ui.separator();
        if ui
            .checkbox(&mut self.stock_ui.show, "Preview material removal")
            .changed()
            && self.stock_ui.show
        {
            self.show_paths = false;
        }
        if self.stock_ui.resolution == 0 {
            self.stock_ui.resolution = 128;
        }
        ui.horizontal(|ui| {
            ui.label("Preview cells");
            for n in [64, 128, 192] {
                ui.selectable_value(&mut self.stock_ui.resolution, n, n.to_string());
            }
        });
        if self.stock_ui.receiver.is_some() {
            ui.label("Updating stock…");
        }
        if let Some(e) = &self.stock_ui.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
        if let Some(s) = &self.stock_ui.result {
            ui.label(format!(
                "Stock at {:.0}% · {} × {} cells",
                s.summary.progress * 100.0,
                s.summary.resolution[0],
                s.summary.resolution[1]
            ));
            ui.label(format!("Removed {:.1} mm³", s.summary.removed_volume_mm3));
            if let Some(v) = s.summary.excess_material_mm3 {
                ui.label(format!("Excess material ≈ {v:.1} mm³"));
            }
            if let Some(v) = s.summary.sampled_gouge_mm3 {
                ui.label(format!("Sampled gouge ≈ {v:.1} mm³"));
            }
            if let Some(v) = s.summary.backing_mm {
                ui.label(format!("Backing {v:.3} mm"));
            }
            if let Some(e) = &s.summary.fixture_contact {
                ui.colored_label(Color32::LIGHT_RED, e);
            }
        }
        ui.label(RichText::new("Preview samples the actual cutter moves, including ball ends and changing Z. Volume comparison is available for supported 2.5D targets only. 3D planning protects the mesh upper surface; undercuts remain. Review grid step, cutting data and workholding before export.").size(11.0).color(MUTED));
    }
}
