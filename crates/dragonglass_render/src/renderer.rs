use anyhow::{Context, Result};
use dragonglass_gui::{
    egui::{ClippedMesh, CtxRef},
    GuiRenderWgpu, ScreenDescriptor,
};
use dragonglass_world::World;
use log::error;
use raw_window_handle::HasRawWindowHandle;

use crate::world::{render::WorldRender, texture::Texture};

#[allow(dead_code)]
pub struct Renderer {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    world_render: WorldRender,
    dimensions: [u32; 2],
    depth_texture: Texture,
    gui_render: GuiRenderWgpu,
}

impl Renderer {
    pub fn backends() -> wgpu::Backends {
        wgpu::util::backend_bits_from_env().unwrap_or_else(wgpu::Backends::all)
    }

    pub async fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(Self::backends());

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

        let depth_texture =
            Texture::create_depth_texture(&device, dimensions[0], dimensions[1], "Depth Texture");

        let world_render = WorldRender::new(&device, config.format)?;

        let gui_render = GuiRenderWgpu::new(&device, config.format, 1);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            world_render,
            dimensions: *dimensions,
            depth_texture,
            gui_render,
        })
    }

    fn required_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
        wgpu::Limits::default()
            // Use the texture resolution limits from the adapter
            // to support images the size of the surface
            .using_resolution(adapter.limits())
    }

    fn required_features() -> wgpu::Features {
        wgpu::Features::empty()
    }

    fn optional_features() -> wgpu::Features {
        wgpu::Features::empty()
    }

    async fn create_adapter(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface,
    ) -> Result<wgpu::Adapter> {
        wgpu::util::initialize_adapter_from_env_or_default(
            instance,
            Self::backends(),
            Some(surface),
        )
        .await
        .context("No suitable GPU adapters found on the system!")
    }

    async fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
        log::trace!("WGPU Adapter Features: {:#?}", adapter.features());

        adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: (Self::optional_features() & adapter.features())
                        | Self::required_features(),
                    limits: Self::required_limits(adapter),
                    label: Some("Render Device"),
                },
                None,
            )
            .await
            .context("Failed to request a device!")
    }

    pub fn clear(&mut self) {
        self.world_render.clear_textures();
    }

    pub fn load_world(&mut self, world: &World) -> Result<()> {
        self.world_render.load(&self.device, &self.queue, world)
    }

    pub fn resize(&mut self, dimensions: [u32; 2]) {
        if dimensions[0] == 0 || dimensions[1] == 0 {
            return;
        }
        self.dimensions = dimensions;
        self.config.width = dimensions[0];
        self.config.height = dimensions[1];
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = Texture::create_depth_texture(
            &self.device,
            dimensions[0],
            dimensions[1],
            "Depth Texture",
        );
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        world: &World,
        context: CtxRef,
        ui_meshes: &[ClippedMesh],
        screen_descriptor: ScreenDescriptor,
    ) -> Result<()> {
        match self.render_frame(dimensions, world, context, ui_meshes, screen_descriptor) {
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
        dimensions: &[u32; 2],
        world: &World,
        context: CtxRef,
        ui_meshes: &[ClippedMesh],
        screen_descriptor: ScreenDescriptor,
    ) -> Result<(), wgpu::SurfaceError> {
        let height = if dimensions[1] > 0 {
            dimensions[1] as f32
        } else {
            1.0
        };
        let aspect_ratio = dimensions[0] as f32 / height as f32;

        let frame = self.surface.get_current_texture()?;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        encoder.push_debug_group("Main Passes");

        encoder.insert_debug_marker("Render Entities");
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
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            self.world_render
                .update(&self.queue, world, aspect_ratio)
                .expect("Failed to update world render!");
            self.world_render
                .render(&mut render_pass, world)
                .expect("Failed to render world!");
        }

        encoder.insert_debug_marker("Render GUI");
        self.gui_render
            .render(
                context,
                &self.device,
                &self.queue,
                &screen_descriptor,
                &mut encoder,
                &view,
                &ui_meshes,
            )
            .expect("Failed to execute gui render pass!");

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        Ok(())
    }
}
