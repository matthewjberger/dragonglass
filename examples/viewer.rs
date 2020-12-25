use anyhow::Result;
use dragonglass::{
    app::{run_app, App, AppConfiguration, AppState, Input, OrbitalCamera, System},
    physics::{PhysicsComponent, PhysicsWorld},
    world::{load_gltf, BoundingBoxVisible, Mesh, Transform},
};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use rapier3d::{dynamics::RigidBodyBuilder, geometry::ColliderBuilder};
use std::collections::HashMap;
use winit::event::{ElementState, Event, VirtualKeyCode, WindowEvent};

pub struct CameraMultipliers {
    pub scroll: f32,
    pub rotation: f32,
    pub drag: f32,
}

impl Default for CameraMultipliers {
    fn default() -> Self {
        Self {
            scroll: 1.0,
            rotation: 0.05,
            drag: 0.001,
        }
    }
}

#[derive(Default)]
pub struct Viewer {
    physics_world: PhysicsWorld,
    camera: OrbitalCamera,
    camera_multipliers: CameraMultipliers,
    show_bounding_boxes: bool,
}

impl Viewer {
    pub fn update_camera(&mut self, input: &Input, system: &System) {
        if !input.allowed {
            return;
        }

        self.camera
            .forward(input.mouse.wheel_delta.y * self.camera_multipliers.scroll);

        if input.is_key_pressed(VirtualKeyCode::R) {
            self.camera = OrbitalCamera::default();
        }

        if input.mouse.is_left_clicked {
            let delta = input.mouse.position_delta;
            let rotation = delta * self.camera_multipliers.rotation * system.delta_time as f32;
            self.camera.rotate(&rotation);
        } else if input.mouse.is_right_clicked {
            let delta = input.mouse.position_delta;
            let pan = delta * self.camera_multipliers.drag;
            self.camera.pan(&pan);
        }
    }
}

