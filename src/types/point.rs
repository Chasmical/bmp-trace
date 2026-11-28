/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use std::ops::{Add, Div, Mul, Sub};

/// A generic 2D point.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T> Point<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl Point<isize> {
    pub fn signum(self) -> Self {
        Self { x: self.x.signum(), y: self.y.signum() }
    }
    /// Returns `p1 × p2`, i.e. `p1.x * p2.y - p1.y * p2.x`.
    pub fn xprod(self, p2: Self) -> isize {
        self.x * p2.y - self.y * p2.x
    }
    /// Converts this `Point<isize>` to `Point<f64>`.
    pub const fn to_f64(self) -> Point<f64> {
        Point { x: self.x as f64, y: self.y as f64 }
    }
}
impl Point<usize> {
    pub const fn offset_from(self, other: Self) -> Point<isize> {
        Point { x: self.x as isize - other.x as isize, y: self.y as isize - other.y as isize }
    }
    pub const fn to_f64(self) -> Point<f64> {
        Point { x: self.x as f64, y: self.y as f64 }
    }
}

impl Point<f64> {
    /// Linearly interpolates between two points.
    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self { x: self.x + t * (other.x - self.x), y: self.y + t * (other.y - self.y) }
    }
    /// Returns a direction that is 90 degrees counterclockwise from `p2-p0`,
    /// but then restricted to one of the major wind directions (N, NW, W, etc.).
    pub fn dorth_infty(self, p2: Self) -> Self {
        Self { y: (p2.x - self.x).signum(), x: -(p2.y - self.y).signum() }
    }
    /// Returns `(p1-p0)×(p2-p0)`, the area of a parallelogram.
    pub fn dpara(self, p1: Self, p2: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p2.x - self.x;
        let y2 = p2.y - self.y;
        x1 * y2 - x2 * y1
    }
    pub fn ddenom(self, p2: Self) -> f64 {
        let r = self.dorth_infty(p2);
        r.y * (p2.x - self.x) - r.x * (p2.y - self.y)
    }

    /// Returns `(p1-p0)×(p3-p2)`.
    pub fn cprod(self, p1: Self, p2: Self, p3: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p3.x - p2.x;
        let y2 = p3.y - p2.y;
        x1 * y2 - x2 * y1
    }
    /// Returns `(p1-p0)*(p2-p0)`.
    pub fn iprod(self, p1: Self, p2: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p2.x - self.x;
        let y2 = p2.y - self.y;
        x1 * x2 + y1 * y2
    }
    /// Returns `(p1-p0)*(p3-p2)`.
    pub fn iprod1(self, p1: Self, p2: Self, p3: Self) -> f64 {
        let x1 = p1.x - self.x;
        let y1 = p1.y - self.y;
        let x2 = p3.x - p2.x;
        let y2 = p3.y - p2.y;
        x1 * x2 + y1 * y2
    }
    /// Returns the distance between two points.
    pub fn distance(self, to: Self) -> f64 {
        // TODO: check that powi(2) decomposes into just (x*x)
        ((self.x - to.x).powi(2) + (self.y - to.y).powi(2)).sqrt()
    }

    /// Calculates the point of a bezier curve.
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
    pub fn tangent(self, p1: Self, p2: Self, p3: Self, q0: Self, q1: Self) -> Option<f64> {
        // (1-t)^2 a0 + 2(1-t)t b0 + t^2 c0 = 0
        let a0 = Self::cprod(self, p1, q0, q1);
        let b0 = Self::cprod(p1, p2, q0, q1);
        let c0 = Self::cprod(p2, p3, q0, q1);

        // a t^2 + b t + c = 0
        let a = a0 - 2.0 * b0 + c0;
        let b = -2.0 * a0 + 2.0 * b0;
        let c = a0;

        let d = b * b - 4.0 * a * c;

        if a == 0.0 || d < 0.0 {
            return None;
        }

        let s = d.sqrt();

        let r1 = (-b + s) / (2.0 * a);
        let r2 = (-b - s) / (2.0 * a);

        if matches!(r1, 0.0..=1.0) {
            Some(r1)
        } else if matches!(r2, 0.0..=1.0) {
            Some(r2)
        } else {
            None
        }
    }
}

impl<T: Add<Output = T>> Add for Point<T> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}
/// Note: [`Sub`] is only implemented for [`Point<f64>`] since [`f64`] doesn't overflow.
/// For [`Point<usize>`], [`Point::offset_from`] should be used instead.
impl Sub for Point<f64> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}
impl<T: Mul<Output = T>> Mul for Point<T> {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self { x: self.x * rhs.x, y: self.y * rhs.y }
    }
}
impl<T: Div<Output = T>> Div for Point<T> {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self { x: self.x / rhs.x, y: self.y / rhs.y }
    }
}

// Multiplication and division by a scalar
impl<T: Copy + Mul<Output = T>> Mul<T> for Point<T> {
    type Output = Self;
    fn mul(self, rhs: T) -> Self::Output {
        Self { x: self.x * rhs, y: self.y * rhs }
    }
}
impl<T: Copy + Div<Output = T>> Div<T> for Point<T> {
    type Output = Self;
    fn div(self, rhs: T) -> Self::Output {
        Self { x: self.x / rhs, y: self.y / rhs }
    }
}
