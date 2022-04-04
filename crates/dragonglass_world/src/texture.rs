use anyhow::{bail, Result};
use image::{hdr::HdrDecoder, io::Reader as ImageReader, DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use std::{io::BufReader, path::Path};

// FIXME: Add mip levels
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Texture {
    pub pixels: Vec<u8>,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub sampler: Sampler,
}

impl Texture {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let image = ImageReader::open(path)?.decode()?;
        let pixels = image.to_bytes();
        let (width, height) = image.dimensions();
        let format = Self::map_format(&image)?;

        Ok(Self {
            pixels,
            format,
            width,
            height,
            sampler: Sampler::default(),
        })
    }

    pub fn map_format(image: &DynamicImage) -> Result<Format> {
        Ok(match image {
            DynamicImage::ImageRgb8(_) => Format::R8G8B8,
            DynamicImage::ImageRgba8(_) => Format::R8G8B8A8,
            DynamicImage::ImageBgr8(_) => Format::B8G8R8,
            DynamicImage::ImageBgra8(_) => Format::B8G8R8A8,
            DynamicImage::ImageRgb16(_) => Format::R16G16B16,
            DynamicImage::ImageRgba16(_) => Format::R16G16B16A16,
            _ => bail!("Failed to match the provided image format to a vulkan format!"),
        })
    }

    pub fn from_hdr(path: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(&path)?;
        let decoder = HdrDecoder::new(BufReader::new(file))?;
        let metadata = decoder.metadata();
        let decoded = decoder.read_image_hdr()?;
        let width = metadata.width as u32;
        let height = metadata.height as u32;
        let data = decoded
            .iter()
            .flat_map(|pixel| vec![pixel[0], pixel[1], pixel[2], 1.0])
            .collect::<Vec<_>>();
        let pixels =
            unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) }
                .to_vec();
        Ok(Self {
            pixels,
            format: Format::R32G32B32A32F,
            width,
            height,
            sampler: Sampler::default(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Format {
    R8,
    R8G8,
    R8G8B8,
    R8G8B8A8,
    B8G8R8,
    B8G8R8A8,
    R16,
    R16G16,
    R16G16B16,
    R16G16B16A16,
    R16F,
    R16G16F,
    R16G16B16F,
    R16G16B16A16F,
    R32,
    R32G32,
    R32G32B32,
    R32G32B32A32,
    R32F,
    R32G32F,
    R32G32B32F,
    R32G32B32A32F,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Sampler {
    pub name: String,
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub wrap_s: WrappingMode,
    pub wrap_t: WrappingMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WrappingMode {
    ClampToEdge,
    MirroredRepeat,
    Repeat,
}

impl Default for WrappingMode {
    fn default() -> Self {
        Self::Repeat
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Filter {
    Nearest,
    Linear,
}

impl Default for Filter {
    fn default() -> Self {
        Self::Nearest
    }
}
