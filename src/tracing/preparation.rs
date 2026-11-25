/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Point, Sums};

/// Preparation: fill in the sum* fields of a path (used for later rapid summing).
pub fn calculate_sums(points: &[Point<isize>]) -> Result<Box<[Sums]>, ()> {
    let mut sums = Vec::new();
    sums.try_reserve_exact(points.len() + 1).or(Err(()))?;

    /* preparatory computation for later fast summing */
    let mut acc = Sums::default();
    sums.push(acc);

    let origin = points[0];
    sums.extend(points.iter().map(|pt| {
        acc += (*pt - origin).to_f64();
        acc
    }));

    Ok(sums.into_boxed_slice())
}
