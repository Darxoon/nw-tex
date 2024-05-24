use std::fs;

use anyhow::Result;
use nw_tex::util::bcres::CgfxContainer;

use crate::{extract, AssetFormat};

#[test]
fn extract_texture_archives() -> Result<()> {
    for item_result in fs::read_dir("testing/archives")? {
        let item = item_result?;
        let file_name = item.file_name().to_str().unwrap().to_string();
        
        if file_name.ends_with(".bin") && !file_name.ends_with("_info.bin") {
            println!("Extracting {}", file_name);
            extract(item.path(), None, true, AssetFormat::Bcres)?;
        }
    }
    Ok(())
}

#[test]
fn reencode_bcres_files() -> Result<()> {
    for item_result in fs::read_dir("testing/bcres")? {
        let item = item_result?;
        let file_name = item.file_name().to_str().unwrap().to_string();
        
        if !file_name.ends_with(".bcres") {
            continue;
        }
        
        println!("Parsing {:?}", file_name);
        let content = fs::read(item.path())?;
        let gfx = CgfxContainer::new(&content)?;
        
        let trimmed_content = &content[0..gfx.header.file_length as usize];
        
        println!("Saving {:?}", file_name);
        let reencoded = gfx.to_buffer()?;
        
        assert!(reencoded.len() == gfx.header.file_length as usize, "Length of file {} does not match", file_name);
        assert!(trimmed_content == &reencoded, "File {} does not match its original when reencoded", file_name);
    }
    
    println!("Done!");
    Ok(())
}
