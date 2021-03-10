use anyhow::Result;
use dragonglass::{
    app::Application,
    app::{run_application, AppConfig, ApplicationRunner},
    physics::RigidBody,
    world::{
        Camera, Entity, Hidden, Light, LightKind, Mesh, PerspectiveCamera, Projection, Transform,
    },
};
use imgui::{im_str, Condition, Ui, Window};
use nalgebra::{Point3, UnitQuaternion};
use nalgebra_glm as glm;
use rapier3d::{
    dynamics::BodyStatus,
    dynamics::RigidBodyBuilder,
    geometry::{ColliderBuilder, InteractionGroups},
};
use winit::event::{ElementState, VirtualKeyCode};

// TODO: Create trigger with event on collision
// TODO: Add trimesh component with handle?
// TODO: Visualize triangle mesh colliders as wireframes in renderer?

const PLAYER_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b10, 0b01);
const LEVEL_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b01, 0b10);

#[derive(Default)]
pub struct Game {
    player: Option<Entity>,
    yaw: f32,
    pitch: f32,
}

impl Game {
    fn update_player(&mut self, application: &mut Application, entity: Entity) -> Result<()> {
        let mouse_delta = application.input.mouse.position_delta;
        let speed = 6.0 * application.system.delta_time as f32;
        {
            let mut transform = application.ecs.get_mut::<Transform>(entity)?;
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

            let mouse_speed = 10.0 * application.system.delta_time as f32;
            self.yaw -= mouse_delta.x * mouse_speed;
            if self.yaw > 360.0 {
                self.yaw = 0.0;
            } else if self.yaw < 0.0 {
                self.yaw = 360.0;
            }

            self.pitch -= mouse_delta.y * mouse_speed;
            if self.pitch > 89.0 {
                self.pitch = 89.0;
            } else if self.pitch < -89.0 {
                self.pitch = -89.0;
            }

            transform.rotation = glm::to_quat(&glm::rotate(
                &glm::Mat4::identity(),
                self.pitch.to_radians(),
                &glm::Vec3::y(),
            ));
        }

        sync_rigid_body_to_transform(application, entity)?;
        Ok(())
    }
}

