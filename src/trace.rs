/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Curve, DPoint, Path, PathTreeList, Point, SegmentTag, Sums, util};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub turd_size: usize,
    pub turn_policy: TurnPolicy,
    pub alpha_max: f64,
    pub opt_curves: bool,
    pub opt_tolerance: f64,
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

pub fn process_path_list(
    plist: &mut PathTreeList,
    params: &Params,
    mut progress: Option<&mut dyn FnMut(f64)>,
) -> Result<(), ()> {
    let total_progress = {
        if progress.is_some() {
            /* precompute task size for progress estimates */
            let mut p = &*plist;
            let mut sum = 0;
            loop {
                sum += p.private.len();
                if let Some(next) = &p.next { p = next } else { break }
            }
            sum as f64
        } else {
            0.0
        }
    };

    /* call downstream function with each path */
    let mut cur_progress = 0.0;
    let mut p = plist;
    loop {
        process_path(&mut p.private, params, p.is_negative)?;

        if let Some(progress) = progress.as_mut() {
            cur_progress += p.private.len() as f64;
            progress(cur_progress / total_progress);
        }

        if let Some(next) = &mut p.next { p = next } else { break }
    }

    if let Some(progress) = progress {
        progress(1.0);
    }

    Ok(())
}

pub fn process_path(p: &mut Path, params: &Params, negative: bool) -> Result<(), ()> {
    calc_sums(p)?;
    calc_lon(p)?;
    best_polygon(p)?;
    adjust_vertices(p)?;

    if negative {
        /* reverse orientation of negative paths */
        p.curve.segments.reverse();
    }
    smooth(&mut p.curve, params.alpha_max);

    if params.opt_curves {
        opticurve(p, params.opt_tolerance)?;
    }

    Ok(())
}

/// Preparation: fill in the sum* fields of a path (used for later rapid summing).
fn calc_sums(pp: &mut Path) -> Result<(), ()> {
    let points = &*pp.pt;

    /* origin */
    let origin = points[0];
    pp.p0 = origin;

    let mut sums = Vec::new();
    sums.try_reserve_exact(points.len() + 1).or(Err(()))?;

    /* preparatory computation for later fast summing */
    let mut acc = Sums::default();
    sums.push(acc);

    sums.extend(points.iter().map(|pt| {
        acc += DPoint::from(*pt - origin);
        acc
    }));

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

    let points = &*pp.pt;
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

        let mut constraint = [Point::ZERO; 2];

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

    pp.lon = lon;

    Ok(())
}

