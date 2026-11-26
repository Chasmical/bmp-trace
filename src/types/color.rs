/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub(crate) u32);

impl Color {
    pub const BLACK: Self = Self(0x000000);
    pub const WHITE: Self = Self(0xFFFFFF);

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(u32::from_be_bytes([0, r, g, b]))
    }
    pub fn to_rgb(self) -> (u8, u8, u8) {
        let [_, r, g, b] = self.0.to_be_bytes();
        (r, g, b)
    }
}
impl std::fmt::Debug for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "#{:06x}", self.0)
    }
}
