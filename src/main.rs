#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use bounded_vec_deque::BoundedVecDeque;
use eframe::egui;
use egui::{ColorImage, InputState};

use point_handling::{
    PointCoords, PointCoordsStringy, PointTransform, ScreenLineSegment, Transformable,
    UniquePointBuf,
};

use xcap::Monitor;

static SCREENSHOT_TEXTURE: &str = "screenshot";
static LINE_THICKNESS: f32 = 3.0;
static POINT_RADIUS: f32 = 2.5;
static NUM_CALIBRATION_POINTS: usize = 2;

mod point_handling;
enum PointGatheringState {
    Normal,
    Measurement,
}

struct App {
    preferred_monitor: Monitor,
    screenshot_texture_handle: Option<egui::TextureHandle>,
    gathering_state: PointGatheringState,
    buffered_points: UniquePointBuf,
    measurement_buffer: BoundedVecDeque<PointCoords>,
    measurement_buffer_real_world: BoundedVecDeque<PointCoords>,
    measurement_buffer_rw_s: BoundedVecDeque<PointCoordsStringy>,
    regression_lines: Vec<ScreenLineSegment>,
    current_transform: PointTransform,
}

fn secondary_btn_click_pos(i: &InputState) -> Option<egui::Pos2> {
    if i.pointer.secondary_clicked() {
        return i.pointer.latest_pos();
    }
    None
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
            self.screenshot_texture_handle = Some(handle);
        }

        // unwrap is safe because we just set it if it was None
        ui.image(egui::load::SizedTexture::from_handle(
            self.screenshot_texture_handle.as_ref().unwrap(),
        ));
    }

    fn paint_buffered_points(&mut self, ui: &egui::Ui) {
        for point in self.get_buffer_iterator() {
            ui.painter()
                .add(egui::Shape::Circle(egui::epaint::CircleShape {
                    center: (*point).into(),
                    radius: POINT_RADIUS,
                    fill: egui::Color32::RED,
                    stroke: Default::default(),
                }));
        }
    }

    fn paint_line_segments(&mut self, ui: &egui::Ui, stroke: f32) {
        for line in &self.regression_lines {
            let start_y =
                line.regressor.slope * line.leftmost_pt.x.into_inner() + line.regressor.intercept;
            let end_y =
                line.regressor.slope * line.rightmost_pt.x.into_inner() + line.regressor.intercept;
            let start_pos = egui::Pos2::new(line.leftmost_pt.x.into_inner(), start_y);
            let end_pos = egui::Pos2::new(line.rightmost_pt.x.into_inner(), end_y);
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
        self.regression_lines.push(ScreenLineSegment::new_from_buf(
            self.buffered_points.clone(),
        ));
        self.buffered_points.clear();
    }

    fn transform_line_segments(&mut self) {
        self.regression_lines.iter_mut().for_each(|line| {
            line.regressor.transform_line(&self.current_transform);
        });
    }

    // returns a type-erased iterator over the points to show based on state
    fn get_buffer_iterator(&self) -> Box<dyn Iterator<Item = &PointCoords> + '_> {
        match self.gathering_state {
            PointGatheringState::Normal => Box::from(self.buffered_points.iter()),
            PointGatheringState::Measurement => Box::from(self.measurement_buffer.iter()),
        }
    }

    // pushes a point to the buffer based on the current state
    fn push_to_buffer(&mut self, point: PointCoords) {
        match self.gathering_state {
            PointGatheringState::Normal => {
                self.buffered_points.insert(point);
            }
            PointGatheringState::Measurement => {
                let _ = self.measurement_buffer.push_back(point);
            }
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
                    self.push_to_buffer(pos.into());
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
            for point in self.get_buffer_iterator() {
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
                        ui.add_enabled(
                            false,
                            egui::Button::new(" ".repeat(8)).fill(line.draw_color),
                        );
                        ui.label(line.transformed_line_equation());
                    });
                }
                let mut iter = keep.iter();
                self.regression_lines.retain(|_| *iter.next().unwrap());
            });

        egui::Window::new("Transform calibration")
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
                        let p1_screen = self.measurement_buffer[0];
                        let p2_screen = self.measurement_buffer[1];
                        let p1_rw = self.measurement_buffer_real_world[0];
                        let p2_rw = self.measurement_buffer_real_world[1];
                        self.current_transform = PointTransform::interpolate_from_point_pairs(
                            (p1_screen, p1_rw),
                            (p2_screen, p2_rw),
                        );
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
    eframe::run_native("My egui App", options, Box::new(|_c| Box::<App>::default())).unwrap();
}
