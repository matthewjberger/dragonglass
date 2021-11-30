use anyhow::{Context, Result};
use egui::{epaint::ClippedMesh, CtxRef, FontDefinitions};
use egui_winit_platform::{Platform, PlatformDescriptor};
use epi::*;
use std::{sync::Arc, time::Instant};
use wgpu::CommandEncoder;
use winit::{event::Event, window::Window};

pub struct ScreenDescriptor {
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f32,
}

impl Into<egui_wgpu_backend::ScreenDescriptor> for &ScreenDescriptor {
    fn into(self) -> egui_wgpu_backend::ScreenDescriptor {
        egui_wgpu_backend::ScreenDescriptor {
            physical_width: self.physical_width,
            physical_height: self.physical_height,
            scale_factor: self.scale_factor,
        }
    }
}

// We repaint the UI every frame, so no custom repaint signal is needed
struct RepaintSignal;
impl epi::RepaintSignal for RepaintSignal {
    fn request_repaint(&self) {}
}

pub struct Gui {
    platform: Platform,
    repaint_signal: Arc<RepaintSignal>,
    start_time: Instant,
    last_frame_start: Instant,
    previous_frame_time: Option<f32>,
    pub renderpass: egui_wgpu_backend::RenderPass,
}

impl Gui {
    pub fn new(
        screen_descriptor: ScreenDescriptor,
        renderpass: egui_wgpu_backend::RenderPass,
    ) -> Self {
        let platform = Platform::new(PlatformDescriptor {
            physical_width: screen_descriptor.physical_width,
            physical_height: screen_descriptor.physical_height,
            scale_factor: screen_descriptor.scale_factor as _,
            font_definitions: FontDefinitions::default(),
            style: Default::default(),
        });

        Self {
            platform,
            repaint_signal: std::sync::Arc::new(RepaintSignal {}),
            start_time: Instant::now(),
            previous_frame_time: None,
            last_frame_start: Instant::now(),
            renderpass,
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        self.platform.handle_event(&event);
    }

    pub fn start_frame(&mut self, scale_factor: f32) {
        self.platform
            .update_time(self.start_time.elapsed().as_secs_f64());

        // Begin to draw the UI frame.
        self.last_frame_start = Instant::now();
        self.platform.begin_frame();
        let mut app_output = epi::backend::AppOutput::default();

        let _frame = epi::backend::FrameBuilder {
            info: epi::IntegrationInfo {
                name: "egui_frame",
                web_info: None,
                cpu_usage: self.previous_frame_time,
                native_pixels_per_point: Some(scale_factor),
                prefer_dark_mode: None,
            },
            tex_allocator: &mut self.renderpass,
            output: &mut app_output,
            repaint_signal: self.repaint_signal.clone(),
        }
        .build();
    }

    pub fn context(&self) -> CtxRef {
        self.platform.context().clone()
    }

    pub fn end_frame(&mut self, window: &Window) -> Vec<ClippedMesh> {
        // End the UI frame. We could now handle the output and draw the UI with the backend.
        let (_output, paint_commands) = self.platform.end_frame(Some(&window));

        let frame_time = (Instant::now() - self.last_frame_start).as_secs_f64() as f32;
        self.previous_frame_time = Some(frame_time);

        self.context().tessellate(paint_commands)
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        window: &Window,
        encoder: &mut CommandEncoder,
        output_view: &wgpu::TextureView,
        mut action: impl FnMut(CtxRef) -> Result<()>,
    ) -> Result<()> {
        self.start_frame(screen_descriptor.scale_factor as _);

        action(self.context())?;

        let paint_jobs = self.end_frame(&window);

        let screen_descriptor: egui_wgpu_backend::ScreenDescriptor = screen_descriptor.into();

        self.renderpass
            .update_texture(&device, &queue, &self.context().texture());

        self.renderpass.update_user_textures(&device, &queue);

        self.renderpass
            .update_buffers(&device, &queue, &paint_jobs, &screen_descriptor);

        self.renderpass
            .execute(encoder, &output_view, &paint_jobs, &screen_descriptor, None)
            .context("Failed to execute egui renderpass!")?;

        Ok(())
    }
}
