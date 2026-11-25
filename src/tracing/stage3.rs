/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Curve, Point, Sums, util};

/* Adjust vertices of optimal polygon: calculate the intersection of
the two "optimal" line segments, then move it into the unit square
if it lies outside. Return 1 with errno set on error; 0 on
success. */
pub fn adjust_vertices(pt: &[Point<isize>], sums: &[Sums], po: &[usize]) -> Result<Curve, ()> {
    let n = pt.len();
    let m = po.len();
    let origin = pt[0].to_f64();

    let mut ctr = util::alloc_default_slice::<Point<f64>>(m)?;
    let mut dir = util::alloc_default_slice::<Point<f64>>(m)?;
    let mut q = util::alloc_default_slice::<QuadForm>(m)?;

    let mut curve = Curve::try_new(m)?;
    let seg = &mut *curve.segments;

    /* calculate "optimal" point-slope representation for each line
    segment */
    for i in 0..m {
        let j = po[(i + 1).rem_euclid(m)];
        let j = ((j - po[i]).rem_euclid(n)) + po[i];
        let (ctr_i, dir_i) = point_slope(pt, sums, po[i] as isize, j as isize);
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

    /* now calculate the "intersections" of consecutive segments.
    Instead of using the actual intersection, we find the point
    within a given unit square which minimizes the square distance to
    the two lines. */
    for i in 0..m {
        /* let s be the vertex, in coordinates relative to x0/y0 */
        let s = pt[po[i]].to_f64() - origin;

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
                break Point::new(
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
            seg[i].vertex = w + origin;
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

        seg[i].vertex = min_p + origin;
    }

    Ok(curve)
}

fn point_slope(
    pt: &[Point<isize>],
    sums: &[Sums],
    mut i: isize,
    mut j: isize,
) -> (Point<f64>, Point<f64>) {
    /* assume i < j */

    let n = pt.len() as isize;

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

    let ctr = Point::new(x, y) / k;

    let mut a = (x2 - x * x / k) / k;
    let b = (xy - x * y / k) / k;
    let mut c = (y2 - y * y / k) / k;

    let lambda2 = (a + c + ((a - c) * (a - c) + 4.0 * b * b).sqrt()) / 2.0;

    /* now find e.vector for lambda2 */
    a -= lambda2;
    c -= lambda2;

    /* sometimes this can happen when k=4: the two eigenvalues coincide */
    let mut dir = Point::default();

    if a.abs() >= c.abs() {
        let l = (a * a + b * b).sqrt();
        if l != 0.0 {
            dir = Point::new(-b, a) / l;
        }
    } else {
        let l = (c * c + b * b).sqrt();
        if l != 0.0 {
            dir = Point::new(-c, b) / l;
        }
    }

    (ctr, dir)
}

type QuadForm = [[f64; 3]; 3];

fn quadform(q: QuadForm, w: Point<f64>) -> f64 {
    let v = [w.x, w.y, 1.0];
    let mut sum = 0.0;

    for i in 0..3 {
        for j in 0..3 {
            sum += v[i] * q[i][j] * v[j];
        }
    }
    sum
}
