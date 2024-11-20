use std::{
    ffi::OsStr, fs, io::{Cursor, Seek, SeekFrom}, panic, path::{Path, PathBuf}
};

use anyhow::{anyhow, Error, Result};
use binrw::{docs::attribute, BinRead};
use clap::{ArgAction, Parser, ValueEnum};
use compression_cache::{CachedFile, CompressionCache};
use na::Vec3;
use nw_tex::{
    bcres::{
        bcres::CgfxContainer, image_codec::{decode_swizzled_buffer, to_png, ENCODABLE_FORMATS}, model::{AttributeName, CgfxModel, CgfxModelCommon, GlDataType, VertexBuffer, VertexBufferAttribute}, texture::{CgfxTexture, CgfxTextureCommon, PicaTextureFormat}
    },
    util::{blz::{blz_decode, blz_encode}, math},
    ArchiveRegistry, RegistryItem,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

#[cfg(test)]
mod tests;

mod compression_cache;

#[derive(Debug, Clone, ValueEnum)]
enum Method {
    /// TODO: Takes in a 'kdm_*.bin' file and disassembles it into a human-readable '*.kersti' file
    Extract,
    /// TODO: Takes in your modified kersti file and builds it into the original game file
    Rebuild,
}

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
enum AssetFormat {
    Bcrez,
    Bcres,
    Png,
}

#[derive(Parser, Debug)]
#[command(author, version, long_about = None, disable_version_flag = true, disable_help_flag = true)]
struct Args {
    /// Whether to 'extract' a .bin texture archive into the assets it contains or 'rebuild' the
    /// assets back into a texture archive.
    #[arg(verbatim_doc_comment)]
    method: Method,
    
    /// The input 'XXX_xx.bin' file to extract or .yaml file to build to the source asset again.
    /// 
    /// Examples: (extract) EUR_en.bin, EUR_de.bin (rebuild) EUR_en_tex.yaml, EUR_it_tex.yaml
    /// 
    /// If the method is `extract`, then there needs to be an adjacent file with the same name  
    /// but ending on '_info.bin'. This file contains important metadata on the structure of the
    /// archive.
    #[arg(verbatim_doc_comment)]
    input: String,
    
    /// The output file. If left blank, it will be inferred from the input.
    /// 
    /// If method is `extract`, then the output will need to end on `_tex.yaml`.
    /// There is going to be a folder with the same name but without the file extension.
    /// 
    /// If method is `rebuild`, then the output is going to be a .bin file.
    /// The secondary output, ending on _info.bin, will be placed next to this file.
    /// 
    /// Examples: (extract) EUR_en_tex.yaml, EUR_de_tex.yaml (rebuild) EUR_en.bin, EUR_it.bin
    #[arg(short, long, verbatim_doc_comment)]
    output: Option<String>,
    
    /// When the method is 'extract' and this flag is set, it will overwrite everything in the output
    /// directory (e.g. 'EUR_en_tex/') that is already present. Otherwise, the program will cancel
    /// if the output directory exists and contains files already.
    #[arg(short, long, verbatim_doc_comment)]
    clean: bool,
    
    /// The file format which the contents of the texture archive will be output in and will be
    /// expected to have during rebuild. Make sure this argument has the same value during
    /// extraction and rebuilding.
    /// 
    /// .bcrez (default) is how the assets are stored internally. The same as .bcres but has to be decompressed
    /// first and recompressed when rebuilding, e.g. by blz.exe from CUE's GBA/DS compressors.
    /// 
    /// .bcres is the standard 3DS asset file format for 3D models, textures, animations and more.
    /// The bcres files used in texture archives only have one file inside, that being a texture
    /// with the same name as the asset file.
    /// 
    /// Can be opened with CTR-Studio, although I haven't been able to replace textures
    /// with it without causing the game to crash.
    /// 
    /// .png will output plain .png files for easy editing and viewing, however, this CANNOT be used to
    /// rebuild archives yet, so ONLY use this to visualize assets for now!
    #[arg(short, long, verbatim_doc_comment)]
    asset_format: Option<AssetFormat>,
    
    /// Print app version
    #[arg(short, long, action = ArgAction::Version)]
    version: Option<bool>,
    
    // makes clap always display the long help even if you specify -h
    // that's because the short help just does not give you enough information to operate
    // and it's uninutitive that there is a difference between two versions of the same flag
    
    /// Print help
    #[arg(short, long, action = ArgAction::HelpLong)]
    help: Option<bool>,
}

fn get_input_sibling_path(input: &Path, old_file_ending: &str, new_file_ending: &str) -> Result<PathBuf> {
    let mut path_buf = input.parent()
        .ok_or_else(|| Error::msg("Could not find containing directory of input file."))?
        .to_owned();
    
    let input_file_name = input.file_name()
        .ok_or_else(|| Error::msg("Invalid file name for input."))?
        .to_str()
        .ok_or_else(|| Error::msg("Input file name contains invalid (not utf8) characters."))?;
    
    let mut output_file_name =
        if let Some(input_file_name_stem) = input_file_name.strip_suffix(old_file_ending) {
            input_file_name_stem.to_owned()
        } else {
            input_file_name.to_owned()
        };
    
    output_file_name.push_str(new_file_ending);
    path_buf.push(output_file_name);
    
    Ok(path_buf)
}

fn bcres_buffer_into_png(bcres_buffer: &[u8]) -> Result<(Vec<u8>, PicaTextureFormat)> {
    let gfx = CgfxContainer::new(bcres_buffer)?;
    
    assert!(gfx.textures.is_some(), "Texture archive bcres file has to contain a texture section");
    
    let textures = gfx.textures.unwrap();
    let texture_node = textures.nodes.iter()
        .find(|node| node.value.is_some())
        .expect("Texture archive bcres file has to contain at least one texture");
    
    let (common, image) = match texture_node.value.as_ref().unwrap() {
        CgfxTexture::Image(common, image) => (common, image.as_ref().unwrap()),
        other => panic!("Unsupported texture type {:?}, expected Image", other),
    };
    
    let CgfxTextureCommon { texture_format, width, height, .. } = *common;
    let decoded = decode_swizzled_buffer(&image.image_bytes, texture_format, width, height)?;
    
    Ok((to_png(&decoded, width, height)?, texture_format))
}

fn extract(input: PathBuf, opt_output: Option<String>, clean_out_dir: bool, asset_format: AssetFormat) -> Result<()> {
    let secondary_input = get_input_sibling_path(&input, ".bin", "_info.bin")?;
    
    // print warning if output is set but doesn't end on _tex.yaml
    if let Some(output) = &opt_output {
        if !output.ends_with("_tex.yaml") {
            eprintln!("Warning: output path {:?} does not end on '_tex.yaml'.", output)
        }
    }
    
    let output_file_name = match &opt_output {
        Some(path) => PathBuf::from(path),
        None => get_input_sibling_path(&input, ".bin", "_tex.yaml")?,
    };
    
    let output_dir_name = match &opt_output {
        Some(path) => get_input_sibling_path(Path::new(path), ".yaml", "")?,
        None => get_input_sibling_path(&input, ".bin", "_tex")?,
    };
    
    // read input files
    let input_file_buf = fs::read(&input)
        .expect(&format!("Could not open input file \"{}\". \
Make sure that it exists and can be accessed with the current permissions.", input.display()));
    
    let secondary_file_buf = fs::read(&secondary_input)
        .expect(&format!("Could not open file \"{}\". Make sure `input` has an adjacent \
file with the same name but ending on '_info.bin' rather than '.bin'", secondary_input.display()));
    
    // parse files
    let mut registry = ArchiveRegistry::new(&secondary_file_buf)?;
    
    // require --clean if `output_dir_name` contains files already
    if !clean_out_dir && output_dir_name.is_dir() {
        let output_dir_children: Vec<_> = fs::read_dir(&output_dir_name)?.collect();
        
        if !output_dir_children.is_empty() {
            return Err(Error::msg(format!("\
                The output directory \"{}\" contains items. If you want to overwrite them, \
                run the program with the --clean option.", output_dir_name.display()
            )));
        }
    }
    
    // write output files
    if clean_out_dir && output_dir_name.is_dir() {
        fs::remove_dir_all(&output_dir_name)?;
    }
    
    fs::create_dir_all(&output_dir_name)?;
    
    let resource_file_extension = match asset_format {
        AssetFormat::Bcrez => ".bcrez",
        AssetFormat::Bcres => ".bcres",
        AssetFormat::Png => ".png",
    };
    
    let mut compression_cache = if asset_format != AssetFormat::Bcrez {
        Some(CompressionCache::new())
    } else {
        None
    };
    
    for item in registry.items.iter_mut() {
        let start_offset: usize = item.file_offset.try_into().unwrap();
        let end_offset: usize = (item.file_offset + item.byte_length).try_into().unwrap();
        
        let file_content = &input_file_buf[start_offset..end_offset];
        let filename: String;
        let to_write: Vec<u8>;
        
        if asset_format == AssetFormat::Bcrez {
            to_write = file_content.to_owned();
            filename = item.id.clone();
        } else {
            let decompressed = blz_decode(file_content)?;
            let decompressed_hash = md5::compute(&decompressed);
            
            let cached_files = &mut compression_cache.as_mut().unwrap().files;
            cached_files.push(CachedFile {
                name: item.id.clone(),
                decompressed_file_hash: decompressed_hash.0,
                compressed_content: file_content.to_owned(),
            });
            
            if asset_format == AssetFormat::Png {
                let (buf, texture_format) = bcres_buffer_into_png(&decompressed)?;
                let readonly = !ENCODABLE_FORMATS.contains(&texture_format);
                item.image_format = Some(texture_format);
                item.is_readonly = if readonly { Some(readonly) } else { None };
                
                to_write = buf;
                filename = if readonly { "READONLY_".to_owned() + &item.id } else { item.id.clone() };
            } else {
                to_write = decompressed;
                filename = item.id.clone();
            }
        }
        
        let file_name = output_dir_name.join(filename + resource_file_extension);
        fs::write(file_name, to_write)?;
    }
    
    fs::write(&output_file_name, registry.to_yaml()?)?;
    
    if let Some(compression_cache) = compression_cache {
        fs::write(output_file_name.with_extension("cache"), compression_cache.to_buffer()?)?;
    }
    
    Ok(())
}

fn rebuild(input: PathBuf, opt_output: Option<String>, asset_format: AssetFormat) -> Result<()> {
    if asset_format == AssetFormat::Png {
        return Err(anyhow!("Asset format .png not supported during rebuilding yet!"));
    }
    
    let compress = asset_format == AssetFormat::Bcres;
    
    // get adjacent input folder
    let input_folder_name = input.with_extension("");
    let input_cache_name = input.with_extension("cache");
    
    let output_file_name = match &opt_output {
        Some(path) => PathBuf::from(path),
        None => {
            let input_bytes = input.as_os_str().as_encoded_bytes();
            
            if input_bytes.ends_with(OsStr::new("_tex.yaml").as_encoded_bytes()) {
                get_input_sibling_path(&input, "_tex.yaml", ".bin")?
            } else {
                input.with_extension("bin")
            }
        },
    };
    
    let secondary_output_file_name =
        get_input_sibling_path(&output_file_name, ".bin", "_info.bin")?;
    
    let input_string = fs::read_to_string(input)?;
    
    let mut registry = ArchiveRegistry::from_yaml(&input_string)?;
    
    // read compression cache
    let compression_cache = if compress {
        let buffer = fs::read(input_cache_name)
            .map_err(|_| Error::msg(
                "Cache file could not be read, make sure it exists and can be accessed.\n\
                Make sure that you extracted the archive with compression turned on (enable --blz flag during extraction) \
                and that you did not move or delete the [...].cache file."
            ))?;
        
        Some(CompressionCache::from_buffer(&buffer)?)
    } else {
        None
    };
    
    // read files to be written in archive
    let file_extension = if compress { "bcres" } else { "bcrez" };
    
    let read_bcrez = |item: &RegistryItem| {
        let input_path = input_folder_name.join(&item.id).with_extension(file_extension);
        
        let mut buffer = fs::read(&input_path)
            .map_err(|_| Error::msg(format!(
                "File {:?} could not be read. Make sure that the file exists and can be accessed.\n\
                If you used --asset-format {} during extraction, specify the same command line option during rebuilding too.",
                &input_path, if compress { "bcrez" } else { "bcres" }
            )))?;
        
        if compress {
            let cache_item = compression_cache.as_ref().unwrap().files.iter()
                .find(|file| file.name == item.id)
                .unwrap();
            
            let hash = md5::compute(&buffer);
            
            if cache_item.decompressed_file_hash == hash.0 {
                Ok(cache_item.compressed_content.clone())
            } else {
                println!("Encoding {:?}", item.id);
                blz_encode(&mut buffer)
            }
        } else {
            Ok(buffer)
        }
    };
    
    let archived_files_result: Result<Vec<Vec<u8>>> = registry.items.par_iter().map(read_bcrez).collect();
    
    let archived_files = archived_files_result?;

    // write archive file
    let mut archive_buffer: Vec<u8> = Vec::new();
    
    for (i, file_buf) in archived_files.into_iter().enumerate() {
        registry.items[i].file_offset = archive_buffer.len().try_into().unwrap();
        registry.items[i].byte_length = file_buf.len().try_into().unwrap();
        archive_buffer.extend(file_buf);
    }
    
    if let Some(parent) = output_file_name.parent() {
        fs::create_dir_all(parent)?;
    }
    
    fs::write(output_file_name, archive_buffer)?;
    fs::write(secondary_output_file_name, registry.to_buffer()?)?;
    
    Ok(())
}

fn get_vertex_pos(reader: &mut Cursor<&[u8]>, attributes: &[VertexBufferAttribute], all_vertices: &mut Vec<Vec3>) -> Result<Vec3> {
    let mut result: Option<Vec3> = None;
    
    for attr in attributes {
        if attr.attribute_name == AttributeName::Position {
            assert!(attr.elements == 3 && attr.format == GlDataType::Float);
            let pos: Vec3 = math::Vec3::read(reader)?.to_na() * attr.scale;
            
            if !all_vertices.contains(&pos) {
                all_vertices.push(pos);
            }
            
            result = Some(pos);
        } else {
            reader.seek(SeekFrom::Current((attr.format.byte_size() * attr.elements) as i64))?;
        }
    }
    
    Ok(result.unwrap())
}

fn do_model(common: &CgfxModelCommon) -> Result<()> {
    let shapes = common.shapes.as_ref().unwrap();
    let mut all_vertices: Vec<Vec3> = Vec::new();
    let mut all_faces: Vec<Vec<[u32; 3]>> = Vec::new();
    
    for (i, shape) in shapes.iter().enumerate() {
        println!("{}", i);
        let vertex_buffers = shape.vertex_buffers.as_ref().unwrap();
        
        for vb in vertex_buffers {
            match vb {
                VertexBuffer::Attribute(attribute) => {
                    if attribute.vertex_buffer_common.attribute_name == AttributeName::Position {
                        assert!(attribute.format == GlDataType::Float);
                        let raw_bytes: &[u8] = attribute.raw_bytes.as_ref().unwrap();
                        let mut reader = Cursor::new(raw_bytes);
                        
                        for _ in 0..raw_bytes.len() / attribute.elements as usize {
                            let pos: Vec3 = math::Vec3::read(&mut reader)?.to_na() * attribute.scale;
                            
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
                    
                    assert!(vertex_count % 3 == 0);
                    
                    let mut current_faces: Vec<[u32; 3]> = Vec::new();
                    
                    for _ in 0..vertex_count / 3 {
                        let a = get_vertex_pos(&mut reader, attributes, &mut all_vertices)?;
                        let b = get_vertex_pos(&mut reader, attributes, &mut all_vertices)?;
                        let c = get_vertex_pos(&mut reader, attributes, &mut all_vertices)?;
                        
                        let a_index = all_vertices.iter().position(|v| *v == a).unwrap();
                        let b_index = all_vertices.iter().position(|v| *v == b).unwrap();
                        let c_index = all_vertices.iter().position(|v| *v == c).unwrap();
                        current_faces.push([a_index as u32, b_index as u32, c_index as u32]);
                    }
                    
                    all_faces.push(current_faces);
                },
                // it doesn't make sense for Position to be fixed so this is just ignored
                VertexBuffer::Fixed(_) => (),
            }
        }
    }
    
    for vertex in &all_vertices {
        println!("v {} {} {}", vertex.x, vertex.y, vertex.z);
    }
    
    for (i, current_faces) in all_faces.iter().enumerate() {
        println!("\no mesh{}", i);
        
        for [a, b, c] in current_faces {
            println!("v {} {} {}", a + 1, b + 1, c + 1);
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    if cfg!(debug_assertions) {
        println!("{:?}\n", args);
    } else {
        panic::set_hook(Box::new(|info| {
            println!("{}", info);
        }));
    }
    
    let input = Path::new(&args.input).to_owned();
    // let output = args.output;
    // let asset_format = args.asset_format.unwrap_or(AssetFormat::Bcrez);
    
    let input_file = fs::read(input)?;
    let bcres = CgfxContainer::new(&input_file)?;
    println!("{:#?}", bcres.models);
    
    // if let Some(models) = bcres.models {
    //     for node in models.nodes {
    //         if let Some(model) = node.value {
    //             let common = model.common();
    //             println!("{}", common.cgfx_object_header.name.as_ref().unwrap());
                
    //             do_model(common)?;
    //         }
    //     }
    // }
    
    return Ok(());
    
    // match args.method {
    //     Method::Extract => extract(input, output, args.clean, asset_format),
    //     Method::Rebuild => rebuild(input, output, asset_format),
    // }
}
