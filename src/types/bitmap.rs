/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::util;
use std::ptr::NonNull;

/// Packed bitmap data.
pub struct Bitmap {
    width: usize,       // int w
    height: usize,      // int h
    dy: isize,          // int dy
    ptr: NonNull<Word>, // potrace_word* map
}

impl Bitmap {
    /// Tries to create a packed bitmap of specified size. Returns `Err` on allocation errors.
    pub fn try_new(width: usize, height: usize) -> Result<Self, ()> {
        let dy = width.div_ceil(Word::BITS);
        let ptr = util::alloc_default_slice::<Word>(dy * height)?;
        let ptr = Box::into_non_null(ptr).cast();
        Ok(Self { width, height, dy: dy as isize, ptr })
    }

    /// Returns the width of the bitmap in bits/pixels.
    pub fn width(&self) -> usize {
        self.width
    }
    /// Returns the height of the bitmap in bits/pixels.
    pub fn height(&self) -> usize {
        self.height
    }
    /// Returns the size of the bitmap in bytes.
    pub fn byte_size(&self) -> usize {
        size_of_val(self.as_raw_words())
    }
    /// Returns whether the bitmap is physically flipped in memory.
    pub fn is_flipped(&self) -> bool {
        self.dy < 0
    }

    /// Returns the bitmap's backing array. Order of scanlines is inverted if the bitmap is flipped.
    fn as_slice_ptr(&self) -> *mut [Word] {
        let mut ptr = self.ptr.as_ptr();
        if self.dy < 0 && self.height != 0 {
            ptr = ptr.wrapping_offset((self.height - 1) as isize * self.dy);
        }
        std::ptr::slice_from_raw_parts_mut(ptr, self.dy.unsigned_abs() * self.height)
    }

    /// Returns the bitmap's raw data array as a slice.
    /// The order of words' rows is inverted, if the bitmap is flipped.
    pub fn as_raw_words(&self) -> &[Word] {
        unsafe { &*self.as_slice_ptr() }
    }
    /// Returns the bitmap's raw data array as a mutable slice.
    /// The order of words' rows is inverted, if the bitmap is flipped.
    pub fn as_raw_words_mut(&mut self) -> &mut [Word] {
        unsafe { &mut *self.as_slice_ptr() }
    }

    /// Flips the bitmap upside down. This does not move or reallocate the bitmap data.
    pub fn flip(&mut self) {
        if self.height <= 1 {
            return;
        }
        self.ptr = unsafe { self.ptr.offset((self.height as isize - 1) * self.dy) };
        self.dy = -self.dy;
    }

    /// Tries to clone this bitmap. Returns `Err` on allocation errors.
    pub fn try_clone(&self) -> Result<Self, ()> {
        // TODO: this keeps the copy flipped, which may be unwanted. Potrace makes it non-flipped.

        let src = unsafe { &*self.as_slice_ptr() };

        // Create a Box of the same size and copy the data into it
        let mut clone = util::alloc_uninit_slice::<Word>(src.len())?;
        clone.write_copy_of_slice(src);
        let mut ptr = Box::into_non_null(clone).cast::<Word>();

        // If this was flipped, flip the copy's pointer too
        if self.dy < 0 && self.height != 0 {
            ptr = unsafe { ptr.offset((self.height - 1) as isize * -self.dy) };
        }
        Ok(Self { ptr, ..*self })
    }

