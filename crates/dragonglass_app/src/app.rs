use crate::{logger::create_logger, Input, Resources, System};
use anyhow::Result;
use dragonglass_config::Config;
use dragonglass_gui::{Gui, ScreenDescriptor};
use dragonglass_render::{create_render_backend, Backend};
use dragonglass_world::{SdfFont, Viewport, World};
use image::io::Reader;
use std::path::PathBuf;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, WindowBuilder},
};

pub trait App {
    fn initialize(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }
    fn update(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }
    fn gui_active(&mut self) -> bool {
        false
    }
    fn update_gui(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }
    fn on_file_dropped(&mut self, _path: &PathBuf, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
    fn on_mouse(
        &mut self,
        _button: &MouseButton,
        _button_state: &ElementState,
        _resources: &mut Resources,
    ) -> Result<()> {
        Ok(())
    }
    fn on_key(&mut self, _input: KeyboardInput, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: &Event<()>, _resources: &mut Resources) -> Result<()> {
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

    let mut input = Input::default();
    let mut system = System::new(window_dimensions);

    let screen_descriptor = ScreenDescriptor {
        dimensions: window_dimensions,
        scale_factor: window.scale_factor() as _,
    };
    let mut gui = Gui::new(screen_descriptor);

    let viewport = Viewport {
        x: 0.0,
        y: 0.0,
        width: window_dimensions.width as _,
        height: window_dimensions.height as _,
    };
    let mut renderer = create_render_backend(&config.backend, &window, viewport)?;

    let mut world = World::new()?;
    world.fonts.insert(
        "default".to_string(),
        SdfFont::new("assets/fonts/font.fnt", "assets/fonts/font_sdf_rgba.png")?,
    );

    // TODO: Load config from local file if available
    let mut config = Config::default();

    app.initialize(&mut Resources {
        config: &mut config,
        window: &mut window,
        world: &mut world,
        gui: &mut gui,
        renderer: &mut renderer,
        input: &mut input,
        system: &mut system,
    })?;

    event_loop.run(move |event, _, control_flow| {
        let state = Resources {
            config: &mut config,
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
    mut resources: Resources,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    // if app.gui_active() {
    resources.gui.handle_event(&event);
    // }
    // if !app.gui_active() || !resources.gui.captures_event(&event) {
    app.handle_events(&event, &mut resources)?;
    resources.system.handle_event(&event);
    resources
        .input
        .handle_event(&event, resources.system.window_center());
    // }

    match event {
        Event::NewEvents(_) => {
            if resources.system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::WindowEvent { ref event, .. } => match event {
            WindowEvent::Resized(physical_size) => resources.renderer.set_viewport(Viewport {
                x: 0.0,
                y: 0.0,
                width: physical_size.width as _,
                height: physical_size.height as _,
            }),
            WindowEvent::DroppedFile(ref path) => app.on_file_dropped(path, &mut resources)?,
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            WindowEvent::MouseInput { button, state, .. } => {
                app.on_mouse(button, state, &mut resources)?
            }
            WindowEvent::KeyboardInput { input, .. } => {
                app.on_key(*input, &mut resources)?;
            }
            _ => (),
        },
        Event::MainEventsCleared => {
            resources.world.tick(resources.system.delta_time as f32)?;

            let clipped_meshes = if app.gui_active() {
                let _frame_data = resources
                    .gui
                    .start_frame(resources.window.scale_factor() as _);

                app.update_gui(&mut resources)?;
                let shapes = resources.gui.end_frame(resources.window);
                resources.gui.context().tessellate(shapes)
            } else {
                Vec::new()
            };

            app.update(&mut resources)?;

            let context_ref = &resources.gui.context();
            let gui_context = if app.gui_active() {
                Some(context_ref)
            } else {
                None
            };
            resources.renderer.update(
                resources.world,
                gui_context,
                &clipped_meshes,
                resources.system.milliseconds_since_start(),
                resources.config,
            )?;
            resources.renderer.render(resources.world, clipped_meshes)?;
        }
        Event::LoopDestroyed => {
            app.cleanup()?;
        }
        _ => (),
    }

    Ok(())
}

pub fn initialize_resources(mut app: impl App + 'static, config: AppConfig) -> Result<()> {
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

    let mut input = Input::default();
    let mut system = System::new(window_dimensions);

    let screen_descriptor = ScreenDescriptor {
        dimensions: window_dimensions,
        scale_factor: window.scale_factor() as _,
    };
    let mut gui = Gui::new(screen_descriptor);

    let viewport = Viewport {
        x: 0.0,
        y: 0.0,
        width: window_dimensions.width as _,
        height: window_dimensions.height as _,
    };
    let mut renderer = create_render_backend(&config.backend, &window, viewport)?;

    let mut world = World::new()?;
    world.fonts.insert(
        "default".to_string(),
        SdfFont::new("assets/fonts/font.fnt", "assets/fonts/font_sdf_rgba.png")?,
    );

    let mut config = Config::default();

    app.initialize(&mut Resources {
        config: &mut config,
        window: &mut window,
        world: &mut world,
        gui: &mut gui,
        renderer: &mut renderer,
        input: &mut input,
        system: &mut system,
    })?;

    event_loop.run(move |event, _, control_flow| {
        let state = Resources {
            config: &mut config,
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
