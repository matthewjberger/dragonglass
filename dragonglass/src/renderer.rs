use crate::{
    core::{CommandPool, Context, Frame},
    scene::Scene,
    world::{WorldPipelineData, WorldUniformBuffer},
};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use dragonglass_scene::World;
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::{path::Path, sync::Arc};

pub struct Renderer {
    _command_pool: CommandPool,
    frame: Frame,
    scene: Scene,
    context: Arc<Context>,
}

impl Renderer {
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

    pub fn load_world(&mut self, world: &World) -> Result<()> {
        self.scene.load_world(&self.context, world)?;
        Ok(())
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        view: glm::Mat4,
        camera_position: glm::Vec3,
        world: &World,
    ) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let device = self.context.device.clone();

        frame.render(dimensions, |command_buffer, image_index| {
            let projection =
                glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);

            if let Some(world_render) = scene.world_render.as_ref() {
                world_render.pipeline_data.update_dynamic_ubo(world)?;

                let mut camera_position = glm::vec3_to_vec4(&camera_position);
                camera_position.w = 1.0;

                let mut joint_matrices =
                    [glm::Mat4::identity(); WorldPipelineData::MAX_NUMBER_OF_JOINTS];
                joint_matrices
                    .iter_mut()
                    .zip(world.joint_matrices()?.into_iter())
                    .for_each(|(a, b)| *a = b);

                let mut morph_targets =
                    [glm::Vec4::identity(); WorldPipelineData::MAX_NUMBER_OF_MORPH_TARGETS];
                morph_targets
                    .iter_mut()
                    .zip(world.morph_targets()?.into_iter())
                    .for_each(|(a, b)| *a = b);

                let mut morph_target_weights =
                    [0.0; WorldPipelineData::MAX_NUMBER_OF_MORPH_TARGET_WEIGHTS];
                morph_target_weights
                    .iter_mut()
                    .zip(world.morph_target_weights()?.into_iter())
                    .for_each(|(a, b)| *a = b);

                let ubo = WorldUniformBuffer {
                    view,
                    projection,
                    camera_position,
                    joint_matrices,
                    morph_targets,
                    morph_target_weights,
                };
                world_render
                    .pipeline_data
                    .uniform_buffer
                    .upload_data(&[ubo], 0)?;
            }

            scene.skybox_rendering.projection = projection;
            scene.skybox_rendering.view = view;

            scene.rendergraph.execute_pass(
                command_buffer,
                "offscreen",
                image_index,
                |pass, command_buffer| {
                    device.update_viewport(command_buffer, pass.extent, true)?;
                    scene.skybox_rendering.issue_commands(command_buffer)?;
                    if let Some(world_render) = scene.world_render.as_ref() {
                        world_render.issue_commands(command_buffer, world)?;
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
        if let Some(world_render) = self.scene.world_render.as_mut() {
            world_render.wireframe_enabled = !world_render.wireframe_enabled;
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
