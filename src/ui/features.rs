use super::*;
use opensourcecam::features::{self, Report};

pub(super) struct FeatureState {
    pub(super) source: Option<Part>,
    report: Option<Report>,
    receiver: Option<mpsc::Receiver<Result<Report, String>>>,
    cancel: Arc<AtomicBool>,
    selected: usize,
    error: Option<String>,
    candidate: Option<Candidate>,
}
struct Candidate {
    project: Project,
    index: usize,
    result: Result<Operation, String>,
}
impl Default for FeatureState {
    fn default() -> Self {
        Self {
            source: None,
            report: None,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            selected: 0,
            error: None,
            candidate: None,
        }
    }
}
impl Drop for FeatureState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl FeatureState {
    fn operation(&mut self, project: &Project) -> Result<Operation, String> {
        if let Some(candidate) = &self.candidate {
            if candidate.project == *project && candidate.index == self.selected {
                return candidate.result.clone();
            }
        }
        let result = self
            .selected(project)
            .ok_or_else(|| "Recognize the current model first".to_string())
            .and_then(|feature| features::operation(project, feature));
        self.candidate = Some(Candidate {
            project: project.clone(),
            index: self.selected,
            result: result.clone(),
        });
        result
    }
    fn current(&self, project: &Project) -> bool {
        self.source.as_ref() == project.part.as_ref()
    }
    pub(super) fn selected(&self, project: &Project) -> Option<&features::Feature> {
        if !self.current(project) {
            return None;
        }
        self.report.as_ref()?.features.get(self.selected)
    }
    pub(super) fn capture(&mut self, project: &Project) {
        self.source = project.part.clone();
        if let Some(part) = &self.source {
            match features::recognize(part) {
                Ok(report) => self.report = Some(report),
                Err(error) => self.error = Some(error),
            }
        }
    }
}
impl Workbench {
    pub(super) fn open_features(&mut self) {
        self.inspector_tab = InspectorTab::Features;
    }
    fn recognize_features(&mut self) {
        self.features_ui = Default::default();
        let Some(part) = self.project.part.clone() else {
            return;
        };
        self.features_ui.source = Some(part.clone());
        let cancel = self.features_ui.cancel.clone();
        let (sender, receiver) = mpsc::channel();
        self.features_ui.receiver = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(features::recognize_with_cancel(&part, &cancel));
        });
    }
    pub(super) fn poll_features(&mut self, ctx: &egui::Context) {
        if self.features_ui.source.is_some() && !self.features_ui.current(&self.project) {
            self.features_ui = Default::default();
        }
        if let Some(receiver) = &self.features_ui.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.features_ui.receiver = None;
                    match result {
                        Ok(report) => self.features_ui.report = Some(report),
                        Err(error) => self.features_ui.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30))
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.features_ui.receiver = None;
                    self.features_ui.error =
                        Some("Recognition worker stopped; the project was preserved".into());
                }
            }
        }
    }
    pub(super) fn feature_parameters(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recognized features");
        if ui.button("Machine complete part from stock...").clicked() {
            self.inspector_tab = InspectorTab::Machining;
        }
        ui.label(RichText::new("Mesh candidates · top Z access").color(MUTED));
        let busy = self.features_ui.receiver.is_some();
        let scan = ui.add_enabled(
            self.project.part.is_some() && !busy,
            egui::Button::new(if busy {
                "Recognizing…"
            } else {
                "Recognize model"
            }),
        );
        #[cfg(test)]
        self.ui_hits.push(("features:scan".into(), scan.rect));
        if scan.clicked() {
            self.recognize_features();
        }
        if busy {
            ui.spinner();
            if ui.button("Cancel recognition").clicked() {
                self.features_ui = Default::default();
            }
        }
        if let Some(error) = &self.features_ui.error {
            ui.colored_label(Color32::from_rgb(255, 133, 126), error);
        }
        if self.project.part.is_none() {
            ui.label("Import a closed model to recognize holes and rectangular pockets.");
            return;
        }
        if !self.features_ui.current(&self.project) {
            ui.label("Recognize again after changing the model, placement or source units.");
            return;
        }
        let Some(report) = &self.features_ui.report else {
            return;
        };
        ui.label(format!(
            "{} candidates · {} unsupported openings",
            report.features.len(),
            report.skipped_openings
        ));
        if report.features.is_empty() {
            ui.label("No supported top-access features found. Manual operations remain available.");
        }
        egui::ScrollArea::vertical()
            .id_salt("feature_candidates")
            .max_height(210.0)
            .show(ui, |ui| {
                for (index, feature) in report.features.iter().enumerate() {
                    let name = if let Some(d) = feature.diameter {
                        format!(
                            "{} · Ø{d:.3} {}",
                            index + 1,
                            if feature.through {
                                "through hole"
                            } else {
                                "blind hole"
                            }
                        )
                    } else {
                        format!(
                            "{} · {:.3} × {:.3} pocket",
                            index + 1,
                            feature.max[0] - feature.min[0],
                            feature.max[1] - feature.min[1]
                        )
                    };
                    let selected = ui.selectable_value(&mut self.features_ui.selected, index, name);
                    #[cfg(test)]
                    self.ui_hits
                        .push((format!("features:select:{index}"), selected.rect));
                    #[cfg(not(test))]
                    let _ = selected;
                }
            });
        let notes = report.notes.clone();
        if let Some(feature) = self.features_ui.selected(&self.project).cloned() {
            ui.separator();
            section(ui, feature.kind.label());
            ui.label(format!(
                "Center X {:.3}  Y {:.3}",
                feature.center[0], feature.center[1]
            ));
            ui.label(format!(
                "Opening Z {:.3} · bottom Z {:.3}",
                feature.opening_z, feature.bottom_z
            ));
            ui.label(format!("Depth {:.3} mm", feature.depth()));
            if let Ok(Some(tool)) = self.project.library_tool(None) {
                ui.label(
                    RichText::new(format!("Active T{} · {}", tool.post.number, tool.name))
                        .color(ACCENT),
                );
            }
            if ui.button("Choose tool / preset").clicked() {
                self.open_library(None);
            }
            let candidate = self.features_ui.operation(&self.project);
            if let Err(error) = &candidate {
                ui.label(RichText::new(error).color(MUTED).size(11.0));
            }
            let create = ui.add_enabled(
                candidate.is_ok(),
                egui::Button::new("Generate feature toolpath").fill(Color32::from_rgb(32, 91, 81)),
            );
            #[cfg(test)]
            self.ui_hits.push(("features:create".into(), create.rect));
            if create.clicked() {
                if let Ok(mut operation) = candidate {
                    operation.enabled = true;
                    self.project.operations.push(operation);
                    self.selected = self.project.operations.len() - 1;
                    self.inspector_tab = InspectorTab::Operation;
                    self.notice="Model-linked feature toolpath generated. Review stock, cutting data and workholding before export.".into();
                }
            }
            ui.label(RichText::new("Generated operations are linked to the imported geometry. Moving or replacing the model requires rebuilding toolpaths.").color(MUTED).size(11.0));
            if feature.diameter.is_some() {
                ui.label(RichText::new("Round holes support smaller flat end mills. Through drilling includes the configured drill-point length and requires sufficient bottom allowance.").color(MUTED).size(11.0));
            }
        }
        ui.separator();
        for note in notes {
            ui.label(RichText::new(note).color(MUTED).size(11.0));
        }
    }
}
