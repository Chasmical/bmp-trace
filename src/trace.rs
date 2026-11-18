/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Curve, DPoint, Path, PathTreeList, Point, SegmentTag, Sums, util};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub turd_size: usize,
    pub turn_policy: TurnPolicy,
    pub alpha_max: f64,
    pub opt_curves: bool,
    pub opt_tolerance: f64,
    // TODO: progress callback
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPolicy {
    Black = 0,
    White = 1,
    Left = 2,
    Right = 3,
    Minority = 4,
    Majority = 5,
    Random = 6,
}

// TODO: progress param?
pub fn process_path(plist: &mut PathTreeList, param: &Params) -> Result<(), ()> {
    let nn = 0.0;
    let cn = 0.0;

    // TODO:

    //   if (progress->callback) {
    //     /* precompute task size for progress estimates */
    //     nn = 0;
    //     list_forall (p, plist) {
    //       nn += p->priv->len;
    //     }
    //     cn = 0;
    //   }

    /* call downstream function with each path */
    let mut next = Some(plist);
    while let Some(p) = next {
        calc_sums(&mut p.private)?;
        calc_lon(&mut p.private)?;
        best_polygon(&mut p.private)?;
        adjust_vertices(&mut p.private)?;

        if p.sign == b'-' as isize {
            /* reverse orientation of negative paths */
            p.private.curve.segments.reverse();
        }
        smooth(&mut p.private.curve, param.alpha_max);

        if param.opt_curves {
            opticurve(&mut p.private, param.opt_tolerance)?;
        }

        // TODO: p.private.fcurve().copy_structure_to(&mut p.curve)

        // TODO:
        // if (progress->callback) {
        //   cn += p->priv->len;
        //   progress_update(cn/nn, progress);
        // }

        next = p.next.as_mut().and_then(Rc::get_mut);
    }

    // TODO:
    //   progress_update(1.0, progress);

    Ok(())
}

