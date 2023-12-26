use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_binary::binary_stream::Endian;

#[derive(Serialize, Deserialize)]
pub struct CachedFile {
    pub name: String,
    pub decompressed_file_hash: [u8; 16],
    pub compressed_content: Vec<u8>,
}

pub struct CompressionCache {
    pub files: Vec<CachedFile>,
}

impl CompressionCache {
    pub fn new() -> Self {
        CompressionCache { files: Vec::new() }
    }
    
    pub fn to_buffer(&self) -> Result<Vec<u8>> {
        Ok(serde_binary::to_vec(&self.files, Endian::Little)?)
    }
    
    pub fn from_buffer(buffer: &[u8]) -> Result<Self> {
        let files = serde_binary::from_slice(buffer, Endian::Little)?;
        
        Ok(Self { files })
    }
}
