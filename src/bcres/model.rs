use std::{io::{Cursor, Seek, SeekFrom}, ops::{Deref, DerefMut}};

use anyhow::{anyhow, Result};
use binrw::{BinRead, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt};

use crate::{scoped_reader_pos, util::{
    math::{Matrix3x3, SerializableMatrix, Vec3, Vec4},
    pointer::Pointer,
}};

use super::{
    bcres::{CgfxCollectionValue, CgfxDict, WriteContext},
    image_codec::RgbaColor,
    util::{read_inline_list, read_pointer_list, read_pointer_list_magic, CgfxNodeHeader, CgfxObjectHeader, CgfxTransform},
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
    pub mesh_node_visibilities: Option<CgfxDict<()>>, // TODO: implement
    
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
        let discriminant = reader.read_u32::<LittleEndian>()?;
        let cgfx_object_header = CgfxObjectHeader::read(reader)?;
        let cgfx_node_header = CgfxNodeHeader::read(reader)?;
        let transform_node_header = CgfxTransform::read(reader)?;
        
        // TODO: anim groups in node header
        
        // meshes
        let meshes: Option<Vec<Mesh>> = read_pointer_list(reader)?;
        
        // materials
        let material_count = reader.read_u32::<LittleEndian>()?;
        let material_ptr = Pointer::read(reader)?;
        
        let materials = if let Some(material_ptr) = material_ptr {
            scoped_reader_pos!(reader);
            reader.set_position(reader.position() + u64::from(material_ptr) - 4);
            let dict: CgfxDict<Material> = CgfxDict::from_reader(reader)?;
            
            assert!(dict.values_count == material_count);
            Some(dict)
        } else {
            None
        };
        
        // shapes
        let shapes: Option<Vec<Shape>> = read_pointer_list(reader)?;
        
        // mesh node visibilities
        let mesh_node_visibility_count = reader.read_u32::<LittleEndian>()?;
        let mesh_node_visibility_ptr = Pointer::read(reader)?;
        
        let mesh_node_visibilities = if let Some(mesh_node_visibility_ptr) = mesh_node_visibility_ptr {
            scoped_reader_pos!(reader);
            reader.set_position(reader.position() + u64::from(mesh_node_visibility_ptr) - 4);
            let dict: CgfxDict<()> = CgfxDict::from_reader(reader)?;
            
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

    pub fn common(&self) -> &CgfxModelCommon {
        match self {
            CgfxModel::Standard(common) => common,
            CgfxModel::Skeletal(common, _) => common,
        }
    }

    pub fn common_mut(&mut self) -> &mut CgfxModelCommon {
        match self {
            CgfxModel::Standard(common) => common,
            CgfxModel::Skeletal(common, _) => common,
        }
    }
}

impl CgfxCollectionValue for CgfxModel {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, _writer: &mut Cursor<&mut Vec<u8>>, _ctx: &mut WriteContext) -> Result<()> {
        todo!()
    }
}

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little, magic = 0x01000000u32)]
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

#[derive(Clone, Debug)]
pub struct Shape {
    // object header
    pub cgfx_object_header: CgfxObjectHeader,
    
    // shape data
    pub flags: u32,
    pub bounding_box: Option<BoundingBox>,
    pub position_offset: Vec3,
    
    pub sub_meshes: Option<Vec<SubMesh>>,
    pub base_address: u32,
    pub vertex_buffers: Option<Vec<VertexBuffer>>,
    
    // TODO: blend shape
}

impl Shape {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        assert!(reader.read_u32::<LittleEndian>()? == 0x10000001);
        
        let cgfx_object_header = CgfxObjectHeader::read(reader)?;
        let flags = reader.read_u32::<LittleEndian>()?;
        
        let bounding_box_ptr = Pointer::read_relative(reader)?;
        let bounding_box = if let Some(bounding_box_ptr) = bounding_box_ptr {
            scoped_reader_pos!(reader);
            reader.seek(SeekFrom::Start(bounding_box_ptr.into()))?;
            Some(BoundingBox::read(reader)?)
        } else {
            None
        };
        
