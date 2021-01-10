use anyhow::Result;
use dragonglass::{
    app::Application,
    app::{run_application, AppConfig, ApplicationRunner},
    physics::RigidBody,
    world::{Camera, Entity, Hidden, Mesh, PerspectiveCamera, Projection, Transform},
};
use imgui::{im_str, Condition, Ui, Window};
use nalgebra_glm as glm;
use rapier3d::{dynamics::BodyStatus, dynamics::RigidBodyBuilder, geometry::ColliderBuilder};
use winit::event::{ElementState, VirtualKeyCode};

#[derive(Default)]
pub struct Game {
    helmet: Option<Entity>,
    plane: Option<Entity>,
    deer: Option<Entity>,
}

impl ApplicationRunner for Game {
    fn initialize(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        application.load_asset("assets/models/plane.gltf")?;
        application.load_asset("assets/models/DamagedHelmet.glb")?;
        application.load_asset("assets/models/deer.gltf")?;
        application.reload_world()?;
        for (entity, mesh) in application.ecs.query::<&Mesh>().iter() {
            if mesh.name == "mesh_helmet_LP_13930damagedHelmet" {
                self.helmet = Some(entity);
                {
                    {
                        let mut transform = application.ecs.get_mut::<Transform>(entity)?;
                        transform.translation.y = 200.0;
                    }
                }
            }
            if mesh.name == "Cylinder" {
                // The deer was probably modeled from a cylinder
                self.deer = Some(entity);
                {
                    let mut transform = application.ecs.get_mut::<Transform>(entity)?;
                    transform.translation.y = 100.0;
                }
            }
            if mesh.name == "Plane" {
                self.plane = Some(entity);
                {
                    let mut transform = application.ecs.get_mut::<Transform>(entity)?;
                    transform.translation.y = -4.0;
                }
            }
        }

        // Disable active camera
        // let camera_entity = application.world.active_camera(&mut application.ecs)?;
        // application.ecs.get_mut::<Camera>(camera_entity)?.enabled = false;

        if let Some(entity) = self.helmet.as_ref() {
            add_rigid_body(*entity, application, BodyStatus::Dynamic)?;
        }
        if let Some(entity) = self.deer.as_ref() {
            // application.ecs.insert_one(*entity, Hidden {})?;
            // application.ecs.insert_one(
            //     *entity,
            //     Camera {
            //         name: "Player Camera".to_string(),
            //         projection: Projection::Perspective(PerspectiveCamera {
            //             aspect_ratio: None,
            //             y_fov_rad: 70_f32.to_radians(),
            //             z_far: Some(1000.0),
            //             z_near: 0.1,
            //         }),
            //         enabled: true,
            //     },
            // )?;
            add_rigid_body(*entity, application, BodyStatus::Dynamic)?;
        }
        if let Some(entity) = self.plane.as_ref() {
            add_rigid_body(*entity, application, BodyStatus::Static)?;
        }

        Ok(())
    }

    fn create_ui(&mut self, _application: &mut Application, ui: &Ui) -> Result<()> {
        Window::new(im_str!("Physics Test"))
            .size([100.0, 40.0], Condition::FirstUseEver)
            .no_decoration()
            .build(ui, || {
                ui.text(im_str!("Physics test"));
            });
        Ok(())
    }

    fn update(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        // Sync the render transforms with the physics rigid bodies
        for (_entity, (rigid_body, transform)) in
            application.ecs.query_mut::<(&RigidBody, &mut Transform)>()
        {
            if let Some(body) = application.physics_world.bodies.get(rigid_body.handle) {
                let position = body.position();
                transform.translation = position.translation.vector;
                transform.rotation = *position.rotation.quaternion();
            }
        }

        let speed = 6.0 * application.system.delta_time as f32;
        if let Some(entity) = self.deer.as_ref() {
            {
                let mut transform = application.ecs.get_mut::<Transform>(*entity)?;
                let mut translation = glm::vec3(0.0, 0.0, 0.0);

                if application.input.is_key_pressed(VirtualKeyCode::W) {
                    translation = speed * transform.forward();
                }

                if application.input.is_key_pressed(VirtualKeyCode::A) {
                    translation = -speed * transform.right();
                }

                if application.input.is_key_pressed(VirtualKeyCode::S) {
                    translation = -speed * transform.forward();
                }

                if application.input.is_key_pressed(VirtualKeyCode::D) {
                    translation = speed * transform.right();
                }

                transform.translation += translation;
            }
            sync_rigid_body_to_transform(application, *entity)?;
        }

        Ok(())
    }

    fn on_key(
        &mut self,
        application: &mut Application,
        keystate: ElementState,
        keycode: VirtualKeyCode,
    ) -> Result<()> {
        if let Some(entity) = self.deer.as_ref() {
            if let (VirtualKeyCode::Space, ElementState::Pressed) = (keycode, keystate) {
                if let Some(entity) = self.deer.as_ref() {
                    let rigid_body_handle = application.ecs.get::<RigidBody>(*entity)?.handle;
                    if let Some(rigid_body) =
                        application.physics_world.bodies.get_mut(rigid_body_handle)
                    {
                        let jump_strength = 40.0;
                        let impulse = jump_strength * glm::Vec3::y();
                        rigid_body.apply_impulse(impulse, true);
                    }
                }
            }
            sync_transform_to_rigid_body(application, *entity)?;
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    run_application(
        Game::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Physics Test with Rapier3D".to_string(),
            ..Default::default()
        },
    )
}

/// Adds a rigid body with a box collider to an entity
fn add_rigid_body(
    entity: Entity,
    application: &mut Application,
    body_status: BodyStatus,
) -> Result<()> {
    let handle = {
        let bounding_box = {
            let mesh = application.ecs.get::<Mesh>(entity)?;
            mesh.bounding_box()
        };
        let translation = glm::translation(&bounding_box.center());
        let transform_matrix = application
            .world
            .entity_global_transform_matrix(&mut application.ecs, entity)?
            * translation;
        let transform = Transform::from(transform_matrix);

        // Insert a corresponding rigid body
        let rigid_body = RigidBodyBuilder::new(body_status)
            .translation(
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
            )
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
        handle
    };
    application.ecs.insert_one(entity, RigidBody::new(handle))?;
    Ok(())
}

fn sync_rigid_body_to_transform(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let transform = application.ecs.get::<Transform>(entity)?;
    if let Some(body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
        body.set_position(transform.as_isometry(), false);
    }
    Ok(())
}

fn sync_transform_to_rigid_body(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let mut transform = application.ecs.get_mut::<Transform>(entity)?;
    if let Some(body) = application.physics_world.bodies.get(rigid_body_handle) {
        let position = body.position();
        transform.translation = position.translation.vector;
        transform.rotation = *position.rotation.quaternion();
    }
    if let Some(body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
        body.wake_up(false);
    }
    Ok(())
}
