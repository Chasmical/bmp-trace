/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::types::{PathTree, Point, SegmentTag};

#[derive(Debug, Clone)]
pub struct Transform {
    pub bb: [f64; 2],
    pub orig: [f64; 2],
    pub x: [f64; 2],
    pub y: [f64; 2],
    pub scale: Point<f64>,
}

impl Transform {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            bb: [width, height],
            orig: [0.0, 0.0],
            x: [1.0, 0.0],
            y: [0.0, 1.0],
            scale: Point { x: 1.0, y: 1.0 },
        }
    }

    pub fn apply(&self, p: Point<f64>) -> Point<f64> {
        Point {
            x: self.orig[0] + p.x * self.x[0] + p.y * self.y[0],
            y: self.orig[1] + p.x * self.x[1] + p.y * self.y[1],
        }
    }

    pub fn rescale(&mut self, scale: f64) {
        self.bb[0] *= scale;
        self.bb[1] *= scale;
        self.orig[0] *= scale;
        self.orig[1] *= scale;
        self.x[0] *= scale;
        self.x[1] *= scale;
        self.y[0] *= scale;
        self.y[1] *= scale;
        self.scale.x *= scale;
        self.scale.y *= scale;
    }
    pub fn rescale_to_size(&mut self, width: f64, height: f64) {
        let xsc = width / self.bb[0];
        let ysc = height / self.bb[1];

        self.bb[0] = width;
        self.bb[1] = height;
        self.orig[0] *= xsc;
        self.orig[1] *= ysc;
        self.x[0] *= xsc;
        self.x[1] *= ysc;
        self.y[0] *= xsc;
        self.y[1] *= ysc;
        self.scale.x *= xsc;
        self.scale.y *= ysc;

        if width < 0.0 {
            self.orig[0] -= width;
            self.bb[0] = -width;
        }
        if height < 0.0 {
            self.orig[1] -= height;
            self.bb[1] = -height;
        }
    }

    pub fn rotate_degrees(&mut self, degrees: f64) {
        self.rotate_radians(degrees.to_radians());
    }
    pub fn rotate_radians(&mut self, radians: f64) {
        let s = radians.sin();
        let c = radians.cos();

        // Apply the transformation matrix to the sides of the bounding box
        let x0 = c * self.bb[0];
        let x1 = s * self.bb[0];
        let y0 = -s * self.bb[1];
        let y1 = c * self.bb[1];

        // Determine new bounding box, and origin of old bb within new bb
        self.bb[0] = x0.abs() + y0.abs();
        self.bb[1] = x1.abs() + y1.abs();
        let o0 = -x0.min(0.0) - y0.min(0.0);
        let o1 = -x1.min(0.0) - y1.min(0.0);

        self.orig =
            [o0 + c * self.orig[0] - s * self.orig[1], o1 + s * self.orig[0] + c * self.orig[1]];
        self.x = [c * self.x[0] - s * self.x[1], s * self.x[0] + c * self.x[1]];
        self.y = [c * self.y[0] - s * self.y[1], s * self.y[0] + c * self.y[1]];
    }

    pub fn tighten(&mut self, tree: &PathTree) {
        for j in 0..2 {
            let dir = Point::new(self.x[j], self.y[j]);

            let mut interval = path_limits(tree, dir);
            if interval.0 == interval.1 {
                // Make the extent non-zero to avoid later division by zero errors
                interval = (interval.0 - 0.5, interval.0 + 0.5);
            }
            self.bb[j] = interval.1 - interval.0;
            self.orig[j] = -interval.0;
        }
    }
}

fn iprod(a: Point<f64>, b: Point<f64>) -> f64 {
    a.x * b.x + a.y * b.y
}

fn path_limits(tree: &PathTree, dir: Point<f64>) -> (f64, f64) {
    let i = iprod(tree.path.curve.segments[0].control_points[2], dir);
    let mut interval = (i, i);

    for p in tree.iter() {
        let mut prev = p.path.curve.segments.last().unwrap().control_points[2];

        for segment in &p.path.curve.segments {
            segment_limits(&mut interval, segment.tag, prev, segment.control_points, dir);
            prev = segment.control_points[2];
        }
    }

    interval
}

#[inline]
fn extend(interval: &mut (f64, f64), val: f64) {
    interval.0 = interval.0.min(val);
    interval.1 = interval.1.max(val);
}

/// Extend the interval i to include the inner product <v | dir> for all points v on the segment.
fn segment_limits(
    interval: &mut (f64, f64),
    tag: SegmentTag,
    prev: Point<f64>,
    cur: [Point<f64>; 3],
    dir: Point<f64>,
) {
    match tag {
        SegmentTag::Corner => {
            extend(interval, iprod(cur[1], dir));
            extend(interval, iprod(cur[2], dir));
        },
        SegmentTag::CurveTo => {
            bezier_limits(
                interval,
                iprod(prev, dir),
                iprod(cur[0], dir),
                iprod(cur[1], dir),
                iprod(cur[2], dir),
            );
        },
    }
}

/// Extend the interval i to include the minimum and maximum of a 1-dimensional Bezier segment
/// given by control points x0..x3. For efficiency, x0 in i is assumed as a precondition.
fn bezier_limits(interval: &mut (f64, f64), x0: f64, x1: f64, x2: f64, x3: f64) {
    // The min and max of a cubic curve segment are attained at one of at most 4 critical points:
    // the 2 endpoints and at most 2 local extrema. We don't check the first endpoint, because all
    // our curves are cyclic so it's more efficient not to check endpoints twice.

    // Endpoint
    extend(interval, x3);

    // Optimization: don't bother calculating extrema if all control points are already in i
    if x1 >= interval.0 && x1 <= interval.1 && x2 >= interval.0 && x2 <= interval.1 {
        return;
    }

    // Solve for extrema. at^2 + bt + c = 0
    let a = -3.0 * x0 + 9.0 * x1 - 9.0 * x2 + 3.0 * x3;
    let b = 6.0 * x0 - 12.0 * x1 + 6.0 * x2;
    let c = -3.0 * x0 + 3.0 * x1;
    let d = b * b - 4.0 * a * c;
    if d > 0.0 {
        let r = d.sqrt();
        let t = (-b - r) / (2.0 * a);
        if t > 0.0 && t < 1.0 {
            extend(interval, bezier(t, x0, x1, x2, x3));
        }
        let t = (-b + r) / (2.0 * a);
        if t > 0.0 && t < 1.0 {
            extend(interval, bezier(t, x0, x1, x2, x3));
        }
    }
}

/// Returns a point on a 1-dimensional Bezier segment.
fn bezier(t: f64, x0: f64, x1: f64, x2: f64, x3: f64) -> f64 {
    let s = 1.0 - t;
    s * s * s * x0 + 3.0 * (s * s * t) * x1 + 3.0 * (t * t * s) * x2 + t * t * t * x3
}
