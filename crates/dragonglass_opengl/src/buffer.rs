use gl::types::*;
use std::{mem, slice};

#[derive(Default, Debug)]
pub struct VertexArrayObject {
    id: GLuint,
}

impl VertexArrayObject {
    pub fn new() -> Self {
        let mut id = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut id);
        }
        VertexArrayObject { id }
    }

    pub fn bind(&self) {
        unsafe {
            gl::BindVertexArray(self.id);
        }
    }

    pub fn configure_attribute(&self, index: u32, count: u32, total: u32, offset: u32) {
        self.bind();
        let float_size = mem::size_of::<GLfloat>() as u32;
        unsafe {
            gl::EnableVertexAttribArray(index);
            gl::VertexAttribPointer(
                index,
                count as i32,
                gl::FLOAT,
                gl::FALSE,
                (total * float_size) as i32,
                (offset * float_size) as *const GLvoid,
            );
        }
    }
}

pub enum DrawingHint {
    StreamDraw,
    StreamRead,
    StreamCopy,
    StaticDraw,
    StaticRead,
    StaticCopy,
    DynamicDraw,
    DynamicRead,
    DynamicCopy,
}

pub enum BufferKind {
    Array,
    Element,
}

impl Default for BufferKind {
    fn default() -> Self {
        BufferKind::Array
    }
}

#[derive(Default)]
pub struct Buffer {
    id: GLuint,
    kind: BufferKind,
    data: Vec<u8>,
}

impl Buffer {
    pub fn new(kind: BufferKind) -> Self {
        let mut id = 0;
        unsafe {
            gl::GenBuffers(1, &mut id);
        }
        Buffer {
            id,
            kind,
            ..Default::default()
        }
    }

    pub fn add_data<T>(&mut self, data: &[T]) {
        let len = mem::size_of::<T>() * data.len();
        let byte_slice = unsafe { slice::from_raw_parts(data.as_ptr() as *const u8, len) };
        self.data.extend(byte_slice.iter().clone());
    }

    pub fn upload(&mut self, vao: &VertexArrayObject, hint: DrawingHint) {
        vao.bind();
        self.bind();
        unsafe {
            gl::BufferData(
                self.kind(),
                (self.data.len() * mem::size_of::<u8>()) as GLsizeiptr,
                self.data.as_ptr() as *const GLvoid,
                Buffer::map_hint(&hint),
            );
        }
        self.data.clear();
    }

    pub fn bind(&self) {
        unsafe {
            gl::BindBuffer(self.kind(), self.id as u32);
        }
    }

    pub fn free(&self) {
        unsafe {
            gl::DeleteBuffers(1, &self.id as *const u32);
        }
    }

    pub fn type_size(&self) -> usize {
        Buffer::map_type_size(&self.kind)
    }

    pub fn type_representation(&self) -> u32 {
        Buffer::map_type_representation(&self.kind)
    }

    fn kind(&self) -> GLuint {
        Buffer::map_type(&self.kind)
    }

    fn map_type(buffer_type: &BufferKind) -> GLuint {
        match buffer_type {
            BufferKind::Array => gl::ARRAY_BUFFER,
            BufferKind::Element => gl::ELEMENT_ARRAY_BUFFER,
        }
    }

    fn map_type_size(buffer_type: &BufferKind) -> usize {
        match buffer_type {
            BufferKind::Array => mem::size_of::<GLfloat>(),
            BufferKind::Element => mem::size_of::<GLuint>(),
        }
    }

    fn map_type_representation(buffer_type: &BufferKind) -> u32 {
        match buffer_type {
            BufferKind::Array => gl::FLOAT,
            BufferKind::Element => gl::UNSIGNED_INT,
        }
    }

    fn map_hint(drawing_hint: &DrawingHint) -> GLuint {
        match drawing_hint {
            DrawingHint::StreamDraw => gl::STREAM_DRAW,
            DrawingHint::StreamRead => gl::STREAM_READ,
            DrawingHint::StreamCopy => gl::STREAM_COPY,
            DrawingHint::StaticDraw => gl::STATIC_DRAW,
            DrawingHint::StaticRead => gl::STATIC_READ,
            DrawingHint::StaticCopy => gl::STATIC_COPY,
            DrawingHint::DynamicDraw => gl::DYNAMIC_DRAW,
            DrawingHint::DynamicRead => gl::DYNAMIC_READ,
            DrawingHint::DynamicCopy => gl::DYNAMIC_COPY,
        }
    }
}
