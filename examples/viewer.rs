use anyhow::Result;
use dragonglass::{
    app::{run_app, App, AppConfiguration, AppState, Input, OrbitalCamera, System},
    physics::RigidBody,
    world::{load_gltf, BoundingBoxVisible, Mesh},
};
use dragonglass_world::Entity;
use imgui::{im_str, Ui};
use log::{error, info, warn};
use nalgebra as na;
use nalgebra_glm as glm;
use rapier3d::{
    dynamics::RigidBodyBuilder, geometry::ColliderBuilder, geometry::InteractionGroups,
    geometry::Ray, na::Point3,
};
use winit::event::{ElementState, Event, VirtualKeyCode, WindowEvent};

/// Decomposes a 4x4 augmented rotation matrix without shear into translation, rotation, and scaling components
/// rotation is given as euler angles in radians
/// Output is returned as (translation, rotation, scaling)
fn decompose_matrix(transform: glm::Mat4) -> (glm::Vec3, glm::Vec3, glm::Vec3) {
    let translation = glm::vec3(transform.m14, transform.m24, transform.m34);

    let rotation = glm::to_quat(&na::QR::new(transform).r()).as_vector().xyz();

    let scaling = transform.m44
        * glm::vec3(
            (transform.m11.powi(2) + transform.m21.powi(2) + transform.m31.powi(2)).sqrt(),
            (transform.m12.powi(2) + transform.m22.powi(2) + transform.m32.powi(2)).sqrt(),
            (transform.m13.powi(2) + transform.m23.powi(2) + transform.m33.powi(2)).sqrt(),
        );

    (translation, rotation, scaling)
}

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
    camera: OrbitalCamera,
    camera_multipliers: CameraMultipliers,
    show_bounding_boxes: bool,
}

impl Viewer {
    fn update_camera(&mut self, input: &Input, system: &System) {
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

    fn update_bodies(&mut self, state: &mut AppState) -> Result<()> {
        // Add rigid bodies with colliders for all meshes that do not have one yet
        let mut entity_map = std::collections::HashMap::new();
        for (entity, mesh) in state.world.ecs.query::<&Mesh>().iter() {
            if let Ok(entity) = state.world.ecs.entity(entity) {
                if entity.get::<RigidBody>().is_some() {
                    continue;
                }
            }

            // TODO: This has a bug, because models aren't displaying properly after the collider's initial translation and rotation are assigned

            let transform = state.world.entity_global_transform(entity)?;
            let (translation, rotation, scaling) = decompose_matrix(transform);

            // Insert a corresponding rigid body
            let rigid_body = RigidBodyBuilder::new_static()
                .translation(translation.x, translation.y, translation.z)
                .rotation(rotation)
                .build();
            let handle = state.physics_world.bodies.insert(rigid_body);

            // Insert a collider
            let bounding_box = mesh.bounding_box();
            let half_extents = bounding_box.half_extents().component_mul(&scaling);
            let collider =
                ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z).build();
            state
                .physics_world
                .colliders
                .insert(collider, handle, &mut state.physics_world.bodies);

            entity_map.insert(entity, handle);
        }
        for (entity, handle) in entity_map.into_iter() {
            let _ = state.world.ecs.insert_one(entity, RigidBody { handle });
        }
        Ok(())
    }

    fn highlight_hovered_object(&self, state: &mut AppState) {
        self.clear_bounding_boxes(state);
        if let Some(entity) = self.pick_object(state) {
            let _ = state.world.ecs.insert_one(entity, BoundingBoxVisible {});
        }
    }

    fn clear_bounding_boxes(&self, state: &mut AppState) {
        let entities = state
            .world
            .ecs
            .query::<&Mesh>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            let _ = state.world.ecs.remove_one::<BoundingBoxVisible>(entity);
        }
    }

    fn pick_object(&self, state: &mut AppState) -> Option<Entity> {
        let ray = self.mouse_ray(state);

        match state.physics_world.query.cast_ray(
            &state.physics_world.colliders,
            &ray,
            f32::MAX,
            InteractionGroups::all(),
        ) {
            Some((collider_handle, _collider, _intersection)) => {
                let handle = state.physics_world.colliders[collider_handle].parent();
                let mut picked_entity = None;
                for (entity, rigid_body) in state.world.ecs.query::<&RigidBody>().iter() {
                    if rigid_body.handle == handle {
                        picked_entity = Some(entity);
                        break;
                    }
                }
                picked_entity
            }
            None => None,
        }
    }

    fn mouse_ray(&self, state: &mut AppState) -> Ray {
        let (width, height) = (
            state.system.window_dimensions[0] as f32,
            state.system.window_dimensions[1] as f32,
        );
        let aspect_ratio = state.system.aspect_ratio();
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);
        let near_point = glm::vec2_to_vec3(&state.input.mouse.position);
        let mut far_point = near_point;
        far_point.z = 1.0;
        let p_near = glm::unproject_zo(
            &near_point,
            &state.world.view,
            &projection,
            glm::vec4(0.0, 0.0, width, height),
        );
        let p_far = glm::unproject_zo(
            &far_point,
            &state.world.view,
            &projection,
            glm::vec4(0.0, 0.0, width, height),
        );
        let direction = (p_far - p_near).normalize();
        Ray::new(Point3::from(p_near), direction)
    }
}

impl App for Viewer {
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

        self.highlight_hovered_object(state);
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
                                    self.update_bodies(state).unwrap();
                                    info!("Loaded gltf world: '{}'", raw_path);
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