impl ApplicationRunner for Game {
    fn initialize(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        // Load light 1
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
                    color: glm::vec3(0.0, 1.0, 1.0),
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

        // Load light 2
        {
            let position = glm::vec3(2.0, 5.0, 0.0);
            let mut transform = Transform {
                translation: position,
                ..Default::default()
            };
            transform.look_at(&(-position), &glm::Vec3::y());
            let light_entity = application.ecs.spawn((
                transform,
                Light {
                    color: glm::vec3(1.0, 0.0, 0.0),
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

        // Load player
        let position = glm::vec3(0.0, 40.0, 0.0);
        let transform = Transform {
            translation: position,
            ..Default::default()
        };

        {
            let player_entity = application.ecs.spawn((transform,));
            application
                .world
                .scene
                .default_scenegraph_mut()?
                .add_node(player_entity);
            self.player = Some(player_entity);
        }

        // Load the level
        application.load_asset("assets/models/blocklevel.glb")?;

        application.reload_world()?;

        // Add static box colliders to level meshes
        let level_mesh_names = vec![
            "Cube.006",
            "Cube.002",
            "Sphere",
            "Cube.003",
            "Cube.004",
            "Cube.005",
            "Torus",
            "Cylinder",
            "Icosphere",
        ];
        let mut level_meshes = Vec::new();
        for (entity, mesh) in application.ecs.query::<&Mesh>().iter() {
            if level_mesh_names.iter().any(|x| **x == mesh.name) {
                level_meshes.push(entity);
            }
            log::info!("Mesh available: {}", mesh.name);
        }

        for entity in level_meshes.into_iter() {
            add_rigid_body(application, entity, BodyStatus::Static)?;
            add_box_collider(application, entity, LEVEL_COLLISION_GROUP)?;
            // add_trimesh_collider(application, entity, LEVEL_COLLISION_GROUP)?;
        }

        // Setup player
        if let Some(entity) = self.player.as_ref() {
            activate_first_person(application, *entity)?;
            let rigid_body = RigidBodyBuilder::new(BodyStatus::Dynamic)
                .translation(
                    transform.translation.x,
                    transform.translation.y,
                    transform.translation.z,
                )
                .lock_rotations()
                .build();
            let handle = application.physics_world.bodies.insert(rigid_body);
            application
                .ecs
                .insert_one(*entity, RigidBody::new(handle))?;

            add_cylinder_collider(application, *entity, PLAYER_COLLISION_GROUP)?;
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
        if application.input.is_key_pressed(VirtualKeyCode::Escape) {
            application.system.exit_requested = true;
        }

        sync_all_rigid_bodies(application);
        if let Some(player) = self.player.as_ref() {
            self.update_player(application, *player)?;
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

fn add_rigid_body(
    application: &mut Application,
    entity: Entity,
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
            .position(transform.as_isometry())
            .build();
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
        .collision_groups(collision_groups)
        .build();
    application.physics_world.colliders.insert(
        collider,
        rigid_body_handle,
        &mut application.physics_world.bodies,
    );
    Ok(())
}

fn add_cylinder_collider(
    application: &mut Application,
    entity: Entity,
    collision_groups: InteractionGroups,
) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let (half_height, radius) = (1.0, 0.5);
    let collider = ColliderBuilder::cylinder(half_height, radius)
        .collision_groups(collision_groups)
        .build();
    application.physics_world.colliders.insert(
        collider,
        rigid_body_handle,
        &mut application.physics_world.bodies,
    );
    Ok(())
}

fn add_trimesh_collider(
    application: &mut Application,
    entity: Entity,
    collision_groups: InteractionGroups,
) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let (vertices, indices) = {
        let mesh = application.ecs.get::<Mesh>(entity)?;
        let mut indices = Vec::new();
        let mut vertices = Vec::new();
        let mut index_offset = 0;
        for primitive in mesh.primitives.iter() {
            let vertex_offset = primitive.first_vertex as u32;
            let world_indices = &application.world.geometry.indices
                [primitive.first_index..(primitive.first_index + primitive.number_of_indices)];
            for offset in 0..world_indices.len() / 3 {
                let index = offset * 3;
                indices.push(Point3::new(
                    world_indices[index] - vertex_offset + index_offset,
                    world_indices[index + 1] - vertex_offset + index_offset,
                    world_indices[index + 2] - vertex_offset + index_offset,
                ));
            }
            let world_vertices = &application.world.geometry.vertices
                [primitive.first_vertex..(primitive.first_vertex + primitive.number_of_vertices)];
            for vertex in world_vertices.iter() {
                vertices.push(Point3::from(vertex.position));
            }
            index_offset += world_vertices.len() as u32;
        }
        (vertices, indices)
    };
    let collider = ColliderBuilder::trimesh(vertices, indices)
        .collision_groups(collision_groups)
        .build();
    application.physics_world.colliders.insert(
        collider,
        rigid_body_handle,
        &mut application.physics_world.bodies,
    );
    Ok(())
}

fn sync_rigid_body_to_transform(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let transform = application.ecs.get::<Transform>(entity)?;
    if let Some(body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
        body.set_position(transform.as_isometry(), true);
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
        body.wake_up(true);
    }
    Ok(())
}

fn sync_all_rigid_bodies(application: &mut Application) {
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
}

fn jump_player(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    if let Some(rigid_body) = application.physics_world.bodies.get_mut(rigid_body_handle) {
        let jump_strength = 5.0;
        let impulse = jump_strength * glm::Vec3::y();
        rigid_body.apply_impulse(impulse, true);
    }
    sync_transform_to_rigid_body(application, entity)?;
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
