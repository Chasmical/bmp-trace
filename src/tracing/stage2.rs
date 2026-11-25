/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{DPoint, Path, Point, Sums, util};

/// Stage 2: calculate the optimal polygon (Sec. 2.2.2-2.2.4).
///
/// Find the optimal polygon. Fill in the m and po components.
/// Non-cyclic version: assumes i=0 is in the polygon.
/// FIXME: implement cyclic version.
///
pub fn best_polygon(pp: &mut Path) -> Result<(), ()> {
    let n = pp.len();

    let mut penalties = util::alloc_default_slice::<f64>(n + 1)?;
    let mut prev = util::alloc_default_slice::<usize>(n + 1)?;
    let mut clip1 = util::alloc_default_slice::<usize>(n + 1)?;
    let mut seg0 = util::alloc_default_slice::<usize>(n + 1)?;
    let mut seg1 = util::alloc_default_slice::<usize>(n + 1)?;
    let mut clip0 = util::alloc_uninit_slice::<usize>(n)?;

    /* calculate clipped paths */
    let lon = &*pp.lon;
    for (i, slot) in clip0.iter_mut().enumerate() {
        let mut c = (lon[(i - 1).rem_euclid(n)] - 1).rem_euclid(n);
        if c == i {
            c = (i + 1).rem_euclid(n);
        }
        slot.write(if c < i { n } else { c });
    }

    // SAFETY: All values in clip0 were written to in the loop above
    let clip0 = unsafe { clip0.assume_init() };

    /* calculate backwards path clipping, non-cyclic. j <= clip0[i] iff
    clip1[j] <= i, for i,j=0..n. */
    let mut j = 1;
    for i in 0..n {
        while j <= clip0[i] {
            clip1[j] = i;
            j += 1;
        }
    }

    /* calculate seg0[j] = longest path from 0 with j segments */
    let mut i = 0;
    let mut j = 0;
    while i < n {
        seg0[j] = i;
        i = clip0[i];
        j += 1;
    }
    seg0[j] = n;
    let m = j;

    /* calculate seg1[j] = longest path to n with m-j segments */
    let mut i = n;
    for j in (1..=m).rev() {
        seg1[j] = i;
        i = clip1[i];
    }
    seg1[0] = 0;

    /* now find the shortest path with m segments, based on penalty3 */
    /* note: the outer 2 loops jointly have at most n iterations, thus
    the worst-case behavior here is quadratic. In practice, it is
    close to linear since the inner loop tends to be short. */
    penalties[0] = 0.0;

    let points = &*pp.pt;
    let sums = &*pp.sums;

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

    let mut polygons = util::alloc_default_slice::<usize>(m)?;

    /* read off shortest path */
    let mut i = n;
    let mut j = m - 1;
    while i > 0 {
        i = prev[i];
        polygons[j] = i;
        j -= 1;
    }
    pp.po = polygons;

    Ok(())
}

/// Auxiliary function: calculate the penalty of an edge from i to j in the given path.
/// This needs the "lon" and "sum*" data.
///
fn penalty3(points: &[Point], sums: &[Sums], i: usize, mut j: usize) -> f64 {
    let n = points.len();

    debug_assert!(i < j && j <= n); /* assume 0<=i<j<=n  */

    let k = (j + 1 - i) as f64;

    let sum = if j >= n {
        j -= n;
        sums[j + 1] - sums[i] + sums[n]
    } else {
        sums[j + 1] - sums[i]
    };

    let p = DPoint::from(points[i] + points[j]) / 2.0 - DPoint::from(points[0]);
    let e = DPoint::from(points[j] - points[i]);

    let a = (sum.x2 - 2.0 * sum.x * p.x) / k + p.x * p.x;
    let b = (sum.xy - sum.x * p.y - sum.y * p.x) / k + p.x * p.y;
    let c = (sum.y2 - 2.0 * sum.y * p.y) / k + p.y * p.y;

    let s = e.y * e.y * a - 2.0 * e.y * e.x * b + e.x * e.x * c;

    s.sqrt()
}
