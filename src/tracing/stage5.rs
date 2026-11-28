/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{
    types::{Curve, Point, Segment, SegmentTag},
    util,
};

/// Stage 5: curve optimization (Sec. 2.4).
///
/// Optimizes the given curve, replacing sequences of Bezier segments by a single segment when
/// possible.
///
pub fn optimize_curve(curve: &Curve, opt_tolerance: f64) -> Result<Curve, ()> {
    let seg = &*curve.segments;

    let mut points = util::alloc_default_slice::<usize>(seg.len() + 1)?;
    let mut pen = util::alloc_default_slice::<f64>(seg.len() + 1)?;
    let mut len = util::alloc_default_slice::<usize>(seg.len() + 1)?;
    let mut opt = util::alloc_default_slice::<OptiPenalty>(seg.len() + 1)?;
    let mut convc = util::alloc_default_slice::<isize>(seg.len())?;
    let mut areac = util::alloc_default_slice::<f64>(seg.len() + 1)?;

    // Pre-calculate convexity: +1 = right turn, -1 = left turn, 0 = corner
    for i in 0..seg.len() {
        if seg[i].tag == SegmentTag::CurveTo {
            convc[i] = seg[(i as isize - 1).rem_euclid(seg.len() as isize) as usize]
                .vertex
                .dpara(seg[i].vertex, seg[(i + 1) % seg.len()].vertex)
                .signum() as isize;
        } else {
            convc[i] = 0;
        }
    }

    // Pre-calculate areas
    let mut area = 0.0;
    areac[0] = 0.0;
    let p0 = seg[0].vertex;
    for i in 0..seg.len() {
        let i1 = (i + 1) % seg.len();
        if seg[i1].tag == SegmentTag::CurveTo {
            let alpha = seg[i1].alpha;
            area += 0.3
                * alpha
                * (4.0 - alpha)
                * seg[i].control_points[2].dpara(seg[i1].vertex, seg[i1].control_points[2])
                * 0.5;
            area += p0.dpara(seg[i].control_points[2], seg[i1].control_points[2]) * 0.5;
        }
        areac[i + 1] = area;
    }

    points[0] = 0; // points[0] is never accessed
    pen[0] = 0.0;
    len[0] = 0;

    // FIXME: We always start from a fixed point -- should find the best curve cyclically

    for j in 1..=seg.len() {
        // Calculate best path from 0 to j
        points[j] = j - 1;
        pen[j] = pen[j - 1];
        len[j] = len[j - 1] + 1;

        for i in (0..=j as isize - 2).rev() {
            let i = i as usize;
            let r = opti_penalty(seg, i, j % seg.len(), opt_tolerance, &convc, &areac);
            if let Ok(o) = r {
                if len[j] > len[i] + 1 || (len[j] == len[i] + 1 && pen[j] > pen[i] + o.penalty) {
                    points[j] = i;
                    pen[j] = pen[i] + o.penalty;
                    len[j] = len[i] + 1;
                    opt[j] = o;
                }
            } else {
                break;
            }
        }
    }

    let om = len[seg.len()];
    let mut ocurve = Curve::try_new(om)?;
    let oseg = &mut *ocurve.segments;

    let mut s = util::alloc_default_slice::<f64>(om)?;
    let mut t = util::alloc_default_slice::<f64>(om)?;

    let mut j = seg.len();
    for i in (0..om).rev() {
        if points[j] == j - 1 {
            oseg[i] = seg[j % seg.len()];
            s[i] = 1.0;
            t[i] = 1.0;
        } else {
            oseg[i].tag = SegmentTag::CurveTo;
            oseg[i].control_points[0] = opt[j].c[0];
            oseg[i].control_points[1] = opt[j].c[1];

            let j_mod = j % seg.len();
            oseg[i].control_points[2] = seg[j_mod].control_points[2];
            oseg[i].vertex = seg[j_mod].control_points[2].lerp(seg[j_mod].vertex, opt[j].s);
            oseg[i].alpha = opt[j].alpha;
            oseg[i].alpha0 = opt[j].alpha;
            s[i] = opt[j].s;
            t[i] = opt[j].t;
        }
        j = points[j];
    }

    // Calculate beta parameters
    for i in 0..om {
        let i1 = (i + 1) % om;
        oseg[i].beta = s[i] / (s[i] + t[i1]);
    }

    Ok(ocurve)
}

#[derive(Default, Clone, Copy)]
struct OptiPenalty {
    penalty: f64,
    c: [Point<f64>; 2],
    t: f64,
    s: f64,
    alpha: f64,
}

