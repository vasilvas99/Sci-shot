use faer::{self, mat, solvers::SpSolver};
use ordered_float::OrderedFloat;
pub type UniquePointBuf = HashSet<PointCoords>;
use std::{collections::HashSet, rc::Rc};

#[derive(Debug)]
pub struct PointTransform {
    pub alpha: f32, // Cos theta
    pub beta: f32,  // Sin theta
    pub dx: f32,
    pub dy: f32,
}

#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
pub struct PointCoords {
    pub x: OrderedFloat<f32>,
    pub y: OrderedFloat<f32>,
}

#[derive(Clone, Debug)]
pub struct PointCoordsStringy {
    pub x: String,
    pub y: String,
}

pub trait Transformable {
    fn transform(&self, transform: &PointTransform) -> Self;
}

pub struct RegressionLineSegment {
    pub slope: f32,
    pub intercept: f32,
    pub transformed_slope: f32,
    pub transformed_intercept: f32,
    pub points: Rc<UniquePointBuf>,
    pub transformed_points: Rc<UniquePointBuf>,
}

pub struct ScreenLineSegment {
    pub regressor: RegressionLineSegment,
    pub rightmost_pt: PointCoords,
    pub leftmost_pt: PointCoords,
    pub draw_color: RGBColor,
}

#[derive(Copy, Clone, Debug)]
pub struct RGBColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl PointTransform {
    pub fn new(alpha: f32, beta: f32, dx: f32, dy: f32) -> Self {
        PointTransform {
            alpha,
            beta,
            dx,
            dy,
        }
    }
    pub fn interpolate_from_point_pairs(
        (p1_screen, p1_rw): (PointCoords, PointCoords),
        (p2_screen, p2_rw): (PointCoords, PointCoords),
    ) -> Self {
        let mtx = mat![
            [p1_screen.x.into_inner(), p1_screen.y.into_inner(), 1.0, 0.0],
            [
                -p1_screen.y.into_inner(),
                p1_screen.x.into_inner(),
                0.0,
                1.0
            ],
            [p2_screen.x.into_inner(), p2_screen.y.into_inner(), 1.0, 0.0],
            [
                -p2_screen.y.into_inner(),
                p2_screen.x.into_inner(),
                0.0,
                1.0
            ],
        ];
        let rhs = mat![[
            p1_rw.x.into_inner(),
            p1_rw.y.into_inner(),
            p2_rw.x.into_inner(),
            p2_rw.y.into_inner()
        ]];
        let lu = mtx.full_piv_lu();
        let x = lu.solve(rhs.transpose());

        PointTransform::new(x[(0, 0)], x[(1, 0)], x[(2, 0)], x[(3, 0)])
    }
}

impl Transformable for PointCoords {
    fn transform(&self, transform: &PointTransform) -> Self {
        let m = mat![
            [transform.alpha, -transform.beta],
            [transform.beta, transform.alpha],
        ];
        let t = mat![[transform.dx, transform.dy]];
        let p = mat![[self.x.into_inner(), -self.y.into_inner()]];
        let p_transformed = m * p.transpose() + t.transpose();
        PointCoords {
            x: OrderedFloat(p_transformed[(0, 0)]),
            y: OrderedFloat(p_transformed[(1, 0)]),
        }
    }
}

impl Transformable for UniquePointBuf {
    fn transform(&self, transform: &PointTransform) -> Self {
        self.iter()
            .map(|p| p.transform(transform))
            .collect::<UniquePointBuf>()
    }
}

impl PointCoords {
    pub fn new(x: f32, y: f32) -> Self {
        PointCoords {
            x: OrderedFloat(x),
            y: OrderedFloat(y),
        }
    }
}

impl PointCoordsStringy {
    pub fn new_numeric(x: f32, y: f32) -> Self {
        PointCoordsStringy {
            x: x.to_string(),
            y: y.to_string(),
        }
    }

    pub fn try_as_numeric(&self) -> Option<PointCoords> {
        let x = self.x.parse::<f32>().ok()?;
        let y = self.y.parse::<f32>().ok()?;
        Some(PointCoords::new(x, y))
    }
}

impl RGBColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        RGBColor { r, g, b }
    }
    pub fn random_color() -> Self {
        Self::new(
            rand::random::<u8>(),
            rand::random::<u8>(),
            rand::random::<u8>(),
        )
    }
}

impl From<PointCoords> for egui::Pos2 {
    fn from(val: PointCoords) -> Self {
        egui::Pos2::new(val.x.into_inner(), val.y.into_inner())
    }
}

impl From<egui::Pos2> for PointCoords {
    fn from(val: egui::Pos2) -> Self {
        PointCoords::new(val.x, val.y)
    }
}

impl From<egui::Color32> for RGBColor {
    fn from(val: egui::Color32) -> Self {
        RGBColor::new(val.r(), val.g(), val.b())
    }
}

impl From<RGBColor> for egui::Color32 {
    fn from(val: RGBColor) -> Self {
        egui::Color32::from_rgb(val.r, val.g, val.b)
    }
}

impl RegressionLineSegment {
    pub fn get_regression_line(points: &UniquePointBuf) -> (f32, f32) {
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

    pub fn new(points: UniquePointBuf) -> Self {
        let points_ref = Rc::from(points);
        let (slope, intercept) =
            RegressionLineSegment::get_regression_line(points_ref.clone().as_ref());
        RegressionLineSegment {
            slope,
            intercept,
            transformed_slope: slope,
            transformed_intercept: intercept,
            points: points_ref.clone(),
            transformed_points: points_ref.clone(),
        }
    }
}

impl ScreenLineSegment {
    pub fn new_from_buf(raw_point_buffer: UniquePointBuf) -> Self {
        let rightmost = *raw_point_buffer.iter().max_by_key(|p| p.x).unwrap();
        let leftmost = *raw_point_buffer.iter().min_by_key(|p| p.x).unwrap();
        let line = RegressionLineSegment::new(raw_point_buffer);
        ScreenLineSegment {
            regressor: line,
            rightmost_pt: rightmost,
            leftmost_pt: leftmost,
            draw_color: RGBColor::random_color(),
        }
    }
}
