use anyhow::{Context, Result};
use dragonglass_gui::{Gui, RenderPass as GuiRenderPass, ScreenDescriptor};
use dragonglass_world::World;
use log::error;
use raw_window_handle::HasRawWindowHandle;

use crate::world::{render::WorldRender, texture::Texture};

#[cfg(target_family = "wasm")]
const BACKEND: wgpu::Backends = wgpu::Backends::BROWSER_WEBGPU;

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
    pub gui: Gui,
    world_render: WorldRender,
    dimensions: [u32; 2],
    depth_texture: Texture,
}

impl Renderer {
    pub async fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
        scale_factor: f32,
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

        let depth_texture =
            Texture::create_depth_texture(&device, dimensions[0], dimensions[1], "Depth Texture");

        let world_render = WorldRender::new(&device, config.format)?;

        let gui_renderpass = GuiRenderPass::new(&device, config.format, 1);
        let gui = Gui::new(
            ScreenDescriptor {
                physical_width: dimensions[0],
                physical_height: dimensions[1],
                scale_factor,
            },
            gui_renderpass,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            gui,
            world_render,
            dimensions: *dimensions,
            depth_texture,
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
                force_fallback_adapter: false,
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
        self.world_render.load(&self.queue, world)
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
        // The gui requires winit, but if the gui backend is
        // changed out for a different windowing system this parameter can be removed
        window: &winit::window::Window,
        dimensions: &[u32; 2],
        world: &World,
    ) -> Result<()> {
        match self.render_frame(window, dimensions, world) {
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
        window: &winit::window::Window,
        dimensions: &[u32; 2],
        world: &World,
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

        self.gui.render(
            &self.device,
            &self.queue,
            &ScreenDescriptor {
                physical_width: dimensions[0],
                physical_height: dimensions[1],
                scale_factor: window.scale_factor() as _,
            },
            &window,
            &mut encoder,
            &view,
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        Ok(())
    }
}
