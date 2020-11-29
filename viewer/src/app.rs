use crate::{camera::OrbitalCamera, gui::Gui, input::Input, settings::Settings, system::System};
use anyhow::Result;
use dragonglass::{Backend, Renderer};
use dragonglass_world::{load_gltf, Mesh, World};
use image::ImageFormat;
use imgui::{im_str, Condition};
use log::{error, info, warn};
use winit::{
    dpi::PhysicalSize,
    event::ElementState,
    event::KeyboardInput,
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

pub struct App {
    gui: Gui,
    world: World,
    camera: OrbitalCamera,
    _settings: Settings,
    input: Input,
    system: System,
    window: Window,
    renderer: Box<dyn Renderer>,
    event_loop: EventLoop<()>,
}

impl App {
    pub const TITLE: &'static str = "Dragonglass Vulkan Renderer";

    fn load_icon(icon_bytes: &[u8], format: ImageFormat) -> Result<Icon> {
        let (rgba, width, height) = {
            let image = image::load_from_memory_with_format(icon_bytes, format)?.into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            (rgba, width, height)
        };
        let icon = Icon::from_rgba(rgba, width, height)?;
        Ok(icon)
    }

    pub fn new() -> Result<Self> {
        let settings = Settings::load_current_settings()?;

        let icon = Self::load_icon(
            include_bytes!("../../assets/icon/icon.png"),
            ImageFormat::Png,
        )?;

        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_window_icon(Some(icon))
            .with_title(Self::TITLE)
            .with_inner_size(PhysicalSize::new(settings.width, settings.height))
            .build(&event_loop)?;

        let mut gui = Gui::new(&window);

        let logical_size = window.inner_size();
        let window_dimensions = [logical_size.width, logical_size.height];
        let renderer = Box::new(Renderer::create_backend(
            &Backend::Vulkan,
            &window,
            &window_dimensions,
            gui.context_mut(),
        )?);

        let app = Self {
            gui,
            world: World::default(),
            camera: OrbitalCamera::default(),
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            window,
            renderer,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut camera,
            mut input,
            mut system,
            mut renderer,
            mut world,
            mut gui,
            window,
            event_loop,
            ..
        } = self;

        input.allowed = true;

        let mut camera_multipliers = CameraMultipliers {
            scroll: 1.0,
            rotation: 0.05,
            drag: 0.001,
        };

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            gui.handle_event(&event, &window);
            input.handle_event(&event, system.window_center());
            input.allowed = !gui.capturing_input();

            match event {
                Event::MainEventsCleared => {
                    if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                        *control_flow = ControlFlow::Exit;
                    }

                    let draw_data = gui
                        .render_frame(&window, |ui| {
                            imgui::Window::new(im_str!("Scene Information"))
                                .size([300.0, 400.0], Condition::FirstUseEver)
                                .build(&ui, || {
                                    let number_of_entities = world.ecs.iter().count();
                                    let number_of_meshes = world.ecs.query::<&Mesh>().iter().count();
                                    ui.text(im_str!("Number of entities: {}", number_of_entities));
                                    ui.text(im_str!("Number of meshes: {}", number_of_meshes));
                                    ui.text(im_str!("Number of animations: {}", world.animations.len()));
                                    ui.text(im_str!("Number of textures: {}", world.textures.len()));
                                    ui.text(im_str!("Number of materials: {}", world.materials.len()));

                                    ui.separator();
                                    ui.text(im_str!("Controls"));
                                    if ui.button(im_str!("Toggle Wireframe"), [200.0, 20.0]) {
                                        renderer.toggle_wireframe();
                                    }
                                    ui.text(im_str!("Multipliers"));
                                    let _ = ui.input_float(im_str!("Scroll"), &mut camera_multipliers.scroll)
                                        .step(0.1)
                                        .step_fast(1.0).build();
                                    let _ = ui.input_float(im_str!("Drag"), &mut camera_multipliers.drag)
                                        .step(0.1)
                                        .step_fast(1.0).build();
                                    let _ = ui.input_float(im_str!("Rotation"), &mut camera_multipliers.rotation)
                                        .step(0.1)
                                        .step_fast(1.0).build();
                                    ui.separator();
                                    for (entity, mesh) in world.ecs.query::<&Mesh>().iter() {
                                        ui.text(im_str!("Entity: {:?}, Mesh Name: {}", entity, mesh.name));
                                    }
                                });
                        })
                    .expect("Failed to render gui frame!");

                    Self::update_camera(&mut camera, &input, &system, &camera_multipliers);

                    if !world.animations.is_empty() {
                        if let Err(error) = world.animate(0, 0.75 * system.delta_time as f32) {
                            log::warn!("Failed to animate world: {}", error);
                        }
                    }

                    if let Err(error) = renderer.render(
                        &system.window_dimensions,
                        camera.view_matrix(),
                        camera.position(),
                        &world,
                        draw_data,
                    ) {
                        error!("{}", error);
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::DroppedFile(path),
                    ..
                } => {
                    if let Some(raw_path) = path.to_str() {
                        if let Some(extension) = path.extension() {
                            match extension.to_str() {
                                Some("glb") | Some("gltf") => {
                                    load_gltf(path.clone(), &mut world).unwrap();
                                    // FIXME: Don't reload entire scene whenever something is added
                                    if let Err(error) = renderer.load_world(&world) {
                                        warn!("Failed to load gltf world: {}", error);
                                    }
                                    camera = OrbitalCamera::default();
                                    info!("Loaded gltf world: '{}'", raw_path);
                                }
                                Some("hdr") => {
                                    if let Err(error) = renderer.load_skybox(raw_path) {
                                        error!("Viewer error: {}", error);
                                    }
                                    camera = OrbitalCamera::default();
                                    info!("Loaded hdr cubemap: '{}'", raw_path);
                                }
                                _ => warn!(
                                    "File extension {:#?} is not a valid '.glb', '.gltf', or 'hdr' extension",
                                    extension),
                            }
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(keycode),
                                ..
                            },
                            ..
                    },
                    ..
                } => {
                    match keycode {
                        VirtualKeyCode::T => renderer.toggle_wireframe(),
                        VirtualKeyCode::C => { 
                            world.clear();
                            if let Err(error) = renderer.load_world(&world) {
                                warn!("Failed to load gltf world: {}", error);
                            }
                        }
                        _ => {}
                    }
            }
                _ => {}
            }
        });
    }

    fn update_camera(
        camera: &mut OrbitalCamera,
        input: &Input,
        system: &System,
        multipliers: &CameraMultipliers,
    ) {
        if !input.allowed {
            return;
        }

        camera.forward(input.mouse.wheel_delta.y * multipliers.scroll);

        if input.is_key_pressed(VirtualKeyCode::R) {
            *camera = OrbitalCamera::default();
        }

        if input.mouse.is_left_clicked {
            let delta = input.mouse.position_delta;
            let rotation = delta * multipliers.rotation * system.delta_time as f32;
            camera.rotate(&rotation);
        } else if input.mouse.is_right_clicked {
            let delta = input.mouse.position_delta;
            let pan = delta * multipliers.drag;
            camera.pan(&pan);
        }
    }
}

struct CameraMultipliers {
    scroll: f32,
    rotation: f32,
    drag: f32,
}
