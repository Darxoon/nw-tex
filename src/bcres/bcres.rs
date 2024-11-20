use std::{collections::HashMap, io::{Cursor, Read, Seek, SeekFrom, Write}, str::from_utf8};

use anyhow::Result;
use binrw::{BinRead, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{assert_matching, get_4_byte_string, scoped_reader_pos, util::pointer::Pointer, write_at_pointer};

use super::{model::CgfxModel, texture::CgfxTexture};

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

pub struct WriteContext {
    string_section: String,
    string_references: HashMap<Pointer, String>,
    
    image_section: Vec<u8>,
    // keys in image_references are relative to entire file
    // values are relative to the image section
    image_references: HashMap<Pointer, Pointer>,
}

impl WriteContext {
    pub fn new() -> Self {
        WriteContext {
            string_section: String::new(),
            string_references: HashMap::new(),
            image_section: Vec::new(),
            image_references: HashMap::new(),
        }
    }
    
    pub fn add_string(&mut self, string: &str) -> Result<()> {
        if self.string_section.find(string).is_some() {
            // string exists already, exiting early
            return Ok(());
        }
        
        self.string_section.push_str(string);
        self.string_section.push('\0');
        Ok(())
    }
    
    pub fn add_string_reference(&mut self, origin: Pointer, target_string: String) {
        self.string_references.insert(origin, target_string);
    }
    
    pub fn append_to_image_section(&mut self, content: &[u8]) -> Result<()> {
        // because binrw overwrites Vec::write
        // that's why you don't use "write" as a function name for a method
        // you are extending almost every fucking collection with
        Write::write(&mut self.image_section, content)?;
        Ok(())
    }
    
    pub fn add_image_reference_to_current_end(&mut self, origin: Pointer) -> Result<()> {
        self.image_references.insert(origin, self.image_section.len().try_into()?);
        Ok(())
    }
}

pub trait CgfxCollectionValue : Sized {
    // TODO: migrate this to use impl Read + Seek instead of Cursor
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self>;
    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, ctx: &mut WriteContext) -> Result<()>;
}

// auto implement CgfxCollectionValue for all binrw types
impl<T: BinRead + BinWrite> CgfxCollectionValue for T
where 
    for<'a> <T as BinRead>::Args<'a>: Default,
    for<'a> <T as BinWrite>::Args<'a>: Default,
{
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self::read_le(reader)?)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _ctx: &mut WriteContext) -> Result<()> {
        self.write_le(writer)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct CgfxNode<T: CgfxCollectionValue> {
    pub reference_bit: u32,
    pub left_node_index: u16,
    pub right_node_index: u16,
    
    pub name: Option<String>,
    pub value: Option<T>,
    
    file_offset: Pointer,
    name_pointer: Option<Pointer>,
    value_pointer: Option<Pointer>,
}

impl<T: CgfxCollectionValue> CgfxNode<T> {
    pub fn from_reader(reader: &mut impl Read, start_file_offset: Pointer) -> Result<Self> {
        let file_offset = start_file_offset;
        
        let reference_bit = reader.read_u32::<LittleEndian>()?;
        let left_node_index = reader.read_u16::<LittleEndian>()?;
        let right_node_index = reader.read_u16::<LittleEndian>()?;
        
        let name_pointer = Pointer::read(reader)?;
        let value_pointer = Pointer::read(reader)?;
        
        Ok(CgfxNode {
            reference_bit,
            left_node_index,
            right_node_index,
            
            name: None,
            value: None,
            
            file_offset,
            name_pointer,
            value_pointer,
        })
    }
    
    pub fn to_writer(&self, writer: &mut Cursor<&mut Vec<u8>>, ctx: &mut WriteContext) -> Result<Pointer> {
        writer.write_u32::<LittleEndian>(self.reference_bit)?;
        writer.write_u16::<LittleEndian>(self.left_node_index)?;
        writer.write_u16::<LittleEndian>(self.right_node_index)?;
        
        // name pointer and value pointer, write zero for now and patch it back later
        let name_pointer_location = Pointer::try_from(&writer)?;
        writer.write_u32::<LittleEndian>(0)?;
        let value_pointer_location = Pointer::try_from(&writer)?;
        writer.write_u32::<LittleEndian>(0)?;
        
        if let Some(name) = &self.name {
            ctx.add_string(&name)?;
            ctx.add_string_reference(name_pointer_location, name.clone());
        }
        
        Ok(value_pointer_location)
    }
}

#[derive(Debug, Default, Clone)]
pub struct CgfxDict<T: CgfxCollectionValue> {
    pub magic_number: String,
    pub tree_length: u32,
    pub values_count: u32,
    pub nodes: Vec<CgfxNode<T>>,
}

impl<T: CgfxCollectionValue> CgfxDict<T> {
    pub fn from_buffer(buffer: &[u8], start_position: Pointer) -> Result<Self> {
        let mut cursor = Cursor::new(buffer);
        cursor.set_position(start_position.into());
        
        Self::from_reader(&mut cursor)
    }
    
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let magic_number = get_4_byte_string(reader)?;
        let tree_length = reader.read_u32::<LittleEndian>()?;
        let values_count = reader.read_u32::<LittleEndian>()?;
        
        let nodes_result: Result<Vec<CgfxNode<T>>> = (0..values_count + 1)
            .map(|_| CgfxNode::from_reader(reader, Pointer::try_from(&reader)?))
            .collect();
        
        let mut nodes = nodes_result?;
        
        for node in &mut nodes {
            if let Some(name_pointer) = node.name_pointer {
                scoped_reader_pos!(reader);
                
                let string_offset: Pointer = node.file_offset + 8 + name_pointer;
                reader.seek(SeekFrom::Start(string_offset.into()))?;
                
                node.name = Some(read_string(reader)?);
            }
            
            if let Some(value_pointer) = node.value_pointer {
                scoped_reader_pos!(reader);
                
                let value_offset: Pointer = node.file_offset + 12 + value_pointer;
                reader.seek(SeekFrom::Start(value_offset.into()))?;
                
                node.value = Some(T::read_dict_value(reader)?);
            }
        }
        
        Ok(CgfxDict {
            magic_number,
            tree_length,
            values_count,
            nodes,
        })
    }
    
    pub fn to_writer(&self, writer: &mut Cursor<&mut Vec<u8>>, ctx: &mut WriteContext) -> Result<()> {
        assert!(self.values_count + 1 == self.nodes.len() as u32, "values_count does not match node count");
        
        write!(writer, "{}", self.magic_number)?;
        writer.write_u32::<LittleEndian>(self.tree_length)?;
        writer.write_u32::<LittleEndian>(self.values_count)?;
        
        for node in &self.nodes {
            let value_pointer_location = node.to_writer(writer, ctx)?;
            
            // TODO: when are the values serialized? here or in a separate loop
            if let Some(value) = &node.value {
                // update value pointer to point to current location
                let current_offset = Pointer::try_from(&writer)?;
                let relative_value_offset = current_offset - value_pointer_location;
                
                write_at_pointer(writer, value_pointer_location, relative_value_offset.into())?;
                
                // write value
                value.write_dict_value(writer, ctx)?;
            }
        }
        
        
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, BinRead, BinWrite)]
#[brw(little, magic = b"CGFX")]
pub struct CgfxHeader {
    pub byte_order_mark: u16,
    pub header_length: u16,
    pub revision: u32,
    pub file_length: u32,
    pub sections_count: u32,
    
