use anyhow::Result;
use dragonglass::{
    app::{App, AppState, MouseOrbit},
    world::{
        legion::Entity,
        load_gltf,
        rapier3d::{geometry::InteractionGroups, prelude::RigidBodyType},
        IntoQuery, MeshRender,
    },
};
use log::{info, warn};
use std::path::{Path, PathBuf};
use winit::event::{ElementState, VirtualKeyCode};

pub struct Editor {
    camera: MouseOrbit,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            camera: MouseOrbit::default(),
        }
    }
}

impl Editor {
    fn load_gltf(path: &str, app_state: &mut AppState) -> Result<()> {
        load_gltf(path, &mut app_state.world)?;

        // FIXME: Don't reload entire scene whenever something is added
        match app_state.renderer.load_world(&app_state.world) {
            Ok(_) => {
                info!("Loaded gltf world: '{}'", path);
            }
            Err(error) => {
                warn!("Failed to load gltf world: {}", error);
            }
        }

        Ok(())
    }

    fn load_hdr(path: impl AsRef<Path>, app_state: &mut AppState) -> Result<()> {
        // FIXME: We are loading the hdr even if it's already loaded here
        app_state.world.load_hdr(path)?;
        app_state.world.scene.skybox = Some(app_state.world.hdr_textures.len() - 1);

        // FIXME: Don't reload entire scene whenever something is added
        match app_state.renderer.load_world(&app_state.world) {
            Ok(_) => {
                info!("Reloaded gltf world");
            }
            Err(error) => {
                warn!("Failed to load gltf world: {}", error);
            }
        }

        Ok(())
    }
}

impl App for Editor {
    fn initialize(&mut self, app_state: &mut dragonglass::app::AppState) -> Result<()> {
        app_state.world.add_default_light()?;
        Ok(())
    }

    fn update(&mut self, app_state: &mut dragonglass::app::AppState) -> Result<()> {
        if app_state.input.is_key_pressed(VirtualKeyCode::Escape) {
            app_state.system.exit_requested = true;
        }

        // if !application.world.animations.is_empty() {
        //     application
        //         .world
        //         .animate(0, 0.75 * application.system.delta_time as f32)?;
        // }

        if !app_state.input.allowed {
            return Ok(());
        }

        if app_state.world.active_camera_is_main()? {
            let camera_entity = app_state.world.active_camera()?;
            self.camera.update(app_state, camera_entity)?;
        }

        Ok(())
    }

    fn on_key(
        &mut self,
        input: winit::event::KeyboardInput,
        app_state: &mut dragonglass::app::AppState,
    ) -> Result<()> {
        match (input.virtual_keycode, input.state) {
            (Some(VirtualKeyCode::S), ElementState::Pressed) => {
                app_state.world.save("saved_map.dga")?;
                log::info!("Saved world!");
            }
            (Some(VirtualKeyCode::C), ElementState::Pressed) => {
                app_state.world.clear()?;
                if let Err(error) = app_state.renderer.load_world(&app_state.world) {
                    warn!("Failed to load gltf world: {}", error);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_file_dropped(
        &mut self,
        path: &PathBuf,
        app_state: &mut dragonglass::app::AppState,
    ) -> Result<()> {
        let raw_path = match path.to_str() {
            Some(raw_path) => raw_path,
            None => return Ok(()),
        };

        if let Some(extension) = path.extension() {
            match extension.to_str() {
                Some("glb") | Some("gltf") => Self::load_gltf(raw_path, app_state)?,
                Some("hdr") => Self::load_hdr(raw_path, app_state)?,
                Some("dga") => {
                    app_state.world.reload(raw_path)?;
                    app_state.renderer.load_world(&app_state.world)?;
                    log::info!("Loaded world!");
                }
                _ => warn!(
                    "File extension {:#?} is not a valid '.dga', '.glb', '.gltf', or '.hdr' extension",
                    extension
                ),
            }
        }

        let mut query = <(Entity, &MeshRender)>::query();
        let entities = query
            .iter(&mut app_state.world.ecs)
            .map(|(e, _)| *e)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            app_state
                .world
                .add_rigid_body(entity, RigidBodyType::Static)?;
            app_state
                .world
                .add_trimesh_collider(entity, InteractionGroups::all())?;
        }

        Ok(())
    }
}
