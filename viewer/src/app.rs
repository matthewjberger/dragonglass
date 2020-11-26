use crate::{camera::OrbitalCamera, input::Input, settings::Settings, system::System};
use anyhow::Result;
use dragonglass::RenderingDevice;
use dragonglass_scene::{
    load_gltf_asset, Asset, Geometry, Material, Mesh, Node, Primitive, Scene, SceneGraph,
    Transform, Vertex,
};
use image::ImageFormat;
use log::{error, info, warn};
use nalgebra_glm as glm;
use winit::{
    dpi::PhysicalSize,
    event::ElementState,
    event::KeyboardInput,
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

pub struct App {
    asset: Option<Asset>,
    camera: OrbitalCamera,
    _settings: Settings,
    input: Input,
    system: System,
    _window: Window,
    rendering_device: RenderingDevice,
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
        let mut rendering_device = RenderingDevice::new(&window, &window_dimensions)?;

        let asset = cube_asset()?;

        rendering_device.load_asset(&asset)?;

        let app = Self {
            asset: Some(asset),
            camera: OrbitalCamera::default(),
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            _window: window,
            rendering_device,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut camera,
            mut input,
            mut system,
            mut rendering_device,
            mut asset,
            event_loop,
            ..
        } = self;

        input.allowed = true;

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

                    if let Some(gltf_asset) = asset.as_mut() {
                        if !gltf_asset.animations.is_empty() {
                            gltf_asset.animate(0, 0.75 * system.delta_time as f32);
                        }
                    }

                    if let Err(error) = rendering_device.render(
                        &system.window_dimensions,
                        camera.view_matrix(),
                        camera.position(),
                        &asset,
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
                                    let gltf_asset = load_gltf_asset(path.clone()).unwrap();
                                    if let Err(error) = rendering_device.load_asset(&gltf_asset) {
                                        warn!("Failed to load gltf asset: {}", error);
                                    }
                                    camera = OrbitalCamera::default();
                                    asset = Some(gltf_asset);
                                    info!("Loaded gltf asset: '{}'", raw_path);
                                }
                                Some("dga") => {
                                    let deserialized_asset = deserialize_from_file(raw_path).expect("Failed to deserialize asset!");
                                    if let Err(error) = rendering_device.load_asset(&deserialized_asset) {
                                        warn!("Failed to load dragonglass asset: {}", error);
                                    }
                                    asset = Some(deserialized_asset);
                                    info!("Loaded dragonglass asset: '{}'", raw_path);
                                }
                                Some("hdr") => {
                                    if let Err(error) = rendering_device.load_skybox(raw_path) {
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
                        VirtualKeyCode::T => rendering_device.toggle_wireframe(),
                        VirtualKeyCode::S => {
                            if let Some(asset) = asset.as_ref() {
                                serialize_to_file("asset.dga", &asset).expect("Failed to serialize asset!");
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        });
    }

    fn update_camera(camera: &mut OrbitalCamera, input: &Input, system: &System) {
        if !input.allowed {
            return;
        }
        let scroll_multiplier = 0.01;
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

fn serialize_to_file(path: impl AsRef<std::path::Path>, asset: &Asset) -> Result<()> {
    let serialized_bytes: Vec<u8> = bincode::serialize(&asset)?;
    std::fs::write(path, &serialized_bytes)?;
    Ok(())
}

fn deserialize_from_file(path: impl AsRef<std::path::Path>) -> Result<Asset> {
    let asset_bytes = std::fs::read(path)?;
    let asset: Asset = bincode::deserialize(&asset_bytes)?;
    Ok(asset)
}

fn cube_asset() -> Result<Asset> {
    let positions = vec![
        glm::vec3(-0.5, -0.5, 0.5),
        glm::vec3(0.5, -0.5, 0.5),
        glm::vec3(0.5, 0.5, 0.5),
        glm::vec3(-0.5, 0.5, 0.5),
        glm::vec3(-0.5, -0.5, -0.5),
        glm::vec3(0.5, -0.5, -0.5),
        glm::vec3(0.5, 0.5, -0.5),
        glm::vec3(-0.5, 0.5, -0.5),
    ];
    let vertices = positions
        .into_iter()
        .map(|position| Vertex {
            position,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let indices = vec![
        0, 1, 2, 2, 3, 0, 1, 5, 6, 6, 2, 1, 7, 6, 5, 5, 4, 7, 4, 0, 3, 3, 7, 4, 4, 5, 1, 1, 0, 4,
        3, 2, 6, 6, 7, 3,
    ];
    let geometry = Geometry { vertices, indices };

    let mut scenegraph = SceneGraph::default();
    let parent = scenegraph.add_node(0);
    let child = scenegraph.add_node(1);
    scenegraph.add_edge(parent, child);

    let mut scenegraph_2 = SceneGraph::default();
    scenegraph_2.add_node(2);

    let cube_mesh = Mesh {
        primitives: vec![Primitive {
            number_of_vertices: geometry.vertices.len(),
            number_of_indices: geometry.indices.len(),
            material_index: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    };

    let materials = vec![Material {
        base_color_factor: glm::vec4(1.0, 0.0, 1.0, 1.0),
        is_unlit: true,
        ..Default::default()
    }];

    let asset = Asset {
        scenes: vec![Scene {
            graphs: vec![scenegraph, scenegraph_2],
            ..Default::default()
        }],
        nodes: vec![
            Node {
                mesh: Some(cube_mesh.clone()),
                transform: Transform {
                    rotation: glm::quat_angle_axis(45_f32.to_radians(), &glm::vec3(0.0, 0.0, 1.0)),
                    ..Default::default()
                },
                ..Default::default()
            },
            Node {
                mesh: Some(cube_mesh.clone()),
                transform: Transform {
                    translation: glm::vec3(3.0, 0.0, 1.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            Node {
                mesh: Some(cube_mesh.clone()),
                transform: Transform {
                    translation: glm::vec3(-3.0, 0.0, 0.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        ],
        materials,
        geometry,
        ..Default::default()
    };
    Ok(asset)
}
