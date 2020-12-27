use crate::{gui::Gui, input::Input, logger::create_logger, system::System};
use anyhow::Result;
use dragonglass_render::{Backend, Renderer};
use dragonglass_world::World;
use image::io::Reader;
use imgui::{im_str, Ui};
use log::error;
use ncollide3d::world::CollisionWorld;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

pub struct AppConfiguration {
    pub width: u32,
    pub height: u32,
    pub is_fullscreen: bool, // TODO: This isn't respected yet
    pub title: String,
    pub icon: Option<String>,
}

impl Default for AppConfiguration {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            is_fullscreen: false,
            title: "Dragonglass Application".to_string(),
            icon: None,
        }
    }
}

impl AppConfiguration {
    pub fn create_window(&self) -> Result<(EventLoop<()>, Window)> {
        let event_loop = EventLoop::new();

        let mut window_builder = WindowBuilder::new()
            .with_title(self.title.to_string())
            .with_inner_size(PhysicalSize::new(self.width, self.height));

        if let Some(icon_path) = self.icon.as_ref() {
            let image = Reader::open(icon_path)?.decode()?.into_rgba8();
            let (width, height) = image.dimensions();
            let icon = Icon::from_rgba(image.into_raw(), width, height)?;
            window_builder = window_builder.with_window_icon(Some(icon));
        }

        let window = window_builder.build(&event_loop)?;
        Ok((event_loop, window))
    }
}

pub struct AppState {
    pub world: World,
    pub collision_world: CollisionWorld<f32, ()>,
    pub input: Input,
    pub system: System,
    pub renderer: Box<dyn Renderer>,
}

pub trait App {
    fn initialize(&mut self, _state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn create_ui(&mut self, _state: &mut AppState, ui: &Ui) -> Result<()> {
        ui.text(im_str!("Hello!"));
        Ok(())
    }
    fn update(&mut self, _state: &mut AppState) -> Result<()> {
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
    fn on_key(
        &mut self,
        _state: &mut AppState,
        _keystate: ElementState,
        _keycode: VirtualKeyCode,
    ) -> Result<()> {
        Ok(())
    }
    fn handle_events(
        &mut self,
        _state: &mut AppState,
        _event: winit::event::Event<()>,
    ) -> Result<()> {
        Ok(())
    }
}

pub fn run_app(mut app: impl App + 'static, configuration: AppConfiguration) -> Result<()> {
    create_logger()?;

    let (event_loop, window) = configuration.create_window()?;
    let mut gui = Gui::new(&window);

    let logical_size = window.inner_size();
    let window_dimensions = [logical_size.width, logical_size.height];
    let renderer = Box::new(Renderer::create_backend(
        &Backend::Vulkan,
        &window,
        &window_dimensions,
        gui.context_mut(),
    )?);

    let mut state = AppState {
        world: World::new(),
        collision_world: CollisionWorld::new(0.02f32),
        input: Input::default(),
        system: System::new(window_dimensions),
        renderer,
    };

    app.initialize(&mut state)?;

    event_loop.run(move |event, _, control_flow| {
        if let Err(error) = run_loop(&mut app, &window, &mut state, &mut gui, event, control_flow) {
            error!("Application Error: {}", error);
        }
    });
}

fn run_loop(
    app: &mut impl App,
    window: &Window,
    state: &mut AppState,
    gui: &mut Gui,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    state.system.handle_event(&event);
    gui.handle_event(&event, &window);
    state
        .input
        .handle_event(&event, state.system.window_center());
    state.input.allowed = !gui.capturing_input();

    match event {
        Event::MainEventsCleared => {
            if state.input.is_key_pressed(VirtualKeyCode::Escape) || state.system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }

            let draw_data = gui.render_frame(&window, |ui| app.create_ui(state, ui))?;

            app.update(state)?;

            state.collision_world.update();

            state.renderer.render(
                &state.system.window_dimensions,
                &state.world,
                &state.collision_world,
                draw_data,
            )?;
        }
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: keystate,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                },
            ..
        } => {
            if let Err(error) = app.on_key(state, keystate, keycode) {
                error!("{}", error);
            }
        }
        Event::LoopDestroyed => {
            app.cleanup()?;
        }
        _ => {}
    }

    app.handle_events(state, event)?;
    Ok(())
}
