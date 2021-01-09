use anyhow::Result;
use dragonglass::{
    app::Application,
    app::{run_application, AppConfig, ApplicationRunner},
    physics::RigidBody,
    world::{Entity, Mesh, Transform},
};
use imgui::{im_str, Condition, Ui, Window};
use nalgebra_glm as glm;
use rapier3d::{dynamics::RigidBodyBuilder, geometry::ColliderBuilder};

#[derive(Default)]
pub struct Game {
    helmet: Option<Entity>,
    plane: Option<Entity>,
}

impl ApplicationRunner for Game {
    fn initialize(&mut self, application: &mut dragonglass::app::Application) -> Result<()> {
        application.load_asset("assets/models/plane.gltf")?;
        application.load_asset("assets/models/DamagedHelmet.glb")?;
        application.reload_world()?;
        for (entity, mesh) in application.ecs.query::<&Mesh>().iter() {
            if mesh.name == "mesh_helmet_LP_13930damagedHelmet" {
                self.helmet = Some(entity);
            }
            if mesh.name == "Plane" {
                self.plane = Some(entity);
            }
        }
        if let Some(helmet) = self.helmet.as_ref() {
            {
                let mut transform = application.ecs.get_mut::<Transform>(*helmet)?;
                transform.translation.y = 4.0;
            }

            let handle = {
                let bounding_box = {
                    let mesh = application.ecs.get::<Mesh>(*helmet)?;
                    mesh.bounding_box()
                };
                let translation = glm::translation(&bounding_box.center());
                let transform_matrix = application
                    .world
                    .entity_global_transform_matrix(&mut application.ecs, *helmet)?
                    * translation;
                let transform = Transform::from(transform_matrix);

                // Insert a corresponding rigid body
                let rigid_body = RigidBodyBuilder::new_dynamic()
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
            application
                .ecs
                .insert_one(*helmet, RigidBody::new(handle))?;
        }

        if let Some(plane) = self.plane.as_ref() {
            {
                let mut transform = application.ecs.get_mut::<Transform>(*plane)?;
                transform.translation.y = -4.0;
            }

            let handle = {
                let bounding_box = {
                    let mesh = application.ecs.get::<Mesh>(*plane)?;
                    mesh.bounding_box()
                };
                let translation = glm::translation(&bounding_box.center());
                let transform_matrix = application
                    .world
                    .entity_global_transform_matrix(&mut application.ecs, *plane)?
                    * translation;
                let transform = Transform::from(transform_matrix);

                // Insert a corresponding rigid body
                let rigid_body = RigidBodyBuilder::new_static()
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
            application.ecs.insert_one(*plane, RigidBody::new(handle))?;
        }

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

    fn on_key(
        &mut self,
        _application: &mut dragonglass::app::Application,
        _keystate: winit::event::ElementState,
        _keycode: winit::event::VirtualKeyCode,
    ) -> Result<()> {
        Ok(())
    }

    fn on_file_dropped(
        &mut self,
        _application: &mut dragonglass::app::Application,
        _path: &std::path::PathBuf,
    ) -> Result<()> {
        Ok(())
    }

    fn on_mouse(
        &mut self,
        _application: &mut dragonglass::app::Application,
        _button: winit::event::MouseButton,
        _state: winit::event::ElementState,
    ) -> Result<()> {
        Ok(())
    }

    fn handle_events(
        &mut self,
        _application: &mut dragonglass::app::Application,
        _event: winit::event::Event<()>,
    ) -> Result<()> {
        Ok(())
    }
}

fn main() -> Result<()> {
    run_application(
        Game::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            ..Default::default()
        },
    )
}
