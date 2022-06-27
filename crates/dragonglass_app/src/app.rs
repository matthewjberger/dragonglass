use crate::{logger::create_logger, Resources, State, StateMachine};
use anyhow::Result;
use dragonglass_gui::{Gui, ScreenDescriptor};
use dragonglass_input::{Input, System};
use dragonglass_render::{create_render_backend, Backend};
use dragonglass_world::Viewport;
use image::io::Reader;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, WindowBuilder},
};

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

pub fn run_application(initial_state: impl State + 'static, config: AppConfig) -> Result<()> {
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

    let mut state_machine = StateMachine::new(initial_state);

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

    event_loop.run(move |event, _, control_flow| {
        let resources = Resources {
            window: &mut window,
            gui: &mut gui,
            renderer: &mut renderer,
            input: &mut input,
            system: &mut system,
        };
        if let Err(error) = run_loop(&mut state_machine, resources, event, control_flow) {
            eprintln!("Application Error: {}", error);
        }
    });
}

fn run_loop(
    state_machine: &mut StateMachine,
    mut resources: Resources,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    if !state_machine.is_running() {
        state_machine.start(&mut resources)?;
    }

    resources.gui.handle_event(&event);
    if !resources.gui.captures_event(&event) {
        resources.system.handle_event(&event);
        resources
            .input
            .handle_event(&event, resources.system.window_center());
        state_machine.handle_event(&mut resources, &event)?;
    }

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
            WindowEvent::DroppedFile(ref path) => {
                state_machine.on_file_dropped(&mut resources, path)?
            }
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            WindowEvent::MouseInput { button, state, .. } => {
                state_machine.on_mouse(&mut resources, button, state)?
            }
            WindowEvent::KeyboardInput { input, .. } => {
                if let (Some(VirtualKeyCode::Escape), ElementState::Pressed) =
                    (input.virtual_keycode, input.state)
                {
                    *control_flow = ControlFlow::Exit;
                }
                state_machine.on_key(&mut resources, *input)?;
            }
            _ => (),
        },
        Event::MainEventsCleared => {
            state_machine.update(&mut resources)?;

            let clipped_meshes = {
                let _frame_data = resources
                    .gui
                    .start_frame(resources.window.scale_factor() as _);
                state_machine.update_gui(&mut resources)?;
                let shapes = resources.gui.end_frame(resources.window);
                resources.gui.context().tessellate(shapes)
            };

            let context_ref = &resources.gui.context();
            if let Some(world) = state_machine.world()? {
                resources
                    .renderer
                    .update(world, Some(context_ref), &clipped_meshes)?;
                resources.renderer.render(world, clipped_meshes)?;
            }
        }
        Event::LoopDestroyed => {
            state_machine.stop(&mut resources)?;
        }
        _ => (),
    }

    Ok(())
}
