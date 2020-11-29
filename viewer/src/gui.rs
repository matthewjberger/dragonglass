use anyhow::Result;
use imgui::{Context, DrawData, FontConfig, FontSource, Ui};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use winit::{event::Event, window::Window};

pub struct Gui {
    context: Context,
    platform: WinitPlatform,
}

impl Gui {
    pub fn new(window: &Window) -> Self {
        let mut context = Context::create();
        context.set_ini_filename(None);

        let mut platform = WinitPlatform::init(&mut context);

        let hidpi_factor = platform.hidpi_factor();
        let font_size = (13.0 * hidpi_factor) as f32;
        context.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(FontConfig {
                size_pixels: font_size,
                ..FontConfig::default()
            }),
        }]);
        context.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

        platform.attach_window(context.io_mut(), &window, HiDpiMode::Rounded);

        Self { context, platform }
    }

    pub fn handle_event<T>(&mut self, event: &Event<T>, window: &Window) {
        self.platform
            .handle_event(self.context.io_mut(), &window, &event);
    }

    pub fn render_frame(
        &mut self,
        window: &Window,
        mut action: impl FnMut(&Ui),
    ) -> Result<&DrawData> {
        self.platform.prepare_frame(self.context.io_mut(), window)?;

        let ui = self.context.frame();

        action(&ui);

        self.platform.prepare_render(&ui, window);

        let draw_data = ui.render();

        Ok(draw_data)
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    pub fn capturing_input(&self) -> bool {
        self.context.io().want_capture_keyboard || self.context.io().want_capture_mouse
    }
}
