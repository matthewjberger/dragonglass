use anyhow::Result;
use imgui::{im_str, Condition, Context, DrawData, FontConfig, FontSource};
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

        platform.attach_window(context.io_mut(), window, HiDpiMode::Rounded);

        Self { context, platform }
    }

    pub fn handle_event<T>(&mut self, event: &Event<T>, window: &Window) {
        self.platform
            .handle_event(self.context.io_mut(), window, event);
    }

    pub fn render_frame(&mut self, window: &Window) -> Result<&DrawData> {
        self.platform.prepare_frame(self.context.io_mut(), window)?;

        let ui = self.context.frame();

        imgui::Window::new(im_str!("Hello world"))
            .size([300.0, 100.0], Condition::FirstUseEver)
            .build(&ui, || {
                ui.text(im_str!("Hello world!"));
                ui.text(im_str!("This...is...imgui-rs!"));
                ui.separator();
                let mouse_pos = ui.io().mouse_pos;
                ui.text(format!(
                    "Mouse Position: ({:.1},{:.1})",
                    mouse_pos[0], mouse_pos[1]
                ));
            });

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
