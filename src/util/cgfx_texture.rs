use std::{fmt::Debug, io::{Cursor, Read, Seek, SeekFrom}};

use anyhow::{Result, Error};
use binrw::{parser, BinRead, BinResult};
use byteorder::{ReadBytesExt, LittleEndian};

use super::{pointer::Pointer, bcres::CgfxDictValue};

fn read_string(read: &mut impl Read) -> Result<String> {
	let mut string_buffer = Vec::new();
	
	loop {
		let b = read.read_u8()?;
		
		if b != 0 {
			string_buffer.push(b);
		} else {
			break;
		}
	}
	
	Ok(String::from_utf8(string_buffer)?)
}

#[parser(reader, endian)]
fn brw_read_string() -> BinResult<Option<String>> {
    let reader_pos = reader.stream_position()?;
    let pointer: u64 = u32::read_options(reader, endian, ())?.into();
    
    if pointer == 0 {
        return Ok(None);
    }
    
    reader.seek(SeekFrom::Start(reader_pos + pointer))?;
    
    let string = read_string(reader)
        .map_err(|err| binrw::Error::Custom {
            pos: reader.stream_position().unwrap(),
            err: Box::new(err),
        })?;
    
    reader.seek(SeekFrom::Start(reader_pos + 4))?;
    
    Ok(Some(string))
}

#[parser(reader, endian)]
fn brw_relative_pointer() -> BinResult<Option<Pointer>> {
    let reader_pos = reader.stream_position()?;
    let pointer: u64 = u32::read_options(reader, endian, ())?.into();
    
    if pointer == 0 {
        return Ok(None);
    }
    
    Ok(Some(Pointer::from(reader_pos + pointer)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
#[br(repr(u32), little)]
pub enum PicaTextureFormat {
    RGBA8,
    RGB8,
    RGBA5551,
    RGB565,
    RGBA4,
    LA8,
    HiLo8,
    L8,
    A8,
    LA4,
    L4,
    A4,
    ETC1,
    ETC1A4,
}

#[derive(Clone, PartialEq, Eq, BinRead)]
#[brw(little, assert(location_ptr == 0, "ImageData has location_ptr {}", location_ptr))]
pub struct ImageData {
    pub height: u32,
    pub width: u32,
    
    #[brw(ignore)]
    pub image_bytes: Vec<u8>,
    
    buffer_length: u32,
    #[br(parse_with = brw_relative_pointer)]
    buffer_pointer: Option<Pointer>,
    
    pub dynamic_alloc: u32,
    pub bits_per_pixel: u32,
    pub location_ptr: u32, // ?
    pub memory_area: u32,
}

impl Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageData")
            .field("height", &self.height)
            .field("width", &self.width)
            .field("image_bytes", &format!("<buffer, {} bytes>", self.image_bytes.len()))
            .field("buffer_length", &self.buffer_length)
            .field("buffer_pointer", &self.buffer_pointer)
            .field("dynamic_alloc", &self.dynamic_alloc)
            .field("bits_per_pixel", &self.bits_per_pixel)
            .field("location_ptr", &self.location_ptr)
            .field("memory_area", &self.memory_area)
            .finish()
    }
}

#[derive(Debug, BinRead)]
#[brw(little, assert(metadata_pointer == None, "CgfxTexture {:?} has metadata {:?}", name, metadata_pointer))]
pub struct CgfxTextureCommon {
    // cgfx object header
    pub magic: u32,
    pub revision: u32,
    
    #[br(parse_with = brw_read_string)]
    pub name: Option<String>,
    pub metadata_count: u32,
    
    #[br(map = |x: u32| Pointer::new(x))]
    pub metadata_pointer: Option<Pointer>,
    
    // common texture fields
    pub height: u32,
    pub width: u32,
    pub gl_format: u32,
    pub gl_type: u32,
    pub mipmap_size: u32,
    pub texture_obj: u32,
    pub location_flag: u32,
    pub texture_format: PicaTextureFormat,
}

#[derive(Debug)]
pub enum CgfxTexture {
    Cube(CgfxTextureCommon, Vec<ImageData>),
    Image(CgfxTextureCommon, Option<ImageData>),
}

fn image_data(reader: &mut Cursor<&[u8]>) -> Result<Option<ImageData>> {
    let image_data_pointer = Pointer::read(reader)?;
    
    let data = image_data_pointer
        .map(|pointer| {
            let mut data_reader = reader.clone();
            data_reader.seek(SeekFrom::Current(i64::from(pointer) - 4))?;
            
            let mut data = ImageData::read(&mut data_reader)?;
            data_reader.set_position(data.buffer_pointer.unwrap().into());
            
            let mut image_bytes: Vec<u8> = vec![0; data.buffer_length.try_into()?];
            data_reader.read_exact(&mut image_bytes)?;
            data.image_bytes = image_bytes;
            
            Ok::<ImageData, Error>(data)
        })
        .transpose()?;
    
    Ok(data)
}

impl CgfxTexture {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let texture_type_discriminant = reader.read_u32::<LittleEndian>()?;
        
        let common = CgfxTextureCommon::read(reader)?;
        
        let result = match texture_type_discriminant {
            0x20000009 => CgfxTexture::Cube(common, {
                let mut images = Vec::with_capacity(6);
                
                for _ in 0..6 {
                    images.push(image_data(reader)?.unwrap());
                }
                
                images
            }),
            0x20000011 => CgfxTexture::Image(common, image_data(reader)?),
            
            _ => return Err(Error::msg(format!("Invalid Texture discriminant {:x}", texture_type_discriminant)))
        };
        
        Ok(result)
    }
}

impl CgfxDictValue for CgfxTexture {
    fn read(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }
}
