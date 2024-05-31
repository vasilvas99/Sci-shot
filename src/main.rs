#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::{ColorImage, InputState};

use ordered_float::OrderedFloat;
use std::collections::HashSet;
use std::{hash::Hash, rc::Rc};
use xcap::Monitor;

static SCREENSHOT_TEXTURE: &str = "screenshot";
static LINE_THICKNESS: f32 = 3.0;
static POINT_RADIUS: f32 = 2.5;

enum PointGatheringState {
    Normal,
    Measurement,
}

struct PointTransform {
    rotation_angle: f32,
    scale: f32,
    translation: [f32; 2],
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
struct ScreenPoint {
    x: OrderedFloat<f32>,
    y: OrderedFloat<f32>,
}

type UniquePointBuf = HashSet<ScreenPoint>;

trait Transformable {
    fn transform(&self, transform: &PointTransform) -> Self;
    fn transform_inplace(&mut self, transform: &PointTransform);
}

struct RegressionLineSegment {
    slope: f32,
    intercept: f32,
    transformed_slope: f32,
    transformed_intercept: f32,
    points: UniquePointBuf,
    transformed_points: UniquePointBuf,
    start_x: ScreenPoint,
    end_x: ScreenPoint,
    draw_color: egui::Color32,
}

struct App {
    preferred_monitor: Monitor,
    screenshot_texture_handle: Option<Rc<egui::TextureHandle>>,
    gathering_state: PointGatheringState,
    buffered_points: UniquePointBuf,
    measurement_buffer: UniquePointBuf,
    regression_lines: Vec<RegressionLineSegment>,
    current_transform: PointTransform,
}

fn secondary_btn_click_pos(i: &InputState) -> Option<egui::Pos2> {
    if i.pointer.secondary_clicked() {
        return i.pointer.latest_pos();
    }
    None
}

impl Transformable for ScreenPoint {
    fn transform(&self, transform: &PointTransform) -> Self {
        let x = self.x.into_inner();
        let y = self.y.into_inner();
        let x = x * transform.scale;
        let y = y * transform.scale;
        let x = x * transform.rotation_angle.cos() - y * transform.rotation_angle.sin();
        let y = x * transform.rotation_angle.sin() + y * transform.rotation_angle.cos();
        let x = x + transform.translation[0];
        let y = y + transform.translation[1];
        ScreenPoint::new(x, y)
    }
    fn transform_inplace(&mut self, transform: &PointTransform) {
        self.x = OrderedFloat(
            self.x.into_inner() * transform.scale * transform.rotation_angle.cos()
                - self.y.into_inner() * transform.rotation_angle.sin(),
        );
        self.y = OrderedFloat(
            self.x.into_inner() * transform.rotation_angle.sin()
                + self.y.into_inner() * transform.rotation_angle.cos(),
        );
        self.x = OrderedFloat(self.x.into_inner() + transform.translation[0]);
        self.y = OrderedFloat(self.y.into_inner() + transform.translation[1]);
    }
}

impl Transformable for UniquePointBuf {
    fn transform(&self, transform: &PointTransform) -> Self {
        self.iter()
            .map(|p| p.transform(transform))
            .collect::<UniquePointBuf>()
    }
    fn transform_inplace(&mut self, _transform: &PointTransform) {
        todo!("Implement in-place transformation for UniquePointBuf");
    }
}

fn random_rgb_color32() -> egui::Color32 {
    let r = rand::random::<u8>();
    let g = rand::random::<u8>();
    let b = rand::random::<u8>();
    egui::Color32::from_rgb(r, g, b)
}

impl ScreenPoint {
    fn new(x: f32, y: f32) -> Self {
        ScreenPoint {
            x: OrderedFloat(x),
            y: OrderedFloat(y),
        }
    }
}

impl From<&ScreenPoint> for egui::Pos2 {
    fn from(val: &ScreenPoint) -> Self {
        egui::Pos2::new(val.x.into_inner(), val.y.into_inner())
    }
}

impl RegressionLineSegment {
    fn get_regression_line(points: &UniquePointBuf) -> (f32, f32) {
        let n = points.len() as f32;
        let sum_x = points.iter().map(|p| p.x.into_inner()).sum::<f32>();
        let sum_y = points.iter().map(|p| p.y.into_inner()).sum::<f32>();
        let sum_x_squared = points
            .iter()
            .map(|p| p.x.into_inner() * p.x.into_inner())
            .sum::<f32>();
        let sum_xy = points
            .iter()
            .map(|p| p.x.into_inner() * p.y.into_inner())
            .sum::<f32>();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x_squared - sum_x * sum_x);
        let intercept = (sum_y - slope * sum_x) / n;
        (slope, intercept)
    }

