use crate::world::{
    texture::Texture,
    uniform::{Geometry, Uniform, UniformBinding},
};
use anyhow::Result;
use dragonglass_world::{AlphaMode, EntityStore, MeshRender, World};
use nalgebra_glm as glm;
use wgpu::Queue;

pub(crate) struct WorldRender {
    render: Render,
}

impl WorldRender {
    // TODO: Make this just take a render
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Result<Self> {
        Ok(Self {
            render: Render::new(device, texture_format),
        })
    }

    pub fn load(&self, queue: &Queue, world: &World) -> Result<()> {
        self.render
            .geometry
            .upload_vertices(queue, 0, &world.geometry.vertices);
        self.render
            .geometry
            .upload_indices(queue, 0, &world.geometry.indices);
        Ok(())
    }

    pub fn update(&self, queue: &Queue, world: &World, aspect_ratio: f32) -> Result<()> {
        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;

        self.render.uniform_binding.upload_uniform_data(
            queue,
            0,
            &[Uniform {
                view,
                projection,
                model: glm::Mat4::identity(),
            }],
        );

        Ok(())
    }

    pub fn render<'a, 'b>(
        &'a self,
        queue: &Queue,
        render_pass: &'b mut wgpu::RenderPass<'a>,
        world: &'a World,
    ) -> Result<()> {
        self.render.bind(render_pass);

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            for graph in world.scene.graphs.iter() {
                graph.walk(|node_index| {
                    let entity = graph[node_index];
                    let entry = world.ecs.entry_ref(entity)?;

                    let mesh_name = match entry.get_component::<MeshRender>().ok() {
                        Some(mesh_render) => &mesh_render.name,
                        None => return Ok(()),
                    };

                    let mesh = match world.geometry.meshes.get(mesh_name) {
                        Some(mesh) => mesh,
                        None => return Ok(()),
                    };

                    self.render.uniform_binding.upload_uniform_data(
                        queue,
                        (std::mem::size_of::<glm::Mat4>() * 2) as _,
                        &[world.entity_global_transform_matrix(entity)?],
                    );

                    match alpha_mode {
                        AlphaMode::Opaque | AlphaMode::Mask => {} /* Disable blending*/
                        AlphaMode::Blend => {}                    /* Enable blending */
                    }

                    for primitive in mesh.primitives.iter() {
                        let start = primitive.first_index as u32;
                        let end = start + (primitive.number_of_indices as u32);
                        render_pass.draw_indexed(start..end, 0, 0..1);
                    }

                    Ok(())
                })?;
            }
        }

        Ok(())
    }
}

struct Render {
    pipeline: wgpu::RenderPipeline,
    geometry: Geometry,
    uniform_binding: UniformBinding,
}

impl Render {
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Self {
        let geometry = Geometry::new(device);

        let uniform_binding = UniformBinding::new(device);

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_binding.bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[geometry.vertex_buffer_layout.clone()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[texture_format.into()],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
        });

        Self {
            pipeline,
            geometry,
            uniform_binding,
        }
    }

    pub fn bind<'a, 'b>(&'a self, render_pass: &'b mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, self.geometry.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.geometry.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        render_pass.set_bind_group(0, &self.uniform_binding.bind_group, &[]);
    }
}
