/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{
    backends::{BackendOptions, Grouping},
    types::{Curve, ImageInfo, PathTree, Point, SegmentTag},
};
use std::{
    io::{Cursor, Result, Write},
    mem::MaybeUninit,
};

pub struct SvgWriter<W> {
    inner: W,
    opt: BackendOptions,
    cur: Point<usize>,
    last_op: u8,
    column: usize,
    newline: bool,
}

struct DisplayBuffer<const N: usize> {
    buffer: [MaybeUninit<u8>; N],
    pos: usize,
}
impl<const N: usize> DisplayBuffer<N> {
    pub fn new() -> Self {
        Self { buffer: [MaybeUninit::uninit(); N], pos: 0 }
    }
    pub fn as_slice(&self) -> &[u8] {
        unsafe { self.buffer[..self.pos].assume_init_ref() }
    }
}
impl<const N: usize> Write for DisplayBuffer<N> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.buffer[self.pos..self.pos + buf.len()].write_copy_of_slice(buf);
        self.pos += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

macro_rules! ship {
    ($dst:expr, $($arg:tt)*) => {{
        let mut buf = DisplayBuffer::<1024>::new();
        let res = write!(buf, $($arg)*);
        res.and_then(|_| $dst.ship_token(buf.as_slice()))
    }};
}

impl<W: Write> SvgWriter<W> {
    pub fn new(inner: W, opt: BackendOptions) -> Self {
        Self { inner, opt, cur: Point::default(), last_op: 0, column: 0, newline: true }
    }
    pub fn into_inner(self) -> W {
        self.inner
    }

    fn ship_token(&mut self, token: &[u8]) -> Result<()> {
        for part in token.split(|&x| x == b' ') {
            if !self.newline {
                if self.column + part.len() + 1 > 75 {
                    writeln!(self.inner)?;
                    self.column = 0;
                } else {
                    write!(self.inner, " ")?;
                    self.column += 1;
                }
            }
            self.inner.write_all(part)?;
            self.column += part.len();
            self.newline = false;
        }
        Ok(())
    }

    fn svg_moveto(&mut self, p: Point<f64>) -> Result<()> {
        self.cur = self.opt.round_point_to_unit(p);
        ship!(self, "M{} {}", self.cur.x, self.cur.y)?;
        self.last_op = b'M';
        Ok(())
    }
    fn svg_rmoveto(&mut self, p: Point<f64>) -> Result<()> {
        let q = self.opt.round_point_to_unit(p);
        let delta = q.offset_from(self.cur);
        ship!(self, "m{} {}", delta.x, delta.y)?;
        self.cur = q;
        self.last_op = b'm';
        Ok(())
    }
    fn svg_lineto(&mut self, p: Point<f64>) -> Result<()> {
        let q = self.opt.round_point_to_unit(p);
        let delta = q.offset_from(self.cur);

        if self.last_op != b'l' {
            ship!(self, "l{} {}", delta.x, delta.y)?;
        } else {
            ship!(self, "{} {}", delta.x, delta.y)?;
        }
        self.cur = q;
        self.last_op = b'l';
        Ok(())
    }
    fn svg_curveto(&mut self, p1: Point<f64>, p2: Point<f64>, p3: Point<f64>) -> Result<()> {
        let q1 = self.opt.round_point_to_unit(p1);
        let q2 = self.opt.round_point_to_unit(p2);
        let q3 = self.opt.round_point_to_unit(p3);

        let d1 = q1.offset_from(self.cur);
        let d2 = q2.offset_from(self.cur);
        let d3 = q3.offset_from(self.cur);

        if self.last_op != b'c' {
            ship!(self, "c{} {} {} {} {} {}", d1.x, d1.y, d2.x, d2.y, d3.x, d3.y)?;
        } else {
            ship!(self, "{} {} {} {} {} {}", d1.x, d1.y, d2.x, d2.y, d3.x, d3.y)?;
        }
        self.cur = q3;
        self.last_op = b'c';
        Ok(())
    }

