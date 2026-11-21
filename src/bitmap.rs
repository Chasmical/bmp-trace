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
#[derive(Clone)]
pub struct Bitmap {
    pub(crate) width: usize,           // int w
    pub(crate) height: usize,          // int h
    pub(crate) dy: usize,              // int dy
    pub(crate) raw_words: Box<[Word]>, // potrace_word* map
}

impl Bitmap {
    pub fn try_new(width: usize, height: usize) -> Result<Self, ()> {
        let dy = if width == 0 { 0 } else { (width - 1) / Word::BITS + 1 };
        let raw_words = util::alloc_default_slice(dy * height)?;
        Ok(Self { width, height, dy, raw_words })
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

    pub fn resize(&mut self, new_height: usize) -> Result<(), ()> {
        let dy = if self.width == 0 { 0 } else { (self.width - 1) / Word::BITS + 1 };
        let mut new_words = Vec::new();

        new_words.try_reserve_exact(dy * new_height).or(Err(()))?;
        let src = &self.raw_words[..new_words.len()];
        new_words.spare_capacity_mut().write_copy_of_slice(src);

        self.raw_words = new_words.into_boxed_slice();

        Ok(())
    }
    pub fn flip(&mut self) {
        // TODO: words_per_scanline is unsigned atm, so bitmap flip in expensive
        self.raw_words.reverse();
    }

    /// Returns a scanline at specified Y index.
    ///
    /// Corresponds to `bm_scanline(bm, y)` in Potrace.
    #[inline(always)]
    pub fn scanline(&self, y: usize) -> Option<&Scanline> {
        let word_offset = y * self.dy;
        let line = self.raw_words.get(word_offset..word_offset + self.dy)?;
        Some(Scanline::new(line))
    }
    /// Returns a scanline at specified Y index.
    ///
    /// Corresponds to `bm_scanline(bm, y)` in Potrace.
    #[inline(always)]
    pub fn scanline_mut(&mut self, y: usize) -> Option<&mut Scanline> {
        let word_offset = y * self.dy;
        let line = self.raw_words.get_mut(word_offset..word_offset + self.dy)?;
        Some(Scanline::new_mut(line))
    }
    /// Returns the word at specified X and Y coordinates.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word(&self, x: usize, y: usize) -> Option<&Word> {
        self.scanline(y)?.word(x)
    }
    /// Returns the word at specified X and Y coordinates.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word_mut(&mut self, x: usize, y: usize) -> Option<&mut Word> {
        self.scanline_mut(y)?.word_mut(x)
    }

    /// Returns the value of a pixel at X and Y coordinates.
    ///
    /// Corresponds to `BM_GET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn get(&self, x: usize, y: usize) -> Option<bool> {
        Some(self.word(x, y)?.get(x))
    }
    /// Sets the value of a pixel at X coordinate to specified value.
    ///
    /// Corresponds to `BM_PUT(bm, x, y)` in Potrace.
    #[inline(always)]
    pub unsafe fn put_unchecked(&mut self, x: usize, y: usize, b: bool) {
        debug_assert!(self.word(x, y).is_some());
        unsafe { self.word_mut(x, y).unwrap_unchecked() }.put(x, b);
    }
}

/// A read-only reference to a slice of Potrace words, representing a bitmap row.
///
/// Corresponds to `potrace_word_t*` in Potrace.
#[repr(transparent)]
pub struct Scanline([Word]);

impl Scanline {
    pub const fn new(raw_words: &[Word]) -> &Self {
        unsafe { &*(raw_words as *const [Word] as *const Self) }
    }
    pub const fn new_mut(raw_words: &mut [Word]) -> &mut Self {
        unsafe { &mut *(raw_words as *mut [Word] as *mut Self) }
    }

    /// Returns the word at specified X coordinate.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word(&self, x: usize) -> Option<&Word> {
        self.0.get(x / Word::BITS)
    }
    /// Returns the word at specified X coordinate.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word_mut(&mut self, x: usize) -> Option<&mut Word> {
        self.0.get_mut(x / Word::BITS)
    }
    /// Returns the value of a pixel at X coordinate.
    ///
    /// Corresponds to `BM_GET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn get(&self, x: usize) -> Option<bool> {
        Some(self.word(x)?.get(x))
    }
}

/// A Potrace word, a wrapper over [`usize`].
///
/// Corresponds to `potrace_word` in Potrace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Word(pub usize);

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
    pub const HIGH_BIT: Self = Self(isize::MIN as usize);
    /// The [`Word`] with all bits set.
    ///
    /// Corresponds to `BM_ALLBITS` in Potrace.
    pub const ALL_BITS: Self = Self(usize::MAX);

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
    /// Corresponds to `BM_PUT(bm, x, y)` in Potrace.
    #[inline(always)]
    pub const fn put(&mut self, x: usize, b: bool) {
        if b { self.set(x) } else { self.clear(x) }
    }
}
