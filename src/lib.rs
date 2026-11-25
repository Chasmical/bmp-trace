/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

// TODO: remove these warning suppressions later
#![allow(clippy::result_unit_err)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::missing_safety_doc)]
#![cfg_attr(test, feature(bstr))]

pub mod backends;
mod bitmap;
mod bitmap_io;
mod curve;
mod point;
mod tracing;

pub use bitmap::*;
pub use bitmap_io::*;
pub use curve::*;
pub use point::*;
pub use tracing::*;

mod util;
