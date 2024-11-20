use std::{fmt::Write, io::{Cursor, Seek, SeekFrom}};

use anyhow::Result;
use binrw::BinRead;
use na::Vec3;
use nw_tex::{bcres::model::{AttributeName, CgfxModelCommon, Face, FaceDescriptor, GlDataType, SubMesh, VertexBuffer, VertexBufferAttribute}, util::math};

#[allow(unused)]
pub fn export_bcres_to_obj(common: &CgfxModelCommon) -> Result<String> {
    let shapes = common.shapes.as_ref().unwrap();
    let mut all_vertices: Vec<Vec3> = Vec::new();
    let mut all_faces: Vec<Vec<[u32; 3]>> = Vec::new();
    
    for shape in shapes {
        let vertex_buffers = shape.vertex_buffers.as_ref().unwrap();
        let mut current_vertices: Vec<Vec3> = Vec::new();
        let mut current_faces: Vec<[u32; 3]> = Vec::new();
        
        // collect all vertices
        for vb in vertex_buffers {
            match vb {
                VertexBuffer::Attribute(attribute) => {
                    if attribute.vertex_buffer_common.attribute_name == AttributeName::Position {
                        assert!(attribute.format == GlDataType::Float);
                        let raw_bytes: &[u8] = attribute.raw_bytes.as_ref().unwrap();
                        let mut reader = Cursor::new(raw_bytes);
                        
                        for _ in 0..raw_bytes.len() / attribute.elements as usize {
                            let pos: Vec3 = math::Vec3::read(&mut reader)?.to_na() * attribute.scale;
                            
                            current_vertices.push(pos);
                            
                            if !all_vertices.contains(&pos) {
                                all_vertices.push(pos);
                            }
                        }
                        
                        todo!();
                    }
                },
                VertexBuffer::Interleaved(interleaved) => {
                    let attributes: &[VertexBufferAttribute] = interleaved.attributes.as_ref().unwrap();
                    
                    // check if this vb contains a position attribute
                    if attributes.iter().all(|attr| attr.attribute_name != AttributeName::Position) {
                        continue;
                    }
                    
                    let raw_bytes: &[u8] = interleaved.raw_bytes.as_ref().unwrap();
                    let mut reader = Cursor::new(raw_bytes);
                    
                    let vertex_byte_size: u32 = attributes.iter()
                        .map(|attr| attr.format.byte_size() * attr.elements)
                        .sum();
                    let vertex_count = raw_bytes.len() / vertex_byte_size as usize;
                    
                    for _ in 0..vertex_count {
                        for attr in attributes {
                            if attr.attribute_name == AttributeName::Position {
                                assert!(attr.elements == 3 && attr.format == GlDataType::Float);
                                let pos: Vec3 = math::Vec3::read(&mut reader)?.to_na() * attr.scale;
                                
                                current_vertices.push(pos);
                                
                                if !all_vertices.contains(&pos) {
                                    all_vertices.push(pos);
                                }
                            } else {
                                reader.seek(SeekFrom::Current((attr.format.byte_size() * attr.elements) as i64))?;
                            }
                        }
                    }
                },
                // it doesn't make sense for Position to be fixed so this is just ignored
                VertexBuffer::Fixed(_) => (),
            }
        }
        
        // collect all faces
        let sub_meshes: &[SubMesh] = shape.sub_meshes.as_ref().unwrap();
        
        for sub_mesh in sub_meshes {
            let gfx_faces: &[Face] = sub_mesh.faces.as_ref().unwrap();
            
            for gfx_face in gfx_faces {
                let face_descriptors: &[FaceDescriptor] = gfx_face.face_descriptors.as_ref().unwrap();
                
                for face_descriptor in face_descriptors {
                    let indices: &[u16] = face_descriptor.indices.as_ref().unwrap();
                    assert!(indices.len() % 3 == 0);
                    
                    let mut reader = indices.iter();
                    
                    for _ in 0..indices.len() / 3 {
                        let a: Vec3 = current_vertices[*reader.next().unwrap() as usize];
                        let a_index = all_vertices.iter().position(|v: &Vec3| *v == a).unwrap();
                        
                        let b: Vec3 = current_vertices[*reader.next().unwrap()  as usize];
                        let b_index = all_vertices.iter().position(|v: &Vec3| *v == b).unwrap();
                        
                        let c: Vec3 = current_vertices[*reader.next().unwrap()  as usize];
                        let c_index = all_vertices.iter().position(|v: &Vec3| *v == c).unwrap();
                        
                        current_faces.push([a_index as u32, b_index as u32, c_index as u32]);
                    }
                }
            }
        }
        
        all_faces.push(current_faces);
    }
    
    let mut obj_out = String::new();
    
    for vertex in &all_vertices {
        writeln!(obj_out, "v {} {} {}", vertex.x, vertex.y, vertex.z)?;
    }
    
    for (i, current_faces) in all_faces.iter().enumerate() {
        writeln!(obj_out, "\no mesh{}", i)?;
        
        for [a, b, c] in current_faces {
            writeln!(obj_out, "f {} {} {}", a + 1, b + 1, c + 1)?;
        }
    }
    
    Ok(obj_out)
}
