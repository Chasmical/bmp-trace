use crate::{DPoint, PathTreeList, PrivPath};

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

fn point_slope(
    path: &mut PrivPath,
    mut i: isize,
    mut j: isize,
    ctr: &mut DPoint,
    dir: &mut DPoint,
) {
    /* assume i < j */

    let n = path.len as isize;
    let sums = &mut path.sums;

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

    // TODO: may panic on out-of-bounds
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

// TODO: progress param?
pub const fn process_path(path: &PathTreeList, params: &Params) {
    todo!()
}
