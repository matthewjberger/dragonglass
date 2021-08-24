use anyhow::Result;
use dragonglass_world::World;

pub struct WorldRender {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub shader: wgpu::ShaderModule,
}

impl WorldRender {
    pub fn new(world: &World) -> Result<Self> {
        unimplemented!()
    }
}
