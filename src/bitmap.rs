/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

/// A read-only reference to bitmap data.
///
/// Corresponds to `potrace_bitmap_t` in Potrace.
#[derive(Clone, Copy)]
// TODO: words_per_scanline is unsigned atm, so bitmap flip in expensive
pub struct Bitmap<'a> {
    width: usize,              // int w
    height: usize,             // int h
    words_per_scanline: usize, // int dy
    raw_words: &'a [Word],     // potrace_word* map
}

impl<'a> Bitmap<'a> {
    /// Accesses a scanline at specified Y index.
    ///
    /// Corresponds to `bm_scanline(bm, y)` in Potrace.
    #[inline(always)]
    pub fn scanline(&self, y: usize) -> Option<Scanline<'a>> {
        let line = self.raw_words.get(y * self.words_per_scanline..)?;
        Some(Scanline(line))
    }
    /// Accesses a word at specified X and Y coordinates.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word(&self, x: usize, y: usize) -> Option<&'a Word> {
        self.scanline(y)?.word(x)
    }
    /// Gets the value of a pixel at X and Y coordinates.
    ///
    /// Corresponds to `BM_GET(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn get(&self, x: usize, y: usize) -> Option<bool> {
        Some(self.word(x, y)?.get(x))
    }
}

/// A read-only reference to a slice of Potrace words, representing a bitmap row.
///
/// Corresponds to `potrace_word_t*` in Potrace.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Scanline<'a>(&'a [Word]);
// TODO: ScanlineMut<'a>?

impl<'a> Scanline<'a> {
    /// Accesses a word at specified X coordinate.
    ///
    /// Corresponds to `bm_index(bm, x, y)` in Potrace.
    #[inline(always)]
    pub fn word(&self, x: usize) -> Option<&'a Word> {
        self.0.get(x / Word::BITS)
    }
    /// Gets the value of a pixel at X coordinate.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