const COS179: f64 = -0.999847695156; // The cosine of 179 degrees

/// Calculates best fit from `i+0.5` to `j+0.5`. Assumes `i<j` (cyclically).
/// Returns `Ok` with penalty parameters, if possible. Return `Err` if impossible.
fn opti_penalty(
    seg: &[Segment],
    i: usize,
    j: usize,
    opt_tolerance: f64,
    convc: &[isize],
    areac: &[f64],
) -> Result<OptiPenalty, ()> {
    // Check convexity, corner-freeness, and maximum bend < 179 degrees

    if i == j {
        // Sanity - a full loop can never be an opticurve
        return Err(());
    }

    let k = i;
    let i1 = (i + 1) % seg.len();
    let mut k1 = (k + 1) % seg.len();
    let conv = convc[k1];
    if conv == 0 {
        return Err(());
    }
    let d = seg[i].vertex.distance(seg[i1].vertex);

    let mut k = k1;

    while k != j {
        k1 = (k + 1) % seg.len();
        let k2 = (k + 2) % seg.len();
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

    // The curve we're working in:
    let p0 = seg[i % seg.len()].control_points[2];
    let p1 = seg[(i + 1) % seg.len()].vertex;
    let p2 = seg[j % seg.len()].vertex;
    let p3 = seg[j % seg.len()].control_points[2];

    // Determine its area
    let mut area = areac[j] - areac[i];
    area -= seg[0].vertex.dpara(seg[i].control_points[2], seg[j].control_points[2]) * 0.5;
    if i >= j {
        area += areac[seg.len()];
    }

    // Find intersection o of p0p1 and p2p3. Let t,s such that o = interval(t,p0,p1)
    // = interval(s,p3,p2). Let A be the area of the triangle (p0,o,p3).

    let a1 = p0.dpara(p1, p2);
    let a2 = p0.dpara(p1, p3);
    let a3 = p0.dpara(p2, p3);
    // a4 = dpara(p1, p2, p3);
    let a4 = a1 + a3 - a2;

    if a2 == a1 {
        // This should never happen
        return Err(());
    }

    let t = a3 / (a3 - a4);
    let s = a2 / (a2 - a1);
    let ap = a2 * t * 0.5;

    if ap == 0.0 {
        // This should never happen
        return Err(());
    }

    let r = area / ap; // Relative area
    let alpha = 2.0 - (4.0 - r / 0.3).sqrt(); // Overall alpha for p0-o-p3 curve

    let mut res = OptiPenalty {
        penalty: 0.0,
        c: [p0.lerp(p1, t * alpha), p3.lerp(p2, s * alpha)],
        alpha,
        t,
        s,
    };

    let p1 = res.c[0];
    let p2 = res.c[1]; // The proposed curve is now (p0,p1,p2,p3)

    // Calculate penalty
    // Check tangency with edges
    let mut k = (i + 1) % seg.len();
    while k != j {
        k1 = (k + 1) % seg.len();
        let t = p0.tangent(p1, p2, p3, seg[k].vertex, seg[k1].vertex).ok_or(())?;
        let point = Point::bezier(t, p0, p1, p2, p3);
        let d = seg[k].vertex.distance(seg[k1].vertex);
        if d == 0.0 {
            // This should never happen
            return Err(());
        }
        let d1 = seg[k].vertex.dpara(seg[k1].vertex, point) / d;
        if d1.abs() > opt_tolerance {
            return Err(());
        }
        if seg[k].vertex.iprod(seg[k1].vertex, point) < 0.0
            || seg[k1].vertex.iprod(seg[k].vertex, point) < 0.0
        {
            return Err(());
        }
        res.penalty += d1.powi(2);
        k = k1;
    }

    // Check corners
    let mut k = i;
    while k != j {
        k1 = (k + 1) % seg.len();
        let t =
            p0.tangent(p1, p2, p3, seg[k].control_points[2], seg[k1].control_points[2]).ok_or(())?;
        let point = Point::bezier(t, p0, p1, p2, p3);
        let d = seg[k].control_points[2].distance(seg[k1].control_points[2]);
        if d == 0.0 {
            // This should never happen
            return Err(());
        }
        let mut d1 = seg[k].control_points[2].dpara(seg[k1].control_points[2], point) / d;
        let mut d2 = seg[k].control_points[2].dpara(seg[k1].control_points[2], seg[k1].vertex) / d;
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
