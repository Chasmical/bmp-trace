/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::types::{Bitmap, Curve, PathTree, Point};

mod decomposition;
mod preparation;
mod stage1;
mod stage2;
mod stage3;
mod stage4;
mod stage5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub turd_size: usize,
    pub turn_policy: TurnPolicy,
    pub alpha_max: f64,
    pub opt_tolerance: Option<f64>,
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

impl Default for Params {
    fn default() -> Self {
        Self {
            turd_size: 2,
            turn_policy: TurnPolicy::Minority,
            alpha_max: 1.0,
            opt_tolerance: Some(0.2),
        }
    }
}

// Process tree:
//
//                      preparation
//      decomposition     stage 1    stage 2       stage 3     stage 4+5
//            ┊              ┊          ┊             ┊            ┊
//            ┊  ┌────────┐  ┊          ┊             ┊  ┌───────┐ ┊  ┌───────────┐
//            ┊  │ points │─┬─>─────────┬─>───────────┬─>│ curve │─┬─>│ opt_curve │
// ┌────────┐ ┊  ├────────┤ │  ┌──────┐ │  ┌────────┐ │  └───────┘ │  └───────────┘
// │ bitmap │───>│ area   │ ├─>│ sums │─┴─>│polygons│─┘            │
// └────────┘    ├────────┤ │  ├──────┤    └────────┘              │
//               │ sign   │ └─>│ lon  │──────^                     │
//               └────────┘    └──────┘                            │
//                   └───>─────────────────────────────────────────┘

pub fn trace(
    bm: &Bitmap,
    params: Params,
    mut progress: Option<&mut dyn FnMut(f64)>,
) -> Result<Option<PathTree>, ()> {
    let sub_progress: Option<&mut dyn FnMut(f64)> = {
        if let Some(progress) = progress.as_mut() { Some(&mut |x| progress(x * 0.1)) } else { None }
    };
    let (mut paths, bm1) = decomposition::bm_to_path_list(bm, &params, sub_progress)?;

    let total_progress = paths.iter().map(|x| x.points.len()).sum::<usize>() as f64;

    let mut cur_progress = 0.0;
    for path in &mut paths {
        path.curve = process_path(&path.points, path.is_negative, &params)?;

        if let Some(progress) = progress.as_mut() {
            cur_progress += path.points.len() as f64;
            progress(cur_progress / total_progress);
        }
    }

    decomposition::path_list_to_tree(paths, bm1)
}

fn process_path(points: &[Point<usize>], is_negative: bool, params: &Params) -> Result<Curve, ()> {
    let sums = preparation::calculate_sums(points)?;
    let longest = stage1::calculate_straight_subpaths(points)?;
    let polygons = stage2::calculate_optimal_polygon(points, &sums, &longest)?;
    let mut curve = stage3::adjust_vertices(points, &sums, &polygons)?;

    if is_negative {
        // Reverse orientation of negative paths
        curve.segments.reverse();
    }
    stage4::smooth_curve(&mut curve, params.alpha_max);

    if let Some(opt_tolerance) = params.opt_tolerance {
        stage5::optimize_curve(&curve, opt_tolerance)
    } else {
        Ok(curve)
    }
}