    fn new(points: &UniquePointBuf) -> Self {
        let (slope, intercept) = RegressionLineSegment::get_regression_line(points);

        let leftmost = points.iter().min_by_key(|p| p.x).unwrap();
        let rightmost = points.iter().max_by_key(|p| p.x).unwrap();
        RegressionLineSegment {
            slope,
            intercept,
            transformed_slope: slope,
            transformed_intercept: intercept,
            points: points.clone(),
            transformed_points: points.clone(),
            start_x: leftmost.clone(),
            end_x: rightmost.clone(),
            draw_color: random_rgb_color32(),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let primary = Monitor::all()
            .unwrap()
            .into_iter()
            .find(|m| m.is_primary())
            .unwrap();

        App {
            preferred_monitor: primary,
            screenshot_texture_handle: None,
            gathering_state: PointGatheringState::Normal,
            buffered_points: UniquePointBuf::new(),
            measurement_buffer: UniquePointBuf::new(),
            regression_lines: Vec::new(),
            current_transform: PointTransform {
                rotation_angle: 0.0,
                scale: 1.0,
                translation: [0.0, 0.0],
            },
        }
    }
}

impl App {
    fn screenshot_from_preferred(&self) -> ColorImage {
        let screenshot: image::RgbaImage = self.preferred_monitor.capture_image().unwrap();
        let pixels = screenshot.as_flat_samples();
        let size = [screenshot.width() as _, screenshot.height() as _]; // needed to match usize
        ColorImage::from_rgba_unmultiplied(size, pixels.as_slice())
    }

    fn draw_screenshot_layer(&mut self, ui: &mut egui::Ui) {
        if self.screenshot_texture_handle.is_none() {
            let handle = ui.ctx().load_texture(
                SCREENSHOT_TEXTURE,
                self.screenshot_from_preferred(),
                Default::default(),
            );
            self.screenshot_texture_handle = Some(Rc::from(handle));
        }

        // unwrap is safe because we just set it if it was None
        let h = self.screenshot_texture_handle.as_ref().unwrap().clone();
        ui.image(egui::load::SizedTexture::from_handle(&h));
    }

    fn paint_buffered_points(&mut self, ui: &mut egui::Ui) {
        let points_to_paint = match self.gathering_state {
            PointGatheringState::Normal => &self.buffered_points,
            PointGatheringState::Measurement => &self.measurement_buffer,
        };
        for point in points_to_paint {
            ui.painter()
                .add(egui::Shape::Circle(egui::epaint::CircleShape {
                    center: point.into(),
                    radius: POINT_RADIUS,
                    fill: egui::Color32::RED,
                    stroke: Default::default(),
                }));
        }
    }

    fn paint_line_segments(&mut self, ui: &mut egui::Ui, stroke: f32) {
        for line in &self.regression_lines {
            let start_y = line.slope * line.start_x.x.into_inner() + line.intercept;
            let end_y = line.slope * line.end_x.x.into_inner() + line.intercept;
            let start_pos = egui::Pos2::new(line.start_x.x.into_inner(), start_y);
            let end_pos = egui::Pos2::new(line.end_x.x.into_inner(), end_y);
            let points = [start_pos, end_pos];
            ui.painter().add(egui::Shape::line_segment(
                points,
                egui::Stroke::new(stroke, line.draw_color),
            ));
        }
    }

