use crate::world::WorldRender;
use anyhow::{Context, Result};
use dragonglass_world::{EntityStore, MeshRender, World};
use log::error;
use raw_window_handle::HasRawWindowHandle;

#[cfg(target_os = "windows")]
const BACKEND: wgpu::Backends = wgpu::Backends::DX12;

#[cfg(target_os = "macos")]
const BACKEND: wgpu::Backends = wgpu::Backends::METAL;

#[cfg(target_os = "linux")]
const BACKEND: wgpu::Backends = wgpu::Backends::VULKAN;

#[allow(dead_code)]
pub struct Renderer {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    world_render: Option<WorldRender>,
    dimensions: [u32; 2],
}

impl Renderer {
    pub async fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(BACKEND);

        let surface = unsafe { instance.create_surface(window_handle) };

        let adapter = Self::create_adapter(&instance, &surface).await?;

        let (device, queue) = Self::request_device(&adapter).await?;

        let swapchain_format = surface
            .get_preferred_format(&adapter)
            .context("Failed to get preferred surface format!")?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: dimensions[0],
            height: dimensions[1],
            present_mode: wgpu::PresentMode::Fifo,
        };

        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            world_render: None,
            dimensions: *dimensions,
        })
    }

    async fn create_adapter(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface,
    ) -> Result<wgpu::Adapter> {
        instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
            })
            .await
            .context("Failed to request a GPU adapter!")
    }

    async fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .context("Failed to request a device!")?;
        Ok((device, queue))
    }

    pub fn load_world(&mut self, world: &World) -> Result<()> {
        self.world_render = Some(WorldRender::new(&self.device, self.config.format, world)?);
        Ok(())
    }

    pub fn resize(&mut self, dimensions: [u32; 2]) {
        if dimensions[0] == 0 || dimensions[1] == 0 {
            return;
        }
        self.dimensions = dimensions;
        self.config.width = dimensions[0];
        self.config.height = dimensions[1];
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, dimensions: &[u32; 2], world: &World) -> Result<()> {
        match self.render_frame(dimensions, world) {
            Ok(_) => {}
            // Recreate the swapchain if lost
            Err(wgpu::SurfaceError::Lost) => self.resize(self.dimensions),
            // The system is out of memory, we should probably quit
            // Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
            // All other errors should be resolved by the next frame
            Err(e) => error!("{:?}", e),
        }
        Ok(())
    }

    fn render_frame(
        &mut self,
        _dimensions: &[u32; 2],
        world: &World,
    ) -> Result<(), wgpu::SurfaceError> {
        if let Some(world_render) = self.world_render.as_mut() {
            world_render
                .update(&self.queue, world)
                .expect("Failed to update world!");
        }

        let frame = self.surface.get_current_frame()?.output;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            if let Some(world_render) = self.world_render.as_ref() {
                render_pass.set_pipeline(&world_render.render_pipeline);

                render_pass.set_vertex_buffer(0, world_render.vertex_buffer.slice(..));
                render_pass.set_index_buffer(
                    world_render.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );

                render_pass.set_bind_group(0, &world_render.world_uniforms.bind_group, &[]);

                // let uniform_alignment = self.device.limits().min_uniform_buffer_offset_alignment;
                let uniform_alignment = 256;
                for node in world.flatten_scenegraphs().iter() {
                    let entity = world
                        .ecs
                        .entry_ref(node.entity)
                        .expect("Failed to get entity!");

                    let mesh_component_result = entity.get_component::<MeshRender>();
                    match mesh_component_result {
                        Ok(mesh_component) => {
                            if let Some(mesh) = world.geometry.meshes.get(&mesh_component.name) {
                                let offset = (node.offset as wgpu::DynamicOffset)
                                    * (uniform_alignment as wgpu::DynamicOffset);
                                render_pass.set_bind_group(
                                    1,
                                    &world_render.entity_uniforms.bind_group,
                                    &[offset],
                                );

                                for primitive in mesh.primitives.iter() {
                                    let first_index = primitive.first_index as u32;
                                    let last_index = (primitive.first_index
                                        + primitive.number_of_indices)
                                        as u32;
                                    render_pass.draw_indexed(first_index..last_index, 0, 0..1);
                                }
                            }
                        }
                        Err(_) => return Ok(()),
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }
}
