use std::{
    fmt::Debug,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    slice,
    str::from_utf8,
};

use anyhow::Result;
use binrw::{
    meta::{EndianKind, ReadEndian, WriteEndian},
    parser, writer, BinRead, BinResult, BinWrite, Endian,
};
use byteorder::{LittleEndian, ReadBytesExt};
use na::Matrix3x4;

use crate::{scoped_reader_pos, util::{math::Vec3, pointer::Pointer}};

use super::bcres::{CgfxCollectionValue, CgfxDict};

#[allow(path_statements)] // to disable warning on `endian;`
#[parser(reader, endian)]
pub fn brw_read_4_byte_string() -> BinResult<String> {
    // I don't need to know the endianness and I can't find a
    // better way to ignore the warning
    endian;
    
    let mut bytes: [u8; 4] = [0; 4];
    reader.read(&mut bytes)?;
    
    Ok(from_utf8(&bytes).unwrap().to_string()) // ughhh error handling is so painful with binrw
}

#[writer(writer, endian)]
pub fn brw_write_4_byte_string(string: &String) -> BinResult<()> {
    let bytes = string.as_bytes();
    let out = u32::from_le_bytes(bytes.try_into().unwrap()); // unwrap because BinResult is a pain
    
    out.write_options(writer, endian, ())?;
    Ok(())
}

fn read_string(read: &mut impl Read) -> Result<String> {
    let mut string_buffer = Vec::new();
    
    loop {
        let b = read.read_u8().unwrap();
        
        if b != 0 {
            string_buffer.push(b);
        } else {
            break;
        }
    }
    
    Ok(String::from_utf8(string_buffer)?)
}

#[parser(reader, endian)]
pub fn brw_read_string() -> BinResult<Option<String>> {
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

#[writer(writer, endian)]
pub fn brw_write_zero(_: &Option<String>) -> BinResult<()> {
    0u32.write_options(writer, endian, ())?;
    Ok(())
}

#[parser(reader, endian)]
pub fn brw_relative_pointer() -> BinResult<Option<Pointer>> {
    let reader_pos = reader.stream_position()?;
    let pointer: u64 = u32::read_options(reader, endian, ())?.into();
    
    if pointer == 0 {
        return Ok(None);
    }
    
    Ok(Some(Pointer::from(reader_pos + pointer)))
}

pub fn read_pointer_list<T: CgfxCollectionValue>(reader: &mut Cursor<&[u8]>, magic: Option<u32>) -> Result<Option<Vec<T>>> {
    let count = reader.read_u32::<LittleEndian>()?;
    let list_ptr = Pointer::read_relative(reader)?;
    println!("a {:?}", list_ptr);
    
    let values: Option<Vec<T>> = if let Some(list_ptr) = list_ptr {
        scoped_reader_pos!(reader);
        let mut values: Vec<T> = Vec::with_capacity(count as usize);
        
        reader.seek(SeekFrom::Start(list_ptr.into()))?;
        
        let object_pointers: Vec<Option<Pointer>> = (0..count)
            .map(|_| Pointer::read_relative(reader))
            .collect::<Result<Vec<Option<Pointer>>>>()?;
        
        for object_pointer in object_pointers {
            if let Some(object_pointer) = object_pointer {
                reader.seek(SeekFrom::Start(object_pointer.into()))?;
                
                if let Some(magic) = magic {
                    assert!(reader.read_u32::<LittleEndian>()? == magic);
                }
                
                values.push(T::read_dict_value(reader)?);
            }
        }
        
        Some(values)
    } else {
        None
    };
    
    Ok(values)
}

pub fn read_inline_list<T: CgfxCollectionValue>(reader: &mut Cursor<&[u8]>) -> Result<Option<Vec<T>>> {
    let count = reader.read_u32::<LittleEndian>()?;
    let list_ptr = Pointer::read(reader)?;
    
    let values: Option<Vec<T>> = if let Some(list_ptr) = list_ptr {
        scoped_reader_pos!(reader);
        
        reader.seek(SeekFrom::Current(i64::from(list_ptr) - 4))?;
        
        let values: Vec<T> = (0..count)
            .map(|_| T::read_dict_value(reader))
            .collect::<Result<Vec<T>>>()?;
        
        Some(values)
    } else {
        None
    };
    
    Ok(values)
}

#[derive(Debug, Clone, BinRead, BinWrite)]
// vvv required because brw_write_4_byte_string might panic otherwise
#[brw(assert(magic.bytes().len() == 4, "Length of magic number {:?} must be 4 bytes", magic))]
#[br(assert(metadata_pointer == None, "CgfxTexture {:?} has metadata {:?}", name, metadata_pointer))]
#[brw(little)]
pub struct CgfxObjectHeader {
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
}

#[derive(Debug, Clone, BinRead, BinWrite)]
#[brw(little)]
pub struct CgfxNodeHeader {
    pub branch_visible: u32,
    pub is_branch_visible: u32,
    
    pub child_count: u32,
    pub children_pointer: Option<Pointer>,
    
    #[brw(ignore)]
    pub anim_groups: CgfxDict<()>,
    
    anim_group_count: u32,
    anim_group_pointer: Option<Pointer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CgfxTransform {
    pub scale: Vec3,
    pub rotation: Vec3,
    pub translation: Vec3,
    
    pub local_transform: Matrix3x4<f32>,
    pub world_transform: Matrix3x4<f32>,
}

impl BinRead for CgfxTransform {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, endian: Endian, args: Self::Args<'_>) -> BinResult<Self> {
        let numbers_result: Result<Vec<f32>, binrw::Error> = (0..33)
            .map(|_| f32::read_options(reader, endian, args))
            .collect();
        
        let numbers = numbers_result?;
        
        Ok(Self {
            scale: Vec3::new(numbers[0], numbers[1], numbers[2]),
            rotation: Vec3::new(numbers[3], numbers[4], numbers[5]),
            translation: Vec3::new(numbers[6], numbers[7], numbers[8]),
            
            local_transform: Matrix3x4::from_row_slice(&numbers[9..21]),
            world_transform: Matrix3x4::from_row_slice(&numbers[21..33]),
        })
    }
}

impl ReadEndian for CgfxTransform {
    const ENDIAN: EndianKind = EndianKind::Endian(Endian::Little);
}

impl BinWrite for CgfxTransform {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        // sorry 1 person trying to use this on a big endian machine (?)
        assert!(endian == Endian::Little);
        
        let vec_numbers: [f32; 9] = [
            self.scale.x,
            self.scale.y,
            self.scale.z,
            self.rotation.x,
            self.rotation.y,
            self.rotation.z,
            self.translation.x,
            self.translation.y,
            self.translation.z,
        ];
        
        let local_numbers = self.local_transform.data.as_slice();
        let world_numbers = self.world_transform.data.as_slice();
        
        let vec_bytes: &[u8] = unsafe {
            slice::from_raw_parts(vec_numbers.as_ptr() as *const u8, vec_numbers.len() * 4)
        };
        
        let local_bytes: &[u8] = unsafe {
            slice::from_raw_parts(local_numbers.as_ptr() as *const u8, local_numbers.len() * 4)
        };
        
        let world_bytes: &[u8] = unsafe {
            slice::from_raw_parts(world_numbers.as_ptr() as *const u8, world_numbers.len() * 4)
        };
        
        writer.write(vec_bytes)?;
        writer.write(local_bytes)?;
        writer.write(world_bytes)?;
        
        Ok(())
    }
}

impl WriteEndian for CgfxTransform {
    const ENDIAN: EndianKind = EndianKind::Endian(Endian::Little);
}
