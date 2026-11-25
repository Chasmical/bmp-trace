/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Point, Sums, util};

/// Stage 2: calculate the optimal polygon (Sec. 2.2.2-2.2.4).
///
/// Finds the optimal polygon. Non-cyclic version: assumes i=0 is in the polygon.
/// FIXME: implement cyclic version.
///
pub fn calculate_optimal_polygon(
    points: &[Point<usize>],
    sums: &[Sums],
    lon: &[usize],
) -> Result<Box<[usize]>, ()> {
    let mut penalties = util::alloc_default_slice::<f64>(points.len() + 1)?;
    let mut prev = util::alloc_default_slice::<usize>(points.len() + 1)?;
    let mut clip1 = util::alloc_default_slice::<usize>(points.len() + 1)?;
    let mut seg0 = util::alloc_default_slice::<usize>(points.len() + 1)?;
    let mut seg1 = util::alloc_default_slice::<usize>(points.len() + 1)?;
    let mut clip0 = util::alloc_uninit_slice::<usize>(points.len())?;

    // Calculate clipped paths
    for (i, slot) in clip0.iter_mut().enumerate() {
        let ci = (i as isize - 1).rem_euclid(points.len() as isize) as usize;
        let mut c = (lon[ci] as isize - 1).rem_euclid(points.len() as isize) as usize;
        if c == i {
            c = (i + 1) % points.len();
        }
        slot.write(if c < i { points.len() } else { c });
    }

    // SAFETY: All values in clip0 were written to in the loop above
    let clip0 = unsafe { clip0.assume_init() };

    // Calculate backwards path clipping, non-cyclic.
    // j <= clip0[i] iff clip1[j] <= i, for i,j in 0..n.
    let mut j = 1;
    for i in 0..points.len() {
        while j <= clip0[i] {
            clip1[j] = i;
            j += 1;
        }
    }

    // Calculate seg0[j] = longest path from 0 with j segments
    let mut i = 0;
    let mut j = 0;
    while i < points.len() {
        seg0[j] = i;
        i = clip0[i];
        j += 1;
    }
    seg0[j] = points.len();
    let m = j;

    // Calculate seg1[j] = longest path to n with m-j segments
    let mut i = points.len();
    for j in (1..=m).rev() {
        seg1[j] = i;
        i = clip1[i];
    }
    seg1[0] = 0;

    // Now find the shortest path with m segments, based on penalty3.
    // Note: the outer 2 loops jointly have at most n interations, thus the worst case behaviour
    // here is quadratic. In practice, it is close to linear since the inner loop tends to be short.
    penalties[0] = 0.0;

    for j in 1..=m {
        for i in seg1[j]..=seg0[j] {
            let mut best = -1.0;
            for k in (clip1[i]..=seg0[j - 1]).rev() {
                let this_penalty = penalty3(points, sums, k, i) + penalties[k];
                if best < 0.0 || this_penalty < best {
                    prev[i] = k;
                    best = this_penalty;
                }
            }
            penalties[i] = best;
        }
    }

    let mut polygon = util::alloc_default_slice::<usize>(m)?;

    // Read off shortest path
    let mut i = points.len();
    let mut j = m;
    while i > 0 {
        i = prev[i];
        j -= 1;
        polygon[j] = i;
    }

    Ok(polygon)
}

/// Calculates the penalty of an edge from `i` to `j` in the given path.
///
fn penalty3(points: &[Point<usize>], sums: &[Sums], i: usize, mut j: usize) -> f64 {
    debug_assert!(i < j && j <= points.len());

    let k = ((j + 1 - i) as f64).recip();

    let sum = if j >= points.len() {
        j -= points.len();
        sums[j + 1] - sums[i] + sums[points.len()]
    } else {
        sums[j + 1] - sums[i]
    };

    let p = (points[i] + points[j]).to_f64() * 0.5 - points[0].to_f64();
    let e = points[j].offset_from(points[i]).to_f64();

    let a = (sum.x2 - 2.0 * sum.x * p.x) * k + p.x * p.x;
    let b = (sum.xy - sum.x * p.y - sum.y * p.x) * k + p.x * p.y;
    let c = (sum.y2 - 2.0 * sum.y * p.y) * k + p.y * p.y;

    let s = e.y * e.y * a - 2.0 * e.y * e.x * b + e.x * e.x * c;

    s.sqrt()
}
