use anyhow::{Context, Result};
use egui_wgpu_backend::{
    egui::{ClippedMesh, CtxRef},
    RenderPass,
};
use egui_winit_platform::{Platform, PlatformDescriptor};
use std::{sync::Arc, time::Instant};
use wgpu::CommandEncoder;
use winit::{event::Event, window::Window};

pub struct ScreenDescriptor {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

impl Into<egui_wgpu_backend::ScreenDescriptor> for &ScreenDescriptor {
    fn into(self) -> egui_wgpu_backend::ScreenDescriptor {
        egui_wgpu_backend::ScreenDescriptor {
            physical_width: self.width,
            physical_height: self.height,
            scale_factor: self.scale_factor,
        }
    }
}

// We repaint the UI every frame, so no custom repaint signal is needed
struct RepaintSignal;
impl epi::backend::RepaintSignal for RepaintSignal {
    fn request_repaint(&self) {}
}

pub struct Gui {
    platform: Platform,
    repaint_signal: Arc<RepaintSignal>,
    start_time: Instant,
    last_frame_start: Instant,
    previous_frame_time: Option<f32>,
}

impl Gui {
    pub fn new(screen_descriptor: ScreenDescriptor) -> Self {
        let platform = Platform::new(PlatformDescriptor {
            physical_width: screen_descriptor.width,
            physical_height: screen_descriptor.height,
            scale_factor: screen_descriptor.scale_factor as _,
            font_definitions: egui_wgpu_backend::egui::FontDefinitions::default(),
            style: Default::default(),
        });

        Self {
            platform,
            repaint_signal: std::sync::Arc::new(RepaintSignal {}),
            start_time: Instant::now(),
            previous_frame_time: None,
            last_frame_start: Instant::now(),
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        self.platform.handle_event(&event);
    }

    pub fn context(&self) -> CtxRef {
        self.platform.context()
    }

    pub fn start_frame<'a>(&mut self, scale_factor: f32) -> epi::backend::FrameData {
        self.platform
            .update_time(self.start_time.elapsed().as_secs_f64());

        // Begin to draw the UI frame.
        self.last_frame_start = Instant::now();
        self.platform.begin_frame();
        let app_output = epi::backend::AppOutput::default();

        epi::backend::FrameData {
            info: epi::IntegrationInfo {
                name: "egui_frame",
                web_info: None,
                cpu_usage: self.previous_frame_time,
                native_pixels_per_point: Some(scale_factor),
                prefer_dark_mode: None,
            },
            output: app_output,
            repaint_signal: self.repaint_signal.clone(),
        }
    }

    pub fn end_frame(&mut self, window: &Window) -> Vec<ClippedMesh> {
        let (_output, paint_commands) = self.platform.end_frame(Some(&window));
        let frame_time = (Instant::now() - self.last_frame_start).as_secs_f64() as f32;
        self.previous_frame_time = Some(frame_time);
        self.platform.context().tessellate(paint_commands)
    }
}

pub struct GuiRenderWgpu {
    pub renderpass: egui_wgpu_backend::RenderPass,
}

impl GuiRenderWgpu {
    pub fn new(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        msaa_samples: u32,
    ) -> Self {
        Self {
            renderpass: RenderPass::new(device, output_format, msaa_samples),
        }
    }

    pub fn render(
        &mut self,
        context: CtxRef,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        encoder: &mut CommandEncoder,
        output_view: &wgpu::TextureView,
        paint_jobs: &[ClippedMesh],
    ) -> Result<()> {
        self.renderpass
            .update_texture(&device, &queue, &context.texture());

        self.renderpass.update_user_textures(&device, &queue);

        let screen_descriptor: egui_wgpu_backend::ScreenDescriptor = screen_descriptor.into();

        self.renderpass
            .update_buffers(&device, &queue, &paint_jobs, &screen_descriptor);

        self.renderpass
            .execute(encoder, &output_view, &paint_jobs, &screen_descriptor, None)
            .context("Failed to execute egui renderpass!")?;

        Ok(())
    }
}
