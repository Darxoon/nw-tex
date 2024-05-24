use std::{fmt::Debug, io::{Cursor, Read, Seek, SeekFrom}, str::from_utf8};

use anyhow::{Result, Error};
use binrw::{parser, writer, BinRead, BinResult, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use super::{bcres::{CgfxDictValue, WriteContext}, pointer::Pointer};

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

#[allow(path_statements)] // to disable warning on `endian;`
#[parser(reader, endian)]
fn brw_read_4_byte_string() -> BinResult<String> {
    // I don't need to know the endianness and I can't find a
    // better way to ignore the warning
    endian;
    
    let mut bytes: [u8; 4] = [0; 4];
	reader.read(&mut bytes)?;
	
	Ok(from_utf8(&bytes).unwrap().to_string()) // ughhh error handling is so painful with binrw
}

#[allow(path_statements)] // to disable warning on `endian;`
#[writer(writer, endian)]
fn brw_write_4_byte_string(string: &String) -> BinResult<()> {
    let bytes = string.as_bytes();
    let out = u32::from_le_bytes(bytes.try_into().unwrap()); // unwrap because BinResult is a pain
	
	out.write_options(writer, endian, ())?;
    Ok(())
}

#[writer(writer, endian)]
fn brw_write_zero(_: &Option<String>) -> BinResult<()> {
    0u32.write_options(writer, endian, ())?;
    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead, BinWrite)]
#[brw(repr(u32), little)]
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

#[derive(Clone, PartialEq, Eq, BinRead, BinWrite)]
#[brw(little)]
#[br(assert(location_ptr == 0, "ImageData has location_ptr {}", location_ptr))]
pub struct ImageData {
    pub height: u32,
    pub width: u32,
    
    #[brw(ignore)]
    pub image_bytes: Vec<u8>,
    
    buffer_length: u32,
    #[br(parse_with = brw_relative_pointer)]
    #[bw(map = |_| 0u32)]
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

#[derive(Debug, BinRead, BinWrite)]
// vvv required because brw_write_4_byte_string might panic otherwise
#[brw(assert(magic.bytes().len() == 4, "Length of magic number {:?} must be 4 bytes", magic))]
#[br(assert(metadata_pointer == None, "CgfxTexture {:?} has metadata {:?}", name, metadata_pointer))]
#[brw(little)]
pub struct CgfxTextureCommon {
    // cgfx object header
    #[br(parse_with = brw_read_4_byte_string)]
    #[bw(write_with = brw_write_4_byte_string)]
    pub magic: String,
    pub revision: u32,
    
    #[br(parse_with = brw_read_string)]
    #[bw(write_with = brw_write_zero)]
    pub name: Option<String>,
    pub metadata_count: u32,
    
    #[br(map = |x: u32| Pointer::new(x))]
    #[bw(map = |x: &Option<Pointer>| x.map_or(0, |ptr| ptr.0))]
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
    
    pub fn to_writer(&self, writer: &mut Cursor<&mut Vec<u8>>, ctx: &mut WriteContext) -> Result<()> {
        // write discriminant
        let discriminant: u32 = match self {
            CgfxTexture::Cube(_, _) => 0x20000009,
            CgfxTexture::Image(_, _) => 0x20000011,
        };
        
        writer.write_u32::<LittleEndian>(discriminant)?;
        
        // write common stuff
        let common = match self {
            CgfxTexture::Cube(common, _) => common,
            CgfxTexture::Image(common, _) => common,
        };
        
        let common_offset = Pointer::try_from(&writer)?;
        let name_offset = common_offset + 8;
        assert!(common.metadata_pointer == None);
        
        if let Some(name) = &common.name {
            ctx.add_string(name)?;
            ctx.add_string_reference(name_offset, name.clone());
        }
        
        common.write(writer)?;
        
        // write texture specific stuff
        match self {
            CgfxTexture::Cube(_, _images) => todo!(),
            CgfxTexture::Image(_, image) => {
                writer.write_u32::<LittleEndian>(4)?;
                
                if let Some(image) = image {
                    let current_offset = Pointer::try_from(&writer)?;
                    ctx.add_image_reference_to_current_end(current_offset + 12)?;
                    ctx.append_to_image_section(&image.image_bytes)?;
                }
                
                // when are they serialized? here or after the textures in general?
                image.write(writer)?;
            },
        }
        
        Ok(())
    }
}

impl CgfxDictValue for CgfxTexture {
    fn read(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }
    
    fn write(&self, writer: &mut Cursor<&mut Vec<u8>>, ctx: &mut WriteContext) -> Result<()> {
        self.to_writer(writer, ctx)
    }
}