    fn svg_path(&mut self, curve: &Curve, absolute_coords: bool) -> Result<()> {
        let start = curve.segments.last().unwrap().control_points;

        if absolute_coords {
            self.svg_moveto(start[2])?;
        } else {
            self.svg_rmoveto(start[2])?;
        }

        for seg in &curve.segments {
            match seg.tag {
                SegmentTag::Corner => {
                    println!("{:?} {:?}", seg.control_points[1], seg.control_points[2]);
                    self.svg_lineto(seg.control_points[1])?;
                    self.svg_lineto(seg.control_points[2])?;
                },
                SegmentTag::CurveTo => {
                    self.svg_curveto(
                        seg.control_points[0],
                        seg.control_points[1],
                        seg.control_points[2],
                    )?;
                },
            }
        }

        self.newline = true;
        self.ship_token("z".as_bytes())
    }

    fn svg_jaggy_path(&mut self, points: &[Point<usize>], absolute_coords: bool) -> Result<()> {
        if absolute_coords {
            let mut cur = *points.last().unwrap();
            let mut prev = cur;
            self.svg_moveto(cur.to_f64())?;
            for &pt in points {
                if pt.x != cur.x && pt.y != cur.y {
                    cur = prev;
                    self.svg_lineto(cur.to_f64())?;
                }
                prev = pt;
            }
            self.svg_lineto(points.last().unwrap().to_f64())?;
        } else {
            let mut cur = *points.first().unwrap();
            let mut prev = cur;
            self.svg_rmoveto(cur.to_f64())?;
            for &pt in points.iter().rev() {
                if pt.x != cur.x && pt.y != cur.y {
                    cur = prev;
                    self.svg_lineto(cur.to_f64())?;
                }
                prev = pt;
            }
            self.svg_lineto(points.first().unwrap().to_f64())?;
        }

        self.newline = true;
        self.ship_token("z".as_bytes())
    }

    fn write_paths_opaque(&mut self, tree: &PathTree) -> Result<()> {
        for p in tree.self_and_siblings() {
            if self.opt.grouping == Grouping::Hierarchical {
                writeln!(self.inner, "<g>")?;
                writeln!(self.inner, "<g>")?;
            }
            let mut buf = DisplayBuffer::<38>::new(); // 38 is the exact length of this string
            write!(buf, "<path fill=\"#{:06x}\" stroke=\"none\" d=\"", self.opt.color.0)?;
            self.inner.write_all(buf.as_slice())?;
            self.column = buf.as_slice().len();

            self.newline = true;
            self.last_op = 0;
            if self.opt.debug == 1 {
                self.svg_jaggy_path(&p.path.points, true)?;
            } else {
                self.svg_path(&p.path.curve, true)?;
            }
            writeln!(self.inner, "\"/>")?;

            for q in p.children() {
                let mut buf = DisplayBuffer::<38>::new(); // 38 is the exact length of this string
                write!(buf, "<path fill=\"#{:06x}\" stroke=\"none\" d=\"", self.opt.fill_color.0)?;
                self.inner.write_all(buf.as_slice())?;
                self.column = buf.as_slice().len();

                self.newline = true;
                self.last_op = 0;
                if self.opt.debug == 1 {
                    self.svg_jaggy_path(&q.path.points, true)?;
                } else {
                    self.svg_path(&q.path.curve, true)?;
                }
                writeln!(self.inner, "\"/>")?;
            }

            if self.opt.grouping == Grouping::Hierarchical {
                writeln!(self.inner, "</g>")?;
            }
            for q in p.children() {
                if let Some(first_child) = q.first_child() {
                    self.write_paths_opaque(first_child)?;
                }
            }
            if self.opt.grouping == Grouping::Hierarchical {
                writeln!(self.inner, "</g>")?;
            }
        }
        Ok(())
    }

