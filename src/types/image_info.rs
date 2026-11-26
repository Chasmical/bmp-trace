/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::types::Transformation;

#[derive(Debug, Default, Clone)]
pub struct ImageInfo {
    pub pix_width: usize,
    pub pix_height: usize,
    pub width: f64,
    pub height: f64,
    pub left_margin: f64,
    pub right_margin: f64,
    pub top_margin: f64,
    pub bottom_margin: f64,
    pub transform: Transformation,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Dimension {
    value: f64,
    dimension: f64, // in pt, or 0 if not given
}
impl Dimension {
    pub fn inches(inches: f64) -> Self {
        Self { value: inches, dimension: 72.0 }
    }
    pub fn centimeters(centimeters: f64) -> Self {
        Self { value: centimeters, dimension: 72.0 / 2.54 }
    }
    pub fn millimeters(millimeters: f64) -> Self {
        Self { value: millimeters, dimension: 72.0 / 25.4 }
    }
    pub fn points(points: f64) -> Self {
        Self { value: points, dimension: 1.0 }
    }
}
