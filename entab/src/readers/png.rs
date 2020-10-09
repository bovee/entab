use alloc::collections::BTreeMap;
use core::marker::Copy;
use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer, FromSlice};
use crate::record::{StateMetadata, Value};
use crate::EtError;
use crate::{impl_reader, impl_record};

/// The way the color is encoded in the PNG
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PngColorType {
    /// Each color is indexed from a palette
    Indexed,
    /// Only shades of gray
    Grayscale,
    /// Transparent shades of gray
    AlphaGrayscale,
    /// Full RGB color
    Color,
    /// Full RGB color with transparency
    AlphaColor,
}

impl Default for PngColorType {
    fn default() -> Self {
        PngColorType::Indexed
    }
}

impl PngColorType {
    fn from_byte(byte: u8) -> Result<Self, EtError> {
        match byte {
            0 => Ok(PngColorType::Grayscale),
            2 => Ok(PngColorType::Color),
            3 => Ok(PngColorType::Indexed),
            4 => Ok(PngColorType::AlphaGrayscale),
            6 => Ok(PngColorType::AlphaColor),
            _ => Err(EtError::new("Unknown PNG color type")),
        }
    }

    fn pixel_size(&self) -> usize {
        match self {
            PngColorType::Indexed => 1,
            PngColorType::Grayscale => 1,
            PngColorType::AlphaGrayscale => 2,
            PngColorType::Color => 3,
            PngColorType::AlphaColor => 4,
        }
    }
}

/// The state of the PNG parser
#[derive(Clone, Debug, Default)]
pub struct PngState {
    color_type: PngColorType,
    bit_depth: u8,
    width: usize,
    height: usize,
    cur_x: usize,
    cur_y: usize,
    image_data: Vec<u8>,
    palette: Option<Vec<(u16, u16, u16)>>,
}

impl PngState {
    fn line_len(&self) -> usize {
        // line length is scanline byte plus ceil(bit_depth / 8)
        1 + (self.width * self.color_type.pixel_size() * usize::from(self.bit_depth) + 7) / 8
    }

    fn unfilter_line(&mut self, line_num: usize) -> Result<(), EtError> {
        let bytes_per_pixel = (self.color_type.pixel_size() * usize::from(self.bit_depth) + 7) / 8;
        let line_len = self.line_len();

        for pos in line_num * line_len + 1..(line_num + 1) * line_len {
            let left = if pos < line_num * line_len + 1 + bytes_per_pixel {
                0
            } else {
                self.image_data[pos - bytes_per_pixel]
            };
            let above = if line_num == 0 {
                0
            } else {
                self.image_data[pos - line_len]
            };
            self.image_data[pos] = match self.image_data[line_num * line_len] {
                // no filtering; skip
                0 => self.image_data[pos],
                // sub filtering
                1 => self.image_data[pos].wrapping_add(left),
                // up filtering
                2 => self.image_data[pos].wrapping_add(above),
                // average filtering
                3 => {
                    let average = (u16::from(left) + u16::from(above)) as u8 / 2;
                    self.image_data[pos].wrapping_add(average)
                }
                // paeth filtering
                4 => {
                    let immediate_left = if pos == line_num * line_len + 1 {
                        0
                    } else {
                        self.image_data[pos - 1]
                    };
                    let above_left = if pos == line_num * line_len + 1 || line_num == 0 {
                        0
                    } else {
                        self.image_data[pos - 1 - line_len]
                    };
                    let estimate =
                        i16::from(immediate_left) + i16::from(above) - i16::from(above_left);
                    let pred_left = (estimate - i16::from(immediate_left)).abs();
                    let pred_above = (estimate - i16::from(above)).abs();
                    let pred_above_left = (estimate - i16::from(above_left)).abs();
                    let paeth = if pred_left <= pred_above && pred_left <= pred_above_left {
                        immediate_left
                    } else if pred_above <= pred_above_left {
                        above
                    } else {
                        above_left
                    };
                    self.image_data[pos].wrapping_add(paeth)
                }
                _ => return Err(EtError::new("Unknown line filter")),
            }
        }
        self.image_data[line_num * line_len] = 0;
        Ok(())
    }
}

