use crate::Point;

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

#[derive(Debug, Default, Clone)]
pub struct ImageInfo {
    pub pix_width: usize,
    pub pix_height: usize,
    pub width: f64,
    pub height: f64,
    pub left_margin: f64,
    pub right_margin: f64,
    pub top_margin: f64,
    pub bottom_margin: f64,
    pub transform: Transformation,
}
#[derive(Debug, Default, Clone)]
pub struct Transformation {
    pub bb: [f64; 2],
    pub orig: [f64; 2],
    pub x: [f64; 2],
    pub y: [f64; 2],
    pub scale: Point<f64>,
}

// #[derive(Debug, Default, Clone, Copy, PartialEq)]
// pub struct Dimension {
//     value: f64,
//     dimension: f64, // in pt, or 0 if not given
// }
// impl Dimension {
//     pub fn inches(inches: f64) -> Self {
//         Self { value: inches, dimension: 72.0 }
//     }
//     pub fn centimeters(centimeters: f64) -> Self {
//         Self { value: centimeters, dimension: 72.0 / 2.54 }
//     }
//     pub fn millimeters(millimeters: f64) -> Self {
//         Self { value: millimeters, dimension: 72.0 / 25.4 }
//     }
//     pub fn points(points: f64) -> Self {
//         Self { value: points, dimension: 1.0 }
//     }
// }

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    Flat = 0,
    #[default]
    Connected = 1,
    Hierarchical = 2,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Color(u32);

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

#[cfg(test)]
mod tests {
    use crate::{
        Bitmap, Params,
        backends::{BackendOptions, ImageInfo},
        trace,
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
                        pix_width: bm.width,
                        pix_height: bm.height,
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
