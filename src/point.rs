/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

/// A 2D [`isize`] point.
///
/// Corresponds to `point_t` in Potrace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: isize,
    pub y: isize,
}

/// A 2D [`f64`] point.
///
/// Corresponds to `dpoint_t` in Potrace.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct DPoint {
    pub x: f64,
    pub y: f64,
}

impl From<Point> for DPoint {
    /// Converts [`Point`] to [`DPoint`].
    ///
    /// Corresponds to `dpoint_t dpoint(point_t)` in Potrace.
    // CHANGE: Implemented the `dpoint` function as a From<> impl.
    #[inline]
    fn from(value: Point) -> Self {
        Self { x: value.x as f64, y: value.y as f64 }
    }
}
impl From<DPoint> for Point {
    fn from(value: DPoint) -> Self {
        Self { x: value.x as isize, y: value.y as isize }
    }
}

impl Point {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    /// Returns `p1 × p2`.
    ///
    /// Corresponds to `int xprod(point_t, point_t)` in Potrace.
    pub const fn xprod(self, p2: Self) -> isize {
        self.x * p2.y - self.y * p2.x
    }
}

impl DPoint {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Range over the straight line segment `a..=b` when lambda ranges over `0..=1`.
    ///
    /// Corresponds to `dpoint_t interval(double, dpoint_t, dpoint_t)` in Potrace.
    // CHANGE: Renamed interval to lerp.
    #[inline]
    pub fn lerp(self, to: Self, lambda: f64) -> Self {
        Self { x: self.x + lambda * (to.x - self.x), y: self.y + lambda * (to.y - self.y) }
    }
    /// Returns a direction that is 90 degrees counterclockwise from `p2-p0`,
    /// but then restricted to one of the major wind directions (N, NW, W, etc.).
    ///
    /// Corresponds to `point_t dorth_infty(dpoint_t, dpoint_t)` in Potrace.
    // CHANGE: dorth_infty returns DPoint instead of Point to avoid f64->isize->f64 conversions.
    #[inline]
    pub fn dorth_infty(self, p2: Self) -> Self {
        Self { y: (p2.x - self.x).signum(), x: (p2.y - self.y).signum() }
    }
    /// Returns `(p1-p0)×(p2-p0)`, the area of a parallelogram.
    ///
    /// Corresponds to `double dpara(dpoint_t, dpoint_t, dpoint_t)` in Potrace.
    #[inline]
    pub fn dpara(self, p1: Self, p2: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p2.x - self.x;
        let y2 = p2.y - self.y;
        x1 * y2 - x2 * y1
    }
    /// Corresponds to `double ddenom(dpoint_t, dpoint_t)` in Potrace.
    #[inline]
    pub fn ddenom(self, p2: Self) -> f64 {
        let r = self.dorth_infty(p2);
        r.y * (p2.x - self.x) - r.x * (p2.y - self.y)
    }

    /// Returns `(p1-p0)×(p3-p2)`.
    ///
    /// Corresponds to `double cprod(dpoint_t, dpoint_t, dpoint_t, dpoint_t)` in Potrace.
    #[inline]
    pub fn cprod(self, p1: Self, p2: Self, p3: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p3.x - p2.x;
        let y2 = p3.y - p2.y;
        x1 * y2 - x2 * y1
    }
    /// Returns `(p1-p0)*(p2-p0)`.
    ///
    /// Corresponds to `double cprod(dpoint_t, dpoint_t, dpoint_t)` in Potrace.
    #[inline]
    pub fn iprod(self, p1: Self, p2: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p2.x - self.x;
        let y2 = p2.y - self.y;
        x1 * x2 + y1 * y2
    }
    /// Returns `(p1-p0)*(p3-p2)`.
    ///
    /// Corresponds to `double cprod(dpoint_t, dpoint_t, dpoint_t, dpoint_t)` in Potrace.
    #[inline]
    pub fn iprod1(self, p1: Self, p2: Self, p3: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p3.x - p2.x;
        let y2 = p3.y - p2.y;
        x1 * x2 + y1 * y2
    }
    /// Returns the distance between two points.
    ///
    /// Corresponds to `double ddist(dpoint_t, dpoint_t)` in Potrace.
    // CHANGE: Renamed ddist to distance.
    #[inline]
    pub fn distance(self, to: Self) -> f64 {
        // TODO: check that powi(2) decomposes into just (x*x)
        ((self.x - to.x).powi(2) + (self.y - to.y).powi(2)).sqrt()
    }

    /// Calculates the point of a bezier curve.
    ///
    /// Corresponds to `dpoint_t bezier(double, dpoint_t, dpoint_t, dpoint_t, dpoint_t)` in Potrace.
    #[inline]
    pub fn bezier(t: f64, p0: Self, p1: Self, p2: Self, p3: Self) -> Self {
        let s = 1.0 - t;

        // Note: a good optimizing compiler (such as gcc-3) reduces the following
        // to 16 multiplications, using common subexpression elimination.

        Self {
            x: s * s * s * p0.x
                + 3.0 * (s * s * t) * p1.x
                + 3.0 * (t * t * s) * p2.x
                + t * t * t * p3.x,
            y: s * s * s * p0.y
                + 3.0 * (s * s * t) * p1.y
                + 3.0 * (t * t * s) * p2.y
                + t * t * t * p3.y,
        }
    }

    /// Calculates the point `t` in `0..=1` on the (convex) bezier curve `(p0,p1,p2,p3)`
    /// which is tangent to `q1-q0`. Returns `None` if there is no solution in `0..=1`.
    ///
    /// Corresponds to `double tangent(dpoint_t, dpoint_t, dpoint_t, dpoint_t, dpoint_t, dpoint_t)`
    /// in Potrace.
    pub fn tangent(self, p1: Self, p2: Self, p3: Self, q0: Self, q1: Self) -> f64 {
        /* (1-t)^2 a0 + 2(1-t)t b0 + t^2 c0 = 0 */
        let a0 = Self::cprod(self, p1, q0, q1);
        let b0 = Self::cprod(p1, p2, q0, q1);
        let c0 = Self::cprod(p2, p3, q0, q1);

        /* a t^2 + b t + c = 0 */
        let a = a0 - 2.0 * b0 + c0;
        let b = -2.0 * a0 + 2.0 * b0;
        let c = a0;

        let d = b * b - 4.0 * a * c;

        if a == 0.0 || d < 0.0 {
            return -1.0;
        }

        let s = d.sqrt();

        let r1 = (-b + s) / (2.0 * a);
        let r2 = (-b - s) / (2.0 * a);

        if matches!(r1, 0.0..=1.0) {
            r1
        } else if matches!(r2, 0.0..=1.0) {
            r2
        } else {
            -1.0
        }
    }
}
