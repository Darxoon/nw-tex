use std::io::Cursor;

use anyhow::{anyhow, Result};
use binrw::{BinRead, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt};

use super::{bcres::CgfxDictValue, util::{CgfxNodeHeader, CgfxObjectHeader, CgfxTransform}};

#[derive(Debug, Clone, BinRead, BinWrite)]
#[brw(little)]
pub struct CgfxModelCommon {
    // header stuff
    pub cgfx_object_header: CgfxObjectHeader,
    pub cgfx_node_header: CgfxNodeHeader,
    pub transform_node_header: CgfxTransform,
    
    // model data
}

#[derive(Debug, Clone)]
pub enum CgfxModel {
    Standard(CgfxModelCommon),
    Skeletal(CgfxModelCommon, ()),
}

impl CgfxModel {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let discriminant = reader.read_u32::<LittleEndian>()?;
        let common = CgfxModelCommon::read(reader)?;
        
        let model = match discriminant {
            0x40000012 => CgfxModel::Standard(common),
            0x40000092 => CgfxModel::Skeletal(common, ()),
            _ => return Err(anyhow!("Invalid model type discriminant {:x}", discriminant)),
        };
        
        Ok(model)
    }
}

impl CgfxDictValue for CgfxModel {
    fn read(reader: &mut std::io::Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write(&self, _writer: &mut std::io::Cursor<&mut Vec<u8>>, _ctx: &mut super::bcres::WriteContext) -> Result<()> {
        todo!()
    }
}
