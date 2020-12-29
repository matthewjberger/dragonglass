use anyhow::Result;
use dragonglass::{
    app::{run_application, AppConfig, Application, ApplicationRunner},
    world::Transform,
    world::{load_gltf, BoxCollider, BoxColliderVisible, Camera, Mesh, Selected},
};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

mod camera;

#[derive(Default)]
pub struct Viewer;

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

impl ApplicationRunner for Viewer {
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

        // ui.separator();
        // ui.text(im_str!("Multipliers"));
        // let _ = ui
        //     .input_float(im_str!("Scroll"), &mut self.camera.scroll)
        //     .step(0.1)
        //     .step_fast(1.0)
        //     .build();
        // let _ = ui
        //     .input_float(im_str!("Drag"), &mut self.camera.drag)
        //     .step(0.1)
        //     .step_fast(1.0)
        //     .build();
        // let _ = ui
        //     .input_float(im_str!("Rotation"), &mut self.camera.rotation)
        //     .step(0.1)
        //     .step_fast(1.0)
        //     .build();

        ui.separator();
        ui.text(im_str!("Selected Entities"));
        for (entity, _) in application.world.ecs.query::<&Selected>().iter() {
            ui.text(im_str!("{:#?}", entity));
        }

        ui.separator();
        ui.text(im_str!("Cameras"));
        for (_entity, camera) in application.world.ecs.query::<&Camera>().iter() {
            ui.text(im_str!("{}", camera.name,));
        }

        Ok(())
    }

    fn update(&mut self, application: &mut Application) -> Result<()> {
        if application.input.is_key_pressed(VirtualKeyCode::Escape) {
            application.system.exit_requested = true;
        }

        // FIXME_CAMERA: Update camera here to have arcball or fps controls. Move systems to separate module
        let active_camera = application
            .world
            .active_camera(application.system.aspect_ratio())?;
        if application.input.is_key_pressed(VirtualKeyCode::Space) {
            let mut transform = application
                .world
                .ecs
                .get_mut::<Transform>(active_camera.entity)?;
            transform.translation.y += 2.0 * application.system.delta_time as f32;
        }

        if !application.world.animations.is_empty() {
            application
                .world
                .animate(0, 0.75 * application.system.delta_time as f32)?;
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
        Viewer::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            ..Default::default()
        },
    )
}
