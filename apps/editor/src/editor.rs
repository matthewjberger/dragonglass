use dragonglass::{
    app::{Application, ApplicationRunner, MouseOrbit},
    deps::{
        anyhow::Result,
        imgui::{im_str, Condition, Ui, Window},
        legion::{Entity, IntoQuery},
        log::{self, info, warn},
        rapier3d::{dynamics::BodyStatus, geometry::InteractionGroups},
        winit::event::{ElementState, MouseButton, VirtualKeyCode},
    },
    world::{load_gltf, MeshRender, World},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub struct Editor {
    camera: MouseOrbit,
    reload_shaders: Arc<AtomicBool>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            camera: MouseOrbit::default(),
            reload_shaders: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Editor {
    fn setup_file_reloading(&mut self) -> Result<()> {
        Ok(())
    }

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

    fn load_hdr(path: impl AsRef<Path>, application: &mut Application) -> Result<()> {
        // FIXME: We are loading the hdr even if it's already loaded here
        application.world.load_hdr(path)?;
        application.world.scene.skybox = Some(application.world.hdr_textures.len() - 1);

        // FIXME: Don't reload entire scene whenever something is added
        match application.renderer.load_world(&application.world) {
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

impl ApplicationRunner for Editor {
    fn initialize(&mut self, application: &mut Application) -> Result<()> {
        self.setup_file_reloading()?;
        application.world.add_default_light()?;
        Ok(())
    }

    fn create_ui(&mut self, _application: &mut Application, ui: &Ui) -> Result<()> {
        Window::new(im_str!("Scene Info"))
            .collapsed(true, Condition::FirstUseEver)
            .build(ui, || {
                ui.text(im_str!(" ",));
            });
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

        // if !application.world.animations.is_empty() {
        //     application
        //         .world
        //         .animate(0, 0.75 * application.system.delta_time as f32)?;
        // }

        if !application.input.allowed {
            return Ok(());
        }

        if application.world.active_camera_is_main()? {
            let camera_entity = application.world.active_camera()?;
            self.camera.update(application, camera_entity)?;
        }

        Ok(())
    }

    fn on_key(
        &mut self,
        application: &mut Application,
        keystate: ElementState,
        keycode: VirtualKeyCode,
    ) -> Result<()> {
        match (keycode, keystate) {
            (VirtualKeyCode::S, ElementState::Pressed) => {
                application.world.save("saved_map.dga")?;
                log::info!("Saved world!");
            }
            (VirtualKeyCode::T, ElementState::Pressed) => application.renderer.toggle_wireframe(),
            (VirtualKeyCode::C, ElementState::Pressed) => {
                application.world.clear()?;
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
                Some("hdr") => Self::load_hdr(raw_path, application)?,
                Some("dga") => {
                    application.world = World::load(raw_path)?;
                    application.renderer.load_world(&application.world)?;
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
            .iter(&mut application.world.ecs)
            .map(|(e, _)| *e)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            application
                .world
                .add_rigid_body(entity, BodyStatus::Static)?;
            application
                .world
                .add_trimesh_collider(entity, InteractionGroups::all())?;
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

        if (MouseButton::Left, ElementState::Pressed) == (button, state) {
            if let Some(entity) = application.pick_object(f32::MAX, InteractionGroups::all())? {
                log::info!("Picked entity: {:?}", entity);
            }
        }
        Ok(())
    }
}
