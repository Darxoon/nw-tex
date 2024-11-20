use std::{cmp::max, io::Cursor, slice::from_raw_parts};

use anyhow::{anyhow, Result};
use binrw::{BinRead, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt};
use png::{BitDepth, ColorType, Encoder, ScaledFloat, SourceChromaticities};

use super::texture::PicaTextureFormat;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, BinRead, BinWrite)]
#[brw(little)]
#[repr(C)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub fn grayscale(lightness: u8) -> Self {
        Self {
            r: lightness,
            g: lightness,
            b: lightness,
            a: 0xFF,
        }
    }
    
    pub fn grayscale_alpha(lightness: u8, alpha: u8) -> Self {
        Self {
            r: lightness,
            g: lightness,
            b: lightness,
            a: alpha,
        }
    }
    
    pub fn from_alpha(alpha: u8) -> Self {
        Self {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
            a: alpha,
        }
    }
}

// TODO: verify that input length is divisible by 4
pub fn colors_to_bytes(image_buffer: &[RgbaColor]) -> &[u8] {
    unsafe {
        let bytes_pointer = (&image_buffer[0] as *const RgbaColor) as *const u8;
        
        from_raw_parts(bytes_pointer, image_buffer.len() * 4)
    }
}

pub fn bytes_to_colors(bytes: &[u8]) -> &[RgbaColor] {
    unsafe {
        let colors_pointer = (&bytes[0] as *const u8) as *const RgbaColor;
        
        from_raw_parts(colors_pointer, bytes.len() / 4)
    }
}

pub fn to_png(image_buffer: &[RgbaColor], width: u32, height: u32) -> Result<Vec<u8>> {
    let bytes = colors_to_bytes(image_buffer);
    let mut out: Vec<u8> = Vec::new();
    
    {
        // setup png encoder
        let mut encoder = Encoder::new(&mut out, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_source_gamma(ScaledFloat::from_scaled(45455));
        encoder.set_source_gamma(ScaledFloat::new(1.0 / 2.2));
        let source_chromaticities = SourceChromaticities::new(
            (0.31270, 0.32900),
            (0.64000, 0.33000),
            (0.30000, 0.60000),
            (0.15000, 0.06000),
        );
        encoder.set_source_chromaticities(source_chromaticities);
        let mut writer = encoder.write_header().unwrap();
        
        // write png
        writer.write_image_data(bytes)?;
    }
    
    Ok(out)
}

pub const ENCODABLE_FORMATS: [PicaTextureFormat; 0] = [
    // PicaTextureFormat::RGBA5551,
];

// look-up table for 3ds swizzling
// all of this is confusing so this
// is from SPICA/CTR Studio
const SWIZZLE_LUT: [u32; 64] = [
    0,  1,  8,  9,  2,  3, 10, 11,
    16, 17, 24, 25, 18, 19, 26, 27,
    4,  5, 12, 13,  6,  7, 14, 15,
    20, 21, 28, 29, 22, 23, 30, 31,
    32, 33, 40, 41, 34, 35, 42, 43,
    48, 49, 56, 57, 50, 51, 58, 59,
    36, 37, 44, 45, 38, 39, 46, 47,
    52, 53, 60, 61, 54, 55, 62, 63
];

pub fn decode_swizzled_buffer(image_buffer: &[u8], input_format: PicaTextureFormat, width: u32, height: u32) -> Result<Vec<RgbaColor>> {
    if input_format == PicaTextureFormat::ETC1A4 || input_format == PicaTextureFormat::ETC1 {
        return decode_etc1(image_buffer, width, height, input_format == PicaTextureFormat::ETC1A4);
    }
    
    let bytes_per_pixel = max(input_format.get_bpp() / 8, 1);
    let mut input_offset: usize = 0;
    let mut output: Vec<RgbaColor> = vec![RgbaColor::default(); (width * height).try_into()?];
    
    // iterate over every 8x8px chunk
    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            
            // iterate over every pixel in the current chunk
            for p in SWIZZLE_LUT {
                let local_x = p & 7;
                let local_y = (p - local_x) >> 3;
                
                let output_offset: usize = (x + local_x + (y + local_y) * width).try_into()?;
                
                match input_format {
                    PicaTextureFormat::RGBA8 => {
                        output[output_offset] = RgbaColor {
                            r: image_buffer[input_offset + 3],
                            g: image_buffer[input_offset + 2],
                            b: image_buffer[input_offset + 1],
                            a: image_buffer[input_offset + 0],
                        }
                    },
                    PicaTextureFormat::RGBA4 => {
                        let raw = u16::from_le_bytes(image_buffer[input_offset..input_offset + 2].try_into().unwrap());
                        
                        let r: u8 = ((raw >> 12) & 0xf).try_into()?;
                        let g: u8 = ((raw >> 8) & 0xf).try_into()?;
                        let b: u8 = ((raw >> 4) & 0xf).try_into()?;
                        let a: u8 = (raw & 0xf).try_into()?;
                        
                        output[output_offset] = RgbaColor {
                            r: r | (r << 4),
                            g: g | (g << 4),
                            b: b | (b << 4),
                            a: a | (a << 4),
                        }
                    },
                    PicaTextureFormat::RGB565 => {
                        let raw = u16::from_le_bytes(image_buffer[input_offset..input_offset + 2].try_into().unwrap());
                        
                        let r: u8 = (((raw >> 11) & 0x1f) << 3).try_into()?;
                        let g: u8 = (((raw >> 5) & 0x3f) << 2).try_into()?;
                        let b: u8 = (((raw >> 0) & 0x1f) << 3).try_into()?;
                        
                        output[output_offset] = RgbaColor {
                            r: r | (r >> 5),
                            g: g | (g >> 6),
                            b: b | (b >> 5),
                            a: 0xFF,
                        }
                    },
                    PicaTextureFormat::RGBA5551 => {
                        let raw = u16::from_le_bytes(image_buffer[input_offset..input_offset + 2].try_into().unwrap());
                        
                        let r: u8 = (((raw >> 11) & 0x1f) << 3).try_into()?;
                        let g: u8 = (((raw >> 6) & 0x1f) << 3).try_into()?;
                        let b: u8 = (((raw >> 1) & 0x1f) << 3).try_into()?;
                        let a: u8 = ((raw & 1) * 0xFF).try_into()?;
                        
                        output[output_offset] = RgbaColor {
                            r: r | (r >> 5),
                            g: g | (g >> 5),
                            b: b | (b >> 5),
                            a,
                        }
                    },
                    PicaTextureFormat::L8 => {
                        output[output_offset] = RgbaColor::grayscale(image_buffer[input_offset])
                    },
                    PicaTextureFormat::L4 => {
                        let raw = image_buffer[input_offset / 2];
                        
                        let color = if input_offset % 2 == 0 {
                            (raw & 0x0F) | (raw << 4)
                        } else {
                            (raw & 0xF0) | (raw >> 4)
                        };
                        
                        output[output_offset] = RgbaColor::grayscale(color)
                    },
                    PicaTextureFormat::A8 => {
                        output[output_offset] = RgbaColor::from_alpha(image_buffer[input_offset])
                    },
                    PicaTextureFormat::A4 => {
                        let raw = image_buffer[input_offset / 2];
                        
                        let alpha = if input_offset % 2 == 0 {
                            (raw & 0x0F) | (raw << 4)
                        } else {
                            (raw & 0xF0) | (raw >> 4)
                        };
                        
                        output[output_offset] = RgbaColor::from_alpha(alpha)
                    },
                    PicaTextureFormat::LA8 => {
                        let alpha: u8 = image_buffer[input_offset];
                        let color: u8 = image_buffer[input_offset + 1];
                        
                        output[output_offset] = RgbaColor::grayscale_alpha(color, alpha)
                    },
                    PicaTextureFormat::LA4 => {
                        let high: u8 = (image_buffer[input_offset] & 0xF0) as u8;
                        let low: u8 = (image_buffer[input_offset] & 0x0F) as u8;
                        
                        output[output_offset] = RgbaColor {
                            r: high | (high >> 4),
                            g: high | (high >> 4),
                            b: high | (high >> 4),
                            a: low | (low << 4),
                        }
                    },
                    _ => {
                        return Err(anyhow!("Format {:?} not implemented yet", input_format));
                    }
                }
                
                input_offset += bytes_per_pixel as usize;
            }
            
        }
    }
    
    Ok(output)
}

