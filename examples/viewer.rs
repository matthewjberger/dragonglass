use anyhow::Result;
use dragonglass::{
    app::{run_app, App, AppConfiguration, AppState, Collider, Input, OrbitalCamera, System},
    world::{load_gltf, BoundingBoxVisible, Entity, Mesh, Transform},
};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use na::Point3;
use nalgebra as na;
use nalgebra_glm as glm;
use ncollide3d::{
    pipeline::{CollisionGroups, GeometricQueryType},
    query::Ray,
    shape::{Cuboid, ShapeHandle},
};
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

    fn update_colliders(&mut self, state: &mut AppState) -> Result<()> {
        // TODO: sync collider position for meshes that have one already

        // Add colliders for all meshes that do not have one yet
        let collision_group = CollisionGroups::new();
        let query_type = GeometricQueryType::Contacts(0.0, 0.0);
        let mut entity_map = HashMap::new();
        for (entity, mesh) in state.world.ecs.query::<&Mesh>().iter() {
            match state.world.ecs.entity(entity) {
                Ok(entity) => {
                    if entity.get::<Collider>().is_some() {
                        continue;
                    }
                }
                Err(_) => continue,
            }

            let bounding_box = mesh.bounding_box();
            let translation = glm::translation(&bounding_box.center());
            let transform_matrix = state.world.entity_global_transform(entity)? * translation;
            let transform = Transform::from(transform_matrix);

            // Insert a collider
            let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
            let collider_shape = Cuboid::new(half_extents);

            let shape_handle = ShapeHandle::new(collider_shape);

            let (handle, _collision_object) = state.collision_world.add(
                transform.as_isometry(),
                shape_handle,
                collision_group,
                query_type,
                (),
            );

            entity_map.insert(entity, handle);
        }
        for (entity, handle) in entity_map.into_iter() {
            let _ = state.world.ecs.insert_one(entity, Collider { handle });
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

        let collision_group = CollisionGroups::new();
        let raycast_result =
            state
                .collision_world
                .first_interference_with_ray(&ray, f32::MAX, &collision_group);

        match raycast_result {
            Some(result) => {
                let handle = result.handle;
                let mut picked_entity = None;
                for (entity, collider) in state.world.ecs.query::<&Collider>().iter() {
                    if collider.handle == handle {
                        picked_entity = Some(entity);
                        break;
                    }
                }
                picked_entity
            }
            None => None,
        }
    }

    fn mouse_ray(&self, state: &mut AppState) -> Ray<f32> {
        let (width, height) = (
            state.system.window_dimensions[0] as f32,
            state.system.window_dimensions[1] as f32,
        );
        let aspect_ratio = state.system.aspect_ratio();
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);
        let mut position = state.input.mouse.position;
        position.y = height - position.y;
        let near_point = glm::vec2_to_vec3(&position);
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
        let world = &state.world;
        ui.text(im_str!("Number of entities: {}", world.ecs.iter().count()));
        let number_of_meshes = world.ecs.query::<&Mesh>().iter().count();
        ui.text(im_str!("Number of meshes: {}", number_of_meshes));
        ui.text(im_str!("Number of animations: {}", world.animations.len()));
        ui.text(im_str!("Number of textures: {}", world.textures.len()));
        ui.text(im_str!("Number of materials: {}", world.materials.len()));
        ui.text(im_str!(
            "Number of collision_objects: {}",
            state.collision_world.collision_objects().count()
        ));

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

        ui.text(im_str!("Meshes"));
        for (_entity, mesh) in state.world.ecs.query::<&Mesh>().iter() {
            ui.text(im_str!("{}", mesh.name));
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

        self.update_colliders(state)
            .expect("Failed to update colliders");

        self.highlight_hovered_object(state);
    }

    fn on_key(&mut self, state: &mut AppState, keystate: ElementState, keycode: VirtualKeyCode) {
        match (keycode, keystate) {
            (VirtualKeyCode::T, ElementState::Pressed) => state.renderer.toggle_wireframe(),
            (VirtualKeyCode::C, ElementState::Pressed) => {
                let colliders = state
                    .world
                    .ecs
                    .query::<&Collider>()
                    .iter()
                    .map(|(_entity, collider)| collider.handle)
                    .collect::<Vec<_>>();
                state.collision_world.remove(&colliders);

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
