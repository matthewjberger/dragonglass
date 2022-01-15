use gl::{self, types::*};
use std::mem;

pub struct GeometryBuffer {
    vao: u32,
    vbo: u32,
    ebo: Option<u32>,
}

impl GeometryBuffer {
    pub fn new<T: Copy>(
        vertices: &[T],
        indices: Option<&[u32]>,
        vertex_attributes: &[usize],
    ) -> Self {
        let vao = Self::create_vao();
        let vbo = Self::create_buffer(&vertices, gl::ARRAY_BUFFER);

        let mut ebo = None;
        if let Some(indices) = indices {
            ebo = Some(Self::create_buffer(&indices, gl::ELEMENT_ARRAY_BUFFER));
        }

        Self::add_vertex_attributes::<T>(vertex_attributes);

        Self { vao, vbo, ebo }
    }

    fn create_vao() -> u32 {
        let mut vao = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
        }
        vao
    }

    fn create_buffer<T: Copy>(data: &[T], kind: GLuint) -> u32 {
        let mut buffer = 0;
        unsafe {
            gl::GenBuffers(1, &mut buffer);
            gl::BindBuffer(kind, buffer);
            gl::BufferData(
                kind,
                (data.len() * mem::size_of::<T>()) as _,
                data.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
        }
        buffer
    }

    fn add_vertex_attributes<T: Copy>(vertex_attributes: &[usize]) {
        let mut index = 0;
        let mut offset = 0;
        let mut add_vertex = |count: usize| {
            unsafe {
                gl::EnableVertexAttribArray(index);
                gl::VertexAttribPointer(
                    index,
                    count as i32,
                    gl::FLOAT,
                    gl::FALSE,
                    std::mem::size_of::<T>() as i32,
                    (offset * mem::size_of::<f32>()) as *const GLvoid,
                );
            }
            index += 1;
            offset += count;
        };
        vertex_attributes.iter().for_each(|i| add_vertex(*i));
    }

    pub fn bind(&self) {
        unsafe {
            gl::BindVertexArray(self.vao);
        }
    }
}

impl Drop for GeometryBuffer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, self.vao as _);
            gl::DeleteBuffers(1, self.vbo as _);
            if let Some(ebo) = self.ebo.as_ref() {
                gl::DeleteBuffers(1, ebo as _);
            }
        }
    }
}
