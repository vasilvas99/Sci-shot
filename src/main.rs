#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::{ColorImage, InputState};

use bounded_vec_deque::BoundedVecDeque;
use faer::solvers::SpSolver;
use ordered_float::OrderedFloat;
use std::collections::HashSet;
use std::{hash::Hash, rc::Rc};
use xcap::Monitor;
use faer::{self, mat};

static SCREENSHOT_TEXTURE: &str = "screenshot";
static LINE_THICKNESS: f32 = 3.0;
static POINT_RADIUS: f32 = 2.5;
static NUM_CALIBRATION_POINTS: usize = 2;

enum PointGatheringState {
    Normal,
    Measurement,
}

#[derive(Debug)]
struct PointTransform {
    alpha: f32, // Cos theta
    beta: f32,  // Sin theta
    dx: f32,
    dy: f32,
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
struct PointCoords {
    x: OrderedFloat<f32>,
    y: OrderedFloat<f32>,
}

#[derive(Clone, Debug)]
struct PointCoordsStringy {
    x: String,
    y: String,
}

type UniquePointBuf = HashSet<PointCoords>;

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
    start_x: PointCoords,
    end_x: PointCoords,
    draw_color: egui::Color32,
}

struct App {
    preferred_monitor: Monitor,
    screenshot_texture_handle: Option<Rc<egui::TextureHandle>>,
    gathering_state: PointGatheringState,
    buffered_points: UniquePointBuf,
    measurement_buffer: BoundedVecDeque<PointCoords>,
    measurement_buffer_real_world: BoundedVecDeque<PointCoords>,
    measurement_buffer_rw_s: BoundedVecDeque<PointCoordsStringy>,
    regression_lines: Vec<RegressionLineSegment>,
    current_transform: PointTransform,
}

fn secondary_btn_click_pos(i: &InputState) -> Option<egui::Pos2> {
    if i.pointer.secondary_clicked() {
        return i.pointer.latest_pos();
    }
    None
}

