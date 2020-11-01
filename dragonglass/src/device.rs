use crate::{adapters::CommandPool, context::Context, frame::Frame, gltf::Asset, scene::Scene};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::{path::Path, sync::Arc};

pub struct RenderingDevice {
    _command_pool: CommandPool,
    frame: Frame,
    pub asset: Option<Arc<Asset>>,
    scene: Option<Scene>,
    context: Arc<Context>,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        log::debug!(
            "Physical Device Properties: {:#?}",
            context.physical_device_properties()
        );
        let frame = Frame::new(context.clone(), dimensions, Self::MAX_FRAMES_IN_FLIGHT)?;
        let scene = Some(Scene::new(
            &context,
            frame.swapchain()?,
            &frame.swapchain_properties,
        )?);
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(context.device.clone(), create_info)?;
        let renderer = Self {
            _command_pool: command_pool,
            frame,
            asset: None,
            scene,
            context,
        };
        Ok(renderer)
    }

    pub fn load_asset<P>(&mut self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        match self.scene.as_mut() {
            Some(scene) => {
                self.asset = None;
                let asset = Arc::new(Asset::new(path)?);
                scene.load_asset(&self.context, asset.clone())?;
                self.asset = Some(asset);
            }
            None => {
                log::warn!("No scene was available to load the asset into!");
            }
        }
        Ok(())
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        view: &glm::Mat4,
        _camera_position: &glm::Vec3,
    ) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let device = self.context.device.clone();

        frame.render(dimensions, |command_buffer, image_index| {
            if let Some(scene) = scene.as_ref() {
                if let Some(asset) = scene.asset.as_ref() {
                    asset.borrow().update_ubo(aspect_ratio, *view)?
                };
                scene
                    .rendergraph
                    .execute_at_index(device.clone(), command_buffer, image_index)?;
            }
            Ok(())
        })?;

        if frame.recreated_swapchain {
            self.scene = None;
            let mut scene = Scene::new(
                &self.context,
                frame.swapchain()?,
                &frame.swapchain_properties,
            )?;
            if let Some(asset) = self.asset.as_ref() {
                scene.load_asset(&self.context, asset.clone())?;
            }
            self.scene = Some(scene);
        }

        Ok(())
    }
}

impl Drop for RenderingDevice {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