/// Stage 2: calculate the optimal polygon (Sec. 2.2.2-2.2.4).
///
/// Find the optimal polygon. Fill in the m and po components.
/// Non-cyclic version: assumes i=0 is in the polygon.
/// FIXME: implement cyclic version.
///
fn best_polygon(pp: &mut Path) -> Result<(), ()> {
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
    let n = pp.len();
    let m = pp.po.len();
    let origin = DPoint::from(pp.p0);

    let mut ctr = util::alloc_default_slice::<DPoint>(m)?;
    let mut dir = util::alloc_default_slice::<DPoint>(m)?;
    let mut q = util::alloc_default_slice::<QuadForm>(m)?;

    pp.curve.init(m)?;

    /* calculate "optimal" point-slope representation for each line
    segment */
    for i in 0..m {
        let j = pp.po[(i + 1).rem_euclid(m)];
        let j = ((j - pp.po[i]).rem_euclid(n)) + pp.po[i];
        let (ctr_i, dir_i) = point_slope(pp, pp.po[i] as isize, j as isize);
        ctr[i] = ctr_i;
        dir[i] = dir_i;
    }

    let mut v = [0.0; 3];
    /* represent each line segment as a singular quadratic form; the
    distance of a point (x,y) from the line segment will be
    (x,y,1)Q(x,y,1)^t, where Q=q[i]. */
    for i in 0..m {
        let d = dir[i].x.powi(2) + dir[i].y.powi(2);
        if d == 0.0 {
            q[i] = QuadForm::default();
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

    let po = &*pp.po;
    let pt = &*pp.pt;

    /* now calculate the "intersections" of consecutive segments.
    Instead of using the actual intersection, we find the point
    within a given unit square which minimizes the square distance to
    the two lines. */
    for i in 0..m {
        /* let s be the vertex, in coordinates relative to x0/y0 */
        let s = DPoint::from(pt[po[i]]) - origin;

        /* intersect segments i-1 and i */

        let j = (i - 1).rem_euclid(m);

        let mut qm = QuadForm::default();
        /* add quadratic forms */
        for l in 0..3 {
            for k in 0..3 {
                qm[l][k] = q[j][l][k] + q[i][l][k];
            }
        }

        let mut w = loop {
            /* minimize the quadratic form Q on the unit square */
            /* find intersection */

            let det = qm[0][0] * qm[1][1] - qm[0][1] * qm[1][0];
            if det != 0.0 {
                break DPoint::new(
                    -qm[0][2] * qm[1][1] + qm[1][2] * qm[0][1],
                    qm[0][2] * qm[1][0] - qm[1][2] * qm[0][0],
                ) / det;
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
        };

        let mut dx = (w.x - s.x).abs();
        let mut dy = (w.y - s.y).abs();
        if dx <= 0.5 && dy <= 0.5 {
            pp.curve.segments[i].vertex = w + origin;
            continue;
        }

        /* the minimum was not in the unit square; now minimize quadratic
        on boundary of square */
        let mut min = quadform(qm, s);
        let mut min_p = s;

        if qm[0][0] != 0.0 {
            for z in 0..2 {
                /* value of the y-coordinate */
                w.y = s.y - 0.5 + z as f64;
                w.x = -(qm[0][1] * w.y + qm[0][2]) / qm[0][0];
                dx = (w.x - s.x).abs();
                let cand = quadform(qm, w);
                if dx <= 0.5 && cand < min {
                    min = cand;
                    min_p = w;
                }
            }
        }

        if qm[1][1] != 0.0 {
            for z in 0..2 {
                /* value of the x-coordinate */
                w.x = s.x - 0.5 + z as f64;
                w.y = -(qm[1][0] * w.x + qm[1][2]) / qm[1][1];
                dy = (w.y - s.y).abs();
                let candidate = quadform(qm, w);
                if dy <= 0.5 && candidate < min {
                    min = candidate;
                    min_p = w;
                }
            }
        }
        /* check four corners */
        for l in 0..2 {
            for k in 0..2 {
                w.x = s.x - 0.5 + l as f64;
                w.y = s.y - 0.5 + k as f64;
                let candidate = quadform(qm, w);
                if candidate < min {
                    min = candidate;
                    min_p = w;
                }
            }
        }

        pp.curve.segments[i].vertex = min_p + origin;
    }

    Ok(())
}

fn point_slope(path: &mut Path, mut i: isize, mut j: isize) -> (DPoint, DPoint) {
    /* assume i < j */

    let n = path.pt.len() as isize;
    let sums = &mut *path.sums;

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

    let k = (j + 1 - i + r * n) as f64;

    // re-bind i, j and n as usize for indexing
    let i = i as usize;
    let j = j as usize;
    let n = n as usize;
    // re-bind r as f64, since it's now only used as f64
    let r = r as f64;

    let x = sums[j + 1].x - sums[i].x + r * sums[n].x;
    let y = sums[j + 1].y - sums[i].y + r * sums[n].y;
    let x2 = sums[j + 1].x2 - sums[i].x2 + r * sums[n].x2;
    let xy = sums[j + 1].xy - sums[i].xy + r * sums[n].xy;
    let y2 = sums[j + 1].y2 - sums[i].y2 + r * sums[n].y2;

    let ctr = DPoint::new(x, y) / k;

    let mut a = (x2 - x * x / k) / k;
    let b = (xy - x * y / k) / k;
    let mut c = (y2 - y * y / k) / k;

    let lambda2 = (a + c + ((a - c) * (a - c) + 4.0 * b * b).sqrt()) / 2.0;

    /* now find e.vector for lambda2 */
    a -= lambda2;
    c -= lambda2;

    /* sometimes this can happen when k=4: the two eigenvalues coincide */
    let mut dir = DPoint::ZERO;

    if a.abs() >= c.abs() {
        let l = (a * a + b * b).sqrt();
        if l != 0.0 {
            dir = DPoint::new(-b, a) / l;
        }
    } else {
        let l = (c * c + b * b).sqrt();
        if l != 0.0 {
            dir = DPoint::new(-c, b) / l;
        }
    }

    (ctr, dir)
}

/// Stage 4: smoothing and corner analysis (Sec. 2.3.3).
///
fn smooth(curve: &mut Curve, alphamax: f64) {
    let m = curve.segments.len();
    let seg = &mut *curve.segments;

    /* examine each vertex and find its best fit */
    for i in 0..m {
        let j = (i + 1).rem_euclid(m);
        let k = (i + 2).rem_euclid(m);

        let v_i = seg[i].vertex;
        let v_j = seg[j].vertex;
        let v_k = seg[k].vertex;

        let p4 = v_k.lerp(v_j, 0.5);

        let mut alpha;
        let denom = v_i.ddenom(v_k);
        if denom != 0.0 {
            let dd = v_i.dpara(v_j, v_k) / denom;
            let dd = dd.abs();
            alpha = if dd > 1.0 { 1.0 - 1.0 / dd } else { 0.0 };
            alpha /= 0.75;
        } else {
            alpha = 4.0 / 3.0;
        }
        seg[j].alpha0 = alpha; /* remember "original" value of alpha */

        if alpha >= alphamax {
            /* pointed corner */
            seg[j].tag = SegmentTag::Corner;
            seg[j].c[1] = v_j;
            seg[j].c[2] = p4;
        } else {
            alpha = alpha.clamp(0.55, 1.0);
            let p2 = v_i.lerp(v_j, 0.5 + 0.5 * alpha);
            let p3 = v_k.lerp(v_j, 0.5 + 0.5 * alpha);
            seg[j].tag = SegmentTag::CurveTo;
            seg[j].c[0] = p2;
            seg[j].c[1] = p3;
            seg[j].c[2] = p4;
        }
        seg[j].alpha = alpha; /* store the "cropped" value of alpha */
        seg[j].beta = 0.5;
    }
    curve.alphacurve = true;
}

/// Stage 5: Curve optimization (Sec. 2.4).
///
fn opticurve(pp: &mut Path, opt_tolerance: f64) -> Result<(), ()> {
    let m = pp.curve.segments.len();
    let seg = &*pp.curve.segments;

    let mut pt = util::alloc_default_slice::<isize>(m + 1)?;
    let mut pen = util::alloc_default_slice::<f64>(m + 1)?;
    let mut len = util::alloc_default_slice::<usize>(m + 1)?;
    let mut opt = util::alloc_default_slice::<OptiPenalty>(m + 1)?;
    let mut convc = util::alloc_default_slice::<isize>(m)?;
    let mut areac = util::alloc_default_slice::<f64>(m + 1)?;

    /* pre-calculate convexity: +1 = right turn, -1 = left turn, 0 = corner */
    for i in 0..m {
        if seg[i].tag == SegmentTag::CurveTo {
            convc[i] = seg[(i - 1).rem_euclid(m)]
                .vertex
                .dpara(seg[i].vertex, seg[(i + 1).rem_euclid(m)].vertex)
                .signum() as isize;
        } else {
            convc[i] = 0;
        }
    }

    /* pre-calculate areas */
    let mut area = 0.0;
    areac[0] = 0.0;
    let p0 = seg[0].vertex;
    for i in 0..m {
        let i1 = (i + 1).rem_euclid(m);
        if seg[i1].tag == SegmentTag::CurveTo {
            let alpha = seg[i1].alpha;
            area +=
                0.3 * alpha * (4.0 - alpha) * seg[i].c[2].dpara(seg[i1].vertex, seg[i1].c[2]) / 2.0;
            area += p0.dpara(seg[i].c[2], seg[i1].c[2]) / 2.0;
        }
        areac[i + 1] = area;
    }

    pt[0] = -1;
    pen[0] = 0.0;
    len[0] = 0;

    /* Fixme: we always start from a fixed point -- should find the best curve cyclically */

    for j in 1..=m {
        /* calculate best path from 0 to j */
        pt[j] = (j - 1) as isize;
        pen[j] = pen[j - 1];
        len[j] = len[j - 1] + 1;

        for i in (0..=j - 2).rev() {
            let r = opti_penalty(pp, i, (j).rem_euclid(m), opt_tolerance, &convc, &areac);
            if let Ok(o) = r {
                if len[j] > len[i] + 1 || (len[j] == len[i] + 1 && pen[j] > pen[i] + o.penalty) {
                    pt[j] = i as isize;
                    pen[j] = pen[i] + o.penalty;
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
    let seg = &*pp.curve.segments;
    let oseg = &mut *pp.ocurve.segments;

    let mut s = util::alloc_default_slice::<f64>(om)?;
    let mut t = util::alloc_default_slice::<f64>(om)?;

    let mut j = m;
    for i in (0..om).rev() {
        if pt[j] == (j - 1) as isize {
            oseg[i] = seg[j.rem_euclid(m)];
            s[i] = 1.0;
            t[i] = 1.0;
        } else {
            oseg[i].tag = SegmentTag::CurveTo;
            oseg[i].c[0] = opt[j].c[0];
            oseg[i].c[1] = opt[j].c[1];

            let j_mod = j.rem_euclid(m);
            oseg[i].c[2] = seg[j_mod].c[2];
            oseg[i].vertex = seg[j_mod].c[2].lerp(seg[j_mod].vertex, opt[j].s);
            oseg[i].alpha = opt[j].alpha;
            oseg[i].alpha0 = opt[j].alpha;
            s[i] = opt[j].s;
            t[i] = opt[j].t;
        }
        j = pt[j] as usize;
    }

    /* calculate beta parameters */
    for i in 0..om {
        let i1 = (i + 1).rem_euclid(om);
        oseg[i].beta = s[i] / (s[i] + t[i1]);
    }
    pp.ocurve.alphacurve = true;

    Ok(())
}

#[derive(Default, Clone, Copy)]
struct OptiPenalty {
    penalty: f64,
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
    let seg = &*pp.curve.segments;

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
    let d = seg[i].vertex.distance(seg[i1].vertex);

    let mut k = k1;

    while k != j {
        k1 = (k + 1).rem_euclid(m);
        let k2 = (k + 2).rem_euclid(m);
        if convc[k1] != conv {
            return Err(());
        }
        if seg[i].vertex.cprod(seg[i1].vertex, seg[k1].vertex, seg[k2].vertex).signum() as isize
            != conv
        {
            return Err(());
        }
        if seg[i].vertex.iprod1(seg[i1].vertex, seg[k1].vertex, seg[k2].vertex)
            < d * seg[k1].vertex.distance(seg[k2].vertex) * COS179
        {
            return Err(());
        }
        k = k1;
    }

    /* the curve we're working in: */
    let p0 = seg[(i).rem_euclid(m)].c[2];
    let p1 = seg[(i + 1).rem_euclid(m)].vertex;
    let p2 = seg[(j).rem_euclid(m)].vertex;
    let p3 = seg[(j).rem_euclid(m)].c[2];

    /* determine its area */
    let mut area = areac[j] - areac[i];
    area -= seg[0].vertex.dpara(seg[i].c[2], seg[j].c[2]) / 2.0;
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

    let r = area / ap; /* relative area */
    let alpha = 2.0 - (4.0 - r / 0.3).sqrt(); /* overall alpha for p0-o-p3 curve */

    let mut res = OptiPenalty {
        penalty: 0.0,
        c: [p0.lerp(p1, t * alpha), p3.lerp(p2, s * alpha)],
        alpha,
        t,
        s,
    };

    let p1 = res.c[0];
    let p2 = res.c[1]; /* the proposed curve is now (p0,p1,p2,p3) */

    /* calculate penalty */
    /* check tangency with edges */
    let mut k = (i + 1).rem_euclid(m);
    while k != j {
        k1 = (k + 1).rem_euclid(m);
        let t = p0.tangent(p1, p2, p3, seg[k].vertex, seg[k1].vertex);
        if t < -0.5 {
            return Err(());
        }
        let pt = DPoint::bezier(t, p0, p1, p2, p3);
        let d = seg[k].vertex.distance(seg[k1].vertex);
        if d == 0.0 {
            /* this should never happen */
            return Err(());
        }
        let d1 = seg[k].vertex.dpara(seg[k1].vertex, pt) / d;
        if d1.abs() > opt_tolerance {
            return Err(());
        }
        if seg[k].vertex.iprod(seg[k1].vertex, pt) < 0.0
            || seg[k1].vertex.iprod(seg[k].vertex, pt) < 0.0
        {
            return Err(());
        }
        res.penalty += d1.powi(2);
        k = k1;
    }

    /* check corners */
    let mut k = i;
    while k != j {
        k1 = (k + 1).rem_euclid(m);
        let t = p0.tangent(p1, p2, p3, seg[k].c[2], seg[k1].c[2]);
        if t < -0.5 {
            return Err(());
        }
        let pt = DPoint::bezier(t, p0, p1, p2, p3);
        let d = seg[k].c[2].distance(seg[k1].c[2]);
        if d == 0.0 {
            /* this should never happen */
            return Err(());
        }
        let mut d1 = seg[k].c[2].dpara(seg[k1].c[2], pt) / d;
        let mut d2 = seg[k].c[2].dpara(seg[k1].c[2], seg[k1].vertex) / d;
        d2 *= 0.75 * seg[k1].alpha;
        if d2 < 0.0 {
            d1 = -d1;
            d2 = -d2;
        }
        if d1 < d2 - opt_tolerance {
            return Err(());
        }
        if d1 < d2 {
            res.penalty += (d1 - d2).powi(2);
        }
        k = k1;
    }

    Ok(res)
}
