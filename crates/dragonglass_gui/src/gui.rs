use egui::{epaint::ClippedShape, CtxRef, FontDefinitions, FontFamily, TextStyle};
use egui_winit_platform::{Platform, PlatformDescriptor};
use epi;
use std::{sync::Arc, time::Instant};
use winit::{dpi::PhysicalSize, event::Event, window::Window};

pub struct ScreenDescriptor {
    pub dimensions: PhysicalSize<u32>,
    pub scale_factor: f32,
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
        let mut font_definitions = FontDefinitions::default();

        // Large button text:
        font_definitions
            .family_and_size
            .insert(TextStyle::Body, (FontFamily::Proportional, 18.0));

        let platform = Platform::new(PlatformDescriptor {
            physical_width: screen_descriptor.dimensions.width,
            physical_height: screen_descriptor.dimensions.height,
            scale_factor: screen_descriptor.scale_factor as _,
            font_definitions,
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

    pub fn captures_event(&self, event: &Event<()>) -> bool {
        self.platform.captures_event(event)
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

    pub fn end_frame(&mut self, window: &Window) -> Vec<ClippedShape> {
        let (_output, clipped_shapes) = self.platform.end_frame(Some(window));
        let frame_time = (Instant::now() - self.last_frame_start).as_secs_f64() as f32;
        self.previous_frame_time = Some(frame_time);
        clipped_shapes
    }
}