impl<'r> StateMetadata<'r> for PngState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        let mut metadata = BTreeMap::new();
        let _ = metadata.insert("height".to_string(), (self.height as u64).into());
        let _ = metadata.insert("width".to_string(), (self.width as u64).into());
        metadata
    }
}

impl<'r> FromBuffer<'r> for PngState {
    type State = ();

    fn from_buffer(&mut self, rb: &'r mut ReadBuffer, _state: Self::State) -> Result<bool, EtError> {
        if rb.extract::<&[u8]>(8)? != b"\x89PNG\r\n\x1A\n" {
            return Err(EtError::new("Invalid PNG magic"));
        }
        if rb.extract::<&[u8]>(8)? != b"\x00\x00\x00\x0DIHDR" {
            return Err(EtError::new("Invalid PNG header"));
        }
        self.width = rb.extract::<u32>(Endian::Big)? as usize;
        self.height = rb.extract::<u32>(Endian::Big)? as usize;
        self.bit_depth = rb.extract(Endian::Big)?;
        self.color_type = PngColorType::from_byte(rb.extract(Endian::Big)?)?;
        // skip the compression, filter, and interlace bytes
        if rb.extract::<u8>(Endian::Big)? != 0 {
            return Err(EtError::new("PNG compression must be type 0"));
        }
        if rb.extract::<u8>(Endian::Big)? != 0 {
            return Err(EtError::new("PNG filtering must be type 0"));
        }
        if rb.extract::<u8>(Endian::Big)? != 0 {
            return Err(EtError::new("PNG interlacing not supported yet"));
        }

        // parse through the entire file beforehand; because the data is compressed into multiple
        // chunks and those chunks have to be concatenated before decompression, this makes
        // writing the handler a lot easier (although we should maybe do this in a streaming
        // fashion someday).
        let mut compressed_data = Vec::new();
        loop {
            // throw away the checksum from the previous chunk
            let _ = rb.extract::<&[u8]>(4)?;
            // now read the header for the current chunk
            let chunk_header = rb.extract::<&[u8]>(8)?;
            let chunk_size = u32::out_of(&chunk_header[..4], Endian::Big)? as usize;
            match &chunk_header[4..] {
                b"PLTE" => {
                    let mut raw_palette  = Vec::new();
                    for _ in 0..chunk_size / 3 {
                        let r: u8 = rb.extract(Endian::Big)?;
                        let g: u8 = rb.extract(Endian::Big)?;
                        let b: u8 = rb.extract(Endian::Big)?;
                        raw_palette.push((257 * u16::from(r), 257 * u16::from(g), 257 * u16::from(b)));
                    }
                    self.palette = Some(raw_palette);
                }
                b"IDAT" => {
                    // append all the IDAT chunks together
                    compressed_data.extend_from_slice(rb.extract(chunk_size)?);
                }
                b"IEND" => {
                    break;
                }
                _ => {
                    // just skip any other kinds of chunks
                    let _ = rb.extract::<&[u8]>(chunk_size)?;
                }
            }
        }
        let mut image_data = Vec::new();
        let _ = ZlibDecoder::new(&compressed_data[..]).read_to_end(&mut image_data)?;
        self.image_data = image_data;

        self.cur_x = 0;
        self.cur_y = 0;
        Ok(true)
    }
}

/// A single pixel from a PNG file
#[derive(Clone, Copy, Debug, Default)]
pub struct PngRecord {
    x: u32,
    y: u32,
    red: u16,
    green: u16,
    blue: u16,
    alpha: u16,
}

impl_record!(PngRecord: x, y, red, green, blue, alpha);

fn get_bits(data: &[u8], pos: usize, n_bits: usize, rescale: bool) -> Result<u16, EtError> {
    if n_bits == 16 {
        u16::out_of(&data[pos * 2..], Endian::Big)
    } else {
        let shift = n_bits * (pos % (8 / n_bits));
        let mask = (2u16.pow(n_bits as u32) - 1) as u8;

        let d = data[n_bits * pos / 8];
        let value = mask & (d >> shift);
        if rescale {
            // rescale the value into the u16 space
            Ok((u32::from(value) * 65535 / (2u32.pow(n_bits as u32) - 1)) as u16)
        } else {
            Ok(u16::from(value))
        }
    }
}

