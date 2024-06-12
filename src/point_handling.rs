use faer::{self, mat, solvers::SpSolver};
use ordered_float::OrderedFloat;
pub type UniquePointBuf = HashSet<PointCoords>;
use std::collections::HashSet;

#[derive(Debug)]
pub struct PointTransform {
    pub alpha: f32, // Cos theta
    pub beta: f32,  // Sin theta
    pub dx: f32,
    pub dy: f32,
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
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
    fn transform_inplace(&mut self, transform: &PointTransform);
}

pub struct RegressionLineSegment {
    pub slope: f32,
    pub intercept: f32,
    pub transformed_slope: f32,
    pub transformed_intercept: f32,
    pub points: UniquePointBuf,
    pub transformed_points: UniquePointBuf,
    pub leftmost_pt: PointCoords,
    pub rightmost_pt: PointCoords,
    pub draw_color: egui::Color32,
}

impl PointTransform {
    pub fn _new(alpha: f32, beta: f32, dx: f32, dy: f32) -> Self {
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

        PointTransform {
            alpha: x[(0, 0)],
            beta: x[(1, 0)],
            dx: x[(2, 0)],
            dy: x[(3, 0)],
        }
    }
}

fn random_rgb_color32() -> egui::Color32 {
    let r = rand::random::<u8>();
    let g = rand::random::<u8>();
    let b = rand::random::<u8>();
    egui::Color32::from_rgb(r, g, b)
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

impl From<&PointCoords> for egui::Pos2 {
    fn from(val: &PointCoords) -> Self {
        egui::Pos2::new(val.x.into_inner(), val.y.into_inner())
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

    pub fn new(points: &UniquePointBuf) -> Self {
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
            leftmost_pt: leftmost.clone(),
            rightmost_pt: rightmost.clone(),
            draw_color: random_rgb_color32(),
        }
    }
}