        let position_offset = Vec3::read(reader)?;
        
        let sub_meshes: Option<Vec<SubMesh>> = read_pointer_list(reader)?;
        let base_address = reader.read_u32::<LittleEndian>()?;
        let vertex_buffers: Option<Vec<VertexBuffer>> = read_pointer_list(reader)?;
        
        Ok(Self {
            cgfx_object_header,
            flags,
            bounding_box,
            position_offset,
            sub_meshes,
            base_address,
            vertex_buffers,
        })
    }
    
    pub fn to_writer(&self, _writer: &mut Cursor<&mut Vec<u8>>) -> Result<()> {
        todo!()
    }
}

impl CgfxCollectionValue for Shape {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _: &mut WriteContext) -> Result<()> {
        self.to_writer(writer)
    }
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

#[derive(Clone, Copy, Debug, BinRead, BinWrite)]
#[brw(repr = u32, little)]
pub enum SubMeshSkinning {
    None,
    Rigid,
    Smooth,
}

#[derive(Clone, Debug)]
pub struct SubMesh {
    pub bone_indices: Option<Vec<u32>>,
    pub skinning: SubMeshSkinning,
    pub faces: Option<Vec<Face>>,
}

impl SubMesh {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let bone_index_count = reader.read_u32::<LittleEndian>()?;
        let bone_index_ptr = Pointer::read_relative(reader)?;
        
        let bone_indices = if let Some(bone_index_ptr) = bone_index_ptr {
            scoped_reader_pos!(reader);
            
            let mut bone_indices = Vec::new();
            bone_indices.resize(bone_index_count as usize, 0);
            
            reader.seek(SeekFrom::Start(bone_index_ptr.into()))?;
            reader.read_u32_into::<LittleEndian>(&mut bone_indices)?;
            Some(bone_indices)
        } else {
            None
        };
        
        let skinning: SubMeshSkinning = SubMeshSkinning::read(reader)?;
        let faces: Option<Vec<Face>> = read_pointer_list(reader)?;

        Ok(Self {
            bone_indices,
            skinning,
            faces,
        })
    }
    
    pub fn to_writer(&self, _writer: &mut Cursor<&mut Vec<u8>>) -> Result<()> {
        todo!()
    }
}

impl CgfxCollectionValue for SubMesh {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _: &mut WriteContext) -> Result<()> {
        self.to_writer(writer)
    }
}

#[derive(Clone, Debug)]
pub struct Face {
    pub face_descriptors: Option<Vec<FaceDescriptor>>,
    pub buffer_objs: Option<Vec<u32>>,
    pub flags: u32,
    pub command_alloc: u32,
}

impl Face {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let face_descriptors: Option<Vec<FaceDescriptor>> = read_pointer_list(reader)?;
        let buffer_objs: Option<Vec<u32>> = read_inline_list(reader)?;
        let flags = reader.read_u32::<LittleEndian>()?;
        let command_alloc = reader.read_u32::<LittleEndian>()?;
        
        Ok(Self {
            face_descriptors,
            buffer_objs,
            flags,
            command_alloc,
        })
    }
    
    pub fn to_writer(&self, _: &mut Cursor<&mut Vec<u8>>) -> Result<()> {
        todo!()
    }
}

impl CgfxCollectionValue for Face {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _: &mut WriteContext) -> Result<()> {
        self.to_writer(writer)
    }
}

#[derive(Clone, Debug)]
pub struct FaceDescriptor {
    pub format: GlDataType,
    pub primitive_mode: u8, // TODO: make this an enum
    pub visible: u8,
    
    pub raw_buffer: Option<Vec<u8>>, // TODO: implement speial case for format == Short or UShort
    
    // more fields
    
    pub bounding_volume: u32,
}

impl FaceDescriptor {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let format = GlDataType::read(reader)?;
        assert!(format == GlDataType::Byte || format == GlDataType::UByte
            || format == GlDataType::Short || format == GlDataType::UShort);
        