    /// Resizes the bitmap to specified height. If the new height is less than the current height,
    /// truncates the bitmap data, keeping the side that's first in memory (this uses `realloc`).
    /// So if the bitmap is flipped, the bottom part of the bitmap is kept instead of the top.
    /// If the height is larger, the new bitmap data is set to `0`/`false`.
    ///
    /// Intended for use by [`BmpReader`] to discard the unparsed part of the image.
    ///
    /// ```text
    ///     image      in-memory          resized     in-memory
    ///
    ///   ██░░░░░░░░   ██░░░░░░░░  >>>>  ██░░░░░░░░   ██░░░░░░░░
    ///   ░░██░░░░░░   ░░██░░░░░░  >>>>  ░░██░░░░░░   ░░██░░░░░░
    ///   ░░░░██░░░░   ░░░░██░░░░  >>>>  ░░░░██░░░░   ░░░░██░░░░
    ///   ░░░░░░██░░   ░░░░░░██░░
    ///   ░░░░░░░░██   ░░░░░░░░██
    ///
    ///     image      in-memory          resized     in-memory
    ///    flipped                        flipped
    ///   ░░░░░░░░██   ██░░░░░░░░  >>>>  ░░░░██░░░░   ██░░░░░░░░
    ///   ░░░░░░██░░   ░░██░░░░░░  >>>>  ░░██░░░░░░   ░░██░░░░░░
    ///   ░░░░██░░░░   ░░░░██░░░░  >>>>  ██░░░░░░░░   ░░░░██░░░░
    ///   ░░██░░░░░░   ░░░░░░██░░
    ///   ██░░░░░░░░   ░░░░░░░░██
    /// ```
    ///
    /// [`BmpReader`]: crate::types::BmpReader
    pub fn try_resize_mem_aligned(&mut self, new_height: usize) -> Result<(), ()> {
        let was_flipped = self.dy < 0;
        if was_flipped {
            self.flip();
        }

        // Create a Vec from the current ptr and size (we'll use it to reallocate the data)
        let old_size = self.dy.unsigned_abs() * self.height;
        let mut v = unsafe { Vec::from_raw_parts(self.ptr.as_ptr(), old_size, old_size) };

        // Change the Vec's size (filling new slots with zeroes)
        let new_size = self.dy.unsigned_abs() * new_height;
        v.resize(new_size, Word::default());

        // Vec::into_boxed_slice() calls realloc with the new length
        let ptr = Box::into_non_null(v.into_boxed_slice()).cast();

        self.ptr = ptr;
        self.height = new_height;

        if was_flipped {
            self.flip();
        }
        Ok(())
    }

    /// Sets all bits in the bitmap to specified value.
    ///
    /// Also sets excess bits that don't actually represent the image, which may interfere with
    /// some algorithms that use the bitmap's backing array directly. Use [`Bitmap::clear_excess`]
    /// to clear the excess bits before applying those algorithms.
    pub fn fill(&mut self, b: bool) {
        unsafe { &mut *self.as_slice_ptr() }.fill(if b { Word::ALL_BITS } else { Word(0) });
    }
    /// Sets the excess bits in the backing slice to `0`/`false`.
    pub fn clear_excess(&mut self) {
        if !self.width.is_multiple_of(Word::BITS) {
            let mask = Word::ALL_BITS.0 << (Word::BITS - (self.width % Word::BITS));
            for y in 0..self.height {
                unsafe { self.word_unchecked_mut(self.width, y).0 &= mask };
            }
        }
    }

    /// Returns the word at specified X and Y coordinates.
    pub fn word(&self, x: usize, y: usize) -> Option<&Word> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(unsafe { self.word_unchecked(x, y) })
    }
    /// Returns the word at specified X and Y coordinates, without bounds-checking.
    pub unsafe fn word_unchecked(&self, x: usize, y: usize) -> &Word {
        unsafe { self.ptr.offset(y as isize * self.dy).add(x / Word::BITS).as_ref() }
    }

    /// Returns the word at specified X and Y coordinates.
    pub fn word_mut(&mut self, x: usize, y: usize) -> Option<&mut Word> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(unsafe { self.word_unchecked_mut(x, y) })
    }
    /// Returns the word at specified X and Y coordinates, without bounds-checking.
    pub unsafe fn word_unchecked_mut(&mut self, x: usize, y: usize) -> &mut Word {
        unsafe { self.ptr.offset(y as isize * self.dy).add(x / Word::BITS).as_mut() }
    }

    /// Returns the value of a pixel at X and Y coordinates.
    pub fn get(&self, x: usize, y: usize) -> Option<bool> {
        Some(self.word(x, y)?.get(x))
    }
    /// Returns the value of a pixel at X and Y coordinates, without bounds-checking.
    pub unsafe fn get_unchecked(&self, x: usize, y: usize) -> bool {
        unsafe { self.word_unchecked(x, y) }.get(x)
    }

    /// Tries to set the value of a pixel at X coordinate to specified value.
    pub fn try_put(&mut self, x: usize, y: usize, b: bool) {
        if let Some(w) = self.word_mut(x, y) {
            w.put(x, b);
        }
    }
    /// Sets the value of a pixel at X coordinate to specified value, without bounds-checking.
    pub unsafe fn put_unchecked(&mut self, x: usize, y: usize, b: bool) {
        unsafe { self.word_unchecked_mut(x, y) }.put(x, b);
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        let boxed = unsafe { Box::from_raw(self.as_slice_ptr()) };
        self.ptr = NonNull::dangling();
        self.height = 0;
        drop(boxed);
    }
}
impl Clone for Bitmap {
    fn clone(&self) -> Self {
        self.try_clone().unwrap()
    }
}

