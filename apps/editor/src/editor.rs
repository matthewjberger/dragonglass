use crate::camera::Arcball;
use anyhow::Result;
use dragonglass::{
    app::{Application, ApplicationRunner},
    world::{load_gltf, BoxCollider, BoxColliderVisible, Camera, Mesh, Selected},
};
use hotwatch::{Event, Hotwatch};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use std::{
    ffi::OsStr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

pub struct Editor {
    arcball: Arcball,
    _hotwatch: Option<Hotwatch>,
    reload_shaders: Arc<AtomicBool>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            arcball: Arcball::default(),
            _hotwatch: None,
            reload_shaders: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Editor {
    fn setup_file_reloading(&mut self) -> Result<()> {
        let reload_shaders = self.reload_shaders.clone();
        let mut hotwatch = Hotwatch::new()?;
        hotwatch.watch("assets/shaders/model", move |event: Event| {
            if let Event::Write(path) = event {
                if let Some(extension) = path.extension() {
                    // Don't need to reload shaders again
                    // after a .spv file is generated
                    if extension == OsStr::new("spv") {
                        return;
                    }
                    reload_shaders.store(true, Ordering::Release);
                }
            }
        })?;
        self._hotwatch = Some(hotwatch);
        Ok(())
    }

    fn load_gltf(path: &str, application: &mut Application) -> Result<()> {
        load_gltf(path, &mut application.world, &mut application.ecs)?;

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
        application
            .world
            .remove_all::<BoxColliderVisible>(&mut application.ecs);
        if let Some(entity) = application.pick_object(f32::MAX)? {
            let _ = application.ecs.insert_one(entity, BoxColliderVisible {});
        }
        Ok(())
    }

    fn clear_colliders(application: &mut Application) {
        let colliders = application
            .ecs
            .query::<&BoxCollider>()
            .iter()
            .map(|(_entity, collider)| collider.handle)
            .collect::<Vec<_>>();
        application.collision_world.remove(&colliders);
    }
}

impl ApplicationRunner for Editor {
    fn initialize(&mut self, application: &mut Application) -> Result<()> {
        self.setup_file_reloading()?;
        application.world.add_default_light(&mut application.ecs)?;
        Ok(())
    }

    fn create_ui(&mut self, application: &mut Application, ui: &Ui) -> Result<()> {
        ui.text(im_str!(
            "Number of entities: {}",
            application.ecs.iter().count()
        ));
        let number_of_meshes = application.ecs.query::<&Mesh>().iter().count();
        ui.text(im_str!("Number of meshes: {}", number_of_meshes));
        ui.text(im_str!(
            "Number of animations: {}",
            application.world.animations.len()
        ));
        ui.text(im_str!(
            "Number of textures: {}",
            application.world.textures.len()
        ));
        ui.text(im_str!(
            "Number of materials: {}",
            application.world.materials.len()
        ));
        ui.text(im_str!(
            "Number of collision_objects: {}",
            application.collision_world.collision_objects().count()
        ));

        ui.separator();
        ui.text(im_str!("Cameras"));
        let mut change_camera = None;
        for (index, (entity, camera)) in application.ecs.query::<&Camera>().iter().enumerate() {
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
            for (entity, camera) in application.ecs.query_mut::<&mut Camera>() {
                camera.enabled = entity == selected_camera_entity;
            }
        }

        ui.separator();
        ui.text(im_str!("Selected Entities"));
        for (entity, _) in application.ecs.query::<&Selected>().iter() {
            ui.text(im_str!("{:#?}", entity));
        }

        Ok(())
    }

    fn update(&mut self, application: &mut Application) -> Result<()> {
        if self.reload_shaders.load(Ordering::Acquire) {
            application.renderer.reload_asset_shaders()?;
        }
        self.reload_shaders.store(false, Ordering::Release);

        if application.input.is_key_pressed(VirtualKeyCode::Escape) {
            application.system.exit_requested = true;
        }

        if !application.world.animations.is_empty() {
            application.world.animate(
                &mut application.ecs,
                0,
                0.75 * application.system.delta_time as f32,
            )?;
        }

        if !application.input.allowed {
            return Ok(());
        }

        if application
            .world
            .active_camera_is_main(&mut application.ecs)?
        {
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
                application.world.clear(&mut application.ecs)?;
                application.world.add_default_light(&mut application.ecs)?;
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

            let already_selected = application.ecs.get::<Selected>(entity).is_ok();
            let shift_active = application.input.is_key_pressed(VirtualKeyCode::LShift);
            if !shift_active {
                application
                    .world
                    .remove_all::<Selected>(&mut application.ecs);
            }
            if !already_selected {
                let _ = application.ecs.insert_one(entity, Selected {});
            } else if shift_active {
                let _ = application.ecs.remove_one::<Selected>(entity);
            }
        }
        Ok(())
    }
}