/// Preparation: fill in the sum* fields of a path (used for later rapid summing).
fn calc_sums(pp: &mut Path) -> Result<(), ()> {
    let points = pp.pt.as_ref();

    let mut sums = util::alloc_zeroed::<Sums>(points.len())?;

    /* origin */
    pp.x0 = points[0].x;
    pp.y0 = points[0].y;

    /* preparatory computation for later fast summing */
    sums[0].x2 = 0.0;
    sums[0].xy = 0.0;
    sums[0].y2 = 0.0;
    sums[0].x = 0.0;
    sums[0].y = 0.0;

    // TODO: this does excessive bounds checks
    for i in 0..points.len() {
        let x = (points[i].x - pp.x0) as f64;
        let y = (points[i].y - pp.y0) as f64;
        sums[i + 1].x = sums[i].x + x;
        sums[i + 1].y = sums[i].y + y;
        sums[i + 1].x2 = sums[i].x2 + x * x;
        sums[i + 1].xy = sums[i].xy + x * y;
        sums[i + 1].y2 = sums[i].y2 + y * y;
    }

    pp.sums = sums;
    Ok(())
}

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
fn calc_lon(pp: &mut Path) -> Result<(), ()> {
    #[inline]
    fn floor_div(a: isize, n: isize) -> isize {
        if a >= 0 { a / n } else { -1 - (-1 - a) / n }
    }
    /// Returns 1 if a <= b < c < a, in a cyclic sense (mod n)
    #[inline]
    fn cyclic(a: usize, b: usize, c: usize) -> bool {
        if a <= c { a <= b && b < c } else { a <= b || b < c }
    }

    // TODO: all array accesses here do bounds checks
    let pt = pp.pt.as_ref();
    let n = pt.len();

    let mut pivk = util::alloc_zeroed::<usize>(n)?;
    let mut nc = util::alloc_zeroed::<usize>(n)?;

    /* initialize the nc data structure. Point from each point to the
    furthest future point to which it is connected by a vertical or
    horizontal segment. We take advantage of the fact that there is
    always a direction change at 0 (due to the path decomposition
    algorithm). But even if this were not so, there is no harm, as
    in practice, correctness does not depend on the word "furthest"
    above.  */
    let mut k = 0;
    for i in (0..=n - 1).rev() {
        if pt[i].x != pt[k].x && pt[i].y != pt[k].y {
            k = i + 1; /* necessarily i<n-1 in this case */
        }
        nc[i] = k;
    }

    pp.lon = util::alloc_zeroed(n)?;

    /* determine pivot points: for each i, let pivk[i] be the furthest k
    such that all j with i<j<k lie on a line connecting i,k. */

    for i in (0..=n - 1).rev() {
        let mut ct = [0u32; 4];

        /* keep track of "directions" that have occurred */
        let dir = (3 + 3 * (pt[(i + 1) % n].x - pt[i].x) + (pt[(i + 1) % n].y - pt[i].y)) / 2;
        ct[dir as usize] += 1;

        let mut constraint = [Point::ZERO, Point::ZERO];

        /* find the next k such that no straight line from i to k */
        let mut k = nc[i] as usize;
        let mut k1 = i;

        let mut cur = Point::ZERO;
        let mut off = Point::ZERO;

        // CHANGE: A flag instead of labels constraint_viol: and foundk:.
        let mut constraint_violated = false;

        loop {
            let dir = (3 + 3 * (pt[k].x - pt[k1].x).signum() + (pt[k].y - pt[k1].y).signum()) / 2;
            ct[dir as usize] += 1;

            /* if all four "directions" have occurred, cut this path */
            if ct[0] != 0 && ct[1] != 0 && ct[2] != 0 && ct[3] != 0 {
                pivk[i] = k1;
                break;
            }

            cur.x = pt[k].x - pt[i].x;
            cur.y = pt[k].y - pt[i].y;

            /* see if current constraint is violated */
            if constraint[0].xprod(cur) < 0 || constraint[1].xprod(cur) > 0 {
                constraint_violated = true;
                break;
            }

            /* else, update constraint */
            if cur.x.abs() <= 1 && cur.y.abs() <= 1 {
                /* no constraint */
            } else {
                off.x = cur.x + (if cur.y >= 0 && (cur.y > 0 || cur.x < 0) { 1 } else { -1 });
                off.y = cur.y + (if cur.x <= 0 && (cur.x < 0 || cur.y < 0) { 1 } else { -1 });
                if constraint[0].xprod(off) >= 0 {
                    constraint[0] = off;
                }
                off.x = cur.x + (if cur.y <= 0 && (cur.y < 0 || cur.x < 0) { 1 } else { -1 });
                off.y = cur.y + (if cur.x >= 0 && (cur.x > 0 || cur.y < 0) { 1 } else { -1 });
                if constraint[1].xprod(off) <= 0 {
                    constraint[1] = off;
                }
            }
            k1 = k;
            k = nc[k1];
            if !cyclic(k, i, k1) {
                break;
            }
        }

        if constraint_violated {
            /* k1 was the last "corner" satisfying the current constraint, and
            k is the first one violating it. We now need to find the last
            point along k1..k which satisfied the constraint. */
            let dk = Point { x: (pt[k].x - pt[k1].x).signum(), y: (pt[k].y - pt[k1].y).signum() };
            cur.x = pt[k1].x - pt[i].x;
            cur.y = pt[k1].y - pt[i].y;
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
            pivk[i] = (k1 as isize + j).rem_euclid(n as isize) as usize;
        }
    } /* for i */

    /* clean up: for each i, let lon[i] be the largest k such that for
    all i' with i<=i'<k, i'<k<=pivk[i']. */

    let lon = pp.lon.as_mut();

    let mut j = pivk[n - 1];
    lon[n - 1] = j;
    for i in (0..=n - 2).rev() {
        if cyclic(i + 1, pivk[i], j) {
            j = pivk[i];
        }
        lon[i] = j;
    }

    let mut i = n - 1;
    while cyclic((i + 1) % n, j, lon[i]) {
        lon[i] = j;
        i -= 1;
    }

    Ok(())
}

