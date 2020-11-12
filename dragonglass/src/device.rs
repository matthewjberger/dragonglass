use crate::{adapters::CommandPool, context::Context, frame::Frame, gltf::Asset, scene::Scene};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc};

pub struct RenderingDevice {
    _command_pool: CommandPool,
    frame: Frame,
    asset: Option<Rc<RefCell<Asset>>>,
    scene: Scene,
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
        let scene = Scene::new(&context, frame.swapchain()?, &frame.swapchain_properties)?;
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
        self.asset = None;
        let asset = Rc::new(RefCell::new(Asset::new(path)?));
        self.scene.load_asset(&self.context, asset.clone())?;
        self.asset = Some(asset);
        Ok(())
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        view: glm::Mat4,
        camera_position: glm::Vec3,
        delta_time: f32,
    ) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let device = self.context.device.clone();

        frame.render(dimensions, |command_buffer, image_index| {
            if let Some(asset) = scene.asset_rendering.as_mut() {
                asset.update_ubo(aspect_ratio, view, camera_position, delta_time)?;
            }

            // TODO: This is decoupled from scene projection matrix for now
            let projection =
                glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);
            scene.skybox_rendering.projection = projection;
            scene.skybox_rendering.view = view;

            scene.rendergraph.execute_pass(
                command_buffer,
                "offscreen",
                image_index,
                |pass, command_buffer| {
                    device.update_viewport(command_buffer, pass.extent, true)?;
                    scene.skybox_rendering.issue_commands(command_buffer)?;
                    if let Some(asset_rendering) = scene.asset_rendering.as_ref() {
                        asset_rendering.issue_commands(command_buffer)?;
                    }
                    Ok(())
                },
            )?;

            scene.rendergraph.execute_pass(
                command_buffer,
                "postprocessing",
                image_index,
                |pass, command_buffer| {
                    device.update_viewport(command_buffer, pass.extent, false)?;
                    if let Some(post_processing_pipeline) = scene.post_processing_pipeline.as_ref()
                    {
                        post_processing_pipeline.issue_commands(command_buffer)?;
                    }
                    Ok(())
                },
            )?;

            Ok(())
        })?;

        if frame.recreated_swapchain {
            let rendergraph = Scene::create_rendergraph(
                &self.context,
                frame.swapchain()?,
                &frame.swapchain_properties,
                scene.samples,
            )?;
            scene.rendergraph = rendergraph;
            scene.create_pipelines(&self.context)?;
        }

        Ok(())
    }

    pub fn toggle_wireframe(&mut self) {
        if let Some(asset_rendering) = self.scene.asset_rendering.as_mut() {
            asset_rendering.wireframe_enabled = !asset_rendering.wireframe_enabled;
        }
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
