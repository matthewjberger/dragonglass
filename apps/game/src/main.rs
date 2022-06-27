use anyhow::{Context, Result};
use dragonglass::{
    app::{run_application, AppConfig, MouseLook, Resources, State, Transition},
    render::Backend,
    world::{
        Camera as WorldCamera, Entity, EntityStore, Hidden, IntoQuery, Light, LightKind,
        MeshRender, PerspectiveCamera, Projection, RigidBody, Transform, World,
    },
};
use nalgebra_glm as glm;
use rapier3d::{dynamics::RigidBodyBuilder, geometry::InteractionGroups, prelude::RigidBodyType};
use winit::event::{ElementState, VirtualKeyCode};

// TODO: Create trigger with event on collision
// TODO: Visualize triangle mesh colliders as wireframes in renderer?
// TODO: Use capsules for picking

const PLAYER_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b10, 0b01);
const LEVEL_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b01, 0b10);

pub struct Game {
    player: Option<Entity>,

    camera: MouseLook,
    world: World,
}

impl Game {
    fn new() -> Result<Self> {
        Ok(Self {
            world: World::new()?,
            camera: MouseLook::default(),
            player: None,
        })
    }

    fn update_player(&mut self, resources: &mut Resources) -> Result<()> {
        let entity = if let Some(player) = self.player.as_ref() {
            *player
        } else {
            return Ok(());
        };
        let speed = 2.0 * resources.system.delta_time as f32;
        {
            let mut entry = self.world.ecs.entry_mut(entity)?;
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
        self.world.sync_rigid_body_to_transform(entity)?;
        Ok(())
    }

    fn activate_first_person(&mut self) -> Result<()> {
        let entity = if let Some(player) = self.player.as_ref() {
            *player
        } else {
            return Ok(());
        };
        // Disable active camera
        let camera_entity = self.world.active_camera()?;
        self.world
            .ecs
            .entry_mut(camera_entity)?
            .get_component_mut::<WorldCamera>()?
            .enabled = false;

        self.world
            .ecs
            .entry(entity)
            .context("entity not found")?
            .add_component(Hidden {});
        self.world
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
}

impl State for Game {
    fn on_start(&mut self, resources: &mut Resources) -> Result<()> {
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
            let light_entity = self.world.ecs.push((
                transform,
                Light {
                    color: glm::vec3(0.0, 10.0, 10.0),
                    intensity: 1.0,
                    kind: LightKind::Point,
                    ..Default::default()
                },
            ));
            self.world
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
            let light_entity = self.world.ecs.push((
                transform,
                Light {
                    color: glm::vec3(20.0, 0.0, 0.0),
                    intensity: 1.0,
                    kind: LightKind::Point,
                    ..Default::default()
                },
            ));
            self.world
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
            let player_entity = self.world.ecs.push((transform,));
            self.world
                .scene
                .default_scenegraph_mut()?
                .add_node(player_entity);
            self.player = Some(player_entity);
        }

        // Load the level
        resources.load_asset("assets/models/arena.glb", &mut self.world)?;

        // Add static colliders to level meshes
        let mut level_meshes = Vec::new();
        let mut query = <(Entity, &MeshRender)>::query();
        for (entity, mesh) in query.iter(&self.world.ecs) {
            level_meshes.push(*entity);
            log::info!("Mesh available: {}", mesh.name);
        }
        for entity in level_meshes.into_iter() {
            self.world.add_rigid_body(entity, RigidBodyType::Static)?;
            self.world
                .add_trimesh_collider(entity, LEVEL_COLLISION_GROUP)?;
        }

        // Setup player
        self.activate_first_person()?;
        if let Some(entity) = self.player.as_ref() {
            let rigid_body = RigidBodyBuilder::new(RigidBodyType::Dynamic)
                .translation(transform.translation)
                .lock_rotations()
                .build();
            let handle = self.world.physics.bodies.insert(rigid_body);
            self.world
                .ecs
                .entry(*entity)
                .context("")?
                .add_component(RigidBody::new(handle));
            self.world
                .add_cylinder_collider(*entity, 0.5, 0.25, PLAYER_COLLISION_GROUP)?;
        }

        Ok(())
    }

    fn update(&mut self, resources: &mut Resources) -> Result<Transition> {
        self.world.tick(resources.system.delta_time as f32)?;

        if resources.input.is_key_pressed(VirtualKeyCode::Escape) {
            resources.system.exit_requested = true;
        }

        self.world.sync_all_rigid_bodies();
        if let Some(player) = self.player.as_ref() {
            self.camera.update(resources, *player, &mut self.world)?;
        }
        self.update_player(resources)?;

        Ok(Transition::None)
    }

    fn on_key(
        &mut self,
        _resources: &mut Resources,
        input: winit::event::KeyboardInput,
    ) -> Result<Transition> {
        if let (Some(VirtualKeyCode::Space), ElementState::Pressed) =
            (input.virtual_keycode, input.state)
        {
            match self.player.as_ref() {
                Some(player) => {
                    {
                        let entity = *player;
                        let rigid_body_handle = self
                            .world
                            .ecs
                            .entry_ref(entity)?
                            .get_component::<RigidBody>()?
                            .handle;
                        if let Some(rigid_body) =
                            self.world.physics.bodies.get_mut(rigid_body_handle)
                        {
                            let jump_strength = 0.5;
                            let impulse = jump_strength * glm::Vec3::y();
                            rigid_body.apply_impulse(impulse, true);
                        }
                        self.world.sync_transform_to_rigid_body(entity)?;
                    };
                }
                _ => (),
            }
        }
        Ok(Transition::None)
    }
}

fn main() -> Result<()> {
    run_application(
        Game::new()?,
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Physics Test with Rapier3D".to_string(),
            backend: Backend::Vulkan,
            ..Default::default()
        },
    )
}