    #[br(assert(content_magic_number == 0x41544144u32,
        "Invalid magic number for data, expected 'DATA' but got '{}'",
        from_utf8(&content_magic_number.to_le_bytes()).unwrap()))]
    pub content_magic_number: u32,
    pub content_length: u32,
}

#[derive(Debug)]
pub struct CgfxContainer {
    pub header: CgfxHeader,
    
    pub models: Option<CgfxDict<CgfxModel>>,
    pub textures: Option<CgfxDict<CgfxTexture>>,
    pub luts: Option<CgfxDict<()>>,
    pub materials: Option<CgfxDict<()>>,
    pub shaders: Option<CgfxDict<()>>,
    pub cameras: Option<CgfxDict<()>>,
    pub lights: Option<CgfxDict<()>>,
    pub fogs: Option<CgfxDict<()>>,
    pub scenes: Option<CgfxDict<()>>,
    pub skeletal_animations: Option<CgfxDict<()>>,
    pub material_animations: Option<CgfxDict<()>>,
    pub visibility_animations: Option<CgfxDict<()>>,
    pub camera_animations: Option<CgfxDict<()>>,
    pub light_animations: Option<CgfxDict<()>>,
    pub fog_animations: Option<CgfxDict<()>>,
    pub emitters: Option<CgfxDict<()>>,
}

