use std::io::{self, Cursor};

use anyhow::{Error, Result};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use rayon::iter::{repeat, ParallelIterator};

/// 3-bytes length, 16MB - 1
const RAW_MAXIM: usize = 0x00FFFFFF;

/// bits to shift
const BLZ_SHIFT: u32 = 1;

/// bits to check:
/// 
///     ((((1 << BLZ_SHIFT) - 1) << (8 - BLZ_SHIFT)
const BLZ_MASK: u32 = 0x80;

/// max number of bytes to not encode
const BLZ_THRESHOLD: usize = 2;

pub fn blz_decode(input_buffer: &[u8]) -> Result<Vec<u8>> {
    if input_buffer.len() % 4 != 0 {
        return Err(Error::msg("Input buffer has an invalid length (must be multiple of 4)"));
    }
    
    if input_buffer.len() < 8 {
        return Err(Error::msg("Input buffer is too small to be a valid Bottom LZ file"));
    }
    
    // extracting basic information
    let input_buffer_length: u32 = input_buffer.len().try_into().unwrap();
    
    let mut input_buffer_u32: Vec<u32> = repeat(0).take(input_buffer.len() / 4).collect();
    LittleEndian::read_u32_into(input_buffer, &mut input_buffer_u32);
    
    let result_size_increase = input_buffer_u32[input_buffer_u32.len() - 1];
    
    if result_size_increase == 0 {
        panic!("Not coded file!");
    }
    
    let header_length: u32 = input_buffer[input_buffer.len() - 5].into();
    assert!(header_length >= 0x08 || header_length <= 0x0B, "Invalid header length");
    assert!(input_buffer_length > header_length, "Invalid header length");
    
    let mut encoded_length = input_buffer_u32[input_buffer_u32.len() - 2] & 0x00FFFFFF;
    let unencoded_length = input_buffer_length - encoded_length;
    
    encoded_length -= header_length;
    
    let encoded_length_usize: usize = encoded_length.try_into().unwrap();
    let unencoded_length_usize: usize = unencoded_length.try_into().unwrap();
    
    let result_size: usize = (input_buffer_length + result_size_increase)
        .try_into()
        .unwrap();
    assert!(result_size <= RAW_MAXIM, "Resulting file too large");
    
    // start populating result with unencoded area
    let mut result_buffer: Vec<u8> = Vec::with_capacity(result_size);
    result_buffer.extend(&input_buffer[0..unencoded_length_usize]);
    
    // decode the encoded area into result
    let mut encoded_buffer = input_buffer[unencoded_length_usize..unencoded_length_usize + encoded_length_usize].to_owned();
    encoded_buffer.reverse();
    
    let mut encoded = Cursor::new(&encoded_buffer);
    let mut mask: u32 = 0;
    let mut flags: u32 = 0;
    
    let read_u8_as_usize = |encoded: &mut Cursor<&Vec<u8>>| {
        Ok::<usize, io::Error>(usize::from(encoded.read_u8()?))
    };
    
    while result_buffer.len() < result_size {
        mask >>= BLZ_SHIFT;
        
        if mask == 0 {
            if encoded.position() == encoded_length.into() {
                break;
            }
            
            flags = encoded.read_u8()?.into();
            mask = BLZ_MASK;
        }
        
        if flags & mask == 0 {
            if encoded.position() == encoded_length.into() {
                break;
            }
            
            result_buffer.push(encoded.read_u8().unwrap());
        } else {
            if encoded.position() + 1 == encoded_length.into() {
                break;
            }
            
            let mut pos: usize = read_u8_as_usize(&mut encoded)? << 8 | read_u8_as_usize(&mut encoded)?;
            let len: usize = (pos >> 12) + BLZ_THRESHOLD + 1;
            
            if result_buffer.len() + len > result_size {
                panic!("Wrong decoded length");
                // len = result_size;
            }
            
            pos = (pos & 0xFFF) + 3;
            
            for _ in 0..len {
                result_buffer.push(result_buffer[result_buffer.len() - pos]);
            }
        }
    }
    
    assert!(result_buffer.len() == result_size, "Unexpected end of encoded file");
    
    result_buffer[unencoded_length_usize..].reverse();
    
    Ok(result_buffer)
}