        let primitive_mode = reader.read_u8()?;
        
        let visible = reader.read_u8()?;
        
        reader.seek(SeekFrom::Current(2))?;
        
        let raw_buffer: Option<Vec<u8>> = read_inline_list(reader)?;
        
        // skip 6 32-bit integers (fields aren't relevant here)
        // TODO: they will be necessary for serializing though
        reader.seek(SeekFrom::Current(6 * 4))?;
        
        let bounding_volume = reader.read_u32::<LittleEndian>()?;
        
        Ok(Self {
            format,
            primitive_mode,
            visible,
            raw_buffer,
            bounding_volume,
        })
    }
    
    pub fn to_writer(&self, _: &mut Cursor<&mut Vec<u8>>) -> Result<()> {
        todo!()
    }
}

impl CgfxCollectionValue for FaceDescriptor {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _: &mut WriteContext) -> Result<()> {
        self.to_writer(writer)
    }
}

#[derive(Clone, Debug, BinRead, BinWrite)]
#[brw(little)]
pub struct VertexBufferCommon {
    pub attribute_name: AttributeName,
    pub vertex_buffer_type: VertexBufferType,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, BinRead, BinWrite)]
#[brw(little, repr = u32)]
pub enum AttributeName {
    Position,
    Normal,
    Tangent,
    Color,
    TexCoord0,
    TexCoord1,
    TexCoord2,
    BoneIndex,
    BoneWeight,
    UserAttribute0,
    UserAttribute1,
    UserAttribute2,
    UserAttribute3,
    UserAttribute4,
    UserAttribute5,
    UserAttribute6,
    UserAttribute7,
    UserAttribute8,
    UserAttribute9,
    UserAttribute10,
    UserAttribute11,
    Interleave,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, BinRead, BinWrite)]
#[brw(little, repr = u32)]
pub enum GlDataType {
    Byte = 0x1400,
    UByte = 0x1401,
    Short = 0x1402,
    UShort = 0x1403,
    Float = 0x1406,
    Fixed = 0x140C,
}

impl GlDataType {
    pub fn byte_size(self) -> u32 {
        match self {
            GlDataType::Byte => 1,
            GlDataType::UByte => 1,
            GlDataType::Short => 2,
            GlDataType::UShort => 2,
            GlDataType::Float => 4,
            GlDataType::Fixed => todo!(), // wtf is Fixed?
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, BinRead, BinWrite)]
#[brw(little, repr = u32)]
pub enum VertexBufferType {
    // TODO: is this necessary? this seems redundant
    None,
    Fixed,
    Interleaved,
}

#[derive(Clone, Debug)]
pub enum VertexBuffer {
    Attribute(VertexBufferAttribute),
    Interleaved(VertexBufferInterleaved),
    Fixed(VertexBufferFixed),
}

impl VertexBuffer {
    fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let discriminant = reader.read_u32::<LittleEndian>()?;
        
        let vertex_buffer = match discriminant {
            0x40000001 => Self::Attribute(VertexBufferAttribute::from_reader(reader)?),
            0x40000002 => Self::Interleaved(VertexBufferInterleaved::from_reader(reader)?),
            0x80000000 => Self::Fixed(VertexBufferFixed::from_reader(reader)?),
            _ => return Err(anyhow!("Invalid model type discriminant {:x}", discriminant)),
        };
        
        Ok(vertex_buffer)
    }
    
    fn to_writer(&self, _writer: &mut Cursor<&mut Vec<u8>>) -> Result<()> {
        todo!()
    }
}

impl CgfxCollectionValue for VertexBuffer {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _: &mut WriteContext) -> Result<()> {
        self.to_writer(writer)
    }
}

#[derive(Clone, Debug)]
pub struct VertexBufferAttribute {
    pub vertex_buffer_common: VertexBufferCommon,
    
    pub buffer_obj: u32,
    pub location_flag: u32,
    