impl CgfxContainer {
    pub fn new(buffer: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(buffer);
        
        let header = CgfxHeader::read(&mut cursor)?;
        let mut dict_references: [(u32, Option<Pointer>); 16] = [Default::default(); 16];
        
        for i in 0..16 {
            let position = Pointer::try_from(&cursor)?;
            
            dict_references[i] = (
                cursor.read_u32::<LittleEndian>()?,
                Pointer::read(&mut cursor)?.map(|pointer| pointer + position + 4),
            );
        }
        
        let mut unit_dicts: [Option<CgfxDict<()>>; 16] = Default::default();
        
        for (i, (count, offset)) in dict_references.into_iter().enumerate() {
            // textures
            if i == 1 {
                continue;
            }
            
            let dict = match offset {
                Some(value) => Some(CgfxDict::from_buffer(buffer, value)?),
                None => None,
            };
            
            if let Some(dict) = &dict {
                assert_eq!(dict.nodes.len(), (count + 1).try_into().unwrap());
            } else {
                assert_eq!(count, 0);
            }
            
            unit_dicts[i] = dict;
        }
        
        let mut unit_dicts_iter = unit_dicts.into_iter();
        
        let models = match dict_references[0].1 {
            Some(pointer) => Some(CgfxDict::<CgfxModel>::from_buffer(buffer, pointer)?),
            None => None,
        };
        
        let textures = match dict_references[1].1 {
            Some(pointer) => Some(CgfxDict::<CgfxTexture>::from_buffer(buffer, pointer)?),
            None => None,
        };
        
        Ok(CgfxContainer {
            header,
            
            models,
            textures,
            luts: unit_dicts_iter.nth(2).unwrap(),
            materials: unit_dicts_iter.next().unwrap(),
            shaders: unit_dicts_iter.next().unwrap(),
            cameras: unit_dicts_iter.next().unwrap(),
            lights: unit_dicts_iter.next().unwrap(),
            fogs: unit_dicts_iter.next().unwrap(),
            scenes: unit_dicts_iter.next().unwrap(),
            skeletal_animations: unit_dicts_iter.next().unwrap(),
            material_animations: unit_dicts_iter.next().unwrap(),
            visibility_animations: unit_dicts_iter.next().unwrap(),
            camera_animations: unit_dicts_iter.next().unwrap(),
            light_animations: unit_dicts_iter.next().unwrap(),
            fog_animations: unit_dicts_iter.next().unwrap(),
            emitters: unit_dicts_iter.next().unwrap(),
        })
    }
    
    pub fn to_buffer(&self)  -> Result<Vec<u8>> {
        self.to_buffer_debug(None)
    }
    
    pub fn to_buffer_debug(&self, original: Option<&[u8]>) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut writer = Cursor::new(&mut out);
        
        self.header.write(&mut writer)?;
        assert_matching!(writer, original);
        
        // write zeroes for all dicts for now and patch them later
        let dict_pointers_location = Pointer::try_from(&writer)?;
        
        for _ in 0..16 {
            writer.write_u32::<LittleEndian>(0)?;
            writer.write_u32::<LittleEndian>(0)?;
        }
        
        // write main content
        let mut ctx = WriteContext::new();
        
        if let Some(textures) = &self.textures {
            // write reference in dict pointer array above
            let reference_offset: Pointer = dict_pointers_location + 8;
            
            let current_offset: Pointer = Pointer::try_from(&writer)?;
            let relative_offset: Pointer = current_offset - (reference_offset + 4);
            let count = textures.nodes.len() - 1;
            
            write_at_pointer(&mut writer, reference_offset, count.try_into()?)?;
            write_at_pointer(&mut writer, reference_offset + 4, relative_offset.into())?;
            
            // write dict
            textures.to_writer(&mut writer, &mut ctx)?;
        }
        
        // apply string references
        let string_section_start = Pointer::try_from(&writer)?;
        
        for (location, target_string) in ctx.string_references {
            if let Some(string_offset_usize) = ctx.string_section.find(&target_string) {
                let string_offset = Pointer::from(string_offset_usize) + string_section_start;
                let relative_offset = string_offset - location;
                
                write_at_pointer(&mut writer, location, relative_offset.into())?;
            }
        }
        
        // write strings
        writer.write(ctx.string_section.as_bytes())?;
        
        // apply padding
        let alignment: i32 = 128;
        let buffer_size: i32 = writer.position().try_into()?;
        let padding_size = ((-buffer_size - 8) % alignment + alignment) % alignment; // weird padding calculation
        
        writer.write(&vec![0u8; padding_size.try_into()?])?;
        
        // apply image section references
        let image_section_offset: Pointer = Pointer::try_from(&writer)? + 8;
        
        for (location, image_offset) in ctx.image_references {
            let absolute_offset = image_section_offset + image_offset;
            let relative_offset = absolute_offset - location;
            
            write_at_pointer(&mut writer, location, relative_offset.into())?;
        }
        
        assert_matching!(writer, original);
        
        // write image data section
        let image_section_length: u32 = ctx.image_section.len().try_into()?;
        
        writer.write(b"IMAG")?;
        writer.write_u32::<LittleEndian>(image_section_length + 8)?;
        
        writer.write(&ctx.image_section)?;
        
        assert_matching!(writer, original);
        assert!(writer.get_ref().len() == self.header.file_length as usize,
            "Written file size does not match expected file size, expected 0x{:x} bytes but got 0x{:x} bytes",
            self.header.file_length,
            writer.get_ref().len());
        
        Ok(out)
    }
}
