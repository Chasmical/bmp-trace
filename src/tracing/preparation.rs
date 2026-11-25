/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Point, Sums};

/// Preparation: compute the sums of point coordinates (later used for rapid summing).
///
pub fn calculate_sums(points: &[Point<usize>]) -> Result<Box<[Sums]>, ()> {
    let mut sums = Vec::new();
    sums.try_reserve_exact(points.len() + 1).or(Err(()))?;

    // TODO: Doesn't this result in two identical zero sums in the beginning?
    // Should this be [Sums; n] instead of [Sums; n+1]?
    let mut acc = Sums::default();
    sums.push(acc);

    let origin = points[0];
    sums.extend(points.iter().map(|&point| {
        acc += point.offset_from(origin).to_f64();
        acc
    }));

    Ok(sums.into_boxed_slice())
}
