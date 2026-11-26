/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{
    tracing::{Params, TurnPolicy},
    types::{Bitmap, Curve, Path, PathTree, Point, Word},
};
use std::mem::ManuallyDrop;

/// Decomposition: decompose the given bitmap into a list of paths.
///
pub fn bm_to_path_list(
    bm: &Bitmap,
    params: &Params,
    mut progress: Option<&mut dyn FnMut(f64)>,
) -> Result<(Vec<Path>, Bitmap), ()> {
    let mut bm1 = bm.clone();

    // Ensure that the byte padding on the right is set to 0, as the fast pixel search relies on it.
    // TODO: may be unnecessary due to the allocated bitmap words being 0 by default.
    bm1.clear_excess();

    let mut paths = Vec::new();

    // Iterate through path components:
    let mut xp = 0;
    let mut yp = bm1.height() - 1;
    while let Some((x, y)) = find_next(&bm1, xp, yp) {
        xp = x;
        yp = y;

        // Calculate the sign by looking at the original
        let is_negative = !bm.get(x, y).unwrap_or(false);

        // Calculate the path points
        let path = find_path(&bm1, x, y + 1, is_negative, params.turn_policy)?;

        // Update buffered image
        xor_path(&mut bm1, &path.points);

        // If it's not a turd, append it to the list
        if path.area > params.turd_size {
            paths.push(path);
        }

        if let Some(progress) = progress.as_mut() {
            progress((1.0 - y as f64) / bm1.height() as f64);
        }
    }

    if let Some(progress) = progress {
        progress(1.0);
    }

    Ok((paths, bm1))
}

/// Gives a tree structure to the given path list, based on "insideness" testing, i.e. path A is
/// considered "below" path B if it is inside path B. The input pathlist is assumed to be ordered
/// so that "outer" paths occur before "inner" paths. The tree structure is stored in the
/// "childlist" and "sibling" components of the path_t structure. The linked list structure is also
/// changed so that negative path components are listed immediately after their positive parent.
/// Note: some backends may ignore the tree structure, others may use it e.g. to group path
/// components. We assume that in the input, point 0 of each path is an "upper left" corner of the
/// path, as returned by bm_to_path_list. This makes it easy to find an "interior" point. The bm
/// argument should be a bitmap of the correct size (large enough to hold all the paths), and will
/// be used as scratch space.
pub fn path_list_to_tree(paths: Vec<Path>, mut bm: Bitmap) -> Result<Option<PathTree>, ()> {
    bm.fill(false);

    let mut plist: *mut PathTree = std::ptr::null_mut();
    unsafe {
        let mut prev: *mut PathTree = std::ptr::null_mut();

        // Save original "next" pointers
        for path in paths {
            let boxed = Box::into_raw(Box::new(PathTree::new(path)));
            if let Some(p) = prev.as_mut() {
                p.next = boxed;
                p.next_sibling = boxed;
            } else {
                plist = boxed;
            }
            prev = boxed;
        }
        // Note: the Vec<Path> is dropped here, but the Path data was moved into boxes, which were
        // leaked, and now we have a plist, pointing at the first element in the linked list.

        let mut heap = plist;

        // The heap holds a list of lists of paths. Use "childlist" field for outer list,
        // "next" field for inner list. Each of the sublists is to be turned into a tree.
        // This code is messy, but it is actually fast. Each path is rendered exactly once.
        // We use the heap to get a tail recursive algorithm: the heap holds a list of pathlists
        // which still need to be transformed.

        while !heap.is_null() {
            // Unlink first sublist
            let mut cur = heap;
            heap = (&*heap).first_child;
            (&mut *cur).first_child = std::ptr::null_mut();

            // Unlink first path
            let head = cur;
            cur = (&*cur).next;
            (&mut *head).next = std::ptr::null_mut();

            // Render path
            xor_path(&mut bm, &(&*head).path.points);
            let bbox = set_bbox_path(&(&*head).path.points);

            // Now do insideness test for each element of cur; append it to (&mut *head).childlist
            // if it's inside head, else append it to (&mut *head).next.
            let mut hook_in = &mut (&mut *head).first_child;
            let mut hook_out = &mut (&mut *head).next;

            let mut p_next = cur;
            while let Some(p) = p_next.as_mut() {
                cur = p.next;
                p.next = std::ptr::null_mut();

                if p.path.points[0].y <= bbox.y0 {
                    p.next = *hook_out;
                    *hook_out = p;
                    hook_out = &mut p.next;
                    // Append the remainder of the list to hook_out
                    *hook_out = cur;
                    break;
                }
                if bm.get(p.path.points[0].x, p.path.points[0].y - 1).unwrap_or(false) {
                    p.next = *hook_in;
                    *hook_in = p;
                    hook_in = &mut p.next;
                } else {
                    p.next = *hook_out;
                    *hook_out = p;
                    hook_out = &mut p.next;
                }

                p_next = cur;
            }

            // Clear bm
            clear_bm_with_bbox(&mut bm, bbox);

            // Now schedule (&*head).childlist and (&*head).next for further processing
            if let Some(next) = (&*head).next.as_mut() {
                next.first_child = heap;
                heap = next;
            }
            if let Some(first_child) = (&*head).first_child.as_mut() {
                first_child.first_child = heap;
                heap = first_child;
            }
        }

        // Copy sibling structure from "next" to "sibling" component
        let mut p_next = plist;
        while let Some(p) = p_next.as_mut() {
            let p1 = p.next_sibling;
            p.next_sibling = p.next;
            p_next = p1;
        }

        // Reconstruct a new linked list ("next") structure from tree ("childlist", "sibling")
        // structure. This code is slightly messy, because we use a heap to make it tail recursive:
        // the heap contains a list of childlists which still need to be processed.
        heap = plist;
        if let Some(heap) = heap.as_mut() {
            heap.next = std::ptr::null_mut(); // heap is a linked list of childlists
        }
        plist = std::ptr::null_mut();
        let mut plist_hook = &mut plist;

        let mut heap_next = heap;
        while let Some(heap) = heap_next.as_mut() {
            let mut heap1 = heap.next;

            let mut p_next = heap_next;
            while let Some(p) = p_next.as_mut() {
                // p is a positive path, append to linked list
                p.next = *plist_hook;
                *plist_hook = p;
                plist_hook = &mut p.next;

                // Go through its children
                let mut p1_next = p.first_child;
                while let Some(p1) = p1_next.as_mut() {
                    // Append to linked list
                    p1.next = *plist_hook;
                    *plist_hook = p1;
                    plist_hook = &mut p1.next;
                    // Append its childlist to heap, if non-empty
                    if let Some(first_child) = p1.first_child.as_mut() {
                        let mut _hook = &mut heap1;
                        while !(*_hook).is_null() {
                            _hook = &mut (&mut *(*_hook)).next;
                        }
                        first_child.next = *_hook;
                        *_hook = first_child;
                    }
                    p1_next = p1.next_sibling;
                }
                p_next = p.next_sibling;
            }
            heap_next = heap1;
        }
    }

    // Re-create the box for the linked list's first element, and drop only the box,
    // keeping the contents (which is the rest of the linked list) in their boxes.
    if let Some(plist) = unsafe { plist.cast::<ManuallyDrop<PathTree>>().as_mut() } {
        let boxed = unsafe { Box::<ManuallyDrop<PathTree>>::from_raw(plist) };
        Ok(Some(ManuallyDrop::into_inner(*boxed)))
    } else {
        Ok(None)
    }
}

