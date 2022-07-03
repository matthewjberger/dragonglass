mod input;
mod system;

pub use self::{input::*, system::*};

use anyhow::Result;
use dragonglass_config::Config;
use dragonglass_gui::Gui;
use dragonglass_render::Renderer;
use dragonglass_world::{load_gltf, MouseRayConfiguration, World};
use nalgebra_glm as glm;
use winit::{
    dpi::PhysicalPosition,
    window::{Fullscreen, Window},
};

// TODO: Don't include renderer (or world) in this
pub struct Resources<'a> {
    pub config: &'a mut Config,
    pub window: &'a mut Window,
    pub input: &'a mut Input,
    pub system: &'a mut System,
    pub gui: &'a mut Gui,
    pub renderer: &'a mut Box<dyn Renderer>,
    pub world: &'a mut World,
}

impl<'a> Resources<'a> {
    pub fn set_cursor_grab(&mut self, grab: bool) -> Result<()> {
        Ok(self.window.set_cursor_grab(grab)?)
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.window.set_cursor_visible(visible)
    }

    pub fn center_cursor(&mut self) -> Result<()> {
        Ok(self.set_cursor_position(&self.system.window_center())?)
    }

    pub fn set_cursor_position(&mut self, position: &glm::Vec2) -> Result<()> {
        Ok(self
            .window
            .set_cursor_position(PhysicalPosition::new(position.x, position.y))?)
    }

    pub fn set_fullscreen(&mut self) {
        self.window
            .set_fullscreen(Some(Fullscreen::Borderless(self.window.primary_monitor())));
    }

    pub fn mouse_ray_configuration(&self) -> Result<MouseRayConfiguration> {
        let viewport = self.renderer.viewport();

        let (projection, view) = self.world.active_camera_matrices(viewport.aspect_ratio())?;

        let mouse_ray_configuration = MouseRayConfiguration {
            viewport,
            projection_matrix: projection,
            view_matrix: view,
            mouse_position: self.input.mouse.position,
        };

        Ok(mouse_ray_configuration)
    }

    pub fn load_asset(&mut self, path: &str) -> Result<()> {
        load_gltf(path, &mut self.world)?;
        self.renderer.load_world(&self.world)?;
        Ok(())
    }
}