/// A Potrace word, a wrapper over [`usize`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Word(pub u32);

// TODO: Is there even any point in this struct? It's just a low-level implementation detail.
// TODO: Should words even be u32 or usize? Why not u8? Will need to benchmark different sizes.
impl Word {
    /// The size of [`Word`], in bytes.
    pub const SIZE: usize = size_of::<Self>();
    /// The size of [`Word`], in bits.
    pub const BITS: usize = Self::SIZE * 8;
    /// The [`Word`] with the highest bit set.
    pub const HIGH_BIT: Self = Self(i32::MIN.cast_unsigned());
    /// The [`Word`] with all bits set.
    pub const ALL_BITS: Self = Self(u32::MAX);

    /// Gets the bit-mask for the specified X coordinate.
    pub const fn mask_for_x(x: usize) -> Self {
        // TODO: this needs to be reworked. Right now bit 0 is the highest bit, which is weird.

        Self(Self::HIGH_BIT.0 >> (x & (Self::BITS - 1)))
    }

    /// Gets the value of a pixel in this word at X coordinate (modulo [`Self::BITS`]).
    pub const fn get(&self, x: usize) -> bool {
        (self.0 & Self::mask_for_x(x).0) != 0
    }
    /// Sets the value of a pixel in this word at X coordinate (modulo [`Self::BITS`]) to 1.
    pub const fn set(&mut self, x: usize) {
        self.0 |= Self::mask_for_x(x).0;
    }
    /// Sets the value of a pixel in this word at X coordinate (modulo [`Self::BITS`]) to 0.
    pub const fn clear(&mut self, x: usize) {
        self.0 &= !Self::mask_for_x(x).0;
    }
    /// Toggles the value of a pixel in this word at X coordinate (modulo [`Self::BITS`]).
    pub const fn toggle(&mut self, x: usize) {
        self.0 ^= Self::mask_for_x(x).0;
    }
    /// Sets the value of a pixel in this word at X coordinate (modulo [`Self::BITS`]) to `b`.
    pub const fn put(&mut self, x: usize, b: bool) {
        if b { self.set(x) } else { self.clear(x) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize() {
        let mut orig = Bitmap::try_new(5, 5).unwrap();
        (0..5).zip(0..5).for_each(|(x, y)| orig.try_put(x, y, x == y));

        // ██░░░░░░░░
        // ░░██░░░░░░
        // ░░░░██░░░░
        // ░░░░░░██░░
        // ░░░░░░░░██
        assert_eq!(orig.word(0, 0), Some(&Word::mask_for_x(0)));
        assert_eq!(orig.word(0, 1), Some(&Word::mask_for_x(1)));
        assert_eq!(orig.word(0, 2), Some(&Word::mask_for_x(2)));
        assert_eq!(orig.word(0, 3), Some(&Word::mask_for_x(3)));
        assert_eq!(orig.word(0, 4), Some(&Word::mask_for_x(4)));
        assert_eq!(orig.word(0, 5), None);

        let mut bm = orig.clone();
        bm.try_resize_mem_aligned(3).unwrap();

        // Non-flipped: keep the top part
        // ██░░░░░░░░
        // ░░██░░░░░░
        // ░░░░██░░░░
        assert_eq!(bm.word(0, 0), Some(&Word::mask_for_x(0)));
        assert_eq!(bm.word(0, 1), Some(&Word::mask_for_x(1)));
        assert_eq!(bm.word(0, 2), Some(&Word::mask_for_x(2)));
        assert_eq!(bm.word(0, 3), None);

        // ░░░░░░░░██
        // ░░░░░░██░░
        // ░░░░██░░░░
        // ░░██░░░░░░
        // ██░░░░░░░░
        let mut bm = orig.clone();
        bm.flip();
        bm.try_resize_mem_aligned(3).unwrap();

        // Flipped: keep the bottom part
        // ░░░░██░░░░
        // ░░██░░░░░░
        // ██░░░░░░░░
        assert_eq!(bm.word(0, 0), Some(&Word::mask_for_x(2)));
        assert_eq!(bm.word(0, 1), Some(&Word::mask_for_x(1)));
        assert_eq!(bm.word(0, 2), Some(&Word::mask_for_x(0)));
        assert_eq!(bm.word(0, 3), None);
    }
}
