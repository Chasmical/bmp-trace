/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Curve, SegmentTag};

/// Stage 4: smoothing and corner analysis (Sec. 2.3.3).
///
pub fn smooth(curve: &mut Curve, alphamax: f64) {
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