impl App for Viewer {
    fn initialize(&mut self, state: &mut AppState) {
        let ground_size = 1000.1;
        let rigid_body = RigidBodyBuilder::new_static()
            .translation(0.0, -10.0, 0.0)
            .build();
        let handle = self.physics_world.bodies.insert(rigid_body);
        let collider = ColliderBuilder::cuboid(ground_size, 1.0, ground_size).build();
        self.physics_world
            .colliders
            .insert(collider, handle, &mut self.physics_world.bodies);

        load_gltf("assets/models/plane.gltf", &mut state.world).unwrap();
        // FIXME: Don't reload entire scene whenever something is added
        if let Err(error) = state.renderer.load_world(&state.world) {
            warn!("Failed to load gltf world: {}", error);
        }

        let mut entity_map = HashMap::new();
        for (entity, mesh) in state.world.ecs.query::<&Mesh>().iter() {
            // FIXME_PHYSICS: This transform needs to come from the scene graph when first setting it
            let transform = state.world.entity_global_transform(entity);

            // Insert a corresponding rigid body
            let rigid_body = RigidBodyBuilder::new_static()
                .translation(
                    transform.translation.x,
                    transform.translation.y,
                    transform.translation.z,
                )
                .rotation(transform.rotation.as_vector().xyz())
                .build();
            let handle = self.physics_world.bodies.insert(rigid_body);

            // Insert a collider
            let bounding_box = mesh.bounding_box();
            let half_extents = bounding_box.half_extents();
            let collider =
                ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z).build();

            self.physics_world
                .colliders
                .insert(collider, handle, &mut self.physics_world.bodies);
            entity_map.insert(entity, handle);
        }
        for (entity, handle) in entity_map.into_iter() {
            let _ = state
                .world
                .ecs
                .insert_one(entity, PhysicsComponent { handle });
        }
    }

    fn create_ui(&mut self, state: &mut AppState, ui: &Ui) {
        let number_of_entities = state.world.ecs.iter().count();
        let number_of_meshes = state.world.ecs.query::<&Mesh>().iter().count();
        ui.text(im_str!("Number of entities: {}", number_of_entities));
        ui.text(im_str!("Number of meshes: {}", number_of_meshes));
        ui.text(im_str!(
            "Number of animations: {}",
            state.world.animations.len()
        ));
        ui.text(im_str!(
            "Number of textures: {}",
            state.world.textures.len()
        ));
        ui.text(im_str!(
            "Number of materials: {}",
            state.world.materials.len()
        ));
        ui.separator();
        ui.text(im_str!("Controls"));

        if ui.button(im_str!("Toggle Wireframe"), [200.0, 20.0]) {
            state.renderer.toggle_wireframe();
        }

        ui.text(im_str!("Multipliers"));
        let _ = ui
            .input_float(im_str!("Scroll"), &mut self.camera_multipliers.scroll)
            .step(0.1)
            .step_fast(1.0)
            .build();
        let _ = ui
            .input_float(im_str!("Drag"), &mut self.camera_multipliers.drag)
            .step(0.1)
            .step_fast(1.0)
            .build();
        let _ = ui
            .input_float(im_str!("Rotation"), &mut self.camera_multipliers.rotation)
            .step(0.1)
            .step_fast(1.0)
            .build();
        ui.separator();

        for (_entity, mesh) in state.world.ecs.query::<&Mesh>().iter() {
            ui.text(im_str!("Mesh: {}", mesh.name));
        }
    }

    fn update(&mut self, state: &mut AppState) {
        self.update_camera(&state.input, &state.system);
        state.world.view = self.camera.view_matrix();
        state.world.camera_position = self.camera.position();

        if !state.world.animations.is_empty() {
            if let Err(error) = state
                .world
                .animate(0, 0.75 * state.system.delta_time as f32)
            {
                log::warn!("Failed to animate world: {}", error);
            }
        }

        self.physics_world.step();

        for (_entity, (_mesh, transform, physics)) in
            state
                .world
                .ecs
                .query_mut::<(&Mesh, &mut Transform, &PhysicsComponent)>()
        {
            if let Some(body) = self.physics_world.bodies.get(physics.handle) {
                let position = body.position();
                transform.translation = position.translation.vector;
                transform.rotation = *position.rotation.quaternion();
            }
        }
    }

    fn on_key(&mut self, state: &mut AppState, keystate: ElementState, keycode: VirtualKeyCode) {
        match (keycode, keystate) {
            (VirtualKeyCode::T, ElementState::Pressed) => state.renderer.toggle_wireframe(),
            (VirtualKeyCode::C, ElementState::Pressed) => {
                state.world.clear();
                if let Err(error) = state.renderer.load_world(&state.world) {
                    warn!("Failed to load gltf world: {}", error);
                }
            }
            (VirtualKeyCode::B, ElementState::Pressed) => {
                self.show_bounding_boxes = !self.show_bounding_boxes;
                let entities = state
                    .world
                    .ecs
                    .query::<&Mesh>()
                    .iter()
                    .map(|(entity, _)| entity)
                    .collect::<Vec<_>>();
                entities.into_iter().for_each(|entity| {
                    if self.show_bounding_boxes {
                        let _ = state.world.ecs.insert_one(entity, BoundingBoxVisible {});
                    } else {
                        let _ = state.world.ecs.remove_one::<BoundingBoxVisible>(entity);
                    }
                });
            }
            _ => {}
        }
    }

    fn handle_events(&mut self, state: &mut AppState, event: winit::event::Event<()>) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                if let Some(raw_path) = path.to_str() {
                    if let Some(extension) = path.extension() {
                        match extension.to_str() {
                                Some("glb") | Some("gltf") => {
                                    load_gltf(path.clone(), &mut state.world).unwrap();
                                    // FIXME: Don't reload entire scene whenever something is added
                                    if let Err(error) = state.renderer.load_world(&state.world) {
                                        warn!("Failed to load gltf world: {}", error);
                                    }
                                    info!("Loaded gltf world: '{}'", raw_path);

                                    let mut entity_map = HashMap::new();
                                    for (entity, (mesh, transform)) in state.world.ecs.query::<(&Mesh, &Transform)>().iter() {
                                        // Insert a corresponding rigid body
                                        let rigid_body = RigidBodyBuilder::new_dynamic()
                                            .translation(
                                                transform.translation.x,
                                                transform.translation.y,
                                                transform.translation.z,
                                            )
                                            .rotation(transform.rotation.as_vector().xyz())
                                            .build();
                                        let handle = self.physics_world.bodies.insert(rigid_body);
                                        // Insert a collider
                                        let bounding_box = mesh.bounding_box();
                                        let half_extents = bounding_box.half_extents();
                                        let collider =
                                            ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z).build();
                                        self.physics_world
                                            .colliders
                                            .insert(collider, handle, &mut self.physics_world.bodies);

                                        entity_map.insert(entity, handle);
                                    }
                                    for (entity, handle) in entity_map.into_iter() {
                                        let _ = state.world.ecs.insert_one(entity, PhysicsComponent { handle });
                                    }
                                }
                                Some("hdr") => {
                                    if let Err(error) = state.renderer.load_skybox(raw_path) {
                                        error!("Viewer error: {}", error);
                                    }
                                    info!("Loaded hdr cubemap: '{}'", raw_path);
                                }
                                _ => warn!(
                                    "File extension {:#?} is not a valid '.glb', '.gltf', or 'hdr' extension",
                                    extension),
                            }
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    run_app(
        Viewer::default(),
        AppConfiguration {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Gltf Viewer".to_string(),
            ..Default::default()
        },
    )
}