const ETC1_X: [u32; 4] = [ 0, 4, 0, 4 ];
const ETC1_Y: [u32; 4] = [ 0, 0, 4, 4 ];

fn decode_etc1(image_buffer: &[u8], width: u32, height: u32, use_alpha: bool) -> Result<Vec<RgbaColor>> {
    let mut input_reader = Cursor::new(image_buffer);
    let mut output: Vec<RgbaColor> = vec![RgbaColor::default(); (width * height).try_into()?];
    
    // iterate over every 8x8px chunk
    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            
            // iterate over every 4x4px block in this chunk
            for (sub_x, sub_y) in ETC1_X.into_iter().zip(ETC1_Y) {
                let alpha_block = if use_alpha {
                    input_reader.read_u64::<LittleEndian>()?
                } else {
                    u64::MAX
                };
                
                let color_block_low = input_reader.read_u32::<LittleEndian>()?;
                let color_block_high = input_reader.read_u32::<LittleEndian>()?;
                
                // decode color block
                let mut base0: RgbaColor;
                let mut base1: RgbaColor;
                
                // determines whether the current 4x4px chunk is
                // subdivided horizontally (true) or vertically (false)
                let flip = color_block_high & 0x1 != 0;
                // if true, base0 will be RGBA5 and base1 will only
                // encode the difference to base0 in RGBA3
                let diff = color_block_high & 0x2 != 0;
                
                if diff {
                    base0 = RgbaColor {
                        r: ((color_block_high & 0xf8000000) >> 24) as u8,
                        g: ((color_block_high & 0x00f80000) >> 16) as u8,
                        b: ((color_block_high & 0x0000f800) >> 8) as u8,
                        a: 0xFF,
                    };
                    base1 = RgbaColor { // confusing calculation I don't really understand but I hope this checks out
                        r: ((base0.r >> 3) as i32 + (((color_block_high & 0x07000000) >> 19) as i8 >> 5) as i32) as u8,
                        g: ((base0.g >> 3) as i32 + (((color_block_high & 0x00070000) >> 11) as i8 >> 5) as i32) as u8,
                        b: ((base0.b >> 3) as i32 + (((color_block_high & 0x00000700) >> 3) as i8 >> 5) as i32) as u8,
                        a: 0xFF,
                    };
                    base0.r |= base0.r >> 5;
                    base0.g |= base0.g >> 5;
                    base0.b |= base0.b >> 5;
                    
                    base1.r = (base1.r << 3) | (base1.r >> 2);
                    base1.g = (base1.g << 3) | (base1.g >> 2);
                    base1.b = (base1.b << 3) | (base1.b >> 2);
                } else {
                    base0 = RgbaColor {
                        r: ((color_block_high & 0xf0000000) >> 24) as u8,
                        g: ((color_block_high & 0x00f00000) >> 16) as u8,
                        b: ((color_block_high & 0x0000f000) >> 8) as u8,
                        a: 0xFF,
                    };
                    base1 = RgbaColor {
                        r: ((color_block_high & 0x0f000000) >> 20) as u8,
                        g: ((color_block_high & 0x000f0000) >> 12) as u8,
                        b: ((color_block_high & 0x00000f00) >> 4) as u8,
                        a: 0xFF,
                    };
                    base0.r |= base0.r >> 4;
                    base0.g |= base0.g >> 4;
                    base0.b |= base0.b >> 4;
                    
                    base1.r |= base1.r >> 4;
                    base1.g |= base1.g >> 4;
                    base1.b |= base1.b >> 4;
                }
                
                let table0 = (color_block_high >> 5) & 0b111;
                let table1 = (color_block_high >> 2) & 0b111;
                
                let mut current_chunk: [RgbaColor; 16] = [RgbaColor::default(); 16];
                
                for local_y in if flip { 0u32..2u32 } else { 0u32..4u32 } {
                    for local_x in if flip { 0u32..4u32 } else { 0u32..2u32 } {
                        let offset0 = local_y * 4 + local_x;
                        let offset1 = if flip {
                            (local_y + 2) * 4 + local_x
                        } else {
                            local_y * 4 + local_x + 2
                        };
                        let x1: u32 = if flip { local_x } else { local_x + 2 }.try_into()?;
                        let y1: u32 = if flip { local_y + 2 } else { local_y }.try_into()?;
                        
                        current_chunk[offset0 as usize] =
                            decode_etc1_pixel(base0, local_x, local_y, color_block_low.to_be(), table0)?;
                        current_chunk[offset1 as usize] =
                            decode_etc1_pixel(base1, x1, y1, color_block_low.to_be(), table1)?;
                    }
                }
                
                // write colors into output
                let mut tile_offset: u32 = 0;
                
                for local_y in sub_y..sub_y + 4 {
                    for local_x in sub_x..sub_x + 4 {
                        let output_offset = x + local_x + (y + local_y) * width;
                        
                        output[output_offset as usize] = current_chunk[tile_offset as usize];
                        
                        let alpha_shift = ((local_x & 3) * 4 + (local_y & 3)) << 2;
                        let alpha = (alpha_block >> alpha_shift) as u8 & 0xF;
                        
                        output[output_offset as usize].a = alpha | alpha << 4;
                        tile_offset += 1;
                    }
                }
            }
            
        }
    }
    
    Ok(output)
}