impl<'r> FromBuffer<'r> for PngRecord {
    type State = &'r mut PngState;

    fn from_buffer(&mut self, _rb: &'r mut ReadBuffer, state: Self::State) -> Result<bool, EtError> {
        if state.cur_y >= state.height {
            return Ok(false);
        }
        if state.cur_x == 0 {
            state.unfilter_line(state.cur_y)?;
        }
        let bd = usize::from(state.bit_depth);

        let line = &state.image_data
            [state.cur_y * state.line_len() + 1..(state.cur_y + 1) * state.line_len()];
        let pos = state.cur_x * state.color_type.pixel_size();
        let (red, green, blue, alpha) = match state.color_type {
            PngColorType::Indexed => {
                let palette_pos = get_bits(&line, pos, bd, false)? as usize;
                if let Some(palette) = &state.palette {
                    if palette_pos >= palette.len() {
                        return Err(EtError::new("Color index was outside palette dimensions"));
                    }
                    let (red, green, blue) = palette[palette_pos];
                    (red, green, blue, u16::MAX)
                } else {
                    return Err(EtError::new("No palette was provided"));
                }
            }
            PngColorType::Grayscale => {
                let gray = get_bits(&line, pos, bd, true)?;
                (gray, gray, gray, u16::MAX)
            }
            PngColorType::AlphaGrayscale => {
                let gray = get_bits(&line, pos, bd, true)?;
                let alpha = get_bits(&line, pos + 1, bd, true)?;
                (gray, gray, gray, alpha)
            }
            PngColorType::Color => {
                let red = get_bits(&line, pos, bd, true)?;
                let green = get_bits(&line, pos + 1, bd, true)?;
                let blue = get_bits(&line, pos + 2, bd, true)?;
                (red, green, blue, u16::MAX)
            }
            PngColorType::AlphaColor => {
                let red = get_bits(&line, pos, bd, true)?;
                let green = get_bits(&line, pos + 1, bd, true)?;
                let blue = get_bits(&line, pos + 2, bd, true)?;
                let alpha = get_bits(&line, pos + 3, bd, true)?;
                (red, green, blue, alpha)
            }
        };

        let (x, y) = (state.cur_x as u32, state.cur_y as u32);
        state.cur_x += 1;
        if state.cur_x == state.width {
            state.cur_x = 0;
            state.cur_y += 1;
        }
        self.x = x;
        self.y = y;
        self.red = red;
        self.green = green;
        self.blue = blue;
        self.alpha = alpha;
        Ok(true)
    }
}

impl_reader!(PngReader, PngRecord, PngState, ());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::RecordReader;

    #[test]
    fn test_png_reader() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!("../../tests/data/bmp_24.png"));
        let mut reader = PngReader::new(rb, ())?;
        let _ = reader.metadata();

        let mut n_recs = 0;
        while let Some(_) = reader.next()? {
            n_recs += 1;
        }
        // 200x200 image
        assert_eq!(n_recs, 40000);
        Ok(())
    }

    #[test]
    fn test_minimal_png() -> Result<(), EtError> {
        // data from https://en.wikipedia.org/wiki/Portable_Network_Graphics
        const TEST_IMAGE: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D,
            0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let rb = ReadBuffer::from_slice(TEST_IMAGE);
        let mut reader = PngReader::new(rb, ())?;
        let _ = reader.metadata();
        let pixel = reader.next()?.expect("first pixel exists");
        assert_eq!(pixel.x, 0);
        assert_eq!(pixel.y, 0);
        assert_eq!(pixel.red, 65535);
        assert_eq!(pixel.green, 0);
        assert_eq!(pixel.blue, 0);
        assert_eq!(pixel.alpha, 65535);
        assert!(reader.next()?.is_none());

        Ok(())
    }
}
