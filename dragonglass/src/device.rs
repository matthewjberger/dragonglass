use super::{
    core::{Context, LogicalDevice},
    forward::ForwardSwapchain,
    render::{
        Buffer, CommandPool, DescriptorSetLayout, Fence, GraphicsPipeline,
        GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass, Semaphore, ShaderCache,
        ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, bail, Result};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct RenderingDevice {
    triangle: Option<TriangleRendering>,
    shader_cache: ShaderCache,
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    transient_command_pool: CommandPool,
    forward_swapchain: Option<ForwardSwapchain>,
    context: Arc<Context>,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let device = context.logical_device.clone();
        let frame_locks = Self::create_frame_locks(device.clone())?;

        let command_pool = Self::create_command_pool(
            context.logical_device.clone(),
            context.physical_device.graphics_queue_index,
        )?;

        let transient_command_pool = Self::create_transient_command_pool(
            context.logical_device.clone(),
            context.physical_device.graphics_queue_index,
        )?;

        let forward_swapchain = ForwardSwapchain::new(context.clone(), dimensions)?;

        let command_buffers = command_pool.allocate_command_buffers(
            forward_swapchain.framebuffers.len() as _,
            vk::CommandBufferLevel::PRIMARY,
        )?;

        let mut shader_cache = ShaderCache::default();

        let triangle = TriangleRendering::new(
            context.clone(),
            &transient_command_pool,
            forward_swapchain.render_pass.clone(),
            &mut shader_cache,
        )?;

        let renderer = Self {
            triangle: Some(triangle),
            shader_cache,
            frame: 0,
            frame_locks,
            command_buffers,
            _command_pool: command_pool,
            transient_command_pool,
            forward_swapchain: Some(forward_swapchain),
            context,
        };
        Ok(renderer)
    }

    fn create_frame_locks(device: Arc<LogicalDevice>) -> Result<Vec<FrameLock>> {
        (0..Self::MAX_FRAMES_IN_FLIGHT)
            .map(|_| FrameLock::new(device.clone()))
            .collect()
    }

    fn create_command_pool(device: Arc<LogicalDevice>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = CommandPool::new(device, create_info)?;
        Ok(command_pool)
    }

    fn create_transient_command_pool(
        device: Arc<LogicalDevice>,
        queue_index: u32,
    ) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(device, create_info)?;
        Ok(command_pool)
    }

    fn forward_swapchain(&self) -> Result<&ForwardSwapchain> {
        self.forward_swapchain
            .as_ref()
            .ok_or_else(|| anyhow!("No forward swapchain was available for rendering!"))
    }

    fn device(&self) -> ash::Device {
        self.context.logical_device.handle.clone()
    }

    pub fn render(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            self.record_command_buffer(image_index)?;
            self.submit_command_buffer(image_index)?;
            let result = self.present_next_frame(image_index)?;
            self.check_presentation_result(result, dimensions)?;
            self.frame = (1 + self.frame) % Self::MAX_FRAMES_IN_FLIGHT;
        }
        Ok(())
    }

    fn reset_in_flight_fence(&self) -> Result<()> {
        let in_flight_fence = self.frame_lock()?.in_flight.handle;
        unsafe { self.device().reset_fences(&[in_flight_fence]) }?;
        Ok(())
    }

    fn wait_for_in_flight_fence(&self) -> Result<()> {
        let fence = self.frame_lock()?.in_flight.handle;
        unsafe { self.device().wait_for_fences(&[fence], true, std::u64::MAX) }?;
        Ok(())
    }

    fn acquire_next_frame(&mut self, dimensions: &[u32; 2]) -> Result<Option<usize>> {
        let result = self
            .forward_swapchain()?
            .swapchain
            .acquire_next_image(self.frame_lock()?.image_available.handle, vk::Fence::null());

        match result {
            Ok((image_index, _)) => Ok(Some(image_index as usize)),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
                Ok(None)
            }
            Err(error) => bail!(error),
        }
    }

    fn present_next_frame(&mut self, image_index: usize) -> Result<VkResult<bool>> {
        let wait_semaphores = [self.frame_lock()?.render_finished.handle];
        let swapchains = [self.forward_swapchain()?.swapchain.handle_khr];
        let image_indices = [image_index as u32];

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let presentation_result = unsafe {
            self.forward_swapchain()?
                .swapchain
                .handle_ash
                .queue_present(self.context.presentation_queue(), &present_info)
        };

        Ok(presentation_result)
    }

    fn check_presentation_result(
        &mut self,
        presentation_result: VkResult<bool>,
        dimensions: &[u32; 2],
    ) -> Result<()> {
        match presentation_result {
            Ok(is_suboptimal) if is_suboptimal => {
                self.create_swapchain(dimensions)?;
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
            }
            Err(error) => bail!(error),
            _ => {}
        };
        Ok(())
    }

    fn create_swapchain(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        if dimensions[0] == 0 || dimensions[1] == 0 {
            return Ok(());
        }

        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.forward_swapchain = None;
        self.forward_swapchain = Some(ForwardSwapchain::new(self.context.clone(), dimensions)?);

        self.triangle = None;
        let triangle = TriangleRendering::new(
            self.context.clone(),
            &self.transient_command_pool,
            self.forward_swapchain()?.render_pass.clone(),
            &mut self.shader_cache,
        )?;
        self.triangle = Some(triangle);

        Ok(())
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let command_buffer = *self.command_buffers.get(image_index).ok_or_else(|| {
            anyhow!(
                "No command buffer was found for the forward swapchain at image index: {}",
                image_index
            )
        })?;

        self.context.logical_device.clone().record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::SIMULTANEOUS_USE,
            || {
                self.record_render_pass(command_buffer, image_index)?;
                Ok(())
            },
        )?;

        Ok(())
    }

    fn record_render_pass(
        &self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        let extent = self.forward_swapchain()?.swapchain_properties.extent;
        let clear_values = Self::clear_values();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.forward_swapchain()?.render_pass.handle)
            .framebuffer(self.framebuffer_at(image_index)?)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&clear_values);

        RenderPass::record(
            self.context.logical_device.clone(),
            command_buffer,
            begin_info,
            || {
                self.update_viewport(command_buffer)?;
                if let Some(triangle) = self.triangle.as_ref() {
                    triangle.issue_commands(command_buffer)?;
                }
                Ok(())
            },
        )?;

        Ok(())
    }

    fn update_viewport(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let extent = self.forward_swapchain()?.swapchain_properties.extent;

        let viewport = vk::Viewport {
            x: 0.0,
            y: extent.height as _,
            width: extent.width as _,
            height: (-1.0 * extent.height as f32) as _,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let viewports = [viewport];

        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let scissors = [scissor];

        let device = self.context.logical_device.handle.clone();
        unsafe {
            device.cmd_set_viewport(command_buffer, 0, &viewports);
            device.cmd_set_scissor(command_buffer, 0, &scissors);
        }

        Ok(())
    }

    fn framebuffer_at(&self, image_index: usize) -> Result<vk::Framebuffer> {
        let framebuffer = self
            .forward_swapchain()?
            .framebuffers
            .get(image_index)
            .ok_or_else(|| {
                anyhow!(
                    "No framebuffer was found for the forward swapchain at image index: {}",
                    image_index
                )
            })?
            .handle;
        Ok(framebuffer)
    }

    fn command_buffer_at(&self, image_index: usize) -> Result<vk::CommandBuffer> {
        let command_buffer = *self.command_buffers.get(image_index).ok_or_else(|| {
            anyhow!(
                "No command buffer was found at image index: {}",
                image_index
            )
        })?;
        Ok(command_buffer)
    }

    fn frame_lock(&self) -> Result<&FrameLock> {
        let lock = &self
            .frame_locks
            .get(self.frame)
            .ok_or_else(|| anyhow!("No frame lock was found at frame index: {}", self.frame,))?;
        Ok(lock)
    }

    fn clear_values() -> Vec<vk::ClearValue> {
        let color = vk::ClearColorValue {
            float32: [0.39, 0.58, 0.93, 1.0], // Cornflower blue
        };
        let depth_stencil = vk::ClearDepthStencilValue {
            depth: 1.0,
            stencil: 0,
        };
        vec![vk::ClearValue { color }, vk::ClearValue { depth_stencil }]
    }

    fn submit_command_buffer(&self, image_index: usize) -> Result<()> {
        let lock = self.frame_lock()?;
        let image_available_semaphores = [lock.image_available.handle];
        let wait_semaphores = [lock.render_finished.handle];
        let command_buffers = [self.command_buffer_at(image_index)?];

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&image_available_semaphores)
            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(&command_buffers)
            .signal_semaphores(&wait_semaphores);

        unsafe {
            self.context.logical_device.handle.queue_submit(
                self.context.graphics_queue(),
                &[submit_info.build()],
                lock.in_flight.handle,
            )
        }?;

        Ok(())
    }
}

