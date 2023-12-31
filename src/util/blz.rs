// darxoon's blz implementation v0
// based on CUE's DS/GBA Compressors
use std::io::{self, Cursor, Seek, SeekFrom};

use anyhow::{Error, Result};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use rayon::iter::{repeat, ParallelIterator};

/// 3-bytes length, 16MB - 1
const RAW_MAXIM: usize = 0x00FFFFFF;

/// bits to shift
const BLZ_SHIFT: u32 = 1;

/// bits to check
/// 
///     ((((1 << BLZ_SHIFT) - 1) << (8 - BLZ_SHIFT)
const BLZ_MASK: u32 = 0x80;

/// max number of bytes to not encode
const BLZ_THRESHOLD: usize = 2;

/// max number of bytes to not encode
const BLZ_THRESHOLD_U32: u32 = 2;

/// max lz offset (aka BLZ_N)
/// 
///     ((1 << 12) + 2)
const BLZ_MAX_OFFSET: usize = 0x1002;

/// max coded (aka BLZ_F)
/// 
///     ((1 << 4) + BLZ_THRESHOLD)
const BLZ_MAX_CODED: usize = 0x12;

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
    
    assert!(result_buffer.len() == result_size, "Decompressed byte length doesn't match expected length");
    
    result_buffer[unencoded_length_usize..].reverse();
    
    Ok(result_buffer)
}

/// Mutates input_buffer for efficiency but in the end leaves it
/// in the same state that it was in before calling this function.
pub fn blz_encode(input_buffer: &mut [u8]) -> Result<Vec<u8>> {
    // weird calculation that I don't really understand
    let mut result_buffer: Vec<u8> = Vec::with_capacity(input_buffer.len() + (input_buffer.len() + 7) / 8 + 11);
    
    input_buffer.reverse();
    let mut input = Cursor::new(&*input_buffer);
    
    // TODO: add arm9 support
    
    // Not sure if this actuallly specifies flags. Original name is "flg" though.
    let mut flag_index: usize = 0;
    let mut mask: u32 = 0;
    
    // sum of these two variables is an approximation of the final result size
    let mut input_bytes_left: u32 = input_buffer.len().try_into().unwrap();
    let mut result_bytes_written: u32 = 0;
    
    let mut length_best: u32;
    let mut position_best: Option<u32> = None;
    
    while input.position() < input_buffer.len().try_into().unwrap() {
        mask >>= BLZ_SHIFT;
        
        if mask == 0 {
            flag_index = result_buffer.len();
            result_buffer.push(0);
            mask = BLZ_MASK;
        }
        
        (length_best, position_best) = search(&input, input_buffer, position_best);
        
        // TODO: add "best" compression ratio support (LZ-CUE optimization)
        
        result_buffer[flag_index] <<= 1;
        
        if length_best > BLZ_THRESHOLD_U32 {
            // encode 
            input.seek(SeekFrom::Current(length_best.try_into().unwrap()))?;
            result_buffer[flag_index] |= 1;
            
            result_buffer.push(u8::try_from(
                ((length_best - (BLZ_THRESHOLD_U32 + 1)) << 4) | ((position_best.unwrap() - 3) >> 8)
            ).unwrap());
            
            result_buffer.push(u8::try_from((position_best.unwrap() - 3) & 0xFF).unwrap());
        } else {
            result_buffer.push(input.read_u8()?);
        }
        
        // converting numbers
        let result_length: u32 = result_buffer.len().try_into().unwrap();
        let input_length: u32 = input_buffer.len().try_into().unwrap();
        let input_position: u32 = input.position().try_into().unwrap();
        
        let remaining_input_bytes = input_length - input_position;
        
        // update approximation of final result length
        let new_result_approximation = result_length + remaining_input_bytes;
        let previous_result_approxiation = input_bytes_left + result_bytes_written;
        
        if new_result_approximation < previous_result_approxiation {
            input_bytes_left = remaining_input_bytes;
            result_bytes_written = result_length;
        }
    }
    
    while mask != 0 && mask != 1 {
        mask >>= BLZ_SHIFT;
        result_buffer[flag_index] <<= 1;
    }
    
    input_buffer.reverse();
    
    let input_length: u32 = input_buffer.len().try_into().unwrap();
    
    // what does this condition mean?
    let idk = input_length + 4 < ((result_bytes_written + input_bytes_left + 3) & (u32::MAX - 3)) + 8;
    
    if result_bytes_written == 0 || idk {
        todo!()
    } else {
        // convert numbers
        let input_buffer_length: u32 = input_buffer.len().try_into().unwrap();
        let input_bytes_left_usize: usize = input_bytes_left.try_into().unwrap();
        let result_bytes_written_usize: usize = result_bytes_written.try_into().unwrap();
        
        // allocate buffer for BLZ container file and write main content
        let mut container_buffer: Vec<u8> = Vec::new();
        
        container_buffer.extend(&input_buffer[..input_bytes_left_usize]);
        container_buffer.extend(&result_buffer[result_buffer.len() - result_bytes_written_usize..result_buffer.len()]);
        
        // write container header
        let size_increase = input_buffer_length - input_bytes_left - result_bytes_written;
        let mut header_length = 8;
        
        while container_buffer.len() % 4 != 0 {
            container_buffer.push(0xFF);
            header_length += 1;
        }
        
        container_buffer.write_u24::<LittleEndian>(result_bytes_written + header_length)?;
        container_buffer.write_u8(header_length.try_into().unwrap())?;
        container_buffer.write_u32::<LittleEndian>(size_increase - header_length)?;
        
        Ok(container_buffer)
    }
}

/// Searches for biggest occurence of the input cursor's upcoming bytes in the
/// previously read input bytes.
///
/// Returns slice of search result in the form of
/// 
///     (found_length, found_position)
fn search(input: &Cursor<&[u8]>, input_buffer: &[u8], prev_position_result: Option<u32>) -> (u32, Option<u32>) {
    let mut length_result: usize = BLZ_THRESHOLD;
    let mut position_result: Option<u32> = prev_position_result;
    
    let input_position: usize = input.position().try_into().unwrap();
    
    let max = Ord::min(input_position, BLZ_MAX_OFFSET);
    
    for current_position in 3..=max {
        let length = (0..BLZ_MAX_CODED).find(|current_length| {
            // make sure to not overflow beyond the input buffer
            input_position + *current_length == input_buffer.len()
            // make sure to not go beyond the already read bytes
            || *current_length >= current_position
            // length has been found if it can't be increased anymore
            // without the search result and upcoming input bytes to start diverging
            || input_buffer[input_position + *current_length]
                != input_buffer[input_position + *current_length - current_position]
        }).unwrap_or(BLZ_MAX_CODED);
        
        if length > length_result {
            position_result = Some(current_position.try_into().unwrap());
            length_result = length;
            
            if length == BLZ_MAX_CODED {
                break;
            }
        }
    }
    
    (length_result.try_into().unwrap(), position_result)
}
