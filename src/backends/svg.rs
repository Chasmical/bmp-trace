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
use core::fmt::NumBuffer;
use std::io::{Cursor, Result, Write};

pub struct SvgWriter<W> {
    inner: W,
    opt: BackendOptions,
    cur: Point<usize>,
    last_op: u8,
    column: usize,
    start_of_seq: bool,
}

impl<W: Write> SvgWriter<W> {
    pub fn new(inner: W, opt: BackendOptions) -> Self {
        Self { inner, opt, cur: Point::default(), last_op: 0, column: 0, start_of_seq: true }
    }
    pub fn into_inner(self) -> W {
        self.inner
    }

    fn write_separator_before_token(&mut self, token_len: usize) -> Result<()> {
        if !self.start_of_seq {
            // Determine and print the separator: newline or space
            let ch = if self.column + token_len + 1 > 75 {
                self.column = 0;
                b'\n'
            } else {
                self.column += 1;
                b' '
            };
            self.inner.write_all(&[ch])?;
        }
        self.column += token_len;
        self.start_of_seq = false;
        Ok(())
    }

    fn write_op_coord(&mut self, op: u8, coord: isize) -> Result<()> {
        let mut buf = NumBuffer::new();
        let buf = coord.format_into(&mut buf);

        self.write_separator_before_token(buf.len() + 1)?;
        self.inner.write_all(&[op])?;
        self.inner.write_all(buf.as_bytes())?;
        Ok(())
    }
    fn write_coord(&mut self, coord: isize) -> Result<()> {
        let mut buf = NumBuffer::new();
        let buf = coord.format_into(&mut buf);

        self.write_separator_before_token(buf.len())?;
        self.inner.write_all(buf.as_bytes())?;
        Ok(())
    }

    fn svg_move_to(&mut self, p: Point<f64>) -> Result<()> {
        self.cur = self.opt.round_point_to_unit(p);
        self.write_op_coord(b'M', self.cur.x as isize)?;
        self.write_coord(self.cur.y as isize)?;
        self.last_op = b'M';
        Ok(())
    }
    fn svg_rel_move_to(&mut self, p: Point<f64>) -> Result<()> {
        let q = self.opt.round_point_to_unit(p);
        let delta = q.offset_from(self.cur);
        self.write_op_coord(b'm', delta.x)?;
        self.write_coord(delta.y)?;
        self.cur = q;
        self.last_op = b'm';
        Ok(())
    }
    fn svg_line_to(&mut self, p: Point<f64>) -> Result<()> {
        let q = self.opt.round_point_to_unit(p);
        let delta = q.offset_from(self.cur);

        if self.last_op != b'l' {
            self.write_op_coord(b'l', delta.x)?;
        } else {
            self.write_coord(delta.x)?;
        }
        self.write_coord(delta.y)?;

        self.cur = q;
        self.last_op = b'l';
        Ok(())
    }
    fn svg_curve_to(&mut self, p1: Point<f64>, p2: Point<f64>, p3: Point<f64>) -> Result<()> {
        let q3 = self.opt.round_point_to_unit(p3);
        let d1 = self.opt.round_point_to_unit(p1).offset_from(self.cur);
        let d2 = self.opt.round_point_to_unit(p2).offset_from(self.cur);
        let d3 = q3.offset_from(self.cur);

        if self.last_op != b'c' {
            self.write_op_coord(b'c', d1.x)?;
        } else {
            self.write_coord(d1.x)?;
        }
        self.write_coord(d1.y)?;
        self.write_coord(d2.x)?;
        self.write_coord(d2.y)?;
        self.write_coord(d3.x)?;
        self.write_coord(d3.y)?;

        self.cur = q3;
        self.last_op = b'c';
        Ok(())
    }

    fn svg_path(&mut self, curve: &Curve, absolute_coords: bool) -> Result<()> {
        let start = curve.segments.last().unwrap().control_points;

        if absolute_coords {
            self.svg_move_to(start[2])?;
        } else {
            self.svg_rel_move_to(start[2])?;
        }

        for seg in &curve.segments {
            match seg.tag {
                SegmentTag::Corner => {
                    self.svg_line_to(seg.control_points[1])?;
                    self.svg_line_to(seg.control_points[2])?;
                },
                SegmentTag::CurveTo => {
                    self.svg_curve_to(
                        seg.control_points[0],
                        seg.control_points[1],
                        seg.control_points[2],
                    )?;
                },
            }
        }

        // Note: "z" ignores token wrapping for some reason
        self.inner.write_all(b"z")?;
        self.column += 1;
        Ok(())
    }

    fn svg_jaggy_path(&mut self, points: &[Point<usize>], absolute_coords: bool) -> Result<()> {
        if absolute_coords {
            let mut cur = *points.last().unwrap();
            let mut prev = cur;
            self.svg_move_to(cur.to_f64())?;
            for &pt in points {
                if pt.x != cur.x && pt.y != cur.y {
                    cur = prev;
                    self.svg_line_to(cur.to_f64())?;
                }
                prev = pt;
            }
            self.svg_line_to(points.last().unwrap().to_f64())?;
        } else {
            let mut cur = *points.first().unwrap();
            let mut prev = cur;
            self.svg_rel_move_to(cur.to_f64())?;
            for &pt in points.iter().rev() {
                if pt.x != cur.x && pt.y != cur.y {
                    cur = prev;
                    self.svg_line_to(cur.to_f64())?;
                }
                prev = pt;
            }
            self.svg_line_to(points.first().unwrap().to_f64())?;
        }

        // Note: "z" ignores token wrapping for some reason
        self.inner.write_all(b"z")?;
        self.column += 1;
        Ok(())
    }

