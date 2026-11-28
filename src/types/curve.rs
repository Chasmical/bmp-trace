/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{types::Point, util};

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
    Corner,
    CurveTo,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Segment {
    pub(crate) tag: SegmentTag,                 // int* tag
    pub(crate) control_points: [Point<f64>; 3], // dpoint_t[3]* c
    pub(crate) vertex: Point<f64>,              // dpoint_t* vertex
    pub(crate) alpha: f64,                      // double* alpha
    pub(crate) alpha0: f64,                     // double* alpha0
    pub(crate) beta: f64,                       // double* beta
}

impl Curve {
    /// Initializes this curve to the given size.
    pub fn try_new(len: usize) -> Result<Self, ()> {
        let segments = util::alloc_default_slice(len)?;
        Ok(Self { segments })
    }
}

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

/// A path is represented as an array of points, which are thought to lie on the corners of pixels
/// (not their centers). The path point `(x,y)` is the lower left corner of the pixel `(x,y)`.
#[derive(Default, Clone)]
pub struct Path {
    pub(crate) points: Vec<Point<usize>>,
    pub(crate) is_negative: bool,
    pub(crate) area: usize,
    pub(crate) curve: Curve,
}

#[derive(Default)]
pub struct PathTree {
    pub(crate) path: Path,
    pub(crate) next: *mut PathTree,
    pub(crate) first_child: *mut PathTree,
    pub(crate) next_sibling: *mut PathTree,
}

impl PathTree {
    pub fn new(path: Path) -> Self {
        Self { path, ..Default::default() }
    }
    pub fn next(&self) -> Option<&PathTree> {
        unsafe { self.next.as_ref() }
    }
    pub fn first_child(&self) -> Option<&PathTree> {
        unsafe { self.first_child.as_ref() }
    }
    pub fn next_sibling(&self) -> Option<&PathTree> {
        unsafe { self.next_sibling.as_ref() }
    }

    pub fn children(&self) -> PathSiblingsIter<'_> {
        PathSiblingsIter { sibling: self.first_child() }
    }
    pub fn self_and_siblings(&self) -> PathSiblingsIter<'_> {
        PathSiblingsIter { sibling: Some(self) }
    }
    pub fn iter(&self) -> PathFlatIter<'_> {
        PathFlatIter { next: Some(self) }
    }
}

impl Drop for PathTree {
    fn drop(&mut self) {
        if let Some(p) = unsafe { std::mem::take(&mut self.next).as_mut() } {
            drop(unsafe { Box::from_raw(p) });
        }
    }
}

#[derive(Default, Clone)]
pub struct PathFlatIter<'a> {
    next: Option<&'a PathTree>,
}

impl<'a> Iterator for PathFlatIter<'a> {
    type Item = &'a PathTree;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next?;
        self.next = next.next();
        Some(next)
    }
}

#[derive(Default, Clone)]
pub struct PathSiblingsIter<'a> {
    sibling: Option<&'a PathTree>,
}

impl<'a> Iterator for PathSiblingsIter<'a> {
    type Item = &'a PathTree;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.sibling?;
        self.sibling = next.next_sibling();
        Some(next)
    }
}