    fn process_points_buffer(&mut self) {
        if self.buffered_points.len() < 2 {
            return;
        }
        self.regression_lines
            .push(RegressionLineSegment::new(&self.buffered_points));
        self.buffered_points.clear();
    }

    fn transform_line_segments(&mut self) {
        for line in &mut self.regression_lines {
            line.transformed_points = line.points.transform(&self.current_transform);
            let (new_slope, new_intercept) =
                RegressionLineSegment::get_regression_line(&line.transformed_points);
            line.transformed_slope = new_slope;
            line.transformed_intercept = new_intercept;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.draw_screenshot_layer(ui);
                if let Some(pos) = ui.input(secondary_btn_click_pos) {
                    match self.gathering_state {
                        PointGatheringState::Normal => {
                            self.buffered_points.insert(ScreenPoint::new(pos.x, pos.y));
                        }
                        PointGatheringState::Measurement => {
                            self.measurement_buffer
                                .insert(ScreenPoint::new(pos.x, pos.y));
                        }
                    }
                }
                self.paint_buffered_points(ui);

                // if l is pressed calculate regression line and clear the points buffer
                if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                    self.process_points_buffer();
                }

                // apply current transform to line segments (Slow and in the hot loop)
                self.transform_line_segments();

                // paint line segments
                self.paint_line_segments(ui, LINE_THICKNESS);

                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    std::process::exit(0);
                }
            });

        egui::Window::new("Buffered points").show(ctx, |ui| {
            ui.label("Buffered points:");
            let points_to_show = match self.gathering_state {
                PointGatheringState::Normal => &self.buffered_points,
                PointGatheringState::Measurement => &self.measurement_buffer,
            };
            for point in points_to_show {
                ui.label(format!("({}, {})", point.x, point.y));
            }
        });

        egui::Window::new("Line equations")
            .default_pos(egui::pos2(500.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Line equations:");
                let mut keep = vec![true; self.regression_lines.len()];
                for (idx, line) in self.regression_lines.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.button("❌").clicked() {
                            keep[idx] = false;
                        }
                        ui.add_enabled(false, egui::Button::new("        ").fill(line.draw_color));
                        ui.label(format!(
                            "y = {:.3}x + {:.3}",
                            line.transformed_slope, line.transformed_intercept
                        ));
                    });
                }
                let mut iter = keep.iter();
                self.regression_lines.retain(|_| *iter.next().unwrap());
            });

        egui::Window::new("Configure Transform")
            .default_pos(egui::pos2(0.0, 500.0))
            .show(ctx, |ui| {
                ui.label("Configure Transform:");
                ui.horizontal(|ui| {
                    ui.label("Rotation Angle:");
                    ui.add(
                        egui::Slider::new(
                            &mut self.current_transform.rotation_angle,
                            -std::f32::consts::PI..=std::f32::consts::PI,
                        )
                        .text("Rotation Angle")
                        .clamp_to_range(true),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    ui.add(
                        egui::Slider::new(&mut self.current_transform.scale, 0.0..=10.0)
                            .text("Scale")
                            .clamp_to_range(true),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Translation:");
                    ui.add(
                        egui::Slider::new(
                            &mut self.current_transform.translation[0],
                            -1000.0..=1000.0,
                        )
                        .text("X")
                        .clamp_to_range(true),
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut self.current_transform.translation[1],
                            -1000.0..=1000.0,
                        )
                        .text("Y")
                        .clamp_to_range(true),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button("Measure Transform").clicked() {
                        self.gathering_state = PointGatheringState::Measurement;
                    }
                });
            });
    }
}

fn main() {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_fullscreen(true),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Box::<App>::default()
        }),
    )
    .unwrap();
}
