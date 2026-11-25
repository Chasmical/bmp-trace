/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{BmpReader, BmpWriter, ReadError, util};
use std::io::{Read, Seek, Write};

/// Bitmap data.
///
/// Corresponds to `potrace_bitmap_t` in Potrace.
pub struct Bitmap {
    pub(crate) width: usize,   // int w
    pub(crate) height: usize,  // int h
    pub(crate) dy: isize,      // int dy
    pub(crate) ptr: *mut Word, // potrace_word* map
}

// TODO: untangle the mess here
impl Bitmap {
    pub fn try_new(width: usize, height: usize) -> Result<Self, ()> {
        let dy = width.div_ceil(Word::BITS);
        let ptr = Box::into_raw(util::alloc_default_slice::<Word>(dy * height)?).cast();
        Ok(Self { width, height, dy: dy as isize, ptr })
    }

    pub fn len(&self) -> usize {
        self.dy.unsigned_abs() * self.height
    }
    fn as_raw_slice(&self) -> &[Word] {
        let ptr = if self.dy >= 0 || self.height == 0 {
            self.ptr.cast_const()
        } else {
            self.ptr.cast_const().wrapping_offset((self.height - 1) as isize * self.dy)
        };
        unsafe { std::slice::from_raw_parts(ptr, self.len()) }
    }
    fn take_boxed_slice(&mut self) -> Box<[Word]> {
        let boxed = unsafe { Box::from_raw((&raw const *self.as_raw_slice()).cast_mut()) };
        self.ptr = std::ptr::dangling_mut();
        self.height = 0;
        boxed
    }

    pub fn try_clone(&self) -> Result<Self, ()> {
        let mut boxed = util::alloc_uninit_slice::<Word>(self.len())?;
        boxed.write_copy_of_slice(self.as_raw_slice());
        Ok(Self { ptr: Box::into_raw(boxed).cast(), ..*self })
    }

    pub fn try_resize(&mut self, new_height: usize) -> Result<(), ()> {
        let old_words = self.take_boxed_slice();

        let mut new_words = Vec::new();
        new_words.try_reserve_exact(self.dy.unsigned_abs() * new_height).or(Err(()))?;
        new_words.extend_from_slice(&old_words[..new_words.capacity()]);

        self.ptr = Box::into_raw(new_words.into_boxed_slice()).cast();
        self.height = new_height;
        drop(old_words);

        Ok(())
    }

    pub fn flip(&mut self) {
        self.ptr = self.ptr.wrapping_offset((self.height as isize - 1) * self.dy);
        self.dy = -self.dy;
    }
    pub(crate) fn clear_excess(&mut self) {
        // TODO: the words are zeroed, so is this necessary?
    }

    pub fn read_from<R: Read + Seek>(reader: R, threshold: f64) -> Result<Self, ReadError> {
        let r = BmpReader::new(reader).read(threshold)?;
        if let Some(err) = r.1 {
            return Err(err);
        }
        Ok(r.0)
    }
    pub fn write_to<W: Write>(&self, writer: W) -> std::io::Result<()> {
        let mut w = BmpWriter::new(writer);
        w.write(self)
    }

