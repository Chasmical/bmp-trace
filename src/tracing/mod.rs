/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Bitmap, Path};

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

pub fn trace(
    bm: &Bitmap,
    params: Params,
    mut progress: Option<&mut dyn FnMut(f64)>,
) -> Result<Vec<Path>, ()> {
    let sub_progress: Option<&mut dyn FnMut(f64)> = {
        if let Some(progress) = progress.as_mut() { Some(&mut |x| progress(x * 0.1)) } else { None }
    };
    let mut paths = decomposition::bm_to_path_list(bm, params, sub_progress)?;

    let total_progress = paths.iter().map(|x| x.len()).sum::<usize>() as f64;

    let mut cur_progress = 0.0;
    for path in &mut paths {
        process_path(path, params)?;

        if let Some(progress) = progress.as_mut() {
            cur_progress += path.len() as f64;
            progress(cur_progress / total_progress);
        }
    }

    Ok(paths)
}

fn process_path(p: &mut Path, params: Params) -> Result<(), ()> {
    preparation::calc_sums(p)?;
    stage1::calc_lon(p)?;
    stage2::best_polygon(p)?;
    stage3::adjust_vertices(p)?;

    if p.is_negative {
        /* reverse orientation of negative paths */
        p.curve.segments.reverse();
    }
    stage4::smooth(&mut p.curve, params.alpha_max);

    if params.opt_curves {
        stage5::opticurve(p, params.opt_tolerance)?;
    }

    Ok(())
}