impl Drop for RenderingDevice {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.logical_device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}

pub struct FrameLock {
    pub image_available: Semaphore,
    pub render_finished: Semaphore,
    pub in_flight: Fence,
}

impl FrameLock {
    pub fn new(device: Arc<LogicalDevice>) -> Result<Self> {
        let handles = Self {
            image_available: Semaphore::new(device.clone())?,
            render_finished: Semaphore::new(device.clone())?,
            in_flight: Fence::new(device, vk::FenceCreateFlags::SIGNALED)?,
        };
        Ok(handles)
    }
}

pub struct UniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    pub model: glm::Mat4,
}

pub struct TriangleRendering {
    pub pipeline: GraphicsPipeline,
    pub pipeline_layout: PipelineLayout,
    pub vertex_buffer: Buffer,
    number_of_vertices: usize,
    device: Arc<LogicalDevice>,
}

impl TriangleRendering {
    pub fn new(
        context: Arc<Context>,
        pool: &CommandPool,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder().build();
        let descriptor_set_layout =
            DescriptorSetLayout::new(context.logical_device.clone(), create_info)?;
        let descriptor_set_layout = Arc::new(descriptor_set_layout);

        let shader_paths = ShaderPathSetBuilder::default()
            .vertex("dragonglass/shaders/triangle/triangle.vert.spv")
            .fragment("dragonglass/shaders/triangle/triangle.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        let device = context.logical_device.clone();
        let shader_set = shader_cache.create_shader_set(device.clone(), &shader_paths)?;

        let descriptions = Self::vertex_input_descriptions();
        let attributes = Self::vertex_attributes();
        let vertex_state_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&descriptions)
            .vertex_attribute_descriptions(&attributes)
            .build();

        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_state_info(vertex_state_info)
            .descriptor_set_layout(descriptor_set_layout)
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        let (pipeline, pipeline_layout) =
            GraphicsPipeline::from_settings(device.clone(), settings)?;

        #[rustfmt::skip]
        let vertices: [f32; 15] = [
           -0.5,  -0.5, 1.0, 0.0, 0.0,
            0.0,  0.5, 0.0, 1.0, 0.0,
            0.5,  -0.5, 0.0, 0.0, 1.0,
        ];
        let number_of_vertices = vertices.len();

        let vertex_buffer = pool.new_gpu_buffer(
            &vertices,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            context.allocator.clone(),
            context.graphics_queue(),
        )?;

        let rendering = Self {
            pipeline,
            pipeline_layout,
            vertex_buffer,
            number_of_vertices,
            device,
        };

        Ok(rendering)
    }

    pub fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 2] {
        let position_description = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        let color_description = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset((std::mem::size_of::<f32>() * 2) as _)
            .build();

        [position_description, color_description]
    }

    pub fn vertex_input_descriptions() -> [vk::VertexInputBindingDescription; 1] {
        let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride((5 * std::mem::size_of::<f32>()) as _)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        self.pipeline.bind(&self.device.handle, command_buffer);

        let offsets = [0];
        let vertex_buffers = [self.vertex_buffer.handle];
        unsafe {
            self.device.handle.cmd_bind_vertex_buffers(
                command_buffer,
                0,
                &vertex_buffers,
                &offsets,
            );

            self.device
                .handle
                .cmd_draw(command_buffer, self.number_of_vertices as _, 1, 0, 0)
        };

        Ok(())
    }
}
