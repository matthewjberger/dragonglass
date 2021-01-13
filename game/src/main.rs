use anyhow::Result;
use dragonglass::{
    app::Application,
    app::{run_application, AppConfig, ApplicationRunner},
    physics::RigidBody,
    world::{
        Camera, Entity, Hidden, Light, LightKind, Mesh, PerspectiveCamera, Projection, SceneGraph,
        Transform,
    },
};
use imgui::{im_str, Condition, Ui, Window};
use nalgebra::Point3;
use nalgebra_glm as glm;
use rapier3d::{dynamics::BodyStatus, dynamics::RigidBodyBuilder, geometry::ColliderBuilder};
use winit::event::{ElementState, VirtualKeyCode};

#[derive(Default)]
pub struct Game {
    level: Option<Entity>,
    player: Option<Entity>,
}

impl ApplicationRunner for Game {
    fn initialize(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        let (player_path, player_handle) = (
            "assets/models/DamagedHelmet.glb",
            "mesh_helmet_LP_13930damagedHelmet",
        );
        let (level_path, level_handle) = ("assets/models/plane.gltf", "Plane");

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

        application.load_asset(player_path)?;
        application.load_asset(level_path)?;
        application.reload_world()?;

        for (entity, mesh) in application.ecs.query::<&Mesh>().iter() {
            if mesh.name == player_handle {
                self.player = Some(entity);
                {
                    {
                        let mut transform = application.ecs.get_mut::<Transform>(entity)?;
                        transform.translation.y = 40.0;
                        transform.scale = glm::vec3(0.5, 0.5, 0.5);
                    }
                }
            }
            if mesh.name == level_handle {
                self.level = Some(entity);
            }
            log::info!("Mesh available: {}", mesh.name);
        }

        if let Some(entity) = self.player.as_ref() {
            add_rigid_body(application, *entity, BodyStatus::Dynamic)?;
            add_box_collider(application, *entity)?;
        }

        if let Some(entity) = self.level.as_ref() {
            add_rigid_body(application, *entity, BodyStatus::Static)?;
            add_box_collider(application, *entity)?;
            // add_trimesh_collider(application, *entity)?;
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
        sync_all_rigid_bodies(application);
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
            .translation(
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
            )
            .rotation(transform.rotation.as_vector().xyz())
            .build();
        application.physics_world.bodies.insert(rigid_body)
    };
    application.ecs.insert_one(entity, RigidBody::new(handle))?;
    Ok(())
}

fn add_box_collider(application: &mut Application, entity: Entity) -> Result<()> {
    let bounding_box = {
        let mesh = application.ecs.get::<Mesh>(entity)?;
        mesh.bounding_box()
    };
    let transform = application.ecs.get::<Transform>(entity)?;
    let rigid_body_handle = application.ecs.get::<RigidBody>(entity)?.handle;
    let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
    let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z).build();
    application.physics_world.colliders.insert(
        collider,
        rigid_body_handle,
        &mut application.physics_world.bodies,
    );
    Ok(())
}

fn add_trimesh_collider(application: &mut Application, entity: Entity) -> Result<()> {
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
    let collider = ColliderBuilder::trimesh(vertices, indices).build();
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

fn update_player(application: &mut Application, entity: Entity) -> Result<()> {
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
    }
    sync_rigid_body_to_transform(application, entity)?;
    Ok(())
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