const ETC1_LUT: [[i32; 4]; 8] = [
    [   2,   8,    -2,   -8  ],
    [   5,   17,   -5,  -17  ],
    [   9,   29,   -9,  -29  ],
    [  13,   42,  -13,  -42  ],
    [  18,   60,  -18,  -60  ],
    [  24,   80,  -24,  -80  ],
    [  33,  106,  -33, -106  ],
    [  47,  183,  -47, -183  ],
];

fn saturate(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > 0xFF {
        0xFF
    } else {
        value.try_into().unwrap()
    }
}

fn decode_etc1_pixel(base_color: RgbaColor, x: u32, y: u32, block_big_endian: u32, table: u32) -> Result<RgbaColor> {
    let index = x * 4 + y;
    let msb = block_big_endian << 1; // why?
    
    let pixel = if index < 8 {
        ETC1_LUT[table as usize][((block_big_endian >> (index + 24)) & 1) as usize + ((msb >> (index + 8)) & 2) as usize]
    } else {
        ETC1_LUT[table as usize][((block_big_endian >> (index +  8)) & 1) as usize + ((msb >> (index - 8)) & 2) as usize]
    };
    
    Ok(RgbaColor {
        r: saturate(base_color.r as i32 + pixel),
        g: saturate(base_color.g as i32 + pixel),
        b: saturate(base_color.b as i32 + pixel),
        a: 0xFF,
    })
}
