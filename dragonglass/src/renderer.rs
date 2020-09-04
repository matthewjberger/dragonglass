use super::{
    core::{Context, LogicalDevice},
    forward::ForwardSwapchain,
    resource::{CommandPool, Fence, Semaphore},
};
use anyhow::Result;
use ash::vk;
use log::info;
use raw_window_handle::RawWindowHandle;
use std::sync::Arc;

pub struct Renderer {
    frame_locks: Vec<FrameLock>,
    command_pool: CommandPool,
    transient_command_pool: CommandPool,
    forward_swapchain: ForwardSwapchain,
    context: Arc<Context>,
}

impl Renderer {
    const MAX_FRAMES_IN_FLIGHT: u32 = 2;

    pub fn new(raw_window_handle: &RawWindowHandle, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(&raw_window_handle)?);

        let frame_locks = (0..Self::MAX_FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| FrameLock::new(context.logical_device.clone()))
            .collect::<Result<Vec<FrameLock>, anyhow::Error>>()?;

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = CommandPool::new(context.logical_device.clone(), create_info)?;

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let transient_command_pool = CommandPool::new(context.logical_device.clone(), create_info)?;

        let forward_swapchain = ForwardSwapchain::new(context.clone(), dimensions)?;

        let renderer = Self {
            frame_locks,
            command_pool,
            transient_command_pool,
            forward_swapchain,
            context,
        };

        Ok(renderer)
    }

    pub fn initialize(&mut self) -> Result<()> {
        info!("Initializing renderer");
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        Ok(())
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
            in_flight: Fence::new(device.clone(), vk::FenceCreateFlags::SIGNALED)?,
        };
        Ok(handles)
    }
}
