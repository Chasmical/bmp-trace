/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{Bitmap, Curve, Params, Path, Point, TurnPolicy, Word};

/* Decompose the given bitmap into paths. Returns a linked list of
path_t objects with the fields len, pt, area, sign filled
in. Returns 0 on success with plistp set, or -1 on error with errno
set. */
pub fn bm_to_path_list(
    bm: &Bitmap,
    params: Params,
    mut progress: Option<&mut dyn FnMut(f64)>,
) -> Result<Vec<Path>, ()> {
    let mut bm1 = bm.clone();

    /* be sure the byte padding on the right is set to 0, as the fast
    pixel search below relies on it */
    bm1.clear_excess();

    // plist is a Vec instead of LinkedList
    let mut plist = Vec::new();

    /* iterate through components */
    let mut xp = 0;
    let mut yp = bm1.height - 1;
    while let Some((x, y)) = find_next(&bm1, xp, yp) {
        xp = x;
        yp = y;

        /* calculate the sign by looking at the original */
        let is_negative = !bm.get(x, y).unwrap();

        /* calculate the path */
        let p = find_path(&bm1, x, y + 1, is_negative, params.turn_policy)?;

        /* update buffered image */
        xor_path(&mut bm1, &p.pt);

        /* if it's a turd, eliminate it, else append it to the list */
        if p.area <= params.turd_size {
        } else {
            plist.push(p);
        }

        if let Some(progress) = progress.as_mut()
            && bm1.height > 0
        {
            progress((1.0 - y as f64) / bm1.height as f64);
        }
    }

    // TODO: list to tree

    if let Some(progress) = progress {
        progress(1.0);
    }

    Ok(plist)
}

/* find the next set pixel in a row <= y. Pixels are searched first
left-to-right, then top-down. In other words, (x,y)<(x',y') if y>y'
or y=y' and x<x'. If found, return 0 and store pixel in
(*xp,*yp). Else return 1. Note that this function assumes that
excess bytes have been cleared with bm_clearexcess. */
fn find_next(bm: &Bitmap, xp: usize, yp: usize) -> Option<(usize, usize)> {
    let mut x0 = xp & !(Word::BITS - 1);
    for y in (0..=yp).rev() {
        let mut x = x0;
        while x < bm.width {
            if bm.word(x, y).unwrap().0 != 0 {
                while !bm.get(x, y).unwrap() {
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

/* compute a path in the given pixmap, separating black from white.
Start path at the point (x0,x1), which must be an upper left corner
of the path. Also compute the area enclosed by the path. Return a
new path_t object, or NULL on error (note that a legitimate path
cannot have length 0). Sign is required for correct interpretation
of turnpolicies. */
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
    let mut pt = Vec::new();

    loop {
        /* add point to path */
        pt.push(Point { x, y });

        /* move to next point */
        x += dirx;
        y += diry;
        area += x * diry;

        /* path complete? */
        if x == x0 as isize && y == y0 as isize {
            break;
        }

        /* determine next direction */
        let c = bm
            .get((x + (dirx + diry - 1) / 2) as usize, (y + (diry - dirx - 1) / 2) as usize)
            .unwrap();
        let d = bm
            .get((x + (dirx - diry - 1) / 2) as usize, (y + (diry + dirx - 1) / 2) as usize)
            .unwrap();

        if c && !d {
            /* ambiguous turn */
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
                /* right turn */
                let tmp = dirx;
                dirx = diry;
                diry = -tmp;
            } else {
                /* left turn */
                let tmp = dirx;
                dirx = -diry;
                diry = tmp;
            }
        } else if c {
            /* right turn */
            let tmp = dirx;
            dirx = diry;
            diry = -tmp;
        } else if !d {
            /* left turn */
            let tmp = dirx;
            dirx = -diry;
            diry = tmp;
        }
    }

    /* allocate new path object */
    Ok(Path { pt, area: area as usize, is_negative, curve: Curve::default() })
}

/* deterministically and efficiently hash (x,y) into a pseudo-random bit */
fn detrand(x: isize, y: isize) -> bool {
    const T: [bool; 256] = [
        /* non-linear sequence: constant term of inverse in GF(8), mod x^8+x^4+x^3+x+1 */
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

    /* 0x04b3e375 and 0x05a8ef93 are chosen to contain every possible 5-bit sequence */
    let z = ((0x04b3e375 * x as usize) ^ y as usize) * 0x05a8ef93;
    T[z & 0xff] ^ T[(z >> 8) & 0xff] ^ T[(z >> 16) & 0xff] ^ T[(z >> 24) & 0xff]
}

/* return the "majority" value of bitmap bm at intersection (x,y). We
assume that the bitmap is balanced at "radius" 1.  */
fn majority(bm: &Bitmap, x: isize, y: isize) -> bool {
    for i in 2..5isize {
        /* check at "radius" i */
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

/* a path is represented as an array of points, which are thought to
lie on the corners of pixels (not on their centers). The path point
(x,y) is the lower left corner of the pixel (x,y). Paths are
represented by the len/pt components of a path_t object (which
also stores other information about the path) */

/* xor the given pixmap with the interior of the given path. Note: the
path must be within the dimensions of the pixmap. */
fn xor_path(bm: &mut Bitmap, pt: &[Point<isize>]) {
    if pt.is_empty() {
        /* a path of length 0 is silly, but legal */
        return;
    }

    let mut y1 = pt.last().unwrap().y;
    let xa = pt[0].x & Word::BITS.wrapping_neg() as isize;

    for &pt in pt {
        if pt.y != y1 {
            /* efficiently invert the rectangle [x,xa] x [y,y1] */
            xor_to_ref(bm, pt.x as usize, pt.y.min(y1) as usize, xa as usize);
            y1 = pt.y;
        }
    }
}

/* efficiently invert bits [x,infty) and [xa,infty) in line y. Here xa
must be a multiple of BM_WORDBITS. */
fn xor_to_ref(bm: &mut Bitmap, x: usize, y: usize, xa: usize) {
    let xhi = x & Word::BITS.wrapping_neg();
    let xlo = x & (Word::BITS - 1); /* = x % BM_WORDBITS */

    if xhi < xa {
        let mut i = xhi;
        while i < xa {
            bm.word_mut(i, y).unwrap().0 ^= Word::ALL_BITS.0;
            i += Word::BITS;
        }
    } else {
        let mut i = xa;
        while i < xhi {
            bm.word_mut(i, y).unwrap().0 ^= Word::ALL_BITS.0;
            i += Word::BITS;
        }
    }

    /* note: the following "if" is needed because x86 treats a<<b as
    a<<(b&31). I spent hours looking for this bug. */
    if xlo != 0 {
        bm.word_mut(xhi, y).unwrap().0 ^= Word::ALL_BITS.0 << (Word::BITS - xlo);
    }
}
