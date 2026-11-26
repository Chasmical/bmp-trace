/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::types::Color;

mod svg;

pub use svg::*;

#[derive(Debug, Clone)]
pub struct BackendOptions {
    unit: f64,
    debug: u8,
    grouping: Grouping,
    color: Color,
    fill_color: Color,
    angle: f64,
    opaque: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    Flat = 0,
    #[default]
    Connected = 1,
    Hierarchical = 2,
}

impl Default for BackendOptions {
    fn default() -> Self {
        Self {
            unit: 10.0,
            debug: 0,
            grouping: Grouping::Connected,
            color: Color::BLACK,
            fill_color: Color::WHITE,
            angle: 0.0,
            opaque: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        backends::BackendOptions,
        tracing::{Params, trace},
        types::{Bitmap, ImageInfo},
    };
    use std::{bstr::ByteStr, ffi::OsStr, io::Cursor};

    #[test]
    fn test_directory_files() {
        let files = std::fs::read_dir("./tests").unwrap();
        let files = Vec::from_iter(files.map(|f| f.unwrap().path()));

        for f in &files {
            if f.extension() == Some(OsStr::new("pbm")) {
                let bm_bytes = std::fs::read(f).unwrap();
                let bm = Bitmap::read_from(Cursor::new(bm_bytes), 0.5).unwrap();
                let tree = trace(&bm, Params::default(), None)
                    .expect("tracing shouldn't fail")
                    .expect("tracing should result in at least one path");

                let tests = files.iter().filter(|&t| t != f && t.file_stem() == f.file_stem());
                for test in tests {
                    let expected = std::fs::read(test).unwrap();

                    let opt = BackendOptions::default();
                    let img_info = ImageInfo {
                        px_width: bm.width(),
                        px_height: bm.height(),
                        ..Default::default()
                    };

                    let ext = test.extension().unwrap().to_str().unwrap();
                    let real = match ext {
                        "svg" => tree.to_svg_string(opt, img_info),
                        _ => continue,
                    };

                    let expected_lines = Vec::from_iter(expected.split(|&x| x == b'\n'));
                    let real_lines = Vec::from_iter(real.as_bytes().split(|&x| x == b'\n'));
                    let max_len = expected_lines.len().max(real_lines.len());

                    for i in 0..max_len {
                        assert_eq!(
                            expected_lines.get(i).map(|&b| ByteStr::new(b)),
                            real_lines.get(i).map(|&b| ByteStr::new(b))
                        );
                    }

                    println!("Tested {:?} => {:?} successfully", f, test);
                }
            }
        }
    }
}
