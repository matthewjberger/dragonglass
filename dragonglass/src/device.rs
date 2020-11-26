use crate::{
    asset::{AssetUniformBuffer, PipelineData},
    core::{CommandPool, Context, Frame},
    scene::Scene,
};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use dragonglass_scene::Asset;
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::{path::Path, sync::Arc};

pub struct RenderingDevice {
    _command_pool: CommandPool,
    frame: Frame,
    scene: Scene,
    context: Arc<Context>,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let frame = Frame::new(context.clone(), dimensions, Self::MAX_FRAMES_IN_FLIGHT)?;
        let scene = Scene::new(&context, frame.swapchain()?, &frame.swapchain_properties)?;
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(context.device.clone(), create_info)?;
        let renderer = Self {
            _command_pool: command_pool,
            frame,
            scene,
            context,
        };
        Ok(renderer)
    }

    pub fn load_skybox(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.scene.load_skybox(&self.context, path)
    }

    pub fn load_asset(&mut self, asset: &Asset) -> Result<()> {
        self.scene.load_asset(&self.context, asset)?;
        Ok(())
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        view: glm::Mat4,
        camera_position: glm::Vec3,
        asset: &Option<Asset>,
    ) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let device = self.context.device.clone();

        frame.render(dimensions, |command_buffer, image_index| {
            if let Some(asset) = asset.as_ref() {
                if let Some(asset_rendering) = scene.asset_rendering.as_ref() {
                    asset_rendering.pipeline_data.update_dynamic_ubo(asset)?;
                    let projection =
                        glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);

                    let mut camera_position = glm::vec3_to_vec4(&camera_position);
                    camera_position.w = 1.0;

                    let mut joint_matrices =
                        [glm::Mat4::identity(); PipelineData::MAX_NUMBER_OF_JOINTS];
                    joint_matrices
                        .iter_mut()
                        .zip(asset.joint_matrices()?.into_iter())
                        .for_each(|(a, b)| *a = b);

                    let mut morph_targets =
                        [glm::Vec4::identity(); PipelineData::MAX_NUMBER_OF_MORPH_TARGETS];
                    morph_targets
                        .iter_mut()
                        .zip(asset.morph_targets()?.into_iter())
                        .for_each(|(a, b)| *a = b);

                    let mut morph_target_weights =
                        [0.0; PipelineData::MAX_NUMBER_OF_MORPH_TARGET_WEIGHTS];
                    morph_target_weights
                        .iter_mut()
                        .zip(asset.morph_target_weights()?.into_iter())
                        .for_each(|(a, b)| *a = b);

                    let ubo = AssetUniformBuffer {
                        view,
                        projection,
                        camera_position,
                        joint_matrices,
                        morph_targets,
                        morph_target_weights,
                    };
                    asset_rendering
                        .pipeline_data
                        .uniform_buffer
                        .upload_data(&[ubo], 0)?;
                }
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
                        if let Some(asset) = asset.as_ref() {
                            asset_rendering.issue_commands(command_buffer, asset)?;
                        }
                    }
                    Ok(())
                },
            )?;

            scene.rendergraph.execute_pass(
                command_buffer,
                "fullscreen",
                image_index,
                |pass, command_buffer| {
                    device.update_viewport(command_buffer, pass.extent, false)?;
                    if let Some(fullscreen_pipeline) = scene.fullscreen_pipeline.as_ref() {
                        fullscreen_pipeline.issue_commands(command_buffer)?;
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
