use crate::{
    camera::OrbitalCamera, input::Input, physics::PhysicsWorld, settings::Settings, system::System,
};
use anyhow::Result;
use dragonglass::{Backend, Renderer};
use dragonglass_world::{load_gltf, Mesh, Transform, World};
use image::ImageFormat;
use log::{error, info, warn};
use nalgebra_glm as glm;
use rapier3d::data::arena::Index;
use winit::{
    dpi::PhysicalSize,
    event::ElementState,
    event::KeyboardInput,
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

pub struct RigidBody(pub Index);

pub struct App {
    world: World,
    physics_world: PhysicsWorld,
    camera: OrbitalCamera,
    _settings: Settings,
    input: Input,
    system: System,
    _window: Window,
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

        let logical_size = window.inner_size();
        let window_dimensions = [logical_size.width, logical_size.height];
        let renderer = Box::new(Renderer::create_backend(
            &Backend::Vulkan,
            &window,
            &window_dimensions,
        )?);

        let app = Self {
            world: World::default(),
            physics_world: PhysicsWorld::new(),
            camera: OrbitalCamera::default(),
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            _window: window,
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
            mut physics_world,
            event_loop,
            ..
        } = self;

        input.allowed = true;

        log::info!("bodies: {}", physics_world.bodies.len());
        log::info!("colliders: {}", physics_world.colliders.len());
        physics_world.add_cubes();
        log::info!("bodies: {}", physics_world.bodies.len());
        log::info!("colliders: {}", physics_world.colliders.len());

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            match event {
                Event::MainEventsCleared => {
                    if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                        *control_flow = ControlFlow::Exit;
                    }

                    Self::update_camera(&mut camera, &input, &system);
                    physics_world.step();
                    Self::update_physics(&mut world, &mut physics_world);

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
                    if let VirtualKeyCode::T = keycode { renderer.toggle_wireframe(); }
                }
                _ => {}
            }
        });
    }

    fn update_physics(world: &mut World, physics_world: &mut PhysicsWorld) {
        for (_id, (transform,)) in world.ecs.query_mut::<(&mut Transform,)>() {
            transform.rotation = glm::quat_rotate_normalized_axis(
                &transform.rotation,
                1_f32.to_radians(),
                &glm::vec3(0.0, 0.0, 1.0),
            );
        }
    }

    fn update_camera(camera: &mut OrbitalCamera, input: &Input, system: &System) {
        if !input.allowed {
            return;
        }
        let scroll_multiplier = 1.0;
        let rotation_multiplier = 0.05;
        let drag_multiplier = 0.001;

        camera.forward(input.mouse.wheel_delta.y * scroll_multiplier);

        if input.is_key_pressed(VirtualKeyCode::R) {
            *camera = OrbitalCamera::default();
        }

        if input.mouse.is_left_clicked {
            let delta = input.mouse.position_delta;
            let rotation = delta * rotation_multiplier * system.delta_time as f32;
            camera.rotate(&rotation);
        } else if input.mouse.is_right_clicked {
            let delta = input.mouse.position_delta;
            let pan = delta * drag_multiplier;
            camera.pan(&pan);
        }
    }
}
