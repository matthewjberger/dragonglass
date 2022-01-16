use anyhow::{Context, Result};
use dragonglass::{
    app::{run_application, AppConfig, Application, ApplicationRunner, MouseLook},
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

const PLAYER_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b10, 0b01);
const LEVEL_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b01, 0b10);

#[derive(Default)]
pub struct Game {
    player: Option<Entity>,
    camera: MouseLook,
}

impl ApplicationRunner for Game {
    fn initialize(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        application.set_fullscreen();
        self.camera.orientation.sensitivity = glm::vec2(0.05, 0.05);

        // Load light 1
        {
            let position = glm::vec3(-2.0, 5.0, 0.0);
            let mut transform = Transform {
                translation: position,
                ..Default::default()
            };
            transform.look_at(&(-position), &glm::Vec3::y());
            let light_entity = application.world.ecs.push((
                transform,
                Light {
                    color: glm::vec3(0.0, 10.0, 10.0),
                    intensity: 1.0,
                    kind: LightKind::Point,
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
            let light_entity = application.world.ecs.push((
                transform,
                Light {
                    color: glm::vec3(20.0, 0.0, 0.0),
                    intensity: 1.0,
                    kind: LightKind::Point,
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
        let position = glm::vec3(0.0, 1.0, 0.0);
        let transform = Transform {
            translation: position,
            ..Default::default()
        };

        {
            let player_entity = application.world.ecs.push((transform,));
            application
                .world
                .scene
                .default_scenegraph_mut()?
                .add_node(player_entity);
            self.player = Some(player_entity);
        }

        // Load the level
        application.load_asset("assets/models/gamemap.glb")?;

        application.reload_world()?;

        // Add static colliders to level meshes
        let mut level_meshes = Vec::new();
        let mut query = <(Entity, &MeshRender)>::query();
        for (entity, mesh) in query.iter(&application.world.ecs) {
            level_meshes.push(*entity);
            log::info!("Mesh available: {}", mesh.name);
        }
        for entity in level_meshes.into_iter() {
            application
                .world
                .add_rigid_body(entity, RigidBodyType::Static)?;
            application
                .world
                .add_trimesh_collider(entity, LEVEL_COLLISION_GROUP)?;
        }

        // Setup player
        if let Some(entity) = self.player.as_ref() {
            activate_first_person(application, *entity)?;
            let rigid_body = RigidBodyBuilder::new(RigidBodyType::Dynamic)
                .translation(transform.translation)
                .lock_rotations()
                .build();
            let handle = application.world.physics.bodies.insert(rigid_body);
            application
                .world
                .ecs
                .entry(*entity)
                .context("")?
                .add_component(RigidBody::new(handle));

            application
                .world
                .add_cylinder_collider(*entity, 1.0, 0.5, PLAYER_COLLISION_GROUP)?;
        }

        Ok(())
    }

    fn update(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        if application.input.is_key_pressed(VirtualKeyCode::Escape) {
            application.system.exit_requested = true;
        }

        application.world.sync_all_rigid_bodies();
        if let Some(player) = self.player.as_ref() {
            self.camera.update(application, *player)?;
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
            backend: Backend::Vulkan,
            ..Default::default()
        },
    )
}

fn update_player(application: &mut Application, entity: Entity) -> Result<()> {
    let speed = 6.0 * application.system.delta_time as f32;
    {
        let mut entry = application.world.ecs.entry_mut(entity)?;
        let transform = entry.get_component_mut::<Transform>()?;
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
    application.world.sync_rigid_body_to_transform(entity)?;
    Ok(())
}

fn jump_player(application: &mut Application, entity: Entity) -> Result<()> {
    let rigid_body_handle = application
        .world
        .ecs
        .entry_ref(entity)?
        .get_component::<RigidBody>()?
        .handle;
    if let Some(rigid_body) = application.world.physics.bodies.get_mut(rigid_body_handle) {
        let jump_strength = 5.0;
        let impulse = jump_strength * glm::Vec3::y();
        rigid_body.apply_impulse(impulse, true);
    }
    application.world.sync_transform_to_rigid_body(entity)?;
    Ok(())
}

fn activate_first_person(application: &mut Application, entity: Entity) -> Result<()> {
    // Disable active camera
    let camera_entity = application.world.active_camera()?;
    application
        .world
        .ecs
        .entry_mut(camera_entity)?
        .get_component_mut::<WorldCamera>()?
        .enabled = false;

    application
        .world
        .ecs
        .entry(entity)
        .context("entity not found")?
        .add_component(Hidden {});
    application
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