/// Stage 2: calculate the optimal polygon (Sec. 2.2.2-2.2.4).
///
/// Find the optimal polygon. Fill in the m and po components.
/// Non-cyclic version: assumes i=0 is in the polygon.
/// FIXME: implement cyclic version.
///
fn best_polygon(pp: &mut Path) -> Result<(), ()> {
    // TODO: all array accesses here do bounds checks
    let n = pp.pt.len();

    let mut pen = util::alloc_zeroed::<f64>(n + 1)?;
    let mut prev = util::alloc_zeroed::<usize>(n + 1)?;
    let mut clip0 = util::alloc_zeroed::<usize>(n)?;
    let mut clip1 = util::alloc_zeroed::<usize>(n + 1)?;
    let mut seg0 = util::alloc_zeroed::<usize>(n + 1)?;
    let mut seg1 = util::alloc_zeroed::<usize>(n + 1)?;

    /* calculate clipped paths */
    let lon = pp.lon.as_ref();
    for i in 0..n {
        let mut c = (lon[(i - 1).rem_euclid(n)] - 1).rem_euclid(n);
        if c == i {
            c = (i + 1).rem_euclid(n);
        }
        clip0[i] = if c < i { n } else { c };
    }

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
    for j in 0..n {
        seg0[j] = i;
        i = clip0[i];
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
    pen[0] = 0.0;

    for j in 1..=m {
        for i in seg1[j]..=seg0[j] {
            let mut best = -1.0;
            for k in (clip1[i]..=seg0[j - 1]).rev() {
                let thispen = penalty3(pp, k, i) + pen[k];
                if best < 0.0 || thispen < best {
                    prev[i] = k;
                    best = thispen;
                }
            }
            pen[i] = best;
        }
    }

    let mut polygons = util::alloc_zeroed(m)?;

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
fn penalty3(pp: &mut Path, i: usize, mut j: usize) -> f64 {
    let pt = pp.pt.as_ref();
    let sums = pp.sums.as_ref();
    let n = pt.len();

    /* assume 0<=i<j<=n  */

    let mut r = 0; /* rotations from i to j */

    if j >= n {
        j -= n;
        r = 1;
    }

    let (x, y, x2, xy, y2, k);

    /* critical inner loop: the "if" gives a 4.6 percent speedup */
    if r == 0 {
        x = sums[j + 1].x - sums[i].x;
        y = sums[j + 1].y - sums[i].y;
        x2 = sums[j + 1].x2 - sums[i].x2;
        xy = sums[j + 1].xy - sums[i].xy;
        y2 = sums[j + 1].y2 - sums[i].y2;
        k = (j + 1 - i) as f64;
    } else {
        x = sums[j + 1].x - sums[i].x + sums[n].x;
        y = sums[j + 1].y - sums[i].y + sums[n].y;
        x2 = sums[j + 1].x2 - sums[i].x2 + sums[n].x2;
        xy = sums[j + 1].xy - sums[i].xy + sums[n].xy;
        y2 = sums[j + 1].y2 - sums[i].y2 + sums[n].y2;
        k = (j + 1 - i + n) as f64;
    }

    let px = ((pt[i].x + pt[j].x) / 2 - pt[0].x) as f64;
    let py = ((pt[i].y + pt[j].y) / 2 - pt[0].y) as f64;
    let ey = (pt[j].x - pt[i].x) as f64;
    let ex = -(pt[j].y - pt[i].y) as f64;

    let a = (x2 - 2.0 * x * px) / k + px * px;
    let b = (xy - x * py - y * px) / k + px * py;
    let c = (y2 - 2.0 * y * py) / k + py * py;

    let s = ex * ex * a + 2.0 * ex * ey * b + ey * ey * c;

    s.sqrt()
}

pub type QuadForm = [[f64; 3]; 3];

fn quadform(q: QuadForm, w: DPoint) -> f64 {
    let v = [w.x, w.y, 1.0];
    let mut sum = 0.0;

    for i in 0..3 {
        for j in 0..3 {
            sum += v[i] * q[i][j] * v[j];
        }
    }
    sum
}

fn adjust_vertices(pp: &mut Path) -> Result<(), ()> {
    let n = pp.pt.len();
    let m = pp.po.len();
    let x0 = pp.x0 as f64;
    let y0 = pp.y0 as f64;

    //   dpoint_t *ctr = NULL;      /* ctr[m] */
    //   dpoint_t *dir = NULL;      /* dir[m] */
    //   quadform_t *q = NULL;      /* q[m] */
    //   double v[3];
    //   double d;
    //   int i, j, k, l;
    //   dpoint_t s;
    //   int r;

    let mut ctr = util::alloc_zeroed::<DPoint>(m)?;
    let mut dir = util::alloc_zeroed::<DPoint>(m)?;
    let mut q = util::alloc_zeroed::<QuadForm>(m)?;

    pp.curve.init(m)?;

    /* calculate "optimal" point-slope representation for each line
    segment */
    for i in 0..m {
        let j = pp.po[(i + 1) % m];
        let j = ((j - pp.po[i]) % (n)) + pp.po[i];
        point_slope(pp, pp.po[i] as isize, j as isize, &mut ctr[i], &mut dir[i]);
    }

    let po = pp.po.as_ref();
    let pt = pp.pt.as_ref();

    let mut v = [0.0; 3];
    /* represent each line segment as a singular quadratic form; the
    distance of a point (x,y) from the line segment will be
    (x,y,1)Q(x,y,1)^t, where Q=q[i]. */
    for i in 0..m {
        let d = dir[i].x.powi(2) + dir[i].y.powi(2);
        if d == 0.0 {
            for j in 0..3 {
                for k in 0..3 {
                    q[i][j][k] = 0.0;
                }
            }
        } else {
            v[0] = dir[i].y;
            v[1] = -dir[i].x;
            v[2] = -v[1] * ctr[i].y - v[0] * ctr[i].x;

            for l in 0..3 {
                for k in 0..3 {
                    q[i][l][k] = v[l] * v[k] / d;
                }
            }
        }
    }

    /* now calculate the "intersections" of consecutive segments.
    Instead of using the actual intersection, we find the point
    within a given unit square which minimizes the square distance to
    the two lines. */
    for i in 0..m {
        /* let s be the vertex, in coordinates relative to x0/y0 */
        let s = DPoint { x: (pt[po[i]].x as f64) - x0, y: (pt[po[i]].y as f64) - y0 };

        /* intersect segments i-1 and i */

        let j = (i - 1) % m;

        let mut qm = QuadForm::default();
        /* add quadratic forms */
        for l in 0..3 {
            for k in 0..3 {
                qm[l][k] = q[j][l][k] + q[i][l][k];
            }
        }

        let mut w = DPoint::default();
        loop {
            /* minimize the quadratic form Q on the unit square */
            /* find intersection */

            // #ifdef HAVE_GCC_LOOP_BUG
            //       /* work around gcc bug #12243 */
            //       free(NULL);
            // #endif

            let det = qm[0][0] * qm[1][1] - qm[0][1] * qm[1][0];
            if det != 0.0 {
                w.x = (-qm[0][2] * qm[1][1] + qm[1][2] * qm[0][1]) / det;
                w.y = (qm[0][2] * qm[1][0] - qm[1][2] * qm[0][0]) / det;
                break;
            }

            /* matrix is singular - lines are parallel. Add another,
            orthogonal axis, through the center of the unit square */
            if qm[0][0] > qm[1][1] {
                v[0] = -qm[0][1];
                v[1] = qm[0][0];
            } else if qm[1][1] != 0.0 {
                v[0] = -qm[1][1];
                v[1] = qm[1][0];
            } else {
                v[0] = 1.0;
                v[1] = 0.0;
            }
            let d = v[0].powi(2) + v[1].powi(2);
            v[2] = -v[1] * s.y - v[0] * s.x;
            for l in 0..3 {
                for k in 0..3 {
                    qm[l][k] += v[l] * v[k] / d;
                }
            }
        }
        let mut dx = (w.x - s.x).abs();
        let mut dy = (w.y - s.y).abs();
        if dx <= 0.5 && dy <= 0.5 {
            pp.curve.segments[i].vertex = DPoint { x: w.x + x0, y: w.y + y0 };
            continue;
        }

        /* the minimum was not in the unit square; now minimize quadratic
        on boundary of square */
        let mut min = quadform(qm, s);
        let mut xmin = s.x;
        let mut ymin = s.y;

        if qm[0][0] != 0.0 {
            for z in 0..2 {
                /* value of the y-coordinate */
                w.y = s.y - 0.5 + z as f64;
                w.x = -(qm[0][1] * w.y + qm[0][2]) / qm[0][0];
                dx = (w.x - s.x).abs();
                let cand = quadform(qm, w);
                if dx <= 0.5 && cand < min {
                    min = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }

        if qm[1][1] != 0.0 {
            for z in 0..2 {
                /* value of the x-coordinate */
                w.x = s.x - 0.5 + z as f64;
                w.y = -(qm[1][0] * w.x + qm[1][2]) / qm[1][1];
                dy = (w.y - s.y).abs();
                let cand = quadform(qm, w);
                if dy <= 0.5 && cand < min {
                    min = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }
        /* check four corners */
        for l in 0..2 {
            for k in 0..2 {
                w.x = s.x - 0.5 + l as f64;
                w.y = s.y - 0.5 + k as f64;
                let cand = quadform(qm, w);
                if cand < min {
                    min = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }

        pp.curve.segments[i].vertex = DPoint { x: xmin + x0, y: ymin + y0 };
        continue;
    }

    Ok(())
}

fn point_slope(path: &mut Path, mut i: isize, mut j: isize, ctr: &mut DPoint, dir: &mut DPoint) {
    /* assume i < j */

    let n = path.pt.len() as isize;
    let sums = path.sums.as_mut();

    let mut r = 0; /* rotations from i to j */

    while j >= n {
        j -= n;
        r += 1;
    }
    while i >= n {
        i -= n;
        r -= 1;
    }
    while j < 0 {
        j += n;
        r -= 1;
    }
    while i < 0 {
        i += n;
        r += 1;
    }

    // CHANGE: moved calculation of k here, before r is re-binded as f64
    let k = (j + 1 - i + r * n) as f64;
    // CHANGE: re-bind r as f64, since it's now only used as f64
    let r = r as f64;

    // CHANGE: re-bind i, j and n as usize for indexing
    let i = i as usize;
    let j = j as usize;
    let n = n as usize;

    // TODO: this does bounds checks
    let x = sums[j + 1].x - sums[i].x + r * sums[n].x;
    let y = sums[j + 1].y - sums[i].y + r * sums[n].y;
    let x2 = sums[j + 1].x2 - sums[i].x2 + r * sums[n].x2;
    let xy = sums[j + 1].xy - sums[i].xy + r * sums[n].xy;
    let y2 = sums[j + 1].y2 - sums[i].y2 + r * sums[n].y2;

    ctr.x = x / k;
    ctr.y = y / k;

    let mut a = (x2 - x * x / k) / k;
    let b = (xy - x * y / k) / k;
    let mut c = (y2 - y * y / k) / k;

    let lambda2 = (a + c + ((a - c) * (a - c) + 4.0 * b * b).sqrt()) / 2.0;

    /* now find e.vector for lambda2 */
    a -= lambda2;
    c -= lambda2;

    let l;
    if a.abs() >= c.abs() {
        l = (a * a + b * b).sqrt();
        if l != 0.0 {
            dir.x = -b / l;
            dir.y = a / l;
        }
    } else {
        l = (c * c + b * b).sqrt();
        if l != 0.0 {
            dir.x = -c / l;
            dir.y = b / l;
        }
    }
    if l == 0.0 {
        dir.x = 0.0;
        dir.y = 0.0; /* sometimes this can happen when k=4: the two eigenvalues coincide */
    }
}

/// Stage 4: smoothing and corner analysis (Sec. 2.3.3).
///
fn smooth(curve: &mut Curve, alphamax: f64) {
    let m = curve.segments.len();

    //   int i, j, k;
    //   double dd, denom, alpha;
    //   dpoint_t p2, p3, p4;

    /* examine each vertex and find its best fit */
    for i in 0..m {
        let j = (i + 1).rem_euclid(m);
        let k = (i + 2).rem_euclid(m);
        let p4 = curve.segments[k].vertex.lerp(curve.segments[j].vertex, 0.5);

        let mut alpha;
        let denom = curve.segments[i].vertex.ddenom(curve.segments[k].vertex);
        if denom != 0.0 {
            let dd =
                curve.segments[i].vertex.dpara(curve.segments[j].vertex, curve.segments[k].vertex)
                    / denom;
            let dd = dd.abs();
            alpha = if dd > 1.0 { 1.0 - 1.0 / dd } else { 0.0 };
            alpha = alpha / 0.75;
        } else {
            alpha = 4.0 / 3.0;
        }
        curve.segments[j].alpha0 = alpha; /* remember "original" value of alpha */

        if alpha >= alphamax {
            /* pointed corner */
            curve.segments[j].tag = SegmentTag::Corner;
            curve.segments[j].c[1] = curve.segments[j].vertex;
            curve.segments[j].c[2] = p4;
        } else {
            if alpha < 0.55 {
                alpha = 0.55;
            } else if alpha > 1.0 {
                alpha = 1.0;
            }
            let p2 = curve.segments[i].vertex.lerp(curve.segments[j].vertex, 0.5 + 0.5 * alpha);
            let p3 = curve.segments[k].vertex.lerp(curve.segments[j].vertex, 0.5 + 0.5 * alpha);
            curve.segments[j].tag = SegmentTag::CurveTo;
            curve.segments[j].c[0] = p2;
            curve.segments[j].c[1] = p3;
            curve.segments[j].c[2] = p4;
        }
        curve.segments[j].alpha = alpha; /* store the "cropped" value of alpha */
        curve.segments[j].beta = 0.5;
    }
    curve.alphacurve = true;
}

/// Stage 5: Curve optimization (Sec. 2.4).
///
fn opticurve(pp: &mut Path, opt_tolerance: f64) -> Result<(), ()> {
    let m = pp.curve.segments.len();

    let mut pt = util::alloc_zeroed::<isize>(m + 1)?;
    let mut pen = util::alloc_zeroed::<f64>(m + 1)?;
    let mut len = util::alloc_zeroed::<usize>(m + 1)?;
    let mut opt = util::alloc_zeroed::<OptiPenalty>(m + 1)?;
    let mut convc = util::alloc_zeroed::<isize>(m)?;
    let mut areac = util::alloc_zeroed::<f64>(m + 1)?;

    /* pre-calculate convexity: +1 = right turn, -1 = left turn, 0 = corner */
    for i in 0..m {
        if pp.curve.segments[i].tag == SegmentTag::CurveTo {
            convc[i] = pp.curve.segments[(i - 1).rem_euclid(m)]
                .vertex
                .dpara(pp.curve.segments[i].vertex, pp.curve.segments[(i + 1).rem_euclid(m)].vertex)
                .signum() as isize;
        } else {
            convc[i] = 0;
        }
    }

    /* pre-calculate areas */
    let mut area = 0.0;
    areac[0] = 0.0;
    let p0 = pp.curve.segments[0].vertex;
    for i in 0..m {
        let i1 = (i + 1).rem_euclid(m);
        if pp.curve.segments[i1].tag == SegmentTag::CurveTo {
            let alpha = pp.curve.segments[i1].alpha;
            area += 0.3
                * alpha
                * (4.0 - alpha)
                * pp.curve.segments[i].c[2]
                    .dpara(pp.curve.segments[i1].vertex, pp.curve.segments[i1].c[2])
                / 2.0;
            area += p0.dpara(pp.curve.segments[i].c[2], pp.curve.segments[i1].c[2]) / 2.0;
        }
        areac[i + 1] = area;
    }

    pt[0] = -1;
    pen[0] = 0.0;
    len[0] = 0;

    /* Fixme: we always start from a fixed point -- should find the best
    curve cyclically */

    for j in 1..=m {
        /* calculate best path from 0 to j */
        pt[j] = (j - 1) as isize;
        pen[j] = pen[j - 1];
        len[j] = len[j - 1] + 1;

        for i in (0..=j - 2).rev() {
            let r = opti_penalty(
                pp,
                i,
                (j).rem_euclid(m),
                opt_tolerance,
                convc.as_ref(),
                areac.as_ref(),
            );
            if let Ok(o) = r {
                if len[j] > len[i] + 1 || (len[j] == len[i] + 1 && pen[j] > pen[i] + o.pen) {
                    pt[j] = i as isize;
                    pen[j] = pen[i] + o.pen;
                    len[j] = len[i] + 1;
                    opt[j] = o;
                }
            } else {
                break;
            }
        }
    }

    let om = len[m];
    pp.ocurve.init(om)?;

    let mut s = util::alloc_zeroed::<f64>(om)?;
    let mut t = util::alloc_zeroed::<f64>(om)?;

    let mut j = m;
    for i in (0..=om - 1).rev() {
        if pt[j] == (j - 1) as isize {
            pp.ocurve.segments[i].tag = pp.curve.segments[(j).rem_euclid(m)].tag;
            pp.ocurve.segments[i].c[0] = pp.curve.segments[(j).rem_euclid(m)].c[0];
            pp.ocurve.segments[i].c[1] = pp.curve.segments[(j).rem_euclid(m)].c[1];
            pp.ocurve.segments[i].c[2] = pp.curve.segments[(j).rem_euclid(m)].c[2];
            pp.ocurve.segments[i].vertex = pp.curve.segments[(j).rem_euclid(m)].vertex;
            pp.ocurve.segments[i].alpha = pp.curve.segments[(j).rem_euclid(m)].alpha;
            pp.ocurve.segments[i].alpha0 = pp.curve.segments[(j).rem_euclid(m)].alpha0;
            pp.ocurve.segments[i].beta = pp.curve.segments[(j).rem_euclid(m)].beta;
            s[i] = 1.0;
            t[i] = 1.0;
        } else {
            pp.ocurve.segments[i].tag = SegmentTag::CurveTo;
            pp.ocurve.segments[i].c[0] = opt[j].c[0];
            pp.ocurve.segments[i].c[1] = opt[j].c[1];
            pp.ocurve.segments[i].c[2] = pp.curve.segments[(j).rem_euclid(m)].c[2];
            pp.ocurve.segments[i].vertex = pp.curve.segments[(j).rem_euclid(m)].c[2]
                .lerp(pp.curve.segments[(j).rem_euclid(m)].vertex, opt[j].s);
            pp.ocurve.segments[i].alpha = opt[j].alpha;
            pp.ocurve.segments[i].alpha0 = opt[j].alpha;
            s[i] = opt[j].s;
            t[i] = opt[j].t;
        }
        j = pt[j] as usize;
    }

    /* calculate beta parameters */
    for i in 0..om {
        let i1 = (i + 1).rem_euclid(om);
        pp.ocurve.segments[i].beta = s[i] / (s[i] + t[i1]);
    }
    pp.ocurve.alphacurve = true;

    Ok(())
}

#[derive(Default, Clone, Copy)]
struct OptiPenalty {
    pen: f64,
    c: [DPoint; 2],
    t: f64,
    s: f64,
    alpha: f64,
}

const COS179: f64 = -0.999847695156; /* the cosine of 179 degrees */

/// Calculate best fit from i+.5 to j+.5.  Assume i<j (cyclically).
/// Return Ok and set badness and parameters (alpha, beta), if possible. Return Err if impossible.
fn opti_penalty(
    pp: &mut Path,
    i: usize,
    j: usize,
    opt_tolerance: f64,
    convc: &[isize],
    areac: &[f64],
) -> Result<OptiPenalty, ()> {
    let m = pp.curve.segments.len();

    /* check convexity, corner-freeness, and maximum bend < 179 degrees */

    if i == j {
        /* sanity - a full loop can never be an opticurve */
        return Err(());
    }

    let k = i;
    let i1 = (i + 1).rem_euclid(m);
    let mut k1 = (k + 1).rem_euclid(m);
    let conv = convc[k1];
    if conv == 0 {
        return Err(());
    }
    let d = pp.curve.segments[i].vertex.distance(pp.curve.segments[i1].vertex);

    let mut k = k1;

    while k != j {
        k1 = (k + 1).rem_euclid(m);
        let k2 = (k + 2).rem_euclid(m);
        if convc[k1] != conv {
            return Err(());
        }
        if pp.curve.segments[i]
            .vertex
            .cprod(
                pp.curve.segments[i1].vertex,
                pp.curve.segments[k1].vertex,
                pp.curve.segments[k2].vertex,
            )
            .signum() as isize
            != conv
        {
            return Err(());
        }
        if pp.curve.segments[i].vertex.iprod1(
            pp.curve.segments[i1].vertex,
            pp.curve.segments[k1].vertex,
            pp.curve.segments[k2].vertex,
        ) < d * pp.curve.segments[k1].vertex.distance(pp.curve.segments[k2].vertex) * COS179
        {
            return Err(());
        }
        k = k1;
    }

    /* the curve we're working in: */
    let p0 = pp.curve.segments[(i).rem_euclid(m)].c[2];
    let p1 = pp.curve.segments[(i + 1).rem_euclid(m)].vertex;
    let p2 = pp.curve.segments[(j).rem_euclid(m)].vertex;
    let p3 = pp.curve.segments[(j).rem_euclid(m)].c[2];

    /* determine its area */
    let mut area = areac[j] - areac[i];
    area -= pp.curve.segments[0].vertex.dpara(pp.curve.segments[i].c[2], pp.curve.segments[j].c[2])
        / 2.0;
    if i >= j {
        area += areac[m];
    }

    /* find intersection o of p0p1 and p2p3. Let t,s such that o =
    interval(t,p0,p1) = interval(s,p3,p2). Let A be the area of the
    triangle (p0,o,p3). */

    let a1 = p0.dpara(p1, p2);
    let a2 = p0.dpara(p1, p3);
    let a3 = p0.dpara(p2, p3);
    /* A4 = dpara(p1, p2, p3); */
    let a4 = a1 + a3 - a2;

    if a2 == a1 {
        /* this should never happen */
        return Err(());
    }

    let t = a3 / (a3 - a4);
    let s = a2 / (a2 - a1);
    let ap = a2 * t / 2.0;

    if ap == 0.0 {
        /* this should never happen */
        return Err(());
    }

    let r = area as f64 / ap; /* relative area */
    let alpha = 2.0 - (4.0 - r / 0.3).sqrt(); /* overall alpha for p0-o-p3 curve */

    let mut res =
        OptiPenalty { pen: 0.0, c: [p0.lerp(p1, t * alpha), p3.lerp(p2, s * alpha)], alpha, t, s };

    let p1 = res.c[0];
    let p2 = res.c[1]; /* the proposed curve is now (p0,p1,p2,p3) */

    /* calculate penalty */
    /* check tangency with edges */
    let mut k = (i + 1).rem_euclid(m);
    while k != j {
        k1 = (k + 1).rem_euclid(m);
        let t = p0.tangent(p1, p2, p3, pp.curve.segments[k].vertex, pp.curve.segments[k1].vertex);
        if t < -0.5 {
            return Err(());
        }
        let pt = DPoint::bezier(t, p0, p1, p2, p3);
        let d = pp.curve.segments[k].vertex.distance(pp.curve.segments[k1].vertex);
        if d == 0.0 {
            /* this should never happen */
            return Err(());
        }
        let d1 = pp.curve.segments[k].vertex.dpara(pp.curve.segments[k1].vertex, pt) / d;
        if d1.abs() > opt_tolerance {
            return Err(());
        }
        if pp.curve.segments[k].vertex.iprod(pp.curve.segments[k1].vertex, pt) < 0.0
            || pp.curve.segments[k1].vertex.iprod(pp.curve.segments[k].vertex, pt) < 0.0
        {
            return Err(());
        }
        res.pen += d1.powi(2);
        k = k1;
    }

    /* check corners */
    let mut k = i;
    while k != j {
        k1 = (k + 1).rem_euclid(m);
        let t = p0.tangent(p1, p2, p3, pp.curve.segments[k].c[2], pp.curve.segments[k1].c[2]);
        if t < -0.5 {
            return Err(());
        }
        let pt = DPoint::bezier(t, p0, p1, p2, p3);
        let d = pp.curve.segments[k].c[2].distance(pp.curve.segments[k1].c[2]);
        if d == 0.0 {
            /* this should never happen */
            return Err(());
        }
        let mut d1 = pp.curve.segments[k].c[2].dpara(pp.curve.segments[k1].c[2], pt) / d;
        let mut d2 = pp.curve.segments[k].c[2]
            .dpara(pp.curve.segments[k1].c[2], pp.curve.segments[k1].vertex)
            / d;
        d2 *= 0.75 * pp.curve.segments[k1].alpha;
        if d2 < 0.0 {
            d1 = -d1;
            d2 = -d2;
        }
        if d1 < d2 - opt_tolerance {
            return Err(());
        }
        if d1 < d2 {
            res.pen += (d1 - d2).powi(2);
        }
        k = k1;
    }

    Ok(res)
}
