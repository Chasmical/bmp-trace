/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Bitmap, Curve, Params, Path, Point, TurnPolicy, Word};

/// Decomposition: decompose the given bitmap into a list of paths.
///
pub fn bm_to_path_list(
    bm: &Bitmap,
    params: &Params,
    mut progress: Option<&mut dyn FnMut(f64)>,
) -> Result<Vec<Path>, ()> {
    let mut bm1 = bm.clone();

    // Ensure that the byte padding on the right is set to 0, as the fast pixel search relies on it.
    // TODO: may be unnecessary due to the allocated bitmap words being 0 by default.
    bm1.clear_excess();

    let mut paths = Vec::new();

    // Iterate through path components:
    let mut xp = 0;
    let mut yp = bm1.height - 1;
    while let Some((x, y)) = find_next(&bm1, xp, yp) {
        xp = x;
        yp = y;

        // Calculate the sign by looking at the original
        let is_negative = unsafe { !bm.get_unchecked(x, y) };

        // Calculate the path points
        let path = find_path(&bm1, x, y + 1, is_negative, params.turn_policy)?;

        // Update buffered image
        xor_path(&mut bm1, &path.points);

        // If it's not a turd, append it to the list
        if path.area > params.turd_size {
            paths.push(path);
        }

        if let Some(progress) = progress.as_mut() {
            progress((1.0 - y as f64) / bm1.height as f64);
        }
    }

    if let Some(progress) = progress {
        progress(1.0);
    }

    Ok(paths)
}

/// Finds the next set pixel in a row <= y. Pixels are searched first left-to-right, then top-down.
/// In other words, (x,y)<(x',y') if y>y' or y=y' and x<x'. If found, returns the coordinates of the
/// found pixel. Note that this function assumes that the excess bytes have been cleared with
/// [`Bitmap::clear_excess`].
fn find_next(bm: &Bitmap, xp: usize, yp: usize) -> Option<(usize, usize)> {
    let mut x0 = xp & !(Word::BITS - 1);
    for y in (0..=yp).rev() {
        let mut x = x0;
        while x < bm.width {
            if unsafe { bm.word_unchecked(x, y).0 } != 0 {
                while unsafe { !bm.get_unchecked(x, y) } {
                    x += 1;
                }
                return Some((x, y));
            }
            x += Word::BITS;
        }
        x0 = 0;
    }
    None
}

/// Computes a path in the given bitmap, separating black from white. Starts a path at the point
/// (x0,y0), which must be an upper left corner of the path. Also computes and returns the area
/// enclosed by the path ([`Path::area`]). Note that a legitimate path cannot have length 0.
/// Sign is required for correct interpretation of turn policies.
fn find_path(
    bm: &Bitmap,
    x0: usize,
    y0: usize,
    is_negative: bool,
    turn_policy: TurnPolicy,
) -> Result<Path, ()> {
    let mut x = x0 as isize;
    let mut y = y0 as isize;
    let mut dirx: isize = 0;
    let mut diry: isize = -1;
    let mut area: isize = 0;

    // TODO: panics on OOM instead of erroring
    let mut points = Vec::new();

    loop {
        points.push(Point { x, y });

        // Move to the next point
        x += dirx;
        y += diry;
        area += x * diry;

        // Stop when we returned to the starting point (completed the path)
        if x == x0 as isize && y == y0 as isize {
            break;
        }

        // Determine the next direction
        let c = unsafe {
            bm.get_unchecked(
                (x + (dirx + diry - 1) / 2) as usize,
                (y + (diry - dirx - 1) / 2) as usize,
            )
        };
        let d = unsafe {
            bm.get_unchecked(
                (x + (dirx - diry - 1) / 2) as usize,
                (y + (diry + dirx - 1) / 2) as usize,
            )
        };

        if c && !d {
            // Ambiguous turn
            let turn_right = match turn_policy {
                TurnPolicy::Left => false,
                TurnPolicy::Right => true,
                TurnPolicy::Black => !is_negative,
                TurnPolicy::White => is_negative,
                TurnPolicy::Random => detrand(x, y),
                TurnPolicy::Majority => majority(bm, x, y),
                TurnPolicy::Minority => !majority(bm, x, y),
            };
            if turn_right {
                (dirx, diry) = (diry, -dirx); // Right turn
            } else {
                (dirx, diry) = (-diry, dirx); // Left turn
            }
        } else if c {
            (dirx, diry) = (diry, -dirx); // Right turn
        } else if !d {
            (dirx, diry) = (-diry, dirx); // Left turn
        }
    }

    // Create a Path from the collected points
    Ok(Path { points, is_negative, area: area as usize, curve: Curve::default() })
}

