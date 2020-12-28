use anyhow::Result;
use camera::OrbitalCamera;
use dragonglass::{
    app::{run_application, AppConfig, Application, ApplicationRunner},
    world::{load_gltf, Collider, ColliderVisible, Entity, Mesh, Selected, Transform},
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
use std::{collections::HashMap, path::PathBuf};
use winit::event::{ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent};

mod camera;

#[derive(Default)]
pub struct Viewer {
    camera: OrbitalCamera,
    show_bounding_boxes: bool,
}

impl Viewer {
    fn load_gltf(path: &str, application: &mut Application) -> Result<()> {
        load_gltf(path.clone(), &mut application.world)?;

        // FIXME: Don't reload entire scene whenever something is added
        if let Err(error) = application.renderer.load_world(&application.world) {
            warn!("Failed to load gltf world: {}", error);
        }

        info!("Loaded gltf world: '{}'", path);

        Ok(())
    }

    fn load_hdr(path: &str, application: &mut Application) {
        if let Err(error) = application.renderer.load_skybox(path) {
            error!("Viewer error: {}", error);
        }
        info!("Loaded hdr cubemap: '{}'", path);
    }

    fn update_colliders(&mut self, application: &mut Application) -> Result<()> {
        // Add colliders for all meshes that do not have one yet
        let collision_group = CollisionGroups::new();
        let query_type = GeometricQueryType::Contacts(0.0, 0.0);
        let mut entity_map = HashMap::new();
        for (entity, mesh) in application.world.ecs.query::<&Mesh>().iter() {
            let bounding_box = mesh.bounding_box();
            let translation = glm::translation(&bounding_box.center());
            let transform_matrix = application.world.entity_global_transform(entity)? * translation;
            let transform = Transform::from(transform_matrix);
            let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
            let collider_shape = Cuboid::new(half_extents);
            let shape_handle = ShapeHandle::new(collider_shape);

            match application.world.ecs.entity(entity) {
                Ok(entity_ref) => match entity_ref.get::<Collider>() {
                    // collider exists already, sync it
                    Some(collider) => {
                        if let Some(collision_object) =
                            application.collision_world.get_mut(collider.handle)
                        {
                            collision_object.set_position(transform.as_isometry());
                            collision_object.set_shape(shape_handle);
                        }
                    }
                    None => {
                        let (handle, _collision_object) = application.collision_world.add(
                            transform.as_isometry(),
                            shape_handle,
                            collision_group,
                            query_type,
                            (),
                        );
                        entity_map.insert(entity, handle);
                    }
                },
                Err(_) => continue,
            }
        }
        for (entity, handle) in entity_map.into_iter() {
            let _ = application
                .world
                .ecs
                .insert_one(entity, Collider { handle });
        }
        Ok(())
    }

    fn clear_selections(&self, application: &mut Application) {
        let entities = application
            .world
            .ecs
            .query::<&Mesh>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            let _ = application.world.ecs.remove_one::<Selected>(entity);
        }
    }

    fn show_hovered_object_collider(&self, application: &mut Application) {
        self.hide_colliders(application);
        if let Some(entity) = self.pick_object(application) {
            let _ = application.world.ecs.insert_one(entity, ColliderVisible {});
        }
    }

    fn hide_colliders(&self, application: &mut Application) {
        let entities = application
            .world
            .ecs
            .query::<&Mesh>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            let _ = application.world.ecs.remove_one::<ColliderVisible>(entity);
        }
    }

    fn pick_object(&self, application: &mut Application) -> Option<Entity> {
        let ray = self.mouse_ray(application);

        let collision_group = CollisionGroups::new();
        let raycast_result = application.collision_world.first_interference_with_ray(
            &ray,
            f32::MAX,
            &collision_group,
        );

        match raycast_result {
            Some(result) => {
                let handle = result.handle;
                let mut picked_entity = None;
                for (entity, collider) in application.world.ecs.query::<&Collider>().iter() {
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

    fn mouse_ray(&self, application: &mut Application) -> Ray<f32> {
        let (width, height) = (
            application.system.window_dimensions[0] as f32,
            application.system.window_dimensions[1] as f32,
        );
        let aspect_ratio = application.system.aspect_ratio();
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);
        let mut position = application.input.mouse.position;
        position.y = height - position.y;
        let near_point = glm::vec2_to_vec3(&position);
        let mut far_point = near_point;
        far_point.z = 1.0;
        let p_near = glm::unproject_zo(
            &near_point,
            &application.world.view,
            &projection,
            glm::vec4(0.0, 0.0, width, height),
        );
        let p_far = glm::unproject_zo(
            &far_point,
            &application.world.view,
            &projection,
            glm::vec4(0.0, 0.0, width, height),
        );
        let direction = (p_far - p_near).normalize();
        Ray::new(Point3::from(p_near), direction)
    }

    fn clear_colliders(application: &mut Application) {
        let colliders = application
            .world
            .ecs
            .query::<&Collider>()
            .iter()
            .map(|(_entity, collider)| collider.handle)
            .collect::<Vec<_>>();
        application.collision_world.remove(&colliders);
    }
}

impl ApplicationRunner for Viewer {
    fn create_ui(&mut self, application: &mut Application, ui: &Ui) -> Result<()> {
        let world = &application.world;
        ui.text(im_str!("Number of entities: {}", world.ecs.iter().count()));
        let number_of_meshes = world.ecs.query::<&Mesh>().iter().count();
        ui.text(im_str!("Number of meshes: {}", number_of_meshes));
        ui.text(im_str!("Number of animations: {}", world.animations.len()));
        ui.text(im_str!("Number of textures: {}", world.textures.len()));
        ui.text(im_str!("Number of materials: {}", world.materials.len()));
        ui.text(im_str!(
            "Number of collision_objects: {}",
            application.collision_world.collision_objects().count()
        ));

        ui.separator();
        ui.text(im_str!("Multipliers"));
        let _ = ui
            .input_float(im_str!("Scroll"), &mut self.camera.scroll)
            .step(0.1)
            .step_fast(1.0)
            .build();
        let _ = ui
            .input_float(im_str!("Drag"), &mut self.camera.drag)
            .step(0.1)
            .step_fast(1.0)
            .build();
        let _ = ui
            .input_float(im_str!("Rotation"), &mut self.camera.rotation)
            .step(0.1)
            .step_fast(1.0)
            .build();
        ui.separator();

        ui.text(im_str!("Selected Entities"));
        for (entity, _) in application.world.ecs.query::<&Selected>().iter() {
            ui.text(im_str!("{:#?}", entity));
        }
        Ok(())
    }

    fn update(&mut self, application: &mut Application) -> Result<()> {
        if application.input.is_key_pressed(VirtualKeyCode::Escape) {
            application.system.exit_requested = true;
        }

        self.camera.update(&application.input, &application.system);
        if application.input.is_key_pressed(VirtualKeyCode::R) {
            self.camera = OrbitalCamera::default();
        }

        application.world.view = self.camera.view_matrix();
        application.world.camera_position = self.camera.position();

        if !application.world.animations.is_empty() {
            application
                .world
                .animate(0, 0.75 * application.system.delta_time as f32)?;
        }

        self.update_colliders(application)?;
        self.show_hovered_object_collider(application);

        Ok(())
    }

    fn on_key(
        &mut self,
        application: &mut Application,
        keystate: ElementState,
        keycode: VirtualKeyCode,
    ) -> Result<()> {
        match (keycode, keystate) {
            (VirtualKeyCode::T, ElementState::Pressed) => application.renderer.toggle_wireframe(),
            (VirtualKeyCode::C, ElementState::Pressed) => {
                Self::clear_colliders(application);
                application.world.clear();
                if let Err(error) = application.renderer.load_world(&application.world) {
                    warn!("Failed to load gltf world: {}", error);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_file_dropped(&mut self, application: &mut Application, path: &PathBuf) -> Result<()> {
        let raw_path = match path.to_str() {
            Some(raw_path) => raw_path,
            None => return Ok(()),
        };

        if let Some(extension) = path.extension() {
            match extension.to_str() {
                Some("glb") | Some("gltf") => Self::load_gltf(raw_path, application)?,
                Some("hdr") => Self::load_hdr(raw_path, application),
                _ => warn!(
                    "File extension {:#?} is not a valid '.glb', '.gltf', or 'hdr' extension",
                    extension
                ),
            }
        }

        Ok(())
    }

    fn handle_events(
        &mut self,
        application: &mut Application,
        event: winit::event::Event<()>,
    ) -> Result<()> {
        match event {
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        button,
                        state: button_state,
                        ..
                    },
                ..
            } => {
                if let (MouseButton::Left, ElementState::Pressed) = (button, button_state) {
                    if let Some(entity) = self.pick_object(application) {
                        let already_selected =
                            application.world.ecs.get::<Selected>(entity).is_ok();
                        let shift_active = application.input.is_key_pressed(VirtualKeyCode::LShift);
                        if !shift_active {
                            self.clear_selections(application);
                        }
                        if !already_selected {
                            let _ = application.world.ecs.insert_one(entity, Selected {});
                        } else if shift_active {
                            let _ = application.world.ecs.remove_one::<Selected>(entity);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    run_application(
        Viewer::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            ..Default::default()
        },
    )
}
