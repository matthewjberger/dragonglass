use anyhow::Result;
use camera::OrbitalCamera;
use dragonglass::{
    app::{run_application, AppConfig, Application, ApplicationRunner},
    physics::RigidBody,
    world::{
        load_gltf, BoxCollider, BoxColliderVisible, Mesh, Selected, Transform,
        UseLocalTransformOnly,
    },
};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use nalgebra_glm as glm;
use rapier3d::{dynamics::BodyStatus, dynamics::RigidBodyBuilder, geometry::ColliderBuilder};
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

mod camera;

#[derive(Default)]
pub struct Viewer {
    camera: OrbitalCamera,
}

impl Viewer {
    fn load_gltf(path: &str, application: &mut Application) -> Result<()> {
        load_gltf(path, &mut application.world)?;

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

    fn show_hovered_object_collider(&self, application: &mut Application) {
        application.world.remove_all::<BoxColliderVisible>();
        if let Some(entity) = application.pick_object(f32::MAX) {
            let _ = application
                .world
                .ecs
                .insert_one(entity, BoxColliderVisible {});
        }
    }

    fn clear_colliders(application: &mut Application) {
        let colliders = application
            .world
            .ecs
            .query::<&BoxCollider>()
            .iter()
            .map(|(_entity, collider)| collider.handle)
            .collect::<Vec<_>>();
        application.collision_world.remove(&colliders);
    }

    fn update_bodies(&mut self, application: &mut Application) -> Result<()> {
        // Add/sync rigid bodies with colliders for all meshes that do not have one yet
        let mut entity_map = std::collections::HashMap::new();
        for (entity, mesh) in application.world.ecs.query::<&Mesh>().iter() {
            match application.world.ecs.entity(entity) {
                Ok(entity) => {
                    if entity.get::<RigidBody>().is_some() {
                        continue;
                    }
                }
                Err(_) => continue,
            }

            let bounding_box = mesh.bounding_box();
            let translation = glm::translation(&bounding_box.center());
            let transform_matrix = application.world.entity_global_transform(entity)? * translation;
            let transform = Transform::from(transform_matrix);

            // Insert a corresponding rigid body
            let translation = transform.translation;
            let rigid_body = RigidBodyBuilder::new(BodyStatus::Dynamic)
                .translation(translation.x, translation.y, translation.z)
                .rotation(transform.rotation.as_vector().xyz())
                .build();
            let handle = application.physics_world.bodies.insert(rigid_body);

            // Insert a collider
            let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
            let collider =
                ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z).build();
            application.physics_world.colliders.insert(
                collider,
                handle,
                &mut application.physics_world.bodies,
            );

            entity_map.insert(entity, handle);
        }
        for (entity, handle) in entity_map.into_iter() {
            let _ = application
                .world
                .ecs
                .insert(entity, (RigidBody { handle }, UseLocalTransformOnly {}));
        }

        // Sync transforms
        for (_entity, (rigid_body, transform)) in application
            .world
            .ecs
            .query_mut::<(&RigidBody, &mut Transform)>()
        {
            if let Some(body) = application.physics_world.bodies.get(rigid_body.handle) {
                let position = body.position();
                transform.translation = position.translation.vector;
                transform.rotation = *position.rotation.quaternion();
            }
        }

        Ok(())
    }
}

impl ApplicationRunner for Viewer {
    fn initialize(&mut self, application: &mut Application) -> Result<()> {
        // Add an invisible ground plane
        let rigid_body = RigidBodyBuilder::new_static()
            .translation(0.0, -10.0, 0.0)
            .rotation(glm::vec3(0.0, 0.0, 0.0))
            .build();
        let handle = application.physics_world.bodies.insert(rigid_body);
        let collider = ColliderBuilder::cuboid(5.0, 2.0, 5.0).build();
        application.physics_world.colliders.insert(
            collider,
            handle,
            &mut application.physics_world.bodies,
        );
        Ok(())
    }

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

        self.show_hovered_object_collider(application);

        self.update_bodies(application)?;

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

    fn on_mouse(
        &mut self,
        application: &mut Application,
        button: MouseButton,
        state: ElementState,
    ) -> Result<()> {
        if let (MouseButton::Left, ElementState::Pressed) = (button, state) {
            let entity = match application.pick_object(f32::MAX) {
                Some(entity) => entity,
                None => return Ok(()),
            };

            let already_selected = application.world.ecs.get::<Selected>(entity).is_ok();
            let shift_active = application.input.is_key_pressed(VirtualKeyCode::LShift);
            if !shift_active {
                application.world.remove_all::<Selected>();
            }
            if !already_selected {
                let _ = application.world.ecs.insert_one(entity, Selected {});
            } else if shift_active {
                let _ = application.world.ecs.remove_one::<Selected>(entity);
            }
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
