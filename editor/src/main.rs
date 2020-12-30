use anyhow::Result;
use dragonglass::{
    app::{run_application, AppConfig, Application, ApplicationRunner},
    world::Transform,
    world::{load_gltf, BoxCollider, BoxColliderVisible, Camera, Mesh, Selected},
};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use nalgebra_glm as glm;
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

mod camera;

#[derive(Default)]
pub struct Editor {
    arcball: Arcball,
}

impl Editor {
    fn load_gltf(path: &str, application: &mut Application) -> Result<()> {
        load_gltf(path, &mut application.world)?;

        // FIXME: Don't reload entire scene whenever something is added
        match application.renderer.load_world(&application.world) {
            Ok(_) => {
                info!("Loaded gltf world: '{}'", path);
            }
            Err(error) => {
                warn!("Failed to load gltf world: {}", error);
            }
        }

        Ok(())
    }

    fn load_hdr(path: &str, application: &mut Application) {
        match application.renderer.load_skybox(path) {
            Ok(_) => {
                info!("Loaded hdr cubemap: '{}'", path);
            }
            Err(error) => {
                error!("Failed to load hdr map: {}", error);
            }
        }
    }

    fn show_hovered_object_collider(&self, application: &mut Application) -> Result<()> {
        application.world.remove_all::<BoxColliderVisible>();
        if let Some(entity) = application.pick_object(f32::MAX)? {
            let _ = application
                .world
                .ecs
                .insert_one(entity, BoxColliderVisible {});
        }
        Ok(())
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
}

impl ApplicationRunner for Editor {
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
        ui.text(im_str!("Cameras"));
        let mut change_camera = None;
        for (index, (entity, camera)) in application.world.ecs.query::<&Camera>().iter().enumerate()
        {
            let label = if camera.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let clicked = ui.small_button(&im_str!("{} #{} [{}]", camera.name, index, label));
            if change_camera.is_none() && clicked {
                change_camera = Some(entity);
            }
        }
        if let Some(selected_camera_entity) = change_camera {
            for (entity, camera) in application.world.ecs.query_mut::<&mut Camera>() {
                camera.enabled = entity == selected_camera_entity;
            }
        }

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

        if !application.world.animations.is_empty() {
            application
                .world
                .animate(0, 0.75 * application.system.delta_time as f32)?;
        }

        if !application.input.allowed {
            return Ok(());
        }

        if application.world.active_camera_is_main()? {
            self.arcball.update(application)?;
        }

        self.show_hovered_object_collider(application)?;

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
        if !application.input.allowed {
            return Ok(());
        }
        if let (MouseButton::Left, ElementState::Pressed) = (button, state) {
            let entity = match application.pick_object(f32::MAX)? {
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
        Editor::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            ..Default::default()
        },
    )
}

#[derive(Default)]
struct Arcball {
    pub offset: glm::Vec3, // TODO: this needs to track the arcball target
}

impl Arcball {
    pub fn update(&mut self, application: &mut Application) -> Result<()> {
        let delta_time = application.system.delta_time as f32;
        let mouse_delta = application.input.mouse.position_delta;
        let mousewheel_delta = application.input.mouse.wheel_delta;

        let camera_entity = application.world.active_camera()?;
        let mut transform = application.world.ecs.get_mut::<Transform>(camera_entity)?;
        let forward = transform.forward();
        let up = transform.up();
        let right = transform.right();

        if application.input.mouse.scrolled {
            let scroll_multiplier = 100.0;
            transform.translation += forward * scroll_multiplier * mousewheel_delta.y * delta_time;
        }

        if application.input.mouse.is_right_clicked {
            transform.translation -= right * mouse_delta.x * delta_time;
            transform.translation += up * mouse_delta.y * delta_time;
        }

        if application.input.mouse.is_left_clicked {
            let yaw_delta = -mouse_delta.x * delta_time;
            transform.translation =
                glm::rotate_vec3(&transform.translation, yaw_delta, &glm::Vec3::y());

            let pitch_bound = 80_f32.to_radians();
            let pitch = glm::quat_euler_angles(&transform.rotation).z;
            let mut pitch_delta = -mouse_delta.y * delta_time;
            if pitch + pitch_delta > pitch_bound {
                pitch_delta = pitch_bound - pitch;
            }
            if pitch + pitch_delta < -pitch_bound {
                pitch_delta = -pitch_bound - pitch;
            }
            transform.translation =
                glm::rotate_vec3(&transform.translation, pitch_delta, &transform.right());

            let target = -transform.translation;
            transform.look_at(&target, &glm::Vec3::y());
        }

        Ok(())
    }
}
