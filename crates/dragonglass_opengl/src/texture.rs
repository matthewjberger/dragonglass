use gl::types::GLvoid;
use image::{DynamicImage::*, GenericImageView};
use std::collections::HashMap;
use std::{
    ptr,
    sync::{Arc, RwLock},
};

pub type TextureCacheRef = Arc<RwLock<TextureCache>>;

#[derive(Default)]
pub struct TextureCache {
    items: HashMap<String, Texture>,
}

impl TextureCache {
    pub fn new() -> Self {
        TextureCache::default()
    }

    pub fn retrieve(&mut self, path: &str) -> &Texture {
        if !self.items.contains_key(path) {
            self.items
                .insert(path.to_string(), Texture::from_file(path));
        }
        &self.items[path]
    }
}

#[derive(Default)]
pub struct Texture {
    id: u32,
    target: u32,
}

impl Texture {
    pub fn new(target: u32) -> Self {
        let mut id = 0;
        unsafe {
            gl::GenTextures(1, &mut id);
        }
        Texture { id, target }
    }

    pub fn from_file(path: &str) -> Self {
        let mut texture = Texture::new(gl::TEXTURE_2D);
        texture.load_image(path, texture.target, true);
        texture.set_wrapping_repeat();
        texture
    }

    pub fn cubemap_from_files(paths: &[String; 6]) -> Self {
        let mut texture = Texture::new(gl::TEXTURE_CUBE_MAP);
        for (offset, path) in paths.iter().enumerate() {
            texture.load_image(path, gl::TEXTURE_CUBE_MAP_POSITIVE_X + offset as u32, false);
            texture.set_wrapping_clamp();
        }
        texture
    }

    pub fn bind(&self, unit: u32) {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + unit);
            gl::BindTexture(self.target, self.id);
        }
    }

    pub fn free(&self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn load_data(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
        pixel_format: u32,
        target: u32,
    ) {
        self.bind(0);
        let image_data = if pixels.is_empty() {
            ptr::null()
        } else {
            pixels.as_ptr()
        };
        unsafe {
            gl::TexImage2D(
                target,
                0,
                pixel_format as i32,
                width as i32,
                height as i32,
                0,
                pixel_format,
                gl::UNSIGNED_BYTE,
                image_data as *const GLvoid,
            );
            gl::GenerateMipmap(target);
        }
        self.set_filtering_linear();
    }

    fn load_image(&mut self, path: &str, target: u32, flipv: bool) {
        let mut img = image::open(path).expect("Texture failed to load!");
        let pixel_format = match img {
            ImageLuma8(_) => gl::RED,
            ImageLumaA8(_) => gl::RG,
            ImageRgb8(_) => gl::RGB,
            ImageRgba8(_) => gl::RGBA,
            ImageBgr8(_) => gl::BGR,
            ImageBgra8(_) => gl::BGRA,
            _ => gl::RGBA, // FIXME: Bail here instead
        };
        if flipv {
            img = img.flipv();
        }
        self.load_data(
            img.width(),
            img.height(),
            img.as_bytes(),
            pixel_format,
            target,
        );
    }

    fn set_wrapping_clamp(&mut self) {
        unsafe {
            gl::TexParameteri(self.target, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(self.target, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(self.target, gl::TEXTURE_WRAP_R, gl::CLAMP_TO_EDGE as i32);
        }
    }

    fn set_wrapping_repeat(&mut self) {
        unsafe {
            gl::TexParameteri(self.target, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
            gl::TexParameteri(self.target, gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
            gl::TexParameteri(self.target, gl::TEXTURE_WRAP_R, gl::REPEAT as i32);
        }
    }

    fn set_filtering_linear(&mut self) {
        unsafe {
            gl::TexParameteri(self.target, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(self.target, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
        }
    }
}
