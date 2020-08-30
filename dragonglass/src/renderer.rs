use super::{
    command_pool::CommandPool,
    core::{Context, LogicalDevice},
    sync::{Fence, Semaphore},
};
use anyhow::Result;
use ash::vk;
use log::info;
use raw_window_handle::RawWindowHandle;
use std::sync::Arc;

pub struct FrameSyncHandles {
    pub image_available: Semaphore,
    pub render_finished: Semaphore,
    pub in_flight: Fence,
}

impl FrameSyncHandles {
    pub fn new(device: Arc<LogicalDevice>) -> Result<Self> {
        let handles = Self {
            image_available: Semaphore::new(device.clone())?,
            render_finished: Semaphore::new(device.clone())?,
            in_flight: Fence::new(device.clone(), vk::FenceCreateFlags::SIGNALED)?,
        };
        Ok(handles)
    }
}

pub struct Renderer {
    context: Context,
    sync_handles: Vec<FrameSyncHandles>,
    command_pool: CommandPool,
    transient_command_pool: CommandPool,
}

impl Renderer {
    const MAX_FRAMES_IN_FLIGHT: u32 = 2;

    pub fn new(raw_window_handle: &RawWindowHandle) -> Result<Self> {
        let context = Context::new(&raw_window_handle)?;
        let sync_handles = (0..Self::MAX_FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|frame| FrameSyncHandles::new(context.logical_device.clone()))
            .collect::<Result<Vec<FrameSyncHandles>, anyhow::Error>>()?;

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = CommandPool::new(context.logical_device.clone(), create_info)?;

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let transient_command_pool = CommandPool::new(context.logical_device.clone(), create_info)?;

        let renderer = Self {
            context,
            sync_handles,
            command_pool,
            transient_command_pool,
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