    pub raw_bytes: Option<Vec<u8>>,
    
    pub location_ptr: u32,
    pub memory_area: u32,
    
    pub format: GlDataType,
    pub elements: u32,
    pub scale: f32,
    pub offset: u32,
}

impl VertexBufferAttribute {
    fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let vertex_buffer_common = VertexBufferCommon::read(reader)?;
        let buffer_obj = reader.read_u32::<LittleEndian>()?;
        let location_flag = reader.read_u32::<LittleEndian>()?;
        
        let raw_bytes: Option<Vec<u8>> = read_inline_list(reader)?;
        
        let location_ptr = reader.read_u32::<LittleEndian>()?;
        let memory_area = reader.read_u32::<LittleEndian>()?;
        
        let format = GlDataType::read(reader)?;
        let elements = reader.read_u32::<LittleEndian>()?;
        let scale = reader.read_f32::<LittleEndian>()?;
        let offset = reader.read_u32::<LittleEndian>()?;
        
        Ok(Self {
            vertex_buffer_common,
            buffer_obj,
            location_flag,
            raw_bytes,
            location_ptr,
            memory_area,
            format,
            elements,
            scale,
            offset,
        })
    }
    
    fn to_writer(&self, _writer: &mut Cursor<&mut Vec<u8>>) -> Result<()> {
        todo!()
    }
}

impl CgfxCollectionValue for VertexBufferAttribute {
    fn read_dict_value(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Self::from_reader(reader)
    }

    fn write_dict_value(&self, writer: &mut Cursor<&mut Vec<u8>>, _: &mut WriteContext) -> Result<()> {
        self.to_writer(writer)
    }
}

impl Deref for VertexBufferAttribute {
    type Target = VertexBufferCommon;

    fn deref(&self) -> &Self::Target {
        &self.vertex_buffer_common
    }
}

impl DerefMut for VertexBufferAttribute {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vertex_buffer_common
    }
}

#[derive(Clone, Debug)]
pub struct VertexBufferInterleaved {
    pub vertex_buffer_common: VertexBufferCommon,
    
    pub buffer_obj: u32,
    pub location_flag: u32,
    
    pub raw_bytes: Option<Vec<u8>>,
    
    pub location_ptr: u32,
    pub memory_area: u32,
    
    pub vertex_stride: u32,
    pub attributes: Option<Vec<VertexBufferAttribute>>,
}

impl VertexBufferInterleaved {
    fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let vertex_buffer_common = VertexBufferCommon::read(reader)?;
        let buffer_obj = reader.read_u32::<LittleEndian>()?;
        let location_flag = reader.read_u32::<LittleEndian>()?;
        
        let raw_bytes: Option<Vec<u8>> = read_inline_list(reader)?;
        
        let location_ptr = reader.read_u32::<LittleEndian>()?;
        let memory_area = reader.read_u32::<LittleEndian>()?;
        
        let vertex_stride = reader.read_u32::<LittleEndian>()?;
        let attributes: Option<Vec<VertexBufferAttribute>> = read_pointer_list_magic(reader, Some(0x40000001))?;
        
        Ok(Self {
            vertex_buffer_common,
            buffer_obj,
            location_flag,
            raw_bytes,
            location_ptr,
            memory_area,
            vertex_stride,
            attributes,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VertexBufferFixed {
    pub vertex_buffer_common: VertexBufferCommon,
    
    pub format: GlDataType,
    pub elements: u32,
    pub scale: f32,
    pub vector: Option<Vec<f32>>,
}

impl VertexBufferFixed {
    pub fn from_reader(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let vertex_buffer_common = VertexBufferCommon::read(reader)?;
        let format = GlDataType::read(reader)?;
        let elements = reader.read_u32::<LittleEndian>()?;
        let scale = reader.read_f32::<LittleEndian>()?;
        let vector: Option<Vec<f32>> = read_inline_list(reader)?;

        Ok(Self {
            vertex_buffer_common,
            format,
            elements,
            scale,
            vector,
        })
    }
}

