/*
 * Copyright (C) 2001-2019 Peter Selinger
 * Copyright (C) 2026 Chasmical
 *
 * Adapted and modified for Rust from the original project Potrace.
 * This file is part of bmp-trace and is licensed under GNU GPLv2.
 * See the file LICENSE for details.
 */

use crate::{
    types::{Bitmap, Word},
    util,
};
use std::io::{Read, Seek, Write};

impl Bitmap {
    pub fn read_from<R: Read + Seek>(reader: R, threshold: f64) -> Result<Self, ReadError> {
        let res = BmpReader::new(reader).read(threshold)?;
        if let Some(err) = res.1 {
            return Err(err);
        }
        Ok(res.0)
    }
    pub fn write_to<W: Write>(&self, writer: W) -> std::io::Result<()> {
        BmpWriter::new(writer).write(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    Unsupported = -4,
    Empty = -3,
    Format = -2,
    Alloc = -1,
}

#[derive(Default)]
struct BmpInfo {
    file_size: u32,
    reserved: u32,
    data_offset: u32,
    info_size: u32,
    w: u32,
    h: u32,
    planes: u32,
    bits: u32,
    comp: u32,
    image_size: u32,
    x_pixels_per_m: u32,
    y_pixels_per_m: u32,
    n_colors: u32,
    colors_important: u32,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    alpha_mask: u32,
    ctbits: u32,
    topdown: i32,
}

pub struct BmpReader<R> {
    inner: R,
    bmp_count: usize,
    bmp_pos: usize,
}

impl<R: Read + Seek> BmpReader<R> {
    pub const fn new(inner: R) -> Self {
        Self { inner, bmp_count: 0, bmp_pos: 0 }
    }

    fn fgetc(&mut self) -> Result<u8, ()> {
        let mut c = [0; 1];
        self.inner.read_exact(&mut c).or(Err(()))?;
        Ok(c[0])
    }

    fn fgetc_ws(&mut self) -> Result<u8, ()> {
        loop {
            let mut c = self.fgetc()?;
            if c == b'#' {
                while let Ok(c2) = self.fgetc() {
                    c = c2;
                    if c2 == b'\n' {
                        break;
                    }
                }
            }
            /* space, tab, line feed, carriage return, form-feed */
            if !c.is_ascii_whitespace() {
                return Ok(c);
            }
        }
    }

    fn readnum(&mut self) -> Result<u32, ()> {
        /* skip whitespace and comments */
        let c = loop {
            let c = self.fgetc_ws()?;
            if c.is_ascii_digit() {
                break c;
            }
        };

        /* first digit is already in c */
        let mut acc = (c - b'0') as u32;
        while let Ok(c) = self.fgetc() {
            if c.is_ascii_digit() {
                acc *= 10;
                acc += (c - b'0') as u32;
                if acc > i32::MAX as u32 {
                    return Err(());
                }
            } else {
                // unconsume the non-digit byte
                self.inner.seek_relative(-1).or(Err(()))?;
                break;
            }
        }
        Ok(acc)
    }

    fn readbit(&mut self) -> Result<bool, ()> {
        /* skip whitespace and comments */
        loop {
            let c = self.fgetc_ws()?;
            if matches!(c, b'0' | b'1') {
                return Ok(c == b'1');
            }
        }
    }

    pub fn read(&mut self, threshold: f64) -> Result<(Bitmap, Option<ReadError>), ReadError> {
        let magic = [
            self.fgetc_ws().or(Err(ReadError::Empty))?,
            self.fgetc().or(Err(ReadError::Unsupported))?,
        ];
        match magic {
            [b'P', b'1'..=b'6'] => self.read_body_pnm(threshold, magic[1]),
            [b'B', b'M'] => self.read_body_bmp(threshold),
            _ => Err(ReadError::Unsupported),
        }
    }

    fn read_body_pnm(
        &mut self,
        threshold: f64,
        version_byte: u8,
    ) -> Result<(Bitmap, Option<ReadError>), ReadError> {
        let w = self.readnum().or(Err(ReadError::Format))? as usize;
        let h = self.readnum().or(Err(ReadError::Format))? as usize;

        let mut bm = Bitmap::try_new(w as _, h as _).or(Err(ReadError::Alloc))?;

        let mut real_height = 0;
        let mut unexpected_eof = true;

        #[allow(clippy::never_loop)]
        'eof: loop {
            match version_byte {
                b'1' => {
                    /* read P1 format: PBM ascii */

                    for y in 0..h {
                        real_height = y + 1;
                        for x in 0..w {
                            let Ok(b) = self.readbit() else { break 'eof };
                            unsafe { bm.put_unchecked(x, y, b) };
                        }
                    }
                },

                b'2' => {
                    /* read P2 format: PGM ascii */

                    let max = self.readnum().or(Err(ReadError::Format))?;
                    let maxthres = (threshold * max as f64) as u32;

                    for y in 0..h {
                        real_height = y + 1;
                        for x in 0..w {
                            let Ok(b) = self.readnum() else { break 'eof };
                            unsafe { bm.put_unchecked(x, y, !(b > maxthres)) };
                        }
                    }
                },

                b'3' => {
                    /* read P3 format: PPM ascii */

                    let max = self.readnum().or(Err(ReadError::Format))?;
                    let maxthres = (3.0 * threshold * max as f64) as u32;

                    for y in 0..h {
                        real_height = y + 1;
                        for x in 0..w {
                            let mut sum = 0;
                            for _ in 0..3 {
                                let Ok(b) = self.readnum() else { break 'eof };
                                sum += b;
                            }
                            unsafe { bm.put_unchecked(x, y, !(sum > maxthres)) };
                        }
                    }
                },

                b'4' => {
                    /* read P4 format: PBM raw */

                    /* read single white-space character after height */
                    self.fgetc().or(Err(ReadError::Format))?;

                    let bpr = w.div_ceil(8);

                    for y in 0..h {
                        real_height = y + 1;
                        for i in 0..bpr {
                            let Ok(b) = self.fgetc() else { break 'eof };

                            unsafe {
                                bm.word_unchecked_mut(i * 8, y).0 |=
                                    (b as u32) << (8 * (Word::SIZE - 1 - (i % Word::SIZE)))
                            }
                        }
                    }
                },

                b'5' => {
                    /* read P5 format: PGM raw */

                    let max = self.readnum().or(Err(ReadError::Format))?;
                    let maxthres = (threshold * max as f64) as u32;

                    /* read single white-space character after max */
                    self.fgetc().or(Err(ReadError::Format))?;

                    for y in 0..h {
                        real_height = y + 1;
                        for x in 0..w {
                            let Ok(b) = self.fgetc() else { break 'eof };
                            let mut b = b as u32;
                            if max >= 256 {
                                b <<= 8;
                                let Ok(b1) = self.fgetc() else { break 'eof };
                                b |= b1 as u32;
                            }
                            unsafe { bm.put_unchecked(x, y, !(b > maxthres)) };
                        }
                    }
                },

                b'6' => {
                    /* read P6 format: PPM raw */

                    let max = self.readnum().or(Err(ReadError::Format))?;
                    let maxthres = (3.0 * threshold * max as f64) as u32;

                    /* read single white-space character after max */
                    self.fgetc().or(Err(ReadError::Format))?;

                    for y in 0..h {
                        real_height = y + 1;
                        for x in 0..w {
                            let mut sum = 0;
                            for _ in 0..3 {
                                let Ok(b) = self.fgetc() else { break 'eof };
                                let mut b = b as u32;
                                if max >= 256 {
                                    b <<= 8;
                                    let Ok(b1) = self.fgetc() else { break 'eof };
                                    b |= b1 as u32;
                                }
                                sum += b;
                            }
                            unsafe { bm.put_unchecked(x, y, !(sum > maxthres)) };
                        }
                    }
                },

                _ => return Err(ReadError::Format),
            };
            unexpected_eof = false;
            break;
        }

        let err = if unexpected_eof {
            bm.try_resize_mem_aligned(real_height).or(Err(ReadError::Alloc))?;
            Some(ReadError::Format)
        } else {
            None
        };

        bm.flip();

        Ok((bm, err))
    }

    fn bmp_readint(&mut self, n: usize) -> Result<u32, ()> {
        let mut sum = 0;

        for i in 0..n {
            let b = self.fgetc()? as u32;
            sum += b << (8 * i);
        }
        self.bmp_count += n;
        self.bmp_pos += n;
        Ok(sum)
    }
    fn bmp_pad_reset(&mut self) {
        self.bmp_count = 0;
    }
    fn bmp_pad(&mut self) -> Result<(), ()> {
        let pad_bytes = self.bmp_count.wrapping_neg() & 3;
        for _ in 0..pad_bytes {
            self.fgetc()?;
        }
        self.bmp_pos += pad_bytes;
        self.bmp_count = 0;
        Ok(())
    }
    fn bmp_forward(&mut self, pos: usize) -> Result<(), ()> {
        self.inner.seek_relative(pos as i64).or(Err(()))?;
        self.bmp_pos += pos;
        self.bmp_count += pos;
        Ok(())
    }

    fn read_body_bmp(&mut self, threshold: f64) -> Result<(Bitmap, Option<ReadError>), ReadError> {
        let mut bmpinfo = BmpInfo::default();

        self.bmp_pos = 2; /* set file position */

        /* file header (minus magic number) */
        bmpinfo.file_size = self.bmp_readint(4).or(Err(ReadError::Format))?;
        bmpinfo.reserved = self.bmp_readint(4).or(Err(ReadError::Format))?;
        bmpinfo.data_offset = self.bmp_readint(4).or(Err(ReadError::Format))?;

        /* info header */
        bmpinfo.info_size = self.bmp_readint(4).or(Err(ReadError::Format))?;
        if matches!(bmpinfo.info_size, 40 | 64 | 108 | 124) {
            /* Windows or new OS/2 format */
            bmpinfo.ctbits = 32; /* sample size in color table */
            bmpinfo.w = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.h = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.planes = self.bmp_readint(2).or(Err(ReadError::Format))?;
            bmpinfo.bits = self.bmp_readint(2).or(Err(ReadError::Format))?;
            bmpinfo.comp = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.image_size = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.x_pixels_per_m = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.y_pixels_per_m = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.n_colors = self.bmp_readint(4).or(Err(ReadError::Format))?;
            bmpinfo.colors_important = self.bmp_readint(4).or(Err(ReadError::Format))?;
            if bmpinfo.info_size >= 108 {
                /* V4 and V5 bitmaps */
                bmpinfo.red_mask = self.bmp_readint(4).or(Err(ReadError::Format))?;
                bmpinfo.green_mask = self.bmp_readint(4).or(Err(ReadError::Format))?;
                bmpinfo.blue_mask = self.bmp_readint(4).or(Err(ReadError::Format))?;
                bmpinfo.alpha_mask = self.bmp_readint(4).or(Err(ReadError::Format))?;
            }
            if bmpinfo.w > 0x7fffffff {
                return Err(ReadError::Format);
            }
            if bmpinfo.h > 0x7fffffff {
                bmpinfo.h = bmpinfo.h.wrapping_neg();
                bmpinfo.topdown = 1;
            } else {
                bmpinfo.topdown = 0;
            }
            if bmpinfo.h > 0x7fffffff {
                return Err(ReadError::Format);
            }
        } else if bmpinfo.info_size == 12 {
            /* old OS/2 format */
            bmpinfo.ctbits = 24; /* sample size in color table */
            bmpinfo.w = self.bmp_readint(2).or(Err(ReadError::Format))?;
            bmpinfo.h = self.bmp_readint(2).or(Err(ReadError::Format))?;
            bmpinfo.planes = self.bmp_readint(2).or(Err(ReadError::Format))?;
            bmpinfo.bits = self.bmp_readint(2).or(Err(ReadError::Format))?;
            bmpinfo.comp = 0;
            bmpinfo.n_colors = 0;
            bmpinfo.topdown = 0;
        } else {
            return Err(ReadError::Format);
        }

        if bmpinfo.comp == 3 && bmpinfo.info_size < 108 {
            /* bitfield feature is only understood with V4 and V5 format */
            return Err(ReadError::Format);
        }

        if bmpinfo.comp > 3 || bmpinfo.bits > 32 {
            return Err(ReadError::Format);
        }

        /* forward to color table (e.g., if bmpinfo.InfoSize == 64) */
        self.bmp_forward(14 + bmpinfo.info_size as usize).or(Err(ReadError::Format))?;

        if bmpinfo.planes != 1 {
            return Err(ReadError::Format); /* can't handle planes */
        }

        if bmpinfo.n_colors == 0 && bmpinfo.bits <= 8 {
            bmpinfo.n_colors = 1 << bmpinfo.bits;
        }

        let mut col1 = [0; 2];
        let mut coltable: Box<[bool]> = Default::default();

        /* color table, present only if bmpinfo.bits <= 8. */
        if bmpinfo.bits <= 8 {
            coltable = util::alloc_default_slice::<bool>(bmpinfo.n_colors as usize)
                .or(Err(ReadError::Alloc))?;
            /* NOTE: since we are reading a bitmap, we can immediately convert
            the color table entries to bits. */
            let maxthres = (3.0 * 255.0 * threshold) as u32;
            for i in 0..bmpinfo.n_colors as usize {
                let mut c =
                    self.bmp_readint((bmpinfo.ctbits / 8) as usize).or(Err(ReadError::Format))?;

                c = ((c >> 16) & 0xff) + ((c >> 8) & 0xff) + (c & 0xff);
                coltable[i] = !(c > maxthres);
                if i < 2 {
                    col1[i] = c;
                }
            }
        }

        /* forward to data */
        if bmpinfo.info_size != 12 {
            /* not old OS/2 format */
            self.bmp_forward(bmpinfo.data_offset as usize).or(Err(ReadError::Format))?;
        }

        /* allocate bitmap */
        let mut bm = Bitmap::try_new(bmpinfo.w as _, bmpinfo.h as _).or(Err(ReadError::Alloc))?;

        let mut real_height = 0;
        let mut unexpected_eof = true;

        #[allow(clippy::never_loop)]
        'eof: loop {
            match bmpinfo.bits + 0x100 * bmpinfo.comp {
                0x001 => {
                    /* monochrome palette */
                    /* make the darker color black */
                    let mask = if col1[0] < col1[1] { 0xff } else { 0 };

                    /* raster data */
                    for y in 0..bmpinfo.h as usize {
                        real_height = y + 1;
                        self.bmp_pad_reset();

                        for i in 0..(bmpinfo.w / 8) as usize {
                            let Ok(mut b) = self.bmp_readint(1) else { break 'eof };
                            b ^= mask;
                            unsafe {
                                bm.word_unchecked_mut(i * 8, y).0 |=
                                    b << (8 * (Word::SIZE - 1 - (i % Word::SIZE)));
                            }
                        }
                        self.bmp_pad().or(Err(ReadError::Format))?;
                    }
                },

                0x002..=0x008 => {
                    /* 2-bit to 8-bit palettes */
                    for y in 0..bmpinfo.h as usize {
                        real_height = y + 1;
                        self.bmp_pad_reset();
                        let mut bitbuf = 0; /* bit buffer: bits in buffer are high-aligned */
                        let mut n = 0; /* number of bits currently in bitbuffer */
                        for x in 0..bmpinfo.w as usize {
                            if n < bmpinfo.bits as usize {
                                let Ok(b) = self.bmp_readint(1) else { break 'eof };
                                bitbuf |= b << (Word::BITS - 8 - n);
                                n += 8;
                            }
                            let b = bitbuf >> (Word::BITS - bmpinfo.bits as usize);
                            bitbuf <<= bmpinfo.bits;
                            n -= bmpinfo.bits as usize;
                            unsafe {
                                bm.put_unchecked(
                                    x,
                                    y,
                                    coltable.get(b as usize).copied().unwrap_or(false),
                                )
                            };
                        }
                        self.bmp_pad().or(Err(ReadError::Format))?;
                    }
                },

                0x010 => {
                    /* 16-bit encoding */
                    return Err(ReadError::Format);
                },

                0x018 | 0x020 => {
                    let maxthres = (3.0 * 255.0 * threshold) as u32;
                    /* 24- and 32-bit encoding */
                    for y in 0..bmpinfo.h as usize {
                        real_height = y + 1;

                        self.bmp_pad_reset();
                        for x in 0..bmpinfo.w as usize {
                            let Ok(mut c) = self.bmp_readint((bmpinfo.bits / 8) as usize) else {
                                break 'eof;
                            };
                            c = ((c >> 16) & 0xff) + ((c >> 8) & 0xff) + (c & 0xff);
                            unsafe { bm.put_unchecked(x, y, !(c > maxthres)) };
                        }
                        self.bmp_pad().or(Err(ReadError::Format))?;
                    }
                },

                0x320 => {
                    /* 32-bit encoding with bitfields */
                    if bmpinfo.red_mask == 0 || bmpinfo.green_mask == 0 || bmpinfo.blue_mask == 0 {
                        return Err(ReadError::Format);
                    }

                    fn lobit(mut x: u32) -> u32 {
                        let mut res = 32;
                        while x & 0xffffff != 0 {
                            x <<= 8;
                            res -= 8;
                        }
                        while x != 0 {
                            x <<= 1;
                            res -= 1;
                        }
                        res
                    }

                    let redshift = lobit(bmpinfo.red_mask);
                    let greenshift = lobit(bmpinfo.green_mask);
                    let blueshift = lobit(bmpinfo.blue_mask);
                    let maxthres = (3.0 * 255.0 * threshold) as u32;

                    for y in 0..bmpinfo.h as usize {
                        real_height = y + 1;
                        self.bmp_pad_reset();
                        for x in 0..bmpinfo.w as usize {
                            let Ok(mut c) = self.bmp_readint((bmpinfo.bits / 8) as usize) else {
                                break 'eof;
                            };
                            c = ((c & bmpinfo.red_mask) >> redshift)
                                + ((c & bmpinfo.green_mask) >> greenshift)
                                + ((c & bmpinfo.blue_mask) >> blueshift);
                            unsafe { bm.put_unchecked(x, y, !(c > maxthres)) };
                        }
                        self.bmp_pad().or(Err(ReadError::Format))?;
                    }
                },

                0x204 => {
                    /* 4-bit runlength compressed encoding (RLE4) */
                    let mut x = 0;
                    let mut y = 0;

                    loop {
                        let Ok(b) = self.bmp_readint(1) else { break 'eof }; /* opcode */
                        let Ok(c) = self.bmp_readint(1) else { break 'eof }; /* argument */
                        let mut col = [false; 2];

                        if b > 0 {
                            /* repeat count */
                            col[0] =
                                coltable.get(((c >> 4) & 0xf) as usize).copied().unwrap_or(false);
                            col[1] = coltable.get((c & 0xf) as usize).copied().unwrap_or(false);

                            for i in 0..b {
                                if !(x < bmpinfo.w) {
                                    break;
                                }
                                if x >= bmpinfo.w {
                                    x = 0;
                                    y += 1;
                                }
                                if x >= bmpinfo.w || y >= bmpinfo.h {
                                    break;
                                }
                                real_height = (y + 1) as usize;
                                bm.try_put(x as usize, y as usize, col[(i & 1) as usize]);
                                x += 1;
                            }
                        } else if c == 0 {
                            /* end of line */
                            y += 1;
                            x = 0;
                        } else if c == 1 {
                            /* end of bitmap */
                            break;
                        } else if c == 2 {
                            /* "delta": skip pixels in x and y directions */
                            let Ok(b) = self.bmp_readint(1) else { break 'eof }; /* x offset */
                            let Ok(c) = self.bmp_readint(1) else { break 'eof }; /* y offset */
                            x += b;
                            y += c;
                        } else {
                            /* verbatim segment */
                            for i in 0..c {
                                if (i & 1) == 0 {
                                    let Ok(_) = self.bmp_readint(1) else { break 'eof };
                                }
                                if x >= bmpinfo.w {
                                    x = 0;
                                    y += 1;
                                }
                                if x >= bmpinfo.w || y >= bmpinfo.h {
                                    break;
                                }
                                real_height = (y + 1) as usize;
                                bm.try_put(
                                    x as usize,
                                    y as usize,
                                    coltable
                                        .get(((b >> (4 - 4 * (i & 1))) & 0xf) as usize)
                                        .copied()
                                        .unwrap_or(false),
                                );
                                x += 1;
                            }
                            if (c + 1) & 2 != 0 {
                                /* pad to 16-bit boundary */
                                let Ok(_) = self.bmp_readint(1) else { break 'eof };
                            }
                        }
                    }
                },

                0x108 => {
                    /* 8-bit runlength compressed encoding (RLE8) */

                    let mut x = 0;
                    let mut y = 0;
                    loop {
                        let Ok(b) = self.bmp_readint(1) else { break 'eof }; /* opcode */
                        let Ok(c) = self.bmp_readint(1) else { break 'eof }; /* argument */
                        if b > 0 {
                            /* repeat count */
                            for _ in 0..b {
                                if x >= bmpinfo.w {
                                    x = 0;
                                    y += 1;
                                }
                                if x >= bmpinfo.w || y >= bmpinfo.h {
                                    break;
                                }
                                real_height = (y + 1) as usize;
                                bm.try_put(
                                    x as usize,
                                    y as usize,
                                    coltable.get(c as usize).copied().unwrap_or(false),
                                );
                                x += 1;
                            }
                        } else if c == 0 {
                            /* end of line */
                            y += 1;
                            x = 0;
                        } else if c == 1 {
                            /* end of bitmap */
                            break;
                        } else if c == 2 {
                            /* "delta": skip pixels in x and y directions */
                            let Ok(b) = self.bmp_readint(1) else { break 'eof }; /* x offset */
                            let Ok(c) = self.bmp_readint(1) else { break 'eof }; /* y offset */
                            x += b;
                            y += c;
                        } else {
                            /* verbatim segment */
                            for _ in 0..c {
                                let Ok(b) = self.bmp_readint(1) else { break 'eof };
                                if x >= bmpinfo.w {
                                    x = 0;
                                    y += 1;
                                }
                                if x >= bmpinfo.w || y >= bmpinfo.h {
                                    break;
                                }
                                real_height = (y + 1) as usize;
                                bm.try_put(
                                    x as usize,
                                    y as usize,
                                    coltable.get(b as usize).copied().unwrap_or(false),
                                );
                                x += 1;
                            }
                            if (c & 1) != 0 {
                                /* pad input to 16-bit boundary */
                                let Ok(_) = self.bmp_readint(1) else { break 'eof };
                            }
                        }
                    }
                },

                _ => return Err(ReadError::Format),
            }
            unexpected_eof = false;
            break;
        }

        let err = if unexpected_eof {
            bm.try_resize_mem_aligned(real_height).or(Err(ReadError::Alloc))?;
            Some(ReadError::Format)
        } else {
            /* skip any potential junk after the data section, but don't
            complain in case EOF is encountered */
            _ = self.bmp_forward(bmpinfo.file_size as usize);
            None
        };

        if bmpinfo.topdown != 0 {
            bm.flip();
        }

        Ok((bm, err))
    }
}

pub struct BmpWriter<W> {
    inner: W,
}

impl<W: Write> BmpWriter<W> {
    pub const fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn write(&mut self, bm: &Bitmap) -> std::io::Result<()> {
        writeln!(self.inner, "P4")?;
        writeln!(self.inner, "{} {}", bm.width(), bm.height())?;

        let bpr = bm.width().div_ceil(8);
        for y in (0..bm.height()).rev() {
            for i in 0..bpr {
                let c = unsafe {
                    (bm.word_unchecked(i * 8, y).0 >> (8 * (Word::SIZE - 1 - (i % Word::SIZE))))
                        as u8
                };
                self.inner.write_all(&[c])?;
            }
        }
        Ok(())
    }
}
