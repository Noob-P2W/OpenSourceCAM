//! Original OpenSourceCAM vector illustrations. No third-party artwork or icon assets.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Workspace,
    Recognize,
    Setup,
    Tool,
    Pocket,
    Inside,
    Outside,
    Rough3d,
    Finish3d,
    Drill,
    Generate,
    Play,
    Post,
    Duplicate,
    Delete,
    Up,
    Down,
    Fit,
    Top,
    Orbit,
    Import,
    Save,
    Turning,
    Printer,
    Laser,
    Plasma,
    Grinding,
    Wire,
    Electrode,
}

impl Icon {
    pub(super) fn process(process: Process) -> Self {
        match process {
            Process::Milling => Self::Tool,
            Process::Turning => Self::Turning,
            Process::Printing3d => Self::Printer,
            Process::LaserCutting => Self::Laser,
            Process::PlasmaCutting => Self::Plasma,
            Process::Grinding => Self::Grinding,
            Process::WireEdm => Self::Wire,
            Process::SinkerEdm => Self::Electrode,
            Process::Mixed => Self::Workspace,
        }
    }
    pub(super) fn draw(self, painter: &egui::Painter, rect: Rect, enabled: bool) {
        let c = Canvas {
            p: painter,
            r: rect,
            alpha: if enabled { 1.0 } else { 0.28 },
        };
        let blue = Color32::from_rgb(80, 181, 230);
        let pale = Color32::from_rgb(173, 203, 220);
        let dark = Color32::from_rgb(53, 88, 113);
        let steel = Color32::from_rgb(115, 144, 163);
        let white = Color32::from_rgb(220, 234, 240);
        match self {
            Self::Recognize => {
                c.block();
                c.line(
                    &[
                        [10.0, 18.0],
                        [29.0, 18.0],
                        [29.0, 29.0],
                        [10.0, 29.0],
                        [10.0, 18.0],
                    ],
                    ACCENT,
                    2.0,
                );
                c.line(&[[30.0, 30.0], [42.0, 42.0]], white, 3.0);
                c.line(&[[17.0, 11.0], [17.0, 35.0]], blue, 1.0);
            }
            Self::Workspace => {
                for (x, y) in [(6.0, 6.0), (26.0, 6.0), (6.0, 26.0), (26.0, 26.0)] {
                    c.rect(
                        [x, y],
                        [x + 16.0, y + 16.0],
                        if x == y { blue } else { steel },
                    );
                    c.line(
                        &[
                            [x + 3.0, y + 12.0],
                            [x + 8.0, y + 5.0],
                            [x + 13.0, y + 12.0],
                        ],
                        white,
                        1.5,
                    );
                }
            }
            Self::Setup => {
                c.block();
                c.arrow([24.0, 25.0], [41.0, 33.0], Color32::from_rgb(245, 126, 115));
                c.arrow([24.0, 25.0], [8.0, 34.0], ACCENT);
                c.arrow([24.0, 25.0], [24.0, 3.0], blue);
            }
            Self::Pocket | Self::Inside | Self::Outside => {
                c.block();
                let ring = if matches!(self, Self::Outside) {
                    vec![
                        [3.0, 17.0],
                        [24.0, 5.0],
                        [45.0, 17.0],
                        [24.0, 29.0],
                        [3.0, 17.0],
                    ]
                } else {
                    vec![
                        [13.0, 18.0],
                        [24.0, 12.0],
                        [35.0, 18.0],
                        [24.0, 24.0],
                        [13.0, 18.0],
                    ]
                };
                if matches!(self, Self::Pocket) {
                    c.poly(&ring[..4], dark);
                    for y in [16.0, 19.0, 22.0] {
                        c.line(&[[17.0, y], [29.0, y - 4.0]], blue, 1.7);
                    }
                }
                c.line(
                    &ring,
                    if matches!(self, Self::Outside) {
                        RAPID
                    } else {
                        blue
                    },
                    2.2,
                );
                c.circle([ring[0][0], ring[0][1]], 2.4, ACCENT);
            }
            Self::Tool | Self::Drill => {
                c.rect([18.0, 3.0], [30.0, 21.0], pale);
                c.line(&[[21.0, 4.0], [21.0, 19.0]], white, 1.4);
                c.poly(
                    &[
                        [16.0, 20.0],
                        [32.0, 20.0],
                        [30.0, 35.0],
                        [24.0, 44.0],
                        [18.0, 35.0],
                    ],
                    blue,
                );
                for y in [23.0, 29.0, 35.0] {
                    c.line(&[[18.0, y], [30.0, y - 5.0]], dark, 2.2);
                }
                if matches!(self, Self::Drill) {
                    c.line(&[[11.0, 40.0], [37.0, 40.0]], steel, 1.3);
                } else {
                    c.line(&[[19.0, 37.0], [29.0, 37.0]], pale, 2.4);
                }
            }
            Self::Rough3d | Self::Finish3d => {
                c.block();
                for i in 0..4 {
                    let y = 12.0 + i as f32 * 3.0;
                    let z = if matches!(self, Self::Finish3d) {
                        i as f32 * 0.8
                    } else {
                        0.0
                    };
                    c.line(
                        &[
                            [9.0, y + 5.0],
                            [17.0, y],
                            [25.0, y + 3.0 - z],
                            [33.0, y - 2.0],
                            [40.0, y + 1.0],
                        ],
                        blue,
                        2.1,
                    );
                }
            }
            Self::Generate | Self::Post | Self::Duplicate | Self::Import | Self::Save => {
                if matches!(self, Self::Duplicate) {
                    c.poly(
                        &[
                            [8.0, 5.0],
                            [29.0, 5.0],
                            [36.0, 12.0],
                            [36.0, 37.0],
                            [8.0, 37.0],
                        ],
                        steel,
                    );
                }
                c.poly(
                    &[
                        [13.0, 9.0],
                        [31.0, 9.0],
                        [40.0, 18.0],
                        [40.0, 43.0],
                        [13.0, 43.0],
                    ],
                    pale,
                );
                c.poly(&[[31.0, 9.0], [31.0, 18.0], [40.0, 18.0]], white);
                match self {
                    Self::Post => {
                        c.text([26.0, 29.0], "NC", dark, 12.0);
                        c.line(&[[18.0, 38.0], [34.0, 38.0]], blue, 2.0);
                    }
                    Self::Generate => {
                        c.line(
                            &[
                                [18.0, 22.0],
                                [33.0, 22.0],
                                [18.0, 28.0],
                                [31.0, 28.0],
                                [18.0, 34.0],
                            ],
                            blue,
                            1.6,
                        );
                        c.line(&[[28.0, 37.0], [32.0, 41.0], [43.0, 29.0]], ACCENT, 3.0);
                    }
                    Self::Import => {
                        c.arrow([4.0, 28.0], [28.0, 28.0], blue);
                    }
                    Self::Save => {
                        c.rect([19.0, 23.0], [34.0, 38.0], blue);
                        c.rect([22.0, 26.0], [30.0, 31.0], white);
                    }
                    _ => {
                        for y in [24.0, 29.0, 34.0] {
                            c.line(&[[18.0, y], [34.0, y]], blue, 1.4);
                        }
                    }
                }
            }
            Self::Play => {
                c.circle([24.0, 24.0], 18.0, dark);
                c.poly(&[[19.0, 13.0], [35.0, 24.0], [19.0, 35.0]], ACCENT);
            }
            Self::Delete => {
                c.rect([14.0, 15.0], [35.0, 40.0], steel);
                c.line(&[[11.0, 12.0], [38.0, 12.0]], pale, 3.0);
                c.line(&[[20.0, 6.0], [29.0, 6.0]], pale, 3.0);
                for x in [20.0, 28.0] {
                    c.line(&[[x, 20.0], [x, 35.0]], dark, 2.0);
                }
            }
            Self::Up | Self::Down => {
                c.line(&[[9.0, 13.0], [39.0, 13.0]], steel, 2.0);
                c.line(&[[9.0, 35.0], [39.0, 35.0]], steel, 2.0);
                if matches!(self, Self::Up) {
                    c.arrow([24.0, 39.0], [24.0, 7.0], blue);
                } else {
                    c.arrow([24.0, 7.0], [24.0, 39.0], blue);
                }
            }
            Self::Fit => {
                c.rect([15.0, 16.0], [33.0, 32.0], steel);
                for (a, b) in [
                    ([17.0, 17.0], [5.0, 5.0]),
                    ([31.0, 17.0], [43.0, 5.0]),
                    ([17.0, 31.0], [5.0, 43.0]),
                    ([31.0, 31.0], [43.0, 43.0]),
                ] {
                    c.arrow(a, b, blue);
                }
            }
            Self::Top => {
                c.rect([9.0, 9.0], [39.0, 39.0], steel);
                c.rect([16.0, 16.0], [32.0, 32.0], dark);
                c.line(&[[16.0, 24.0], [32.0, 24.0]], blue, 2.0);
                c.arrow([6.0, 44.0], [6.0, 5.0], ACCENT);
            }
            Self::Orbit => {
                c.block();
                c.line(
                    &[
                        [6.0, 22.0],
                        [5.0, 12.0],
                        [15.0, 5.0],
                        [31.0, 5.0],
                        [42.0, 12.0],
                        [43.0, 24.0],
                    ],
                    blue,
                    2.1,
                );
                c.arrow([43.0, 19.0], [39.0, 29.0], blue);
            }
            Self::Turning => {
                c.rect([4.0, 8.0], [13.0, 38.0], steel);
                c.rect([13.0, 17.0], [40.0, 29.0], pale);
                c.circle([38.0, 23.0], 6.0, blue);
                c.poly(&[[25.0, 34.0], [34.0, 34.0], [34.0, 42.0]], ACCENT);
            }
            Self::Printer => {
                c.line(
                    &[[6.0, 42.0], [6.0, 5.0], [42.0, 5.0], [42.0, 42.0]],
                    pale,
                    2.4,
                );
                c.rect([3.0, 39.0], [45.0, 44.0], steel);
                c.rect([17.0, 28.0], [33.0, 38.0], blue);
                c.rect([20.0, 8.0], [28.0, 18.0], dark);
                c.poly(&[[20.0, 18.0], [28.0, 18.0], [24.0, 24.0]], RAPID);
            }
            Self::Laser | Self::Plasma => {
                c.block();
                c.rect([20.0, 3.0], [29.0, 12.0], pale);
                c.poly(&[[20.0, 12.0], [29.0, 12.0], [24.0, 18.0]], steel);
                let beam = if matches!(self, Self::Laser) {
                    Color32::from_rgb(250, 123, 105)
                } else {
                    RAPID
                };
                c.line(&[[24.0, 18.0], [24.0, 28.0]], beam, 2.5);
                for (a, b) in [([20.0, 25.0], [16.0, 29.0]), ([29.0, 25.0], [34.0, 29.0])] {
                    c.line(&[a, b], beam, 1.8);
                }
            }
            Self::Grinding => {
                c.rect([7.0, 35.0], [43.0, 41.0], steel);
                c.circle([24.0, 21.0], 14.0, blue);
                c.circle([24.0, 21.0], 5.0, dark);
                c.line(&[[34.0, 32.0], [41.0, 27.0]], RAPID, 1.8);
            }
            Self::Wire => {
                c.block();
                c.rect([19.0, 1.0], [29.0, 7.0], pale);
                c.rect([19.0, 40.0], [29.0, 46.0], pale);
                c.line(&[[24.0, 7.0], [24.0, 40.0]], RAPID, 2.2);
            }
            Self::Electrode => {
                c.block();
                c.rect([17.0, 3.0], [31.0, 17.0], RAPID);
                c.rect([20.0, 17.0], [28.0, 23.0], pale);
                c.line(
                    &[[18.0, 26.0], [23.0, 23.0], [26.0, 28.0], [31.0, 25.0]],
                    blue,
                    1.8,
                );
            }
        }
    }
}

