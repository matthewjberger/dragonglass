use crate::{
    vulkan::{
        scene::Scene,
        world::{Light, WorldPipelineData, WorldUniformBuffer},
    },
    Render,
};
use anyhow::Result;
use dragonglass_physics::PhysicsWorld;
use dragonglass_vulkan::{
    ash::{version::DeviceV1_0, vk},
    core::{CommandPool, Context, Frame},
};
use dragonglass_world::{Camera, Ecs, PerspectiveCamera, World};
use imgui::{Context as ImguiContext, DrawData};
use log::{error, info};
use nalgebra_glm as glm;
use ncollide3d::world::CollisionWorld;
use raw_window_handle::HasRawWindowHandle;
use shader_compilation::compile_shaders;
use std::sync::Arc;

pub struct VulkanRenderBackend {
    _command_pool: CommandPool,
    frame: Frame,
    scene: Scene,
    context: Arc<Context>,
}

impl VulkanRenderBackend {
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

impl Render for VulkanRenderBackend {
    fn load_skybox(&mut self, path: &str) -> Result<()> {
        self.scene.load_skybox(&self.context, path)
    }

    fn load_world(&mut self, world: &World) -> Result<()> {
        self.scene.load_world(&self.context, world)?;
        Ok(())
    }

    fn reload_asset_shaders(&mut self) -> Result<()> {
        self.scene
            .shader_cache
            .shaders
            .remove("assets/shaders/model/model.vert.spv");
        self.scene
            .shader_cache
            .shaders
            .remove("assets/shaders/model/model.frag.spv");
        if compile_shaders("assets/shaders/model/*.glsl").is_err() {
            error!("Failed to recompile asset shaders!");
            return Ok(());
        }
        unsafe { self.context.device.handle.device_wait_idle() }?;
        self.scene.create_pipelines(&self.context)?;
        info!("Reloaded shaders!");
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

                let mut lights = [Light::default(); WorldPipelineData::MAX_NUMBER_OF_LIGHTS];
                let world_lights = world
                    .lights(ecs)?
                    .iter()
                    .map(|(transform, light)| Light::from_node(transform, light))
                    .collect::<Vec<_>>();
                let number_of_lights = world_lights.len() as u32;
                lights
                    .iter_mut()
                    .zip(world_lights)
                    .for_each(|(a, b)| *a = b);

                let mut joint_matrices =
                    [glm::Mat4::identity(); WorldPipelineData::MAX_NUMBER_OF_JOINTS];
                joint_matrices
                    .iter_mut()
                    .zip(world.joint_matrices(ecs)?.into_iter())
                    .for_each(|(a, b)| *a = b);

                let ubo = WorldUniformBuffer {
                    view,
                    projection,
                    camera_position: camera_transform.translation,
                    number_of_lights,
                    lights,
                    joint_matrices,
                };
                world_render
                    .pipeline_data
                    .uniform_buffer
                    .upload_data(&[ubo], 0)?;
            }

            scene.skybox_render.projection = skybox_projection;
            scene.skybox_render.view = view;

            scene.rendergraph.execute_pass(
                command_buffer,
                "offscreen",
                image_index,
                |pass, command_buffer| {
                    device.update_viewport(command_buffer, pass.extent, true)?;
                    scene.skybox_render.issue_commands(command_buffer)?;
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

impl Drop for VulkanRenderBackend {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
