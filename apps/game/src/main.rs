use anyhow::{Context, Result};
use dragonglass::{
    app::{run_application, App, AppConfig, MouseLook, Resources},
    render::Backend,
    world::{
        Camera as WorldCamera, Entity, EntityStore, Hidden, IntoQuery, Light, LightKind,
        MeshRender, PerspectiveCamera, Projection, RigidBody, Transform,
    },
};
use nalgebra_glm as glm;
use rapier3d::{dynamics::RigidBodyBuilder, geometry::InteractionGroups, prelude::RigidBodyType};
use winit::event::{ElementState, VirtualKeyCode};

// TODO: Create trigger with event on collision
// TODO: Visualize triangle mesh colliders as wireframes in renderer?

const OBJECT_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b100, 0b111);
const PLAYER_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b010, 0b101);
const LEVEL_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b001, 0b110);

#[derive(Default)]
pub struct Game {
    player: Option<Entity>,
    camera: MouseLook,
}

impl App for Game {
    fn initialize(&mut self, resources: &mut dragonglass::app::Resources) -> Result<()> {
        resources
            .world
            .physics
            .set_gravity(glm::vec3(0.0, -4.0, 0.0));

        resources.set_fullscreen();
        self.camera.orientation.sensitivity = glm::vec2(0.05, 0.05);

        // Load light 1
        {
            let position = glm::vec3(-2.0, 5.0, 0.0);
            let mut transform = Transform {
                translation: position,
                ..Default::default()
            };
            transform.look_at(&(-position), &glm::Vec3::y());
            let light_entity = resources.world.ecs.push((
                transform,
                Light {
                    color: glm::vec3(0.0, 10.0, 10.0),
                    intensity: 1.0,
                    kind: LightKind::Point,
                    ..Default::default()
                },
            ));
            resources
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
            let light_entity = resources.world.ecs.push((
                transform,
                Light {
                    color: glm::vec3(20.0, 0.0, 0.0),
                    intensity: 1.0,
                    kind: LightKind::Point,
                    ..Default::default()
                },
            ));
            resources
                .world
                .scene
                .default_scenegraph_mut()?
                .add_node(light_entity);
        }

        // Load player
        let position = glm::vec3(0.0, 4.0, 0.0);
        let transform = Transform {
            translation: position,
            ..Default::default()
        };

        {
            let player_entity = resources.world.ecs.push((transform,));
            resources
                .world
                .scene
                .default_scenegraph_mut()?
                .add_node(player_entity);
            self.player = Some(player_entity);
        }

        // Load the level
        resources.load_asset("assets/models/arena2.glb")?;

        // Add static colliders to level meshes
        let mut level_meshes = Vec::new();
        let mut query = <(Entity, &MeshRender)>::query();
        for (entity, mesh) in query.iter(&resources.world.ecs) {
            level_meshes.push((*entity, mesh.name.to_string()));
            log::info!("Mesh available: {}", mesh.name);
        }
        for (entity, mesh_name) in level_meshes.into_iter() {
            if mesh_name == "Sphere" {
                log::info!("Mesh '{}' will be dynamic", mesh_name);
                resources
                    .world
                    .add_rigid_body(entity, RigidBodyType::Dynamic)?;
                resources
                    .world
                    .add_sphere_collider(entity, OBJECT_COLLISION_GROUP)?;
            } else if mesh_name == "Cube.020" {
                log::info!("Mesh '{}' will be dynamic", mesh_name);
                resources
                    .world
                    .add_rigid_body(entity, RigidBodyType::Dynamic)?;
                resources
                    .world
                    .add_box_collider(entity, OBJECT_COLLISION_GROUP)?;
            } else {
                resources
                    .world
                    .add_rigid_body(entity, RigidBodyType::Static)?;
                resources
                    .world
                    .add_trimesh_collider(entity, LEVEL_COLLISION_GROUP)?;
            }
        }

        // Setup player
        if let Some(entity) = self.player.as_ref() {
            activate_first_person(resources, *entity)?;
            let rigid_body = RigidBodyBuilder::new(RigidBodyType::Dynamic)
                .translation(transform.translation)
                .lock_rotations()
                .build();
            let handle = resources.world.physics.bodies.insert(rigid_body);
            resources
                .world
                .ecs
                .entry(*entity)
                .context("")?
                .add_component(RigidBody::new(handle));

            resources
                .world
                .add_cylinder_collider(*entity, 0.5, 0.25, PLAYER_COLLISION_GROUP)?;
        }

        Ok(())
    }

    fn update(&mut self, resources: &mut Resources) -> Result<()> {
        if resources.input.is_key_pressed(VirtualKeyCode::Escape) {
            resources.system.exit_requested = true;
        }

        if let Some(player) = self.player.as_ref() {
            self.camera.update(resources, *player)?;
            update_player(resources, *player)?;
        }

        Ok(())
    }

    fn on_key(
        &mut self,
        input: winit::event::KeyboardInput,
        resources: &mut Resources,
    ) -> Result<()> {
        if let (Some(VirtualKeyCode::Space), ElementState::Pressed) =
            (input.virtual_keycode, input.state)
        {
            if let Some(player) = self.player.as_ref() {
                jump_player(resources, *player)?;
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
            backend: Backend::Vulkan,
            ..Default::default()
        },
    )
}

fn update_player(resources: &mut Resources, entity: Entity) -> Result<()> {
    let speed = 2.0 * resources.system.delta_time as f32;
    {
        let mut entry = resources.world.ecs.entry_mut(entity)?;
        let transform = entry.get_component_mut::<Transform>()?;
        let mut translation = glm::vec3(0.0, 0.0, 0.0);

        if resources.input.is_key_pressed(VirtualKeyCode::W) {
            translation = speed * transform.forward();
        }

        if resources.input.is_key_pressed(VirtualKeyCode::A) {
            translation = -speed * transform.right();
        }

        if resources.input.is_key_pressed(VirtualKeyCode::S) {
            translation = -speed * transform.forward();
        }

        if resources.input.is_key_pressed(VirtualKeyCode::D) {
            translation = speed * transform.right();
        }

        transform.translation += translation;
    }
    resources.world.sync_rigid_body_to_transform(entity)?;
    Ok(())
}

fn jump_player(resources: &mut Resources, entity: Entity) -> Result<()> {
    let rigid_body_handle = resources
        .world
        .ecs
        .entry_ref(entity)?
        .get_component::<RigidBody>()?
        .handle;
    if let Some(rigid_body) = resources.world.physics.bodies.get_mut(rigid_body_handle) {
        let jump_strength = 0.5;
        let impulse = jump_strength * glm::Vec3::y();
        rigid_body.apply_impulse(impulse, true);
    }
    resources.world.sync_transform_to_rigid_body(entity)?;
    Ok(())
}

fn activate_first_person(resources: &mut Resources, entity: Entity) -> Result<()> {
    // Disable active camera
    let camera_entity = resources.world.active_camera()?;
    resources
        .world
        .ecs
        .entry_mut(camera_entity)?
        .get_component_mut::<WorldCamera>()?
        .enabled = false;

    resources
        .world
        .ecs
        .entry(entity)
        .context("entity not found")?
        .add_component(Hidden {});
    resources
        .world
        .ecs
        .entry(entity)
        .context("entity not found")?
        .add_component(WorldCamera {
            name: "Player Camera".to_string(),
            projection: Projection::Perspective(PerspectiveCamera {
                aspect_ratio: None,
                y_fov_rad: 90_f32.to_radians(),
                z_far: Some(1000.0),
                z_near: 0.001,
            }),
            enabled: true,
        });

    Ok(())
}
