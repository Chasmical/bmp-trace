/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Point, util};

/// Corresponds to `privcurve_t` in Potrace.
// TODO: Putting all of the data into a Segment struct makes the layout slightly more efficient
// (only need to store one length), but may reduce the performance gains due to memory locality.
// This will need to be benchmarked and compared.
#[derive(Clone, Default)]
pub struct Curve {
    pub(crate) segments: Box<[Segment]>, // int n // see Segment struct
}

/// Enum for segment tags.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SegmentTag {
    #[default]
    None = 0,
    /// Corresponds to `POTRACE_CURVETO` in Potrace.
    CurveTo = 1,
    /// Corresponds to `POTRACE_CORNER` in Potrace.
    Corner = 2,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Segment {
    pub(crate) tag: SegmentTag,    // int* tag
    pub(crate) c: [Point<f64>; 3], // dpoint_t[3]* c
    pub(crate) vertex: Point<f64>, // dpoint_t* vertex
    pub(crate) alpha: f64,         // double* alpha
    pub(crate) alpha0: f64,        // double* alpha0
    pub(crate) beta: f64,          // double* beta
}

impl Curve {
    /// Initializes this curve to the given size.
    ///
    /// Corresponds to `int privcurve_init(privcurve_t*, int)` in Potrace.
    pub fn try_new(len: usize) -> Result<Self, ()> {
        let segments = util::alloc_default_slice(len)?;
        Ok(Self { segments })
    }
}

/// Corresponds to `sums_t` in Potrace.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Sums {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) x2: f64,
    pub(crate) xy: f64,
    pub(crate) y2: f64,
}

impl std::ops::Add for Sums {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            x2: self.x2 + rhs.x2,
            xy: self.xy + rhs.xy,
            y2: self.y2 + rhs.y2,
        }
    }
}
impl std::ops::AddAssign<Point<f64>> for Sums {
    fn add_assign(&mut self, rhs: Point<f64>) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.x2 += rhs.x * rhs.x;
        self.xy += rhs.x * rhs.y;
        self.y2 += rhs.y * rhs.y;
    }
}
impl std::ops::Sub for Sums {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            x2: self.x2 - rhs.x2,
            xy: self.xy - rhs.xy,
            y2: self.y2 - rhs.y2,
        }
    }
}

/// Corresponds to `potrace_privpath_t` in Potrace.
#[derive(Clone, Default)]
pub struct Path {
    pub(crate) pt: Vec<Point<isize>>,
    pub(crate) is_negative: bool,
    pub(crate) area: usize,
    pub(crate) curve: Curve,
}
