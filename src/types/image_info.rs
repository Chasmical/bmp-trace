/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::types::{Point, Transform};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) margin_left: f64,
    pub(crate) margin_right: f64,
    pub(crate) margin_top: f64,
    pub(crate) margin_bottom: f64,
    pub(crate) transform: Transform,
}

impl ImageInfo {
    pub fn new(width: f64, height: f64) -> Self {
        assert!(width.is_finite() && height.is_finite());

        Self {
            width,
            height,
            margin_left: 0.0,
            margin_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            transform: Transform::new(width, height),
        }
    }

    pub fn page_width(&self) -> f64 {
        self.transform.bb[0] + self.margin_left + self.margin_right
    }
    pub fn page_height(&self) -> f64 {
        self.transform.bb[1] + self.margin_top + self.margin_bottom
    }
    pub fn origin_top_left(&self) -> Point<f64> {
        Point {
            x: self.transform.orig[0] + self.margin_left,
            y: self.page_height() - self.transform.orig[1] - self.margin_bottom,
        }
    }
    pub fn origin_bottom_left(&self) -> Point<f64> {
        Point {
            x: self.transform.orig[0] + self.margin_left,
            y: self.transform.orig[1] + self.margin_bottom,
        }
    }

    pub fn calculate_dimensions(width: usize, height: usize) -> Self {
        let imginfo = Self::new(
            if width == 0 { 1.0 } else { width as f64 },
            if height == 0 { 1.0 } else { height as f64 },
        );

        let mut used_default_scaling = false;

        // TODO: how the hell do i do this

        imginfo
    }
}