struct Canvas<'a> {
    p: &'a egui::Painter,
    r: Rect,
    alpha: f32,
}
impl Canvas<'_> {
    fn pos(&self, p: [f32; 2]) -> Pos2 {
        self.r.min + Vec2::new(p[0] / 48.0 * self.r.width(), p[1] / 48.0 * self.r.height())
    }
    fn color(&self, c: Color32) -> Color32 {
        c.gamma_multiply(self.alpha)
    }
    fn poly(&self, pts: &[[f32; 2]], color: Color32) {
        self.p.add(egui::Shape::convex_polygon(
            pts.iter().map(|p| self.pos(*p)).collect(),
            self.color(color),
            Stroke::NONE,
        ));
    }
    fn line(&self, pts: &[[f32; 2]], color: Color32, width: f32) {
        self.p.add(egui::Shape::line(
            pts.iter().map(|p| self.pos(*p)).collect(),
            Stroke::new(width * self.r.width() / 48.0, self.color(color)),
        ));
    }
    fn rect(&self, a: [f32; 2], b: [f32; 2], color: Color32) {
        self.p.rect_filled(
            Rect::from_min_max(self.pos(a), self.pos(b)),
            1,
            self.color(color),
        );
    }
    fn circle(&self, p: [f32; 2], radius: f32, color: Color32) {
        self.p.circle_filled(
            self.pos(p),
            radius * self.r.width() / 48.0,
            self.color(color),
        );
    }
    fn text(&self, p: [f32; 2], text: &str, color: Color32, size: f32) {
        self.p.text(
            self.pos(p),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::monospace(size * self.r.width() / 48.0),
            self.color(color),
        );
    }
    fn arrow(&self, a: [f32; 2], b: [f32; 2], color: Color32) {
        let a = self.pos(a);
        let b = self.pos(b);
        self.p.arrow(
            a,
            b - a,
            Stroke::new(2.0_f32 * self.r.width() / 48.0, self.color(color)),
        );
    }
    fn block(&self) {
        self.poly(
            &[[6.0, 20.0], [24.0, 30.0], [24.0, 40.0], [6.0, 30.0]],
            Color32::from_rgb(78, 108, 132),
        );
        self.poly(
            &[[24.0, 30.0], [42.0, 20.0], [42.0, 30.0], [24.0, 40.0]],
            Color32::from_rgb(107, 144, 168),
        );
        self.poly(
            &[[6.0, 20.0], [24.0, 10.0], [42.0, 20.0], [24.0, 30.0]],
            Color32::from_rgb(164, 194, 211),
        );
    }
}

pub(super) fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    hint: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(58.0, 70.0), egui::Sense::click());
        let active = enabled && ui.is_enabled();
        let response = if active {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, active, label));
        if active && (response.hovered() || response.has_focus()) {
            ui.painter()
                .rect_filled(rect, 4, Color32::from_rgb(35, 54, 69));
            ui.painter().rect_stroke(
                rect,
                4,
                Stroke::new(1.0_f32, Color32::from_rgb(69, 122, 145)),
                egui::StrokeKind::Inside,
            );
        }
        icon.draw(
            ui.painter(),
            Rect::from_center_size(
                Pos2::new(rect.center().x, rect.top() + 22.0),
                Vec2::splat(36.0),
            ),
            active,
        );
        let caption = ui.painter().layout(
            label.to_owned(),
            egui::FontId::proportional(11.0),
            if active {
                Color32::from_rgb(206, 219, 229)
            } else {
                Color32::from_rgb(93, 107, 123)
            },
            56.0,
        );
        ui.painter().galley(
            Pos2::new(
                rect.center().x - caption.size().x / 2.0,
                rect.bottom() - 3.0 - caption.size().y,
            ),
            caption,
            Color32::WHITE,
        );
        response.on_hover_text(hint)
    })
    .inner
}
