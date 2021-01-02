use crate::camera::Arcball;
use anyhow::Result;
use dragonglass::{
    app::{Application, ApplicationRunner},
    world::Visibility,
    world::{load_gltf, BoxCollider, Camera, Entity, Mesh, Selection},
};
use imgui::{im_str, Ui};
use legion::IntoQuery;
use log::{error, info, warn};
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

#[derive(Default)]
pub struct Editor {
    arcball: Arcball,
}

impl Editor {
    fn load_gltf(&mut self, path: &str, application: &mut Application) -> Result<()> {
        load_gltf(path, &mut application.world, &mut application.ecs)?;

        self.add_required_components(application);

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
        for box_collider in <&mut BoxCollider>::query().iter_mut(&mut application.ecs) {
            box_collider.visible = false;
        }
        if let Some(entity) = application.pick_object(f32::MAX)? {
            if let Some(mut entry) = application.ecs.entry(entity) {
                let mut box_collider = entry.get_component_mut::<BoxCollider>()?;
                box_collider.visible = true;
            }
        }
        Ok(())
    }

    fn clear_colliders(application: &mut Application) {
        let entities = <(Entity, &BoxCollider)>::query()
            .iter_mut(&mut application.ecs)
            .map(|(entity, _)| *entity)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            if let Some(mut entry) = application.ecs.entry(entity) {
                entry.remove_component::<BoxCollider>();
            }
        }
    }

    fn add_required_components(&mut self, application: &mut Application) {
        let entities = <Entity>::query()
            .iter(&mut application.ecs)
            .map(|entity| *entity)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            if let Some(mut entry) = application.ecs.entry(entity) {
                let has_selection_component = entry.get_component::<Selection>().is_ok();
                if !has_selection_component {
                    entry.add_component(Selection(false));
                }
                let has_visibility_component = entry.get_component::<Visibility>().is_ok();
                if !has_visibility_component {
                    entry.add_component(Visibility(true));
                }
            }
        }
    }
}

impl ApplicationRunner for Editor {
    fn create_ui(&mut self, application: &mut Application, ui: &Ui) -> Result<()> {
        ui.text(im_str!("placeholder text"));
        let number_of_meshes = <&Mesh>::query().iter(&application.ecs).count();
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
        for (index, (entity, camera)) in <(Entity, &Camera)>::query()
            .iter(&mut application.ecs)
            .enumerate()
        {
            let label = if camera.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let clicked = ui.small_button(&im_str!("{} #{} [{}]", camera.name, index, label));
            if change_camera.is_none() && clicked {
                change_camera = Some(*entity);
            }
        }

        if let Some(selected_camera_entity) = change_camera {
            for (entity, camera) in <(Entity, &mut Camera)>::query().iter_mut(&mut application.ecs)
            {
                camera.enabled = *entity == selected_camera_entity;
            }
        }

        ui.separator();
        ui.text(im_str!("Selected Entities"));
        for (entity, _) in <(Entity, &Selection)>::query().iter(&application.ecs) {
            ui.text(im_str!("{:#?}", entity));
        }

        Ok(())
    }

    fn update(&mut self, application: &mut Application) -> Result<()> {
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
                application.world.clear(&mut application.ecs);
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
                Some("glb") | Some("gltf") => self.load_gltf(raw_path, application)?,
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

            let already_selected = {
                match application.ecs.entry(entity) {
                    Some(entry) => {
                        let already_selected = entry.get_component::<Selection>()?.is_selected();
                        let shift_active = application.input.is_key_pressed(VirtualKeyCode::LShift);
                        if !shift_active {
                            for selection in
                                <&mut Selection>::query().iter_mut(&mut application.ecs)
                            {
                                selection.0 = false;
                            }
                        }
                        already_selected
                    }
                    None => false,
                }
            };

            if let Some(mut entry) = application.ecs.entry(entity) {
                entry.get_component_mut::<Selection>()?.0 = !already_selected;
            }
        }
        Ok(())
    }
}
