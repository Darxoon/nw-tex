use std::{io::{Cursor, Read}, str::from_utf8, fmt::Write};

use anyhow::{Result, Error};
use byteorder::{ReadBytesExt, LittleEndian};
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

pub struct RegistryItem {
	pub id: String,
	pub file_offset: u32,
	pub field_0x8: u32,
	pub field_0xc: u32,
}

impl RegistryItem {
	pub fn read(reader: &mut impl Read, get_string: &impl Fn(Pointer) -> Result<String>) -> Result<Self> {
		let id_pointer = Pointer::read(reader)?
			.unwrap_or(Pointer::default());
		let file_offset = reader.read_u32::<LittleEndian>()?;
		let field_0x8 = reader.read_u32::<LittleEndian>()?;
		let field_0xc = reader.read_u32::<LittleEndian>()?;
        
		let id = get_string(id_pointer + 0x1e64)?;
		
		Ok(Self {
			id,
			file_offset,
			field_0x8,
			field_0xc,
		})
	}
}

pub struct CgfxFileRegistry {
	pub items: Vec<RegistryItem>,
}

impl CgfxFileRegistry {
	pub fn new(buffer: &[u8]) -> Result<Self> {
		let get_string = |ptr| get_string(buffer, ptr);
		let mut cursor = Cursor::new(buffer);
		
		let item_count = cursor.read_u32::<LittleEndian>()?;
		let mut items = Vec::default();
		
		for i in 0..item_count {
			items.push(RegistryItem::read(&mut cursor, &get_string)?);
		}
		
		Ok(CgfxFileRegistry {
			items,
		})
	}
	
	pub fn to_yaml(&self) -> Result<String> {
		let mut output = "---\n".to_owned();
		
		for item in &self.items {
			write!(output, "- id: {}\n  file_offset: {}\n  field_0x8: {}\n  field_0xc: {}\n",
				item.id, item.file_offset, item.field_0x8, item.field_0xc)?;
		}
		
		Ok(output)
	}
}
