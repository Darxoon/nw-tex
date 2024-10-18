use std::io::Cursor;

use anyhow::{anyhow, Result};
use binrw::{BinRead, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt};

use crate::util::pointer::Pointer;

use super::{bcres::{CgfxDict, CgfxDictValue}, util::{CgfxNodeHeader, CgfxObjectHeader, CgfxTransform}};

#[derive(Debug, Clone)]
pub struct CgfxModelCommon {
    // header stuff
    pub cgfx_object_header: CgfxObjectHeader,
    pub cgfx_node_header: CgfxNodeHeader,
    pub transform_node_header: CgfxTransform,
    
    // model data
    pub meshes: Option<Vec<Mesh>>,
    pub materials: Option<CgfxDict<()>>,
    pub shapes: Option<Vec<()>>,
    pub mesh_node_visibilities: Option<CgfxDict<()>>,
    
    pub flags: u32,
    pub face_culling: u32,
    pub layer_id: u32,
}

#[derive(Debug, Clone)]
pub enum CgfxModel {
    Standard(CgfxModelCommon),
    Skeletal(CgfxModelCommon, ()),
}

impl CgfxModel {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let mut temp_reader = reader.clone();
        
        let discriminant = reader.read_u32::<LittleEndian>()?;
        let cgfx_object_header = CgfxObjectHeader::read(reader)?;
        let cgfx_node_header = CgfxNodeHeader::read(reader)?;
        let transform_node_header = CgfxTransform::read(reader)?;
        
        // TODO: anim groups in node header
        
        // meshes
        let mesh_count = reader.read_u32::<LittleEndian>()?;
        let mesh_ptr = Pointer::read(reader)?;
        
        let meshes: Option<Vec<Mesh>> = if let Some(mesh_arr_ptr) = mesh_ptr {
            let mut mesh_reader = reader.clone();
            let mut meshes: Vec<Mesh> = Vec::with_capacity(mesh_count as usize);
            
            temp_reader.set_position(reader.position() + u64::from(mesh_arr_ptr) - 4);
            
            for _ in 0..mesh_count {
                let mesh_ptr = Pointer::read(&mut temp_reader)?.unwrap();
                mesh_reader.set_position(temp_reader.position() + u64::from(mesh_ptr) - 4);
                assert!(mesh_reader.read_u32::<LittleEndian>()? == 0x01000000);
                meshes.push(Mesh::read(&mut mesh_reader)?);
            }
            Some(meshes)
        } else {
            None
        };
        
        // materials
        let material_count = reader.read_u32::<LittleEndian>()?;
        let material_ptr = Pointer::read(reader)?;
        
        let materials = if let Some(material_ptr) = material_ptr {
            temp_reader.set_position(reader.position() + u64::from(material_ptr) - 4);
            let dict: CgfxDict<()> = CgfxDict::from_reader(&mut temp_reader)?;
            
            assert!(dict.values_count == material_count);
            Some(dict)
        } else {
            None
        };
        
        // shapes
        let shape_count = reader.read_u32::<LittleEndian>()?;
        let shape_ptr = Pointer::read(reader)?;
        
        let shapes: Option<Vec<()>> = if let Some(shape_ptr) = shape_ptr {
            temp_reader.set_position(reader.position() + u64::from(shape_ptr) - 4);
            Some((0..shape_count).map(|_| ()).collect())
        } else {
            None
        };
        
        // mesh node visibilities
        let mesh_node_visibility_count = reader.read_u32::<LittleEndian>()?;
        let mesh_node_visibility_ptr = Pointer::read(reader)?;
        
        let mesh_node_visibilities = if let Some(mesh_node_visibility_ptr) = mesh_node_visibility_ptr {
            temp_reader.set_position(reader.position() + u64::from(mesh_node_visibility_ptr) - 4);
            let dict: CgfxDict<()> = CgfxDict::from_reader(&mut temp_reader)?;
            
            assert!(dict.values_count == mesh_node_visibility_count);
            Some(dict)
        } else {
            None
        };
        
        
        let flags = reader.read_u32::<LittleEndian>()?;
        let face_culling = reader.read_u32::<LittleEndian>()?;
        let layer_id = reader.read_u32::<LittleEndian>()?;
        
        let common = CgfxModelCommon {
            cgfx_object_header,
            cgfx_node_header,
            transform_node_header,
            meshes,
            materials,
            shapes,
            mesh_node_visibilities,
            flags,
            face_culling,
            layer_id,
        };
        
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

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little)]
pub struct Mesh {
    // object header
    pub cgfx_object_header: CgfxObjectHeader,
    
    // mesh data
    pub shape_index: u32,
    pub material_index: u32,
    
    parent_ptr: i32,
    
    pub visible: u8,
    pub render_priority: u8,
    pub mesh_node_index: u16,
    pub primitive_index: u32,
    pub flags: u32,
}
