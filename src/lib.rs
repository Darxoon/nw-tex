use std::{
    io::{Cursor, Read, Seek, SeekFrom, Write},
    str::from_utf8,
};

use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use util::pointer::Pointer;

pub mod util;

fn get_string(bytes: &[u8], start: Pointer) -> Result<String> {
	let bytes_slice = &bytes[start.into()..];
	let null_position_from_start = bytes_slice.iter().position(|&x| x == 0x0);
	
	let string = if let Some(null_position_from_start) = null_position_from_start {
		from_utf8(&bytes_slice[..null_position_from_start])?
	} else {
		from_utf8(bytes_slice)?
	};
	
	Ok(string.to_owned())
}

pub fn get_4_byte_string(reader: &mut impl Read) -> Result<String> {
	let mut bytes: [u8; 4] = [0; 4];
	reader.read(&mut bytes)?;
	
	Ok(from_utf8(&bytes)?.to_string())
}

pub fn write_at_pointer<W: Write + Seek>(writer: &mut W, pointer: Pointer, value: u32) -> Result<()> {
	let current_offset = writer.stream_position()?;
	
	writer.seek(SeekFrom::Start(pointer.into()))?;
	writer.write_u32::<LittleEndian>(value)?;
	
	writer.seek(SeekFrom::Start(current_offset))?;
	
	Ok(())
}

#[macro_export]
macro_rules! assert_matching {
	($writer:ident, $base_option:ident) => {
		if let Some(base) = $base_option {
            assert!(&***$writer.get_ref() == &base[..$writer.get_ref().len()], "Not matching");
        }
	};
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryItem {
	pub id: String,
	pub file_offset: u32,
	pub field_0x8: u32,
	pub byte_length: u32,
}

impl RegistryItem {
	pub fn read(reader: &mut impl Read, get_string: &impl Fn(Pointer) -> Result<String>) -> Result<Self> {
		let id_pointer = Pointer::read(reader)?
			.unwrap_or(Pointer::default());
		let file_offset = reader.read_u32::<LittleEndian>()?;
		let field_0x8 = reader.read_u32::<LittleEndian>()?;
		let byte_length = reader.read_u32::<LittleEndian>()?;
        
		// TODO: dangerous magic number
		let id = get_string(id_pointer + 0x1e64)?;
		
		Ok(Self {
			id,
			file_offset,
			field_0x8,
			byte_length,
		})
	}
	
	pub fn write(&self, writer: &mut impl Write, write_string: &mut impl FnMut(&str) -> Pointer) -> Result<()> {
		let id_pointer = write_string(&self.id);
		id_pointer.write(writer)?;
		
		writer.write_u32::<LittleEndian>(self.file_offset)?;
		writer.write_u32::<LittleEndian>(self.field_0x8)?;
		writer.write_u32::<LittleEndian>(self.byte_length)?;
		
		Ok(())
	}
}

pub struct ArchiveRegistry {
	pub items: Vec<RegistryItem>,
}

impl ArchiveRegistry {
	pub fn new(buffer: &[u8]) -> Result<Self> {
		let get_string = |ptr| get_string(buffer, ptr);
		let mut cursor = Cursor::new(buffer);
		
		let item_count = cursor.read_u32::<LittleEndian>()?;
		let mut items = Vec::default();
		
		for _ in 0..item_count {
			items.push(RegistryItem::read(&mut cursor, &get_string)?);
		}
		
        Ok(ArchiveRegistry { items })
	}
	
	pub fn to_buffer(&self) -> Result<Vec<u8>> {
		let mut main_buffer: Vec<u8> = Vec::new();
		let mut string_buffer: Vec<u8> = Vec::new();
		
		let mut write_string = |string: &str| {
			let current_offset: Pointer = string_buffer.len().into();
			
			string_buffer.extend(string.bytes());
			string_buffer.extend([0].iter());
			
			current_offset
		};
		
		main_buffer.write_u32::<LittleEndian>(self.items.len().try_into().unwrap())?;
		
		for item in &self.items {
			item.write(&mut main_buffer, &mut write_string)?;
		}
		
		main_buffer.extend(string_buffer);
		
		Ok(main_buffer)
	}
	
	pub fn to_yaml(&self) -> Result<String> {
		let yaml = serde_yaml::to_string(&self.items)?;
		Ok(yaml)
	}
	
	pub fn from_yaml(yaml: &str) -> Result<Self> {
		let items: Vec<RegistryItem> = serde_yaml::from_str(yaml)?;
		Ok(ArchiveRegistry { items })
	}
}
