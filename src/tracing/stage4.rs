/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::types::{Curve, SegmentTag};

/// Stage 4: smoothing and corner analysis (Sec. 2.3.3).
///
pub fn smooth_curve(curve: &mut Curve, alphamax: f64) {
    let seg = &mut *curve.segments;

    // Examine each vertex and find its best fit
    for i in 0..seg.len() {
        let j = (i + 1) % seg.len();
        let k = (i + 2) % seg.len();

        let v_i = seg[i].vertex;
        let v_j = seg[j].vertex;
        let v_k = seg[k].vertex;

        let p4 = v_k.lerp(v_j, 0.5);

        let mut alpha;
        let denom = v_i.ddenom(v_k);
        if denom != 0.0 {
            let dd = (v_i.dpara(v_j, v_k) / denom).abs();
            alpha = if dd > 1.0 { 1.0 - 1.0 / dd } else { 0.0 };
            alpha *= 4.0 / 3.0;
        } else {
            alpha = 4.0 / 3.0;
        }
        seg[j].alpha0 = alpha; // Remember "original" value of alpha

        if alpha >= alphamax {
            // Pointed corner
            seg[j].tag = SegmentTag::Corner;
            seg[j].control_points[1] = v_j;
            seg[j].control_points[2] = p4;
        } else {
            alpha = alpha.clamp(0.55, 1.0);
            let p2 = v_i.lerp(v_j, 0.5 + 0.5 * alpha);
            let p3 = v_k.lerp(v_j, 0.5 + 0.5 * alpha);
            seg[j].tag = SegmentTag::CurveTo;
            seg[j].control_points[0] = p2;
            seg[j].control_points[1] = p3;
            seg[j].control_points[2] = p4;
        }
        seg[j].alpha = alpha; // Store the "cropped" value of alpha
        seg[j].beta = 0.5;
    }
}
