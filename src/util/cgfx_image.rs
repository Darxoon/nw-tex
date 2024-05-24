use std::slice::from_raw_parts;

use anyhow::{anyhow, Result};

use super::cgfx_texture::PicaTextureFormat;

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

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
    let bytes_per_pixel = input_format.get_bpp() / 8;
    let mut input_offset: usize = 0;
    let mut output: Vec<RgbaColor> = vec![RgbaColor::default(); (width * height).try_into()?];
    
    // iterate over every 8x8px chunk
    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            
            // iterate over every pixel in the current chunk
            for p in SWIZZLE_LUT {
                let local_x = p & 7;
                let local_y = (p - local_x) >> 3;
                
                let output_offset = x + local_x + (y + local_y) * width;
                
                match input_format {
                    PicaTextureFormat::RGBA8 => {
                        output[output_offset as usize] = RgbaColor {
                            r: image_buffer[input_offset + 3],
                            g: image_buffer[input_offset + 2],
                            b: image_buffer[input_offset + 1],
                            a: image_buffer[input_offset + 0],
                        }
                    },
                    PicaTextureFormat::RGBA4 => {
                        let raw = u16::from_le_bytes(image_buffer[input_offset..input_offset + 2].try_into().unwrap());
                        
                        let a: u8 = (raw & 0xf).try_into()?;
                        let b: u8 = ((raw >>  4) & 0xf).try_into()?;
                        let g: u8 = ((raw >>  8) & 0xf).try_into()?;
                        let r: u8 = ((raw >> 12) & 0xf).try_into()?;
                        
                        output[output_offset as usize] = RgbaColor {
                            r: r | (r << 4),
                            g: g | (g << 4),
                            b: b | (b << 4),
                            a: a | (a << 4),
                        }
                    }
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
