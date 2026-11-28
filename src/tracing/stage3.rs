/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{
    types::{Curve, Point, Sums},
    util,
};

/// Stage 3: vertex adjustment (Sec. 2.3.1).
///
/// Adjusts the vertices of the optimal polygon. Calculates the intersection of the two "optimal"
/// line segments, then moves it into the unit square if it lies outside.
///
pub fn adjust_vertices(
    points: &[Point<usize>],
    sums: &[Sums],
    polygons: &[usize],
) -> Result<Curve, ()> {
    let origin = points[0].to_f64();

    let mut ctr = util::alloc_default_slice::<Point<f64>>(polygons.len())?;
    let mut dir = util::alloc_default_slice::<Point<f64>>(polygons.len())?;
    let mut q = util::alloc_default_slice::<QuadForm>(polygons.len())?;

    let mut curve = Curve::try_new(polygons.len())?;
    let seg = &mut *curve.segments;

    // Calculate the "optimal" point-slope representation for each line segment
    for i in 0..polygons.len() {
        let j = polygons[(i + 1) % polygons.len()] as isize - polygons[i] as isize;
        let j = j.rem_euclid(points.len() as isize) as usize + polygons[i];
        // TODO: polygons is unsigned, right? And j here is definitely unsigned too, right?
        // So why does point_slope then have two loops for "unsigning" the i and j args?
        let (ctr_i, dir_i) = point_slope(points, sums, polygons[i] as isize, j as isize);
        ctr[i] = ctr_i;
        dir[i] = dir_i;
    }

    // Represent each line segment as a singular quadratic form; the distance of
    // a point (x,y) from the line segment will be (x,y,1)Q(x,y,1)^t, where Q=q[i].
    for i in 0..polygons.len() {
        let d = dir[i].x.powi(2) + dir[i].y.powi(2);
        if d == 0.0 {
            q[i] = QuadForm::default();
        } else {
            let mut v = [dir[i].y, -dir[i].x, 0.0];
            v[2] = -v[1] * ctr[i].y - v[0] * ctr[i].x;

            for l in 0..3 {
                for k in 0..3 {
                    q[i][l][k] = v[l] * v[k] / d;
                }
            }
        }
    }

    // Now calculate the "intersections" of consecutive segments. Instead of using the actual
    // intersection, we find the point within a given unit square which minimizes the square
    // distance to the two lines.
    for i in 0..polygons.len() {
        // Let s be the vertex, in coordinates relative to x0,y0
        let s = points[polygons[i]].to_f64() - origin;

        // Intersect segments i-1 and i

        let j = (i as isize - 1).rem_euclid(polygons.len() as isize) as usize;

        let mut qm = QuadForm::default();
        // Add quadratic forms
        for l in 0..3 {
            for k in 0..3 {
                qm[l][k] = q[j][l][k] + q[i][l][k];
            }
        }

        let mut w = loop {
            // Minimize the quadratic form Q on the unit square
            // Find intersection

            let det = qm[0][0] * qm[1][1] - qm[0][1] * qm[1][0];
            if det != 0.0 {
                break Point::new(
                    -qm[0][2] * qm[1][1] + qm[1][2] * qm[0][1],
                    qm[0][2] * qm[1][0] - qm[1][2] * qm[0][0],
                ) / det;
            }

            // Matrix is singular - lines are parallel. Add another, orthogonal axis,
            // through the center of the unit square.
            let mut v = if qm[0][0] > qm[1][1] {
                [-qm[0][1], qm[0][0], 0.0]
            } else if qm[1][1] != 0.0 {
                [-qm[1][1], qm[1][0], 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let d = v[0].powi(2) + v[1].powi(2);
            v[2] = -v[1] * s.y - v[0] * s.x;

            for l in 0..3 {
                for k in 0..3 {
                    qm[l][k] += v[l] * v[k] / d;
                }
            }
        };

        let dx = (w.x - s.x).abs();
        let dy = (w.y - s.y).abs();
        if dx <= 0.5 && dy <= 0.5 {
            seg[i].vertex = w + origin;
            continue;
        }

        // The minimum was not in the unit square; now minimize quadratic on boundary of square.
        let mut min = quadform(qm, s);
        let mut min_p = s;

        if qm[0][0] != 0.0 {
            for z in 0..2 {
                // Value of the y-coordinate
                w.y = s.y - 0.5 + z as f64;
                w.x = -(qm[0][1] * w.y + qm[0][2]) / qm[0][0];
                let dx = (w.x - s.x).abs();
                let candidate = quadform(qm, w);
                if dx <= 0.5 && candidate < min {
                    min = candidate;
                    min_p = w;
                }
            }
        }

        if qm[1][1] != 0.0 {
            for z in 0..2 {
                // Value of the x-coordinate
                w.x = s.x - 0.5 + z as f64;
                w.y = -(qm[1][0] * w.x + qm[1][2]) / qm[1][1];
                let dy = (w.y - s.y).abs();
                let candidate = quadform(qm, w);
                if dy <= 0.5 && candidate < min {
                    min = candidate;
                    min_p = w;
                }
            }
        }
        // Check four corners
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
    points: &[Point<usize>],
    sums: &[Sums],
    mut i: isize,
    mut j: isize,
) -> (Point<f64>, Point<f64>) {
    debug_assert!(i < j);

    let n = points.len() as isize;

    let mut r = 0; // rotations from i to j

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

    let lambda2 = (a + c + ((a - c) * (a - c) + 4.0 * b * b).sqrt()) * 0.5;

    // Now find e.vector for lambda2
    a -= lambda2;
    c -= lambda2;

    // Sometimes may return 0,0 - when k=4: the two eigenvalues coincide
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
