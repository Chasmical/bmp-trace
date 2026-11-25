/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Point, util};

const INFTY: isize = 10000000;

/// Stage 1: determine the straight subpaths (Sec. 2.2.1). For each `i`, `lon[i]` is the furthest
/// index such that a straight line can be drawn from `i` to `lon[i]`.
///
/// This algorithm depends on the fact that the existence of straight subpaths is a triplewise
/// property, i.e. there exists a straight line through squares `i0..=in` iff there exists a
/// straight line through `i,j,k`, for all `i0<=i<j<k<=in`.
/// FIXME: Proof?
///
/// This implementation of the algorithm is `O(n^2)`. A "constraint" means that future points must
/// satisfy `constraint[0] × cur >= 0` and `constraint[1] × cur <= 0`.
///
pub fn calculate_straight_subpaths(points: &[Point<usize>]) -> Result<Box<[usize]>, ()> {
    #[inline]
    fn floor_div(a: isize, n: isize) -> isize {
        if a >= 0 { a / n } else { -1 - (-1 - a) / n }
    }
    /// Returns 1 if a <= b < c < a, in a cyclic sense (mod n)
    #[inline]
    fn cyclic(a: usize, b: usize, c: usize) -> bool {
        if a <= c { a <= b && b < c } else { a <= b || b < c }
    }

    let mut pivots = util::alloc_uninit_slice(points.len())?;
    let mut nc = util::alloc_uninit_slice(points.len())?;
    let mut lon = util::alloc_uninit_slice(points.len())?;

    // Initialize the nc data structure. Point from each point to the furthest future point to
    // which it is connected by a vertical or horizontal segment. We take advantage of the fact that
    // there is always a direction change at 0 (due to the path decomposition algorithm). But even
    // if this were not so, there is no harm, as in practice, correctness does not depend on the
    // word "furthest" above.
    let mut k = 0;
    let mut k_pt = points[k];

    for (i, point) in points.iter().enumerate().rev() {
        if point.x != k_pt.x && point.y != k_pt.y {
            k = i + 1; // Necessarily i < n-1 in this case
            k_pt = points[k];
        }
        nc[i].write(k);
    }

    // SAFETY: All values in nc were written to in the loop above
    let nc = unsafe { nc.assume_init() };

    // Determine pivot points: for each i, let pivk[i] be the furthest k,
    // such that all j with i < j < k lie on a line connecting i,k.
    for i in (0..points.len()).rev() {
        let mut ct = [0u32; 4];

        // Keep track of "directions" that have occured
        let diff = points[(i + 1).rem_euclid(points.len())].offset_from(points[i]);
        let dir = (3 + 3 * diff.x + diff.y) / 2;
        ct[dir as usize] += 1;

        let mut constraint = [Point::default(); 2];

        // Find the next k such that no straight line from i to k
        let mut k = nc[i];
        let mut k1 = i;

        let constraint_violated = loop {
            let diff = points[k].offset_from(points[k1]).signum();
            let dir = (3 + 3 * diff.x + diff.y) / 2;
            ct[dir as usize] += 1;

            // If all four "directions" have occurred, cut this path
            if ct[0] != 0 && ct[1] != 0 && ct[2] != 0 && ct[3] != 0 {
                pivots[i].write(k1);
                break false;
            }

            let cur = points[k].offset_from(points[i]);

            // See if current constraint is violated
            if constraint[0].xprod(cur) < 0 || constraint[1].xprod(cur) > 0 {
                break true;
            }

            // Else, update constraint
            if cur.x.abs() <= 1 && cur.y.abs() <= 1 {
                // No constraint
            } else {
                let off = Point::new(
                    cur.x + (if cur.y >= 0 && (cur.y > 0 || cur.x < 0) { 1 } else { -1 }),
                    cur.y + (if cur.x <= 0 && (cur.x < 0 || cur.y < 0) { 1 } else { -1 }),
                );
                if constraint[0].xprod(off) >= 0 {
                    constraint[0] = off;
                }
                let off = Point::new(
                    cur.x + (if cur.y <= 0 && (cur.y < 0 || cur.x < 0) { 1 } else { -1 }),
                    cur.y + (if cur.x >= 0 && (cur.x > 0 || cur.y < 0) { 1 } else { -1 }),
                );
                if constraint[1].xprod(off) <= 0 {
                    constraint[1] = off;
                }
            }
            k1 = k;
            k = nc[k1];
            if !cyclic(k, i, k1) {
                break true;
            }
        };

        if constraint_violated {
            // k1 was the last "corner" satisfying the current constraint, and k is the first
            // one violating it. We now need to find the last point along k1..k which satisfied
            // the constraint.
            let dk = points[k].offset_from(points[k1]).signum();
            let cur = points[k1].offset_from(points[i]);
            // Find largest integer j such that constraint[0] × (cur+j*dk) >= 0 and
            // constraint[1] × (cur+j*dk) <= 0. Use bilinearity of xprod.
            let a = constraint[0].xprod(cur);
            let b = constraint[0].xprod(dk);
            let c = constraint[1].xprod(cur);
            let d = constraint[1].xprod(dk);
            // Find largest integer j such that a+j*b >= 0 and c+j*d <= 0.
            // This can be solved with integer arithmetic.
            let mut j = INFTY;
            if b < 0 {
                j = floor_div(a, -b);
            }
            if d > 0 {
                j = j.min(floor_div(-c, d));
            }
            pivots[i].write((k1 as isize + j).rem_euclid(points.len() as isize) as usize);
        }
    }

    // SAFETY: All values in pivk were written to in the loop above
    let pivk = unsafe { pivots.assume_init() };

    // Clean up: for each i, let lon[i] be the largest k,
    // such that for all i' with i <= i' < k, i' < k <= pivk[i'].
    let mut j = *pivk.last().unwrap();
    lon.last_mut().unwrap().write(j);
    for i in (0..points.len() - 1).rev() {
        if cyclic(i + 1, pivk[i], j) {
            j = pivk[i];
        }
        lon[i].write(j);
    }

    // SAFETY: All values in lon were written to in the loop above
    let mut lon = unsafe { lon.assume_init() };

    for i in (0..points.len()).rev() {
        if !cyclic((i + 1).rem_euclid(points.len()), j, lon[i]) {
            break;
        }
        lon[i] = j;
    }

    Ok(lon)
}
