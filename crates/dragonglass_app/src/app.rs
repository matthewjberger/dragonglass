use crate::{
    logger::create_logger,
    state::{Input, System},
    AppState,
};
use anyhow::Result;
use dragonglass_gui::{Gui, ScreenDescriptor};
use dragonglass_render::{create_render_backend, Backend};
use dragonglass_world::{SdfFont, World};
use image::io::Reader;
use std::path::PathBuf;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, WindowBuilder},
};

pub trait App {
    fn initialize(&mut self, _app_state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn update(&mut self, _app_state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn gui_active(&mut self) -> bool {
        return false;
    }
    fn update_gui(&mut self, _app_state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn on_file_dropped(&mut self, _path: &PathBuf, _app_state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
    fn on_mouse(
        &mut self,
        _button: &MouseButton,
        _button_state: &ElementState,
        _app_state: &mut AppState,
    ) -> Result<()> {
        Ok(())
    }
    fn on_key(&mut self, _input: KeyboardInput, _app_state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: &Event<()>, _app_state: &mut AppState) -> Result<()> {
        Ok(())
    }
}

pub struct AppConfig {
    pub width: u32,
    pub height: u32,
    pub is_fullscreen: bool, // TODO: This isn't respected yet
    pub title: String,
    pub icon: Option<String>,
    pub backend: Backend,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            is_fullscreen: false,
            title: "Dragonglass Application".to_string(),
            backend: Backend::Vulkan,
            icon: None,
        }
    }
}

pub fn run_application(mut app: impl App + 'static, config: AppConfig) -> Result<()> {
    create_logger()?;

    let event_loop = EventLoop::new();

    let mut window_builder = WindowBuilder::new()
        .with_title(config.title.to_string())
        .with_inner_size(PhysicalSize::new(config.width, config.height));

    if let Some(icon_path) = config.icon.as_ref() {
        let image = Reader::open(icon_path)?.decode()?.into_rgba8();
        let (width, height) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), width, height)?;
        window_builder = window_builder.with_window_icon(Some(icon));
    }

    let mut window = window_builder.build(&event_loop)?;

    let window_dimensions = window.inner_size();
    let mut renderer = create_render_backend(
        &config.backend,
        &window,
        &[window_dimensions.width, window_dimensions.height],
    )?;

    let mut input = Input::default();
    let mut system = System::new(window_dimensions);

    let screen_descriptor = ScreenDescriptor {
        dimensions: window_dimensions,
        scale_factor: window.scale_factor() as _,
    };
    let mut gui = Gui::new(screen_descriptor);

    let mut world = World::new()?;
    world.fonts.insert(
        "default".to_string(),
        SdfFont::new("assets/fonts/font.fnt", "assets/fonts/font_sdf_rgba.png")?,
    );

    app.initialize(&mut AppState {
        window: &mut window,
        world: &mut world,
        gui: &mut gui,
        renderer: &mut renderer,
        input: &mut input,
        system: &mut system,
    })?;

    event_loop.run(move |event, _, control_flow| {
        let state = AppState {
            window: &mut window,
            world: &mut world,
            gui: &mut gui,
            renderer: &mut renderer,
            input: &mut input,
            system: &mut system,
        };
        if let Err(error) = run_loop(&mut app, state, event, control_flow) {
            eprintln!("Application Error: {}", error);
        }
    });
}

fn run_loop(
    app: &mut impl App,
    mut app_state: AppState,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    if app.gui_active() {
        app_state.gui.handle_event(&event);
    }
    if !app.gui_active() || !app_state.gui.captures_event(&event) {
        app.handle_events(&event, &mut app_state)?;
        app_state.system.handle_event(&event);
        app_state
            .input
            .handle_event(&event, app_state.system.window_center());
    }

    match event {
        Event::NewEvents(_) => {
            if app_state.system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::WindowEvent { ref event, .. } => match event {
            WindowEvent::DroppedFile(ref path) => app.on_file_dropped(path, &mut app_state)?,
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            WindowEvent::MouseInput { button, state, .. } => {
                app.on_mouse(button, state, &mut app_state)?
            }
            WindowEvent::KeyboardInput { input, .. } => {
                if let (Some(VirtualKeyCode::Escape), ElementState::Pressed) =
                    (input.virtual_keycode, input.state)
                {
                    *control_flow = ControlFlow::Exit;
                }
                app.on_key(*input, &mut app_state)?;
            }
            _ => (),
        },
        Event::MainEventsCleared => {
            app_state.world.tick(app_state.system.delta_time as f32)?;
            app.update(&mut app_state)?;

            let clipped_shapes = if app.gui_active() {
                let _frame_data = app_state
                    .gui
                    .start_frame(app_state.window.scale_factor() as _);
                app.update_gui(&mut app_state)?;
                app_state.gui.end_frame(app_state.window)
            } else {
                Vec::new()
            };

            let dimensions = app_state.window.inner_size();
            app_state.renderer.render(
                &[dimensions.width, dimensions.height],
                app_state.world,
                &app_state.gui.context(),
                clipped_shapes,
            )?;
        }
        Event::LoopDestroyed => {
            app.cleanup()?;
        }
        _ => (),
    }

    Ok(())
}
