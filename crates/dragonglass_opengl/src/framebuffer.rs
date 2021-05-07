use crate::texture::*;

#[derive(Default)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    id: u32,
    rbo_id: u32,
    color_texture: Texture,
}

impl Framebuffer {
    pub fn new() -> Self {
        Framebuffer::default()
    }

    pub fn create_with_texture(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.color_texture = Texture::new(gl::TEXTURE_2D);
        self.color_texture
            .load_data(width, height, &[] as &[u8], gl::RGB, gl::TEXTURE_2D);
        unsafe {
            gl::GenFramebuffers(1, &mut self.id);
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.id);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                self.color_texture.id(),
                0,
            );
        }
    }

    pub fn add_depth_buffer(&mut self) {
        unsafe {
            gl::GenRenderbuffers(1, &mut self.rbo_id);
            gl::BindRenderbuffer(gl::RENDERBUFFER, self.rbo_id);
            gl::RenderbufferStorage(
                gl::RENDERBUFFER,
                gl::DEPTH24_STENCIL8,
                self.width as i32,
                self.height as i32,
            );
            gl::BindRenderbuffer(gl::RENDERBUFFER, 0);
            gl::FramebufferRenderbuffer(
                gl::FRAMEBUFFER,
                gl::DEPTH_STENCIL_ATTACHMENT,
                gl::RENDERBUFFER,
                self.rbo_id,
            );
        }
    }

    pub fn bind(&self) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.id);
        }
    }

    pub fn color_texture(&self) -> &Texture {
        &self.color_texture
    }

    pub fn bind_default_framebuffer() {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }
}
