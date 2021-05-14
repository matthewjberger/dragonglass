pub use gl::types::*;
use std::{ffi::CString, fs, ptr, str};

pub enum ShaderKind {
    Vertex,
    Fragment,
    Geometry,
    TessellationControl,
    TessellationEvaluation,
    Compute,
}

impl Default for ShaderKind {
    fn default() -> Self {
        ShaderKind::Vertex
    }
}

#[derive(Default)]
pub struct Shader {
    pub id: GLuint,
    pub kind: ShaderKind,
}

impl Shader {
    pub fn new(kind: ShaderKind) -> Shader {
        let id = unsafe { gl::CreateShader(Shader::map_type(&kind)) };
        Self { id, kind }
    }

    pub fn load_file(&mut self, path: &str) {
        let text = fs::read_to_string(path).unwrap();
        self.load(&text);
    }

    pub fn load(&self, source: &str) {
        let source_str = CString::new(source.as_bytes()).unwrap();
        unsafe {
            gl::ShaderSource(self.id, 1, &source_str.as_ptr(), ptr::null());
            gl::CompileShader(self.id);
        }
        self.check_compilation();
    }

    // TODO: Add something to identify the shader that failed
    fn check_compilation(&self) {
        let mut success = gl::FALSE as GLint;
        unsafe {
            gl::GetShaderiv(self.id, gl::COMPILE_STATUS, &mut success);
        }
        if success == gl::TRUE as GLint {
            return;
        }
        let mut info_log_length = 0;
        unsafe {
            gl::GetShaderiv(self.id, gl::INFO_LOG_LENGTH, &mut info_log_length);
        }
        let mut info_log = vec![0; info_log_length as usize];
        unsafe {
            gl::GetShaderInfoLog(
                self.id,
                info_log_length,
                ptr::null_mut(),
                info_log.as_mut_ptr() as *mut GLchar,
            );
        }
        log::error!(
            "ERROR: Shader compilation failed.\n{}\n",
            str::from_utf8(&info_log).unwrap()
        );
    }

    fn map_type(shader_type: &ShaderKind) -> GLuint {
        match shader_type {
            ShaderKind::Vertex => gl::VERTEX_SHADER,
            ShaderKind::Fragment => gl::FRAGMENT_SHADER,
            ShaderKind::Geometry => gl::GEOMETRY_SHADER,
            ShaderKind::TessellationControl => gl::TESS_CONTROL_SHADER,
            ShaderKind::TessellationEvaluation => gl::TESS_EVALUATION_SHADER,
            ShaderKind::Compute => gl::COMPUTE_SHADER,
        }
    }
}

#[derive(Default)]
pub struct ShaderProgram {
    id: GLuint,
    shader_ids: Vec<GLuint>,
}

impl ShaderProgram {
    pub fn new() -> Self {
        Self {
            id: unsafe { gl::CreateProgram() },
            shader_ids: Vec::new(),
        }
    }

    pub fn id(&self) -> GLuint {
        self.id
    }

    pub fn vertex_shader_file(&mut self, path: &str) -> &mut Self {
        self.attach_shader_file(ShaderKind::Vertex, path)
    }

    pub fn vertex_shader_source(&mut self, source: &str) -> &mut Self {
        self.attach_shader_source(ShaderKind::Vertex, source)
    }

    pub fn geometry_shader_file(&mut self, path: &str) -> &mut Self {
        self.attach_shader_file(ShaderKind::Geometry, path)
    }

    pub fn geometry_shader_source(&mut self, source: &str) -> &mut Self {
        self.attach_shader_source(ShaderKind::Geometry, source)
    }

    pub fn tessellation_control_shader_file(&mut self, path: &str) -> &mut Self {
        self.attach_shader_file(ShaderKind::TessellationControl, path)
    }

    pub fn tessellation_control_shader_source(&mut self, source: &str) -> &mut Self {
        self.attach_shader_source(ShaderKind::TessellationControl, source)
    }

    pub fn tessellation_evaluation_shader_file(&mut self, path: &str) -> &mut Self {
        self.attach_shader_file(ShaderKind::TessellationEvaluation, path)
    }

    pub fn tessellation_evaluation_shader_source(&mut self, source: &str) -> &mut Self {
        self.attach_shader_source(ShaderKind::TessellationEvaluation, source)
    }

    pub fn compute_shader_file(&mut self, path: &str) -> &mut Self {
        self.attach_shader_file(ShaderKind::Compute, path)
    }

    pub fn compute_shader_source(&mut self, source: &str) -> &mut Self {
        self.attach_shader_source(ShaderKind::Compute, source)
    }

    pub fn fragment_shader_file(&mut self, path: &str) -> &mut Self {
        self.attach_shader_file(ShaderKind::Fragment, path)
    }

    pub fn fragment_shader_source(&mut self, source: &str) -> &mut Self {
        self.attach_shader_source(ShaderKind::Fragment, source)
    }

    pub fn link(&mut self) {
        unsafe {
            gl::LinkProgram(self.id);
            for id in &self.shader_ids {
                gl::DeleteShader(*id);
            }
        }
        self.shader_ids.clear();
    }

    pub fn use_program(&self) {
        unsafe {
            gl::UseProgram(self.id);
        }
    }

    pub fn uniform_location(&self, name: &str) -> GLint {
        let name: CString = CString::new(name.as_bytes()).unwrap();
        unsafe { gl::GetUniformLocation(self.id, name.as_ptr()) }
    }

    pub fn set_uniform_int(&self, name: &str, value: i32) {
        self.use_program();
        let location = self.uniform_location(name);
        unsafe {
            gl::Uniform1i(location, value);
        }
    }

    pub fn set_uniform_float(&self, name: &str, value: f32) {
        self.use_program();
        let location = self.uniform_location(name);
        unsafe {
            gl::Uniform1f(location, value);
        }
    }

    pub fn set_uniform_matrix4x4(&self, name: &str, data: &[GLfloat]) {
        self.use_program();
        let location = self.uniform_location(name);
        unsafe {
            gl::UniformMatrix4fv(location, 1, gl::FALSE, data.as_ptr());
        }
    }

    // TODO: Range check the slice parameters
    pub fn set_uniform_vec4(&self, name: &str, data: &[GLfloat]) {
        self.use_program();
        let location = self.uniform_location(name);
        unsafe {
            gl::Uniform4fv(location, 1, data.as_ptr());
        }
    }

    pub fn set_uniform_vec3(&self, name: &str, data: &[GLfloat]) {
        self.use_program();
        let location = self.uniform_location(name);
        unsafe {
            gl::Uniform3fv(location, 1, data.as_ptr());
        }
    }

    fn attach_shader_file(&mut self, kind: ShaderKind, path: &str) -> &mut Self {
        let mut shader = Shader::new(kind);
        shader.load_file(path);
        self.attach(&shader)
    }

    fn attach_shader_source(&mut self, kind: ShaderKind, source: &str) -> &mut Self {
        let shader = Shader::new(kind);
        shader.load(source);
        self.attach(&shader)
    }

    fn attach(&mut self, shader: &Shader) -> &mut Self {
        unsafe {
            gl::AttachShader(self.id, shader.id);
        }
        self.shader_ids.push(shader.id);
        self
    }
}

impl Drop for ShaderProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id);
        }
    }
}
