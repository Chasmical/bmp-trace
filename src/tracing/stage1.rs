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

/// Stage 1: determine the straight subpaths (Sec. 2.2.1). Fill in the "lon" component of a path
/// object (based on pt/len). For each i, lon[i] is the furthest index such that a straight line
/// can be drawn from i to lon[i]. Return 1 on error with errno set, else 0.
///
/// This algorithm depends on the fact that the existence of straight subpaths is a triplewise
/// property. I.e., there exists a straight line through squares i0,...,in iff there exists a
/// straight line through i,j,k, for all i0<=i<j<k<=in. (Proof?)
///
/// This implementation of calc_lon is O(n^2). It replaces an older O(n^3) version.
/// A "constraint" means that future points must satisfy xprod(constraint[0], cur) >= 0
/// and xprod(constraint[1], cur) <= 0.
///
/// Remark for Potrace 1.1: the current implementation of calc_lon is more complex than the
/// implementation found in Potrace 1.0, but it is considerably faster. The introduction of the
/// "nc" data structure means that we only have to test the constraints for "corner" points. On
/// a typical input file, this speeds up the calc_lon function by a factor of 31.2, thereby
/// decreasing its time share within the overall Potrace algorithm from 72.6% to 7.82%, and
/// speeding up the overall algorithm by a factor of 3.36. On another input file, calc_lon was
/// sped up by a factor of 6.7, decreasing its time share from 51.4% to 13.61%, and speeding up
/// the overall algorithm by a factor of 1.78. In any case, the savings are substantial.
///
pub fn calculate_longest_subpaths(points: &[Point<isize>]) -> Result<Box<[usize]>, ()> {
    #[inline]
    fn floor_div(a: isize, n: isize) -> isize {
        if a >= 0 { a / n } else { -1 - (-1 - a) / n }
    }
    /// Returns 1 if a <= b < c < a, in a cyclic sense (mod n)
    #[inline]
    fn cyclic(a: usize, b: usize, c: usize) -> bool {
        if a <= c { a <= b && b < c } else { a <= b || b < c }
    }

    let n = points.len();

    let mut pivk = util::alloc_uninit_slice(n)?;
    let mut nc = util::alloc_uninit_slice(n)?;
    let mut lon = util::alloc_uninit_slice(n)?;

    /* initialize the nc data structure. Point from each point to the
    furthest future point to which it is connected by a vertical or
    horizontal segment. We take advantage of the fact that there is
    always a direction change at 0 (due to the path decomposition
    algorithm). But even if this were not so, there is no harm, as
    in practice, correctness does not depend on the word "furthest"
    above.  */
    let mut k = 0;
    let mut k_pt = points[k];

    for (i, pt) in points.iter().enumerate().rev() {
        if pt.x != k_pt.x && pt.y != k_pt.y {
            k = i + 1; /* necessarily i<n-1 in this case */
            k_pt = points[k];
        }
        nc[i].write(k);
    }

    // SAFETY: All values in nc were written to in the loop above
    let nc = unsafe { nc.assume_init() };

    /* determine pivot points: for each i, let pivk[i] be the furthest k
    such that all j with i<j<k lie on a line connecting i,k. */

    for i in (0..n).rev() {
        let mut ct = [0u32; 4];

        /* keep track of "directions" that have occurred */
        let diff = points[(i + 1).rem_euclid(n)] - points[i];
        let dir = (3 + 3 * diff.x + diff.y) / 2;
        ct[dir as usize] += 1;

        let mut constraint = [Point::default(); 2];

        /* find the next k such that no straight line from i to k */
        let mut k = nc[i];
        let mut k1 = i;

        let constraint_violated = loop {
            let diff = (points[k] - points[k1]).signum();
            let dir = (3 + 3 * diff.x + diff.y) / 2;
            ct[dir as usize] += 1;

            /* if all four "directions" have occurred, cut this path */
            if ct[0] != 0 && ct[1] != 0 && ct[2] != 0 && ct[3] != 0 {
                pivk[i].write(k1);
                break false;
            }

            let cur = points[k] - points[i];

            /* see if current constraint is violated */
            if constraint[0].xprod(cur) < 0 || constraint[1].xprod(cur) > 0 {
                break true;
            }

            /* else, update constraint */
            if cur.x.abs() <= 1 && cur.y.abs() <= 1 {
                /* no constraint */
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
            /* k1 was the last "corner" satisfying the current constraint, and
            k is the first one violating it. We now need to find the last
            point along k1..k which satisfied the constraint. */
            let dk = (points[k] - points[k1]).signum();
            let cur = points[k1] - points[i];
            /* find largest integer j such that xprod(constraint[0], cur+j*dk)
            >= 0 and xprod(constraint[1], cur+j*dk) <= 0. Use bilinearity
            of xprod. */
            let a = constraint[0].xprod(cur);
            let b = constraint[0].xprod(dk);
            let c = constraint[1].xprod(cur);
            let d = constraint[1].xprod(dk);
            /* find largest integer j such that a+j*b>=0 and c+j*d<=0. This
            can be solved with integer arithmetic. */
            let mut j = INFTY;
            if b < 0 {
                j = floor_div(a, -b);
            }
            if d > 0 {
                j = j.min(floor_div(-c, d));
            }
            pivk[i].write((k1 as isize + j).rem_euclid(n as isize) as usize);
        }
    } /* for i */

    // SAFETY: All values in pivk were written to in the loop above
    let pivk = unsafe { pivk.assume_init() };

    /* clean up: for each i, let lon[i] be the largest k such that for
    all i' with i<=i'<k, i'<k<=pivk[i']. */

    let mut j = pivk[n - 1];
    lon[n - 1].write(j);
    for i in (0..n - 1).rev() {
        if cyclic(i + 1, pivk[i], j) {
            j = pivk[i];
        }
        lon[i].write(j);
    }

    let mut lon = unsafe { lon.assume_init() };

    for i in (0..n).rev() {
        if !cyclic((i + 1).rem_euclid(n), j, lon[i]) {
            break;
        }
        lon[i] = j;
    }

    Ok(lon)
}