    fn write_paths_transparent_rec(&mut self, tree: &PathTree) -> Result<()> {
        for p in tree.self_and_siblings() {
            if self.opt.grouping == Grouping::Hierarchical {
                writeln!(self.inner, "<g>")?;
            }
            if self.opt.grouping != Grouping::Flat {
                write!(self.inner, "<path d=\"")?;
                self.column = "<path d=\"".len();
                self.newline = true;
                self.last_op = 0;
            }
            if self.opt.debug == 1 {
                self.svg_jaggy_path(&p.path.points, true)?;
            } else {
                self.svg_path(&p.path.curve, true)?;
            }

            for q in p.children() {
                if self.opt.debug == 1 {
                    self.svg_jaggy_path(&q.path.points, false)?;
                } else {
                    self.svg_path(&q.path.curve, false)?;
                }
            }

            if self.opt.grouping != Grouping::Flat {
                writeln!(self.inner, "\"/>")?;
            }
            for q in p.children() {
                if let Some(first_child) = q.first_child() {
                    self.write_paths_transparent_rec(first_child)?;
                }
            }
            if self.opt.grouping == Grouping::Hierarchical {
                writeln!(self.inner, "</g>")?;
            }
        }
        Ok(())
    }

    fn write_paths_transparent(&mut self, tree: &PathTree) -> Result<()> {
        if self.opt.grouping == Grouping::Flat {
            write!(self.inner, "<path d=\"")?;
            self.column = "<path d=\"".len();
            self.newline = true;
            self.last_op = 0;
        }
        self.write_paths_transparent_rec(tree)?;
        if self.opt.grouping == Grouping::Flat {
            writeln!(self.inner, "\"/>")?;
        }
        Ok(())
    }

    pub fn write_svg_document(&mut self, plist: &PathTree, img_info: &ImageInfo) -> Result<()> {
        let width = img_info.page_width();
        let height = img_info.page_height();
        let origin = img_info.origin_top_left();
        let scale_x = img_info.transform.scale.x / self.opt.unit;
        let scale_y = -img_info.transform.scale.y / self.opt.unit;

        // header
        writeln!(self.inner, "<?xml version=\"1.0\" standalone=\"no\"?>")?;
        writeln!(self.inner, "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 20010904//EN\"")?;
        writeln!(self.inner, " \"http://www.w3.org/TR/2001/REC-SVG-20010904/DTD/svg10.dtd\">")?;
        writeln!(self.inner, "<svg version=\"1.0\" xmlns=\"http://www.w3.org/2000/svg\"")?;

        // set bounding box and namespace
        writeln!(
            self.inner,
            " width=\"{0:.6}pt\" height=\"{1:.6}pt\" viewBox=\"0 0 {0:.6} {1:.6}\"",
            width, height
        )?;
        writeln!(self.inner, " preserveAspectRatio=\"xMidYMid meet\">")?;

        // metadata: creator
        writeln!(self.inner, "<metadata>")?;
        // TODO: customize this metadata
        writeln!(self.inner, "Created by potrace 1.16, written by Peter Selinger 2001-2019")?;
        writeln!(self.inner, "</metadata>")?;

        // use a "group" tag to establish coordinate system and style
        write!(self.inner, "<g transform=\"")?;
        if origin != Point::default() {
            write!(self.inner, "translate({:.6},{:.6}) ", origin.x, origin.y)?;
        }
        if self.opt.angle != 0.0 {
            write!(self.inner, "rotate({:.2}) ", -self.opt.angle)?;
        }
        write!(self.inner, "scale({:.6},{:.6})", scale_x, scale_y)?;
        writeln!(self.inner, "\"")?;
        writeln!(self.inner, "fill=\"#{:06x}\" stroke=\"none\">", self.opt.color.0)?;

        if self.opt.opaque {
            self.write_paths_opaque(plist)?;
        } else {
            self.write_paths_transparent(plist)?;
        }

        /* write footer */
        writeln!(self.inner, "</g>")?;
        writeln!(self.inner, "</svg>")?;
        self.inner.flush()
    }
}

impl PathTree {
    pub fn write_svg<W: Write>(
        &self,
        writer: W,
        opt: BackendOptions,
        img_info: ImageInfo,
    ) -> Result<()> {
        let mut writer = SvgWriter::new(writer, opt);
        writer.write_svg_document(self, &img_info)
    }
    pub fn to_svg_string(&self, opt: BackendOptions, img_info: ImageInfo) -> String {
        let mut writer = SvgWriter::new(Cursor::new(Vec::new()), opt);
        writer.write_svg_document(self, &img_info).unwrap();
        unsafe { String::from_utf8_unchecked(writer.into_inner().into_inner()) }
    }
}