impl Transformable for PointCoords {
    fn transform(&self, transform: &PointTransform) -> Self {
        let x = transform.alpha * self.x.into_inner() - transform.beta * self.y.into_inner()
            + transform.dx;
        let y = transform.beta * self.x.into_inner()
            + transform.alpha * self.y.into_inner()
            + transform.dy;
        PointCoords {
            x: OrderedFloat(x),
            y: OrderedFloat(y),
        }
    }
    fn transform_inplace(&mut self, _transform: &PointTransform) {
        todo!("Implement in-place transformation for ScreenPoint")
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

impl PointCoords {
    fn new(x: f32, y: f32) -> Self {
        PointCoords {
            x: OrderedFloat(x),
            y: OrderedFloat(y),
        }
    }
}

impl PointCoordsStringy {
    fn new_numeric(x: f32, y: f32) -> Self {
        PointCoordsStringy {
            x: x.to_string(),
            y: y.to_string(),
        }
    }

    fn try_as_numeric(&self) -> Option<PointCoords> {
        let x = self.x.parse::<f32>().ok()?;
        let y = self.y.parse::<f32>().ok()?;
        Some(PointCoords::new(x, y))
    }
}

impl From<&PointCoords> for egui::Pos2 {
    fn from(val: &PointCoords) -> Self {
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
            measurement_buffer: BoundedVecDeque::new(NUM_CALIBRATION_POINTS),
            measurement_buffer_real_world: BoundedVecDeque::from_iter(
                std::iter::repeat(PointCoords::new(0.0, 0.0)),
                NUM_CALIBRATION_POINTS,
            ),
            measurement_buffer_rw_s: BoundedVecDeque::from_iter(
                std::iter::repeat(PointCoordsStringy::new_numeric(0.0, 0.0)),
                NUM_CALIBRATION_POINTS,
            ),
            regression_lines: Vec::new(),
            current_transform: PointTransform {
                alpha: 1.0,
                beta: 0.0,
                dx: 0.0,
                dy: 0.0,
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
        let points_to_paint: Box<dyn Iterator<Item = _>> = match self.gathering_state {
            PointGatheringState::Normal => Box::from(self.buffered_points.iter()),
            PointGatheringState::Measurement => Box::from(self.measurement_buffer.iter()),
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
                            self.buffered_points.insert(PointCoords::new(pos.x, pos.y));
                        }
                        PointGatheringState::Measurement => {
                            let _ = self
                                .measurement_buffer
                                .push_back(PointCoords::new(pos.x, pos.y));
                        }
                    }
                }
                self.paint_buffered_points(ui);

                // if l is pressed calculate regression line and clear the points buffer
                if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                    self.process_points_buffer();
                }

                self.transform_line_segments();

                // paint line segments
                self.paint_line_segments(ui, LINE_THICKNESS);

                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    std::process::exit(0);
                }
            });

        egui::Window::new("Buffered points").show(ctx, |ui| {
            ui.label("Buffered points:");
            let points_to_show: Box<dyn Iterator<Item = _>> = match self.gathering_state {
                PointGatheringState::Normal => Box::from(self.buffered_points.iter()),
                PointGatheringState::Measurement => Box::from(self.measurement_buffer.iter()),
            };
            for point in points_to_show {
                let point_rw = point.transform(&self.current_transform);
                ui.label(format!("({}, {})", point_rw.x, point_rw.y));
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
        egui::Window::new("Calibrate Transform")
            .default_pos(egui::pos2(0.0, 500.0))
            .default_open(false)
            .show(ctx, |ui| {
                ui.label("Measure two points on the screen to calibrate the transform");
                ui.horizontal(|ui| {
                    if ui.button("Go to calibration mode").clicked() {
                        self.gathering_state = PointGatheringState::Measurement;
                    }
                    if ui.button("Calibrate").clicked() {
                        for i in 0..self.measurement_buffer.len() {
                            // better crash on bad input than silently ignore it
                            let point = self.measurement_buffer_rw_s[i].try_as_numeric().unwrap();
                            self.measurement_buffer_real_world[i] = point;
                        }
                        let p1_screen = self.measurement_buffer[0].clone();
                        let p2_screen = self.measurement_buffer[1].clone();
                        let p1_rw = self.measurement_buffer_real_world[0].clone();
                        let p2_rw = self.measurement_buffer_real_world[1].clone();
                        todo!("Revise transform calculation!");
                        let mtx = mat![
                            [p1_screen.x.into_inner(), -p1_screen.y.into_inner(), 1.0, 0.0],
                            [p1_screen.y.into_inner(), p1_screen.x.into_inner(), 0.0, 1.0],
                            [p2_screen.x.into_inner(), -p2_screen.y.into_inner(), 1.0, 0.0],
                            [p2_screen.y.into_inner(), p2_screen.x.into_inner(), 0.0, 1.0],
                        ];
                        let rhs = mat![[p1_rw.x.into_inner(), p1_rw.y.into_inner(), p2_rw.x.into_inner(), p2_rw.y.into_inner()]];
                        let lu = mtx.full_piv_lu();
                        let x = lu.solve(rhs.transpose());
                        self.current_transform = PointTransform {
                            alpha: x[(0, 0)],
                            beta: x[(1, 0)],
                            dx: x[(2, 0)],
                            dy: x[(3, 0)],
                        };
                        println!("Transform: {:?}", self.current_transform);
                        self.gathering_state = PointGatheringState::Normal;

                    
                    }
                });
                for i in 0..self.measurement_buffer.len() {
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label(format!("x: {}", self.measurement_buffer[i].x));
                        ui.label(format!("y: {}", self.measurement_buffer[i].y));
                        ui.text_edit_singleline(&mut self.measurement_buffer_rw_s[i].x);
                        ui.text_edit_singleline(&mut self.measurement_buffer_rw_s[i].y);
                    });
                }
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
