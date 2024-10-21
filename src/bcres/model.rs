use std::io::Cursor;

use anyhow::{anyhow, Result};
use binrw::{BinRead, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt};

use crate::util::{
    math::{Matrix3x3, SerializableMatrix, Vec3, Vec4},
    pointer::Pointer,
};

use super::{
    bcres::{CgfxDict, CgfxDictValue, WriteContext},
    image_codec::RgbaColor,
    util::{brw_relative_pointer, CgfxNodeHeader, CgfxObjectHeader, CgfxTransform},
};

#[derive(Debug, Clone)]
pub struct CgfxModelCommon {
    // header stuff
    pub cgfx_object_header: CgfxObjectHeader,
    pub cgfx_node_header: CgfxNodeHeader,
    pub transform_node_header: CgfxTransform,
    
    // model data
    pub meshes: Option<Vec<Mesh>>,
    pub materials: Option<CgfxDict<Material>>,
    pub shapes: Option<Vec<Shape>>,
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
            let dict: CgfxDict<Material> = CgfxDict::from_reader(&mut temp_reader)?;
            
            assert!(dict.values_count == material_count);
            Some(dict)
        } else {
            None
        };
        
        // shapes
        let shape_count = reader.read_u32::<LittleEndian>()?;
        let shape_ptr = Pointer::read(reader)?;
        
        let shapes: Option<Vec<Shape>> = if let Some(shape_arr_ptr) = shape_ptr {
            let mut shape_reader = reader.clone();
            let mut shapes: Vec<Shape> = Vec::with_capacity(shape_count as usize);
            
            temp_reader.set_position(reader.position() + u64::from(shape_arr_ptr) - 4);
            
            for _ in 0..shape_count {
                let shape_ptr = Pointer::read(&mut temp_reader)?.unwrap();
                shape_reader.set_position(temp_reader.position() + u64::from(shape_ptr) - 4);
                shapes.push(Shape::read(&mut shape_reader)?);
            }
            Some(shapes)
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
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, _writer: &mut Cursor<&mut Vec<u8>>, _ctx: &mut super::bcres::WriteContext) -> Result<()> {
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
    
    // runtime initialized data
    // ...
}

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little, magic = 0x8000000u32)]
pub struct Material {
    pub cgfx_object_header: CgfxObjectHeader,
    
    // material stuff
    pub flags: u32,
    pub tex_coord_config: u32,
    pub render_layer: u32,
    pub colors: MaterialColors,
    // ...
}

impl CgfxDictValue for Material {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self::read(reader)?)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _ctx: &mut WriteContext) -> Result<()> {
        self.write(writer)?;
        Ok(())
    }
}

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little)]
pub struct MaterialColors {
    pub emission_float: Vec4,
    pub ambient_float: Vec4,
    pub diffuse_float: Vec4,
    pub specular0_float: Vec4,
    pub specular1_float: Vec4,
    pub constant0_float: Vec4,
    pub constant1_float: Vec4,
    pub constant2_float: Vec4,
    pub constant3_float: Vec4,
    pub constant4_float: Vec4,
    pub constant5_float: Vec4,
    
    pub emission: RgbaColor,
    pub ambient: RgbaColor,
    pub diffuse: RgbaColor,
    pub specular0: RgbaColor,
    pub specular1: RgbaColor,
    pub constant0: RgbaColor,
    pub constant1: RgbaColor,
    pub constant2: RgbaColor,
    pub constant3: RgbaColor,
    pub constant4: RgbaColor,
    pub constant5: RgbaColor,
    
    pub command_cache: u32,
}

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little, magic = 0x10000001u32)]
pub struct Shape {
    // object header
    pub cgfx_object_header: CgfxObjectHeader,
    
    // shape data
    pub flags: u32,
    
    #[br(parse_with = brw_relative_pointer)]
    #[bw(map = |_| 0u32)]
    bounding_box_ptr: Option<Pointer>,
    
    pub position_offset: Vec3,
}

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little)]
pub struct BoundingBox {
    pub flags: u32,
    
    pub center: Vec3,
    #[brw(repr = SerializableMatrix<3, 3>)]
    pub orientation: Matrix3x3<f32>,
    pub size: Vec3,
}