/// Deterministically and efficiently hash (x,y) into a pseudo-random bit.
fn detrand(x: isize, y: isize) -> bool {
    const T: [bool; 256] = [
        // Non-linear sequence: constant term of inverse in GF(8), mod x^8+x^4+x^3+x+1
        false, true, true, false, true, false, true, true, false, true, true, false, false, true,
        true, true, false, false, false, true, true, true, false, true, false, true, true, false,
        true, false, false, false, false, false, false, true, true, true, false, true, true, false,
        false, true, false, false, false, false, false, true, false, false, true, true, false,
        false, false, true, false, true, true, true, true, true, true, false, true, true, true,
        true, true, true, true, false, true, true, false, true, true, true, true, false, true,
        false, false, false, true, true, false, false, false, false, true, false, true, true,
        false, false, true, true, true, false, false, true, false, true, true, true, true, true,
        true, true, true, true, true, true, false, true, false, false, false, false, false, false,
        true, false, true, false, true, false, true, false, false, true, false, false, true, false,
        true, true, true, false, true, false, false, false, false, true, false, false, false,
        false, false, false, true, false, true, false, true, false, true, false, false, true, true,
        false, true, false, false, false, false, false, false, true, false, false, false, false,
        true, true, true, true, false, true, true, false, false, true, true, false, false, true,
        true, false, true, true, false, false, false, true, true, true, true, false, true, false,
        false, false, false, true, false, true, true, true, false, false, false, true, false, true,
        true, false, false, true, true, true, false, true, false, false, true, true, false, false,
        true, true, true, false, false, true, true, true, false, false, false, false, true, false,
        true, false, true, false, true, false, true, false,
    ];

    // 0x04b3e375 and 0x05a8ef93 are chosen to contain every possible 5-bit sequence
    let z = ((0x04b3e375 * x as usize) ^ y as usize) * 0x05a8ef93;
    T[z & 0xff] ^ T[(z >> 8) & 0xff] ^ T[(z >> 16) & 0xff] ^ T[(z >> 24) & 0xff]
}

/// Returns the "majority" value of the bitmap at intersection (x,y).
/// We assume that the bitmap is balanced at "radius" 1.
fn majority(bm: &Bitmap, x: isize, y: isize) -> bool {
    for i in 2..5isize {
        // Check at "radius" i
        let mut ct = 0;
        for a in (-i + 1)..=(i - 1) {
            ct += if bm.get((x + a) as usize, (y + i - 1) as usize).unwrap() { 1 } else { -1 };
            ct += if bm.get((x + i - 1) as usize, (y + a - 1) as usize).unwrap() { 1 } else { -1 };
            ct += if bm.get((x + a - 1) as usize, (y + i) as usize).unwrap() { 1 } else { -1 };
            ct += if bm.get((x - i) as usize, (y + a) as usize).unwrap() { 1 } else { -1 };
        }
        if ct > 0 {
            return true;
        }
        if ct < 0 {
            return false;
        }
    }
    false
}

/// XORs the given bitmap with the interior of the given path.
/// Note: the path must be within the dimensions of the bitmap.
fn xor_path(bm: &mut Bitmap, points: &[Point<isize>]) {
    let mut y1 = points.last().unwrap().y;
    let xa = points[0].x & Word::BITS.wrapping_neg() as isize;

    for &Point { x, y } in points {
        if y != y1 {
            // Efficiently invert the rectangle [x,xa] x [y,y1]
            xor_to_ref(bm, x as usize, y.min(y1) as usize, xa as usize);
            y1 = y;
        }
    }
}

/// Efficiently inverts bits `[x,infty)` and `[xa,infty)` in line `y`.
/// Here `xa` must be a multiple of [`Word::BITS`].
fn xor_to_ref(bm: &mut Bitmap, x: usize, y: usize, xa: usize) {
    debug_assert!(xa.is_multiple_of(Word::BITS));

    let xhi = x & Word::BITS.wrapping_neg();
    let xlo = x % Word::BITS;

    // Batch-XOR entire words
    let mut i = xhi.min(xa);
    let max = xhi.max(xa);
    while i < max {
        unsafe { bm.word_unchecked_mut(i, y).0 ^= Word::ALL_BITS.0 };
        i += Word::BITS;
    }

    // XOR the last one word if needed
    if xlo != 0 {
        unsafe { bm.word_unchecked_mut(xhi, y).0 ^= Word::ALL_BITS.0 << (Word::BITS - xlo) };
    }
}
