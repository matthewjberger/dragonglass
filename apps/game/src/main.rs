use anyhow::Result;
use dragonglass::{
    app::{run_application, AppConfig, Application, ApplicationRunner},
    physics::RigidBody,
    world::{
        Camera, Entity, Hidden, Light, LightKind, Mesh, PerspectiveCamera, Projection, Transform,
    },
};
use imgui::{im_str, Condition, Ui, Window};
use nalgebra::{Isometry3, UnitQuaternion};
use nalgebra_glm as glm;
use rapier3d::{
    dynamics::{BodyStatus, RigidBodyBuilder},
    geometry::{ColliderBuilder, InteractionGroups},
    math::Translation,
};
use winit::event::{ElementState, VirtualKeyCode};

const PLAYER_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b10, 0b01);
const LEVEL_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b01, 0b10);

#[derive(Default)]
pub struct Game {
    player: Option<Entity>,
}

impl ApplicationRunner for Game {
    fn initialize(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        let (player_path, player_handle) = ("assets/models/player.glb", "Player");
        let level_path = "assets/models/segmented_room.glb";

        {
            let position = glm::vec3(-2.0, 5.0, 0.0);
            let mut transform = Transform {
                translation: position,
                ..Default::default()
            };
            transform.look_at(&(-position), &glm::Vec3::y());
            let light_entity = application.ecs.spawn((
                transform,
                Light {
                    color: glm::vec3(1.0, 1.0, 1.0),
                    kind: LightKind::Directional,
                    ..Default::default()
                },
            ));
            application
                .world
                .scene
                .default_scenegraph_mut()?
                .add_node(light_entity);
        }

        application.load_asset(player_path)?;
        application.load_asset(level_path)?;
        application.reload_world()?;

        let level_mesh_names = vec![
            "Cube", "Cube.001", "Cube.002", "Cube.003", "Cube.004", "Cube.005",
        ];
        let mut level_meshes = Vec::new();
        for (entity, mesh) in application.ecs.query::<&Mesh>().iter() {
            if mesh.name == player_handle {
                self.player = Some(entity);
                {
                    {
                        let mut transform = application.ecs.get_mut::<Transform>(entity)?;
                        transform.translation.y = 1.0;
                        transform.scale = glm::vec3(0.5, 0.5, 0.5);
                    }
                }
            }
            if level_mesh_names.iter().any(|x| **x == mesh.name) {
                level_meshes.push(entity);
            }
            log::info!("Mesh available: {}", mesh.name);
        }

        for entity in level_meshes.into_iter() {
            add_rigid_body(application, entity, BodyStatus::Static, 0.0, false)?;
            add_box_collider(application, entity, LEVEL_COLLISION_GROUP)?;
        }

        if let Some(entity) = self.player.as_ref() {
            activate_first_person(application, *entity)?;
            add_rigid_body(application, *entity, BodyStatus::Dynamic, 0.01, true)?;
            add_box_collider(application, *entity, PLAYER_COLLISION_GROUP)?;
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
        if let Some(player) = self.player.as_ref() {
            update_player(application, *player)?;
        }
        Ok(())
    }

    fn on_key(
        &mut self,
        application: &mut Application,
        keystate: ElementState,
        keycode: VirtualKeyCode,
    ) -> Result<()> {
        if let (VirtualKeyCode::Space, ElementState::Pressed) = (keycode, keystate) {
            if let Some(player) = self.player.as_ref() {
                jump_player(application, *player)?;
            }
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

// TODO: This has too many parameters
fn add_rigid_body(
    application: &mut Application,
    entity: Entity,
    body_status: BodyStatus,
    mass: f32,
    lock_rotations: bool,
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
        let rigid_body = {
            let angles = glm::quat_euler_angles(&transform.rotation);
            let mut builder = RigidBodyBuilder::new(body_status)
                .mass(mass, false)
                .translation(
                    transform.translation.x,
                    transform.translation.y,
                    transform.translation.z,
                )
                .rotation(glm::vec3(angles.z, angles.x, angles.y));

            if lock_rotations {
                builder = builder.lock_rotations();
            }

            builder.build()
        };
        application.physics_world.bodies.insert(rigid_body)
    };
    application.ecs.insert_one(entity, RigidBody::new(handle))?;
    Ok(())
}

fn add_box_collider(
    application: &mut Application,
    entity: Entity,
    collision_groups: InteractionGroups,
) -> Result<()> {
    let bounding_box = {
        let mesh = application.ecs.get::<Mesh>(entity)?;
        mesh.bounding_box()
    };
    let transform = application.ecs.get::<Transform>(entity)?;
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
    let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
        .friction(0.1)
        .collision_groups(collision_groups)
        .build();
    application.physics_world.colliders.insert(
        collider,
        rigid_body_handle,
        &mut application.physics_world.bodies,
    );
    Ok(())
}

fn update_player(application: &mut Application, entity: Entity) -> Result<()> {
    let speed = 6.0 * application.system.delta_time as f32;
    {
        let mut translation = glm::vec3(0.0, 0.0, 0.0);

        let mut transform = application.ecs.get_mut::<Transform>(entity)?;
        let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
        if let Some(body) = application.physics_world.bodies.get(rigid_body_handle) {
            let position = body.position();
            transform.translation = position.translation.vector;
            transform.rotation = *position.rotation.quaternion();
        }

        if application.input.is_key_pressed(VirtualKeyCode::W) {
            translation = -speed * glm::Vec3::z();
        }

        if application.input.is_key_pressed(VirtualKeyCode::A) {
            translation = -speed * glm::Vec3::x();
        }

        if application.input.is_key_pressed(VirtualKeyCode::S) {
            translation = speed * glm::Vec3::z();
        }

        if application.input.is_key_pressed(VirtualKeyCode::D) {
            translation = speed * glm::Vec3::x();
        }

        if let Some(rigid_body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
            // rigid_body.apply_force(translation, true);
            let isometry = transform.as_isometry();
            rigid_body.set_position(
                Isometry3::from_parts(
                    Translation::from(transform.translation + translation),
                    UnitQuaternion::from(isometry.rotation),
                ),
                true,
            );
        }
    }
    Ok(())
}

fn jump_player(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    if let Some(rigid_body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
        let jump_strength = 0.1;
        let impulse = jump_strength * glm::Vec3::y();
        rigid_body.apply_impulse(impulse, true);
    }
    sync_transform_to_rigid_body(application, entity)?;
    Ok(())
}

fn sync_transform_to_rigid_body(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let mut transform = application.ecs.get_mut::<Transform>(entity)?;
    if let Some(body) = application.physics_world.bodies.get(rigid_body_handle) {
        let position = body.position();
        transform.translation = position.translation.vector;
    }
    if let Some(body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
        body.wake_up(true);
    }
    Ok(())
}

fn activate_first_person(application: &mut Application, entity: Entity) -> Result<()> {
    // Disable active camera
    let camera_entity = application.world.active_camera(&mut application.ecs)?;
    application.ecs.get_mut::<Camera>(camera_entity)?.enabled = false;

    application.ecs.insert_one(entity, Hidden {})?;
    application.ecs.insert_one(
        entity,
        Camera {
            name: "Player Camera".to_string(),
            projection: Projection::Perspective(PerspectiveCamera {
                aspect_ratio: None,
                y_fov_rad: 90_f32.to_radians(),
                z_far: Some(1000.0),
                z_near: 0.001,
            }),
            enabled: true,
        },
    )?;

    Ok(())
}