/// Finds the next set pixel in a row <= y. Pixels are searched first left-to-right, then top-down.
/// In other words, (x,y)<(x',y') if y>y' or y=y' and x<x'. If found, returns the coordinates of the
/// found pixel. Note that this function assumes that the excess bytes have been cleared with
/// [`Bitmap::clear_excess`].
fn find_next(bm: &Bitmap, xp: usize, yp: usize) -> Option<(usize, usize)> {
    let mut x0 = xp & Word::BITS.wrapping_neg();
    for y in (0..=yp).rev() {
        let mut x = x0;
        while x < bm.width() {
            if unsafe { bm.word_unchecked(x, y).0 } != 0 {
                while !bm.get(x, y).unwrap_or(false) {
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

    let mut points = Vec::new();

    loop {
        // All points' coordinates should be non-negative
        debug_assert!(x >= 0 && y >= 0);

        points.try_reserve(1).or(Err(()))?;
        points.push(Point { x: x as usize, y: y as usize });

        // Move to the next point
        x += dirx;
        y += diry;
        area += x * diry;

        // Stop when we returned to the starting point (completed the path)
        if x as usize == x0 && y as usize == y0 {
            break;
        }

        // Determine the next direction
        let c = bm
            .get((x + (dirx + diry - 1) / 2) as usize, (y + (diry - dirx - 1) / 2) as usize)
            .unwrap_or(false);
        let d = bm
            .get((x + (dirx - diry - 1) / 2) as usize, (y + (diry + dirx - 1) / 2) as usize)
            .unwrap_or(false);

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
            fn get(bm: &Bitmap, x: isize, y: isize) -> bool {
                bm.get(x as usize, y as usize).unwrap_or(false)
            }

            ct += if get(bm, x + a, y + i - 1) { 1 } else { -1 };
            ct += if get(bm, x + i - 1, y + a - 1) { 1 } else { -1 };
            ct += if get(bm, x + a - 1, y + i) { 1 } else { -1 };
            ct += if get(bm, x - i, y + a) { 1 } else { -1 };
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
fn xor_path(bm: &mut Bitmap, points: &[Point<usize>]) {
    let mut y1 = points.last().unwrap().y;
    let xa = points[0].x & Word::BITS.wrapping_neg();

    for &Point { x, y } in points {
        if y != y1 {
            // Efficiently invert the rectangle [x,xa] x [y,y1]
            xor_to_ref(bm, x, y.min(y1), xa);
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

#[derive(Debug, Default, Clone, Copy)]
struct Bbox {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

/// Finds the bounding box of a given path. Path is assumed to be of non-zero length.
fn set_bbox_path(points: &[Point<usize>]) -> Bbox {
    debug_assert!(!points.is_empty());

    let mut bbox = Bbox { y0: usize::MAX, y1: 0, x0: usize::MAX, x1: 0 };

    for &Point { x, y } in points {
        bbox.x0 = bbox.x0.min(x);
        bbox.x1 = bbox.x1.max(x);
        bbox.y0 = bbox.y0.min(y);
        bbox.y1 = bbox.y1.max(y);
    }

    bbox
}

/// Clears the bitmap, assuming the bounding box is set correctly
/// (faster than clearing the whole bitmap).
fn clear_bm_with_bbox(bm: &mut Bitmap, bbox: Bbox) {
    let imin = bbox.x0 / Word::BITS;
    let imax = bbox.x1.div_ceil(Word::BITS);

    for y in bbox.y0..bbox.y1 {
        for i in imin..imax {
            unsafe { bm.word_unchecked_mut(i * Word::BITS, y).0 = 0 };
        }
    }
}
