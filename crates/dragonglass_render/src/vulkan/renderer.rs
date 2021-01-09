use crate::{
    vulkan::{
        core::{CommandPool, Context, Frame},
        scene::Scene,
        world::{WorldPipelineData, WorldUniformBuffer},
    },
    Renderer,
};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use dragonglass_physics::PhysicsWorld;
use dragonglass_world::{Camera, Ecs, PerspectiveCamera, World};
use imgui::{Context as ImguiContext, DrawData};
use log::error;
use nalgebra_glm as glm;
use ncollide3d::world::CollisionWorld;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct VulkanRenderer {
    _command_pool: CommandPool,
    frame: Frame,
    scene: Scene,
    context: Arc<Context>,
}

impl VulkanRenderer {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
        imgui: &mut ImguiContext,
    ) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let frame = Frame::new(context.clone(), dimensions, Self::MAX_FRAMES_IN_FLIGHT)?;
        let scene = Scene::new(
            &context,
            imgui,
            frame.swapchain()?,
            &frame.swapchain_properties,
        )?;
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_family_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(
            context.device.clone(),
            context.graphics_queue(),
            create_info,
        )?;
        let renderer = Self {
            _command_pool: command_pool,
            frame,
            scene,
            context,
        };
        Ok(renderer)
    }
}

impl Renderer for VulkanRenderer {
    fn load_skybox(&mut self, path: &str) -> Result<()> {
        self.scene.load_skybox(&self.context, path)
    }

    fn load_world(&mut self, world: &World) -> Result<()> {
        self.scene.load_world(&self.context, world)?;
        Ok(())
    }

    fn render(
        &mut self,
        dimensions: &[u32; 2],
        ecs: &mut Ecs,
        world: &World,
        physics_world: &PhysicsWorld,
        collision_world: &CollisionWorld<f32, ()>,
        draw_data: &DrawData,
    ) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let device = self.context.device.clone();

        // FIXME: Don't reallocate gui geometry buffers each frame...
        scene.gui_render.resize_geometry_buffer(
            self.context.allocator.clone(),
            &scene.transient_command_pool,
            draw_data,
        )?;

        let (projection, view) = world.active_camera_matrices(ecs, aspect_ratio)?;
        let camera_entity = world.active_camera(ecs)?;
        let camera_transform = world.entity_global_transform(ecs, camera_entity)?;
        let mut camera_position = glm::vec3_to_vec4(&camera_transform.translation);
        camera_position.w = 1.0;

        // Maintain a perspective projection for the skybox
        let using_ortho_projection = ecs.get::<Camera>(camera_entity)?.is_orthographic();
        let skybox_projection = if using_ortho_projection {
            let camera = PerspectiveCamera {
                aspect_ratio: None,
                y_fov_rad: 70_f32.to_radians(),
                z_far: Some(1000.0),
                z_near: 0.01,
            };
            camera.matrix(aspect_ratio)
        } else {
            projection
        };

        frame.render(dimensions, |command_buffer, image_index| {
            if let Some(world_render) = scene.world_render.as_mut() {
                world_render.pipeline_data.update_dynamic_ubo(world, ecs)?;

                let mut joint_matrices =
                    [glm::Mat4::identity(); WorldPipelineData::MAX_NUMBER_OF_JOINTS];
                joint_matrices
                    .iter_mut()
                    .zip(world.joint_matrices(ecs)?.into_iter())
                    .for_each(|(a, b)| *a = b);

                let ubo = WorldUniformBuffer {
                    view,
                    projection,
                    camera_position,
                    joint_matrices,
                };
                world_render
                    .pipeline_data
                    .uniform_buffer
                    .upload_data(&[ubo], 0)?;
            }

            scene.skybox_rendering.projection = skybox_projection;
            scene.skybox_rendering.view = view;

            scene.rendergraph.execute_pass(
                command_buffer,
                "offscreen",
                image_index,
                |pass, command_buffer| {
                    device.update_viewport(command_buffer, pass.extent, true)?;
                    scene.skybox_rendering.issue_commands(command_buffer)?;
                    if let Some(world_render) = scene.world_render.as_ref() {
                        world_render.issue_commands(
                            command_buffer,
                            ecs,
                            world,
                            physics_world,
                            collision_world,
                            aspect_ratio,
                        )?;
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
                    scene.gui_render.issue_commands(command_buffer, draw_data)?;
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

    fn toggle_wireframe(&mut self) {
        if let Some(world_render) = self.scene.world_render.as_mut() {
            world_render.wireframe_enabled = !world_render.wireframe_enabled;
        }
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
