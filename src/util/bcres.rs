use std::io::{Read, Cursor};

use anyhow::Result;
use byteorder::{ReadBytesExt, LittleEndian};

use super::pointer::Pointer;

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

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CgfxHeader {
    pub magic_number: u32,
    pub byte_order_mark: u16,
    pub header_length: u16,
    pub revision: u32,
    pub file_length: u32,
    pub sections_count: u32,
    
    pub content_magic_number: u32,
    pub content_length: u32,
}

impl CgfxHeader {
    pub fn from_reader(reader: &mut impl Read) -> Result<Self> {
        let magic_number = reader.read_u32::<LittleEndian>()?;
        let byte_order_mark = reader.read_u16::<LittleEndian>()?;
        let header_length = reader.read_u16::<LittleEndian>()?;
        let revision = reader.read_u32::<LittleEndian>()?;
        let file_length = reader.read_u32::<LittleEndian>()?;
        let sections_count = reader.read_u32::<LittleEndian>()?;
        let content_magic_number = reader.read_u32::<LittleEndian>()?;
        let content_length = reader.read_u32::<LittleEndian>()?;
        
        Ok(CgfxHeader {
            magic_number,
            byte_order_mark,
            header_length,
            revision,
            file_length,
            sections_count,
            content_magic_number,
            content_length,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TableReference {
    pub count: u32,
    pub table: Option<Pointer>,
}

impl TableReference {
    pub fn from_reader(reader: &mut impl Read) -> Result<Self> {
        let count = reader.read_u32::<LittleEndian>()?;
        let table = Pointer::read(reader)?;
        
        Ok(TableReference {
            count,
            table,
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct CgfxNode {
    pub reference_bit: u32,
    pub left_node_index: u16,
    pub right_node_index: u16,
    
    pub name: Option<String>,
    // TODO: add generic value field and remove dead code suppression ^^^
    
    file_offset: Pointer,
    name_pointer: Option<Pointer>,
    value_pointer: Option<Pointer>,
}

impl CgfxNode {
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
            
            file_offset,
            name_pointer,
            value_pointer,
        })
    }
}

#[derive(Debug, Default)]
pub struct CgfxDict {
    pub magic_number: u32,
    pub tree_length: u32,
    pub values_count: u32,
    pub nodes: Vec<CgfxNode>,
}

impl CgfxDict {
    pub fn from_buffer(buffer: &[u8], start_position: Pointer) -> Result<Self> {
        let mut cursor = Cursor::new(buffer);
        cursor.set_position(start_position.into());
        
        Self::from_reader(&mut cursor)
    }
    
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let magic_number = reader.read_u32::<LittleEndian>().unwrap();
        let tree_length = reader.read_u32::<LittleEndian>()?;
        let values_count = reader.read_u32::<LittleEndian>()?;
        
        let nodes_result: Result<Vec<CgfxNode>> = (0..values_count + 1)
            .map(|_| CgfxNode::from_reader(reader, Pointer::try_from(&reader)?))
            .collect();
        
        let mut nodes = nodes_result?;
        
        for node in &mut nodes {
            if let Some(name_pointer) = node.name_pointer {
                let string_offset: Pointer = node.file_offset + 8 + name_pointer;
                
                let mut string_reader = reader.clone();
                string_reader.set_position(string_offset.into());
                
                node.name = Some(read_string(&mut string_reader)?);
            }
        }
        
        Ok(CgfxDict {
            magic_number,
            tree_length,
            values_count,
            nodes,
        })
    }
}

#[derive(Debug)]
pub struct CgfxContainer {
    pub header: CgfxHeader,
    
    // TODO: replace with actual Table struct when table parsing is done
    pub models: Option<CgfxDict>,
    pub textures: Option<CgfxDict>,
    pub luts: Option<CgfxDict>,
    pub materials: Option<CgfxDict>,
    pub shaders: Option<CgfxDict>,
    pub cameras: Option<CgfxDict>,
    pub lights: Option<CgfxDict>,
    pub fogs: Option<CgfxDict>,
    pub scenes: Option<CgfxDict>,
    pub skeletal_animations: Option<CgfxDict>,
    pub material_animations: Option<CgfxDict>,
    pub visibility_animations: Option<CgfxDict>,
    pub camera_animations: Option<CgfxDict>,
    pub light_animations: Option<CgfxDict>,
    pub fog_animations: Option<CgfxDict>,
    pub emitters: Option<CgfxDict>,
}

impl CgfxContainer {
    pub fn new(buffer: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(buffer);
        
        let header = CgfxHeader::from_reader(&mut cursor)?;
        let mut dict_references: [(u32, Option<Pointer>); 16] = [Default::default(); 16];
        
        for i in 0..16 {
            let position = Pointer::try_from(&cursor)?;
            
            dict_references[i] = (
                cursor.read_u32::<LittleEndian>()?,
                Pointer::read(&mut cursor)?.map(|pointer| pointer + position + 4),
            );
        }
        
        let mut dicts: [Option<CgfxDict>; 16] = Default::default();
        
        for (i, (count, offset)) in dict_references.into_iter().enumerate() {
            let dict = match offset {
                Some(value) => Some(CgfxDict::from_buffer(buffer, value)?),
                None => None,
            };
            
            println!("count: {}, dict: {:#?}", count, &dict);
            dicts[i] = dict;
        }
        
        // TODO: parse tables
        
        let mut dicts_iter = dicts.into_iter();
        
        Ok(CgfxContainer {
            header,
            
            models: dicts_iter.next().unwrap(),
            textures: dicts_iter.next().unwrap(),
            luts: dicts_iter.next().unwrap(),
            materials: dicts_iter.next().unwrap(),
            shaders: dicts_iter.next().unwrap(),
            cameras: dicts_iter.next().unwrap(),
            lights: dicts_iter.next().unwrap(),
            fogs: dicts_iter.next().unwrap(),
            scenes: dicts_iter.next().unwrap(),
            skeletal_animations: dicts_iter.next().unwrap(),
            material_animations: dicts_iter.next().unwrap(),
            visibility_animations: dicts_iter.next().unwrap(),
            camera_animations: dicts_iter.next().unwrap(),
            light_animations: dicts_iter.next().unwrap(),
            fog_animations: dicts_iter.next().unwrap(),
            emitters: dicts_iter.next().unwrap(),
        })
    }
}
