use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zstd;

pub fn save_bincode_zst<T, P>(file_name: P, data: &T) -> Result<(), Box<dyn Error>>
where
    T: Serialize,
    P: AsRef<Path>,
{
    let encoded: Vec<u8> = bincode::serialize(data)?;
    let compressed = zstd::encode_all(&encoded[..], 11)?;
    let mut file = File::create(file_name)?;
    file.write_all(&compressed)?;
    Ok(())
}

pub fn load_bincode_zst<T, P>(file_name: P) -> Result<T, Box<dyn Error>>
where
    T: for<'de> Deserialize<'de>,
    P: AsRef<Path>,
{
    let mut file = File::open(file_name)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let decompressed = zstd::decode_all(&buffer[..])?;
    let data: T = bincode::deserialize(&decompressed)?;
    Ok(data)
}