    fn write_paths_opaque(&mut self, tree: &PathTree) -> Result<()> {
        for p in tree.self_and_siblings() {
            if self.opt.grouping == Grouping::Hierarchical {
                self.inner.write_all(b"<g>\n<g>\n")?;
            }

            write!(self.inner, "<path fill=\"#{:06x}\" stroke=\"none\" d=\"", self.opt.color.0)?;
            self.column = 38; // 38 is the exact length of this string

            self.start_of_seq = true;
            self.last_op = 0;
            if self.opt.debug == 1 {
                self.svg_jaggy_path(&p.path.points, true)?;
            } else {
                self.svg_path(&p.path.curve, true)?;
            }
            self.inner.write_all(b"\"/>\n")?;

            for q in p.children() {
                write!(
                    self.inner,
                    "<path fill=\"#{:06x}\" stroke=\"none\" d=\"",
                    self.opt.fill_color.0
                )?;
                self.column = 38; // 38 is the exact length of this string

                self.start_of_seq = true;
                self.last_op = 0;
                if self.opt.debug == 1 {
                    self.svg_jaggy_path(&q.path.points, true)?;
                } else {
                    self.svg_path(&q.path.curve, true)?;
                }
                self.inner.write_all(b"\"/>\n")?;
            }

            if self.opt.grouping == Grouping::Hierarchical {
                self.inner.write_all(b"</g>\n")?;
            }
            for q in p.children() {
                if let Some(first_child) = q.first_child() {
                    self.write_paths_opaque(first_child)?;
                }
            }
            if self.opt.grouping == Grouping::Hierarchical {
                self.inner.write_all(b"</g>\n")?;
            }
        }
        Ok(())
    }

    fn write_paths_transparent_rec(&mut self, tree: &PathTree) -> Result<()> {
        for p in tree.self_and_siblings() {
            if self.opt.grouping == Grouping::Hierarchical {
                self.inner.write_all(b"<g>\n")?;
            }
            if self.opt.grouping != Grouping::Flat {
                self.inner.write_all(b"<path d=\"")?;
                self.column = "<path d=\"".len();
                self.start_of_seq = true;
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
                self.inner.write_all(b"\"/>\n")?;
            }
            for q in p.children() {
                if let Some(first_child) = q.first_child() {
                    self.write_paths_transparent_rec(first_child)?;
                }
            }
            if self.opt.grouping == Grouping::Hierarchical {
                self.inner.write_all(b"</g>\n")?;
            }
        }
        Ok(())
    }

    fn write_paths_transparent(&mut self, tree: &PathTree) -> Result<()> {
        if self.opt.grouping == Grouping::Flat {
            self.inner.write_all(b"<path d=\"")?;
            self.column = "<path d=\"".len();
            self.start_of_seq = true;
            self.last_op = 0;
        }
        self.write_paths_transparent_rec(tree)?;
        if self.opt.grouping == Grouping::Flat {
            self.inner.write_all(b"\"/>")?;
        }
        Ok(())
    }

    pub fn write_svg_document(&mut self, plist: &PathTree, img_info: &ImageInfo) -> Result<()> {
        let width = img_info.page_width();
        let height = img_info.page_height();
        let origin = img_info.origin_top_left();
        let scale_x = img_info.transform.scale.x / self.opt.unit;
        let scale_y = -img_info.transform.scale.y / self.opt.unit;

        write!(
            self.inner,
            r#"<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 20010904//EN"
 "http://www.w3.org/TR/2001/REC-SVG-20010904/DTD/svg10.dtd">
<svg version="1.0" xmlns="http://www.w3.org/2000/svg"
 width="{0:.6}pt" height="{1:.6}pt" viewBox="0 0 {0:.6} {1:.6}"
 preserveAspectRatio="xMidYMid meet">
<metadata>
Created by potrace 1.16, written by Peter Selinger 2001-2019
</metadata>
"#,
            width, height
        )?;

        // use a "group" tag to establish coordinate system and style
        self.inner.write_all(b"<g transform=\"")?;
        if origin != Point::default() {
            write!(self.inner, "translate({:.6},{:.6}) ", origin.x, origin.y)?;
        }
        if self.opt.angle != 0.0 {
            write!(self.inner, "rotate({:.2}) ", -self.opt.angle)?;
        }
        writeln!(
            self.inner,
            "scale({:.6},{:.6})\"\nfill=\"#{:06x}\" stroke=\"none\">",
            scale_x, scale_y, self.opt.color.0
        )?;

        if self.opt.opaque {
            self.write_paths_opaque(plist)?;
        } else {
            self.write_paths_transparent(plist)?;
        }

        /* write footer */
        self.inner.write_all(b"</g>\n</svg>\n")?;
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
        SvgWriter::new(writer, opt).write_svg_document(self, &img_info)
    }
    pub fn to_svg_string(&self, opt: BackendOptions, img_info: ImageInfo) -> String {
        let mut writer = SvgWriter::new(Cursor::new(Vec::new()), opt);
        writer.write_svg_document(self, &img_info).unwrap();
        unsafe { String::from_utf8_unchecked(writer.into_inner().into_inner()) }
    }
}