    /// Returns the word at specified X and Y coordinates.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word(&self, x: usize, y: usize) -> Option<&Word> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(unsafe { self.word_unchecked(x, y) })
    }
    /// Returns the word at specified X and Y coordinates, without bounds-checking.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub unsafe fn word_unchecked(&self, x: usize, y: usize) -> &Word {
        unsafe { &*self.ptr.wrapping_offset(y as isize * self.dy).wrapping_add(x / Word::BITS) }
    }

    /// Returns the word at specified X and Y coordinates.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word_mut(&mut self, x: usize, y: usize) -> Option<&mut Word> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(unsafe { self.word_unchecked_mut(x, y) })
    }
    /// Returns the word at specified X and Y coordinates, without bounds-checking.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub unsafe fn word_unchecked_mut(&mut self, x: usize, y: usize) -> &mut Word {
        unsafe { &mut *self.ptr.wrapping_offset(y as isize * self.dy).wrapping_add(x / Word::BITS) }
    }

    /// Returns the value of a pixel at X and Y coordinates.
    ///
    /// Corresponds to `BM_GET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn get(&self, x: usize, y: usize) -> Option<bool> {
        Some(self.word(x, y)?.get(x))
    }
    /// Returns the value of a pixel at X and Y coordinates, without bounds-checking.
    ///
    /// Corresponds to `BM_GET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, x: usize, y: usize) -> bool {
        unsafe { self.word_unchecked(x, y) }.get(x)
    }

    /// Tries to set the value of a pixel at X coordinate to specified value.
    ///
    /// Corresponds to `BM_PUT(bm, x, y, b)` in Potrace.
    #[inline(always)]
    pub fn try_put(&mut self, x: usize, y: usize, b: bool) {
        if let Some(w) = self.word_mut(x, y) {
            w.put(x, b);
        }
    }
    /// Sets the value of a pixel at X coordinate to specified value, without bounds-checking.
    ///
    /// Corresponds to `BM_UPUT(bm, x, y, b)` in Potrace.
    #[inline(always)]
    pub unsafe fn put_unchecked(&mut self, x: usize, y: usize, b: bool) {
        unsafe { self.word_unchecked_mut(x, y) }.put(x, b);
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        drop(self.take_boxed_slice());
    }
}
impl Clone for Bitmap {
    fn clone(&self) -> Self {
        self.try_clone().unwrap()
    }
}

/// A Potrace word, a wrapper over [`usize`].
///
/// Corresponds to `potrace_word` in Potrace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Word(pub u32);

// TODO: Is there even any point in this struct? It's just a low-level implementation detail.
// TODO: Should words even be u32 or usize? Why not u8? Will need to benchmark different sizes.
impl Word {
    /// The size of [`Word`], in bytes.
    ///
    /// Corresponds to `BM_WORDSIZE` in Potrace.
    pub const SIZE: usize = size_of::<Self>();
    /// The size of [`Word`], in bits.
    ///
    /// Corresponds to `BM_WORDBITS` in Potrace.
    pub const BITS: usize = Self::SIZE * 8;
    /// The [`Word`] with the highest bit set.
    ///
    /// Corresponds to `BM_HIBIT` in Potrace.
    pub const HIGH_BIT: Self = Self(i32::MIN.cast_unsigned());
    /// The [`Word`] with all bits set.
    ///
    /// Corresponds to `BM_ALLBITS` in Potrace.
    pub const ALL_BITS: Self = Self(u32::MAX);

    /// Gets the bit-mask for the specified X coordinate.
    ///
    /// Corresponds to `bm_mask(x)` in Potrace.
    #[inline(always)]
    pub const fn mask_for_x(x: usize) -> Self {
        Self(Self::HIGH_BIT.0 >> (x & (Self::BITS - 1)))
    }

    /// Get the value of a pixel at X coordinate.
    ///
    /// Corresponds to `BM_GET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub const fn get(&self, x: usize) -> bool {
        (self.0 & Self::mask_for_x(x).0) != 0
    }
    /// Sets the value of a pixel at X coordinate to 1.
    ///
    /// Corresponds to `BM_SET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub const fn set(&mut self, x: usize) {
        self.0 |= Self::mask_for_x(x).0;
    }
    /// Sets the value of a pixel at X coordinate to 0.
    ///
    /// Corresponds to `BM_CLR(bm, x, y)` in Potrace.
    #[inline(always)]
    pub const fn clear(&mut self, x: usize) {
        self.0 &= !Self::mask_for_x(x).0;
    }
    /// Toggles the value of a pixel at X coordinate.
    ///
    /// Corresponds to `BM_INV(bm, x, y)` in Potrace.
    // CHANGE: Renamed `INV` to `toggle`.
    #[inline(always)]
    pub const fn toggle(&mut self, x: usize) {
        self.0 ^= Self::mask_for_x(x).0;
    }
    /// Sets the value of a pixel at X coordinate to specified value.
    ///
    /// Corresponds to `BM_PUT(bm, x, y, b)` in Potrace.
    #[inline(always)]
    pub const fn put(&mut self, x: usize, b: bool) {
        if b { self.set(x) } else { self.clear(x) }
    }
}
