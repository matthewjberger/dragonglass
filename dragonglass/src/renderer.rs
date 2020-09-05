use super::{
    core::{Context, LogicalDevice},
    forward::ForwardSwapchain,
    resource::{CommandPool, Fence, Semaphore},
};
use anyhow::{anyhow, bail, Result};
use ash::{version::DeviceV1_0, vk};
use log::info;
use raw_window_handle::RawWindowHandle;
use std::sync::Arc;

pub struct Renderer {
    current_frame: usize,
    frame_locks: Vec<FrameLock>,
    command_pool: CommandPool,
    transient_command_pool: CommandPool,
    forward_swapchain: Option<ForwardSwapchain>,
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

        let forward_swapchain = Some(ForwardSwapchain::new(context.clone(), dimensions)?);

        let renderer = Self {
            current_frame: 0,
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

    pub fn render(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        let frame_lock = &self.frame_locks[self.current_frame];

        // unsafe {
        //     self.context.logical_device.handle.wait_for_fences(
        //         &[frame_lock.in_flight.handle],
        //         true,
        //         std::u64::MAX,
        //     )
        // }?;

        let presentation_result = if let Some(forward_swapchain) = self.forward_swapchain.as_ref() {
            let image_index_result = {
                unsafe {
                    forward_swapchain
                        .swapchain
                        .acquire_next_image(frame_lock.image_available.handle, vk::Fence::null())
                }
            };

            let image_index = match image_index_result {
                Ok((image_index, _)) => image_index,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return self.create_swapchain(dimensions),
                Err(error) => bail!(error),
            };

            unsafe {
                self.context
                    .logical_device
                    .handle
                    .reset_fences(&[frame_lock.in_flight.handle])
            }?;

            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];

            // TODO: Record and submit command buffers
            // let extent = forward_swapchain.swapchain_properties.extent;
            // self.record_all_command_buffers(&extent, draw_data);
            // self.command_pool
            //     .submit_command_buffer(
            //         image_index as usize,
            //         self.context.graphics_queue(),
            //         &wait_stages,
            //         &current_frame_synchronization,
            //     )
            //     .unwrap();

            let wait_semaphores = [frame_lock.render_finished.handle];
            let swapchains = [forward_swapchain.swapchain.handle_khr];
            let image_indices = [image_index];
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(&wait_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            let presentation_queue_index = self.context.physical_device.presentation_queue_index;
            let presentation_queue = unsafe {
                self.context
                    .logical_device
                    .handle
                    .get_device_queue(presentation_queue_index, 0)
            };

            let presentation_result = unsafe {
                forward_swapchain
                    .swapchain
                    .handle_ash
                    .queue_present(presentation_queue, &present_info)
            };

            presentation_result
        } else {
            bail!(anyhow!("No forward swapchain is available for rendering!"));
        };

        match presentation_result {
            Ok(is_suboptimal) if is_suboptimal => {
                self.create_swapchain(dimensions)?;
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
            }
            Err(error) => bail!(error),
            _ => {}
        }

        self.current_frame += (1 + self.current_frame) % Self::MAX_FRAMES_IN_FLIGHT as usize;

        Ok(())
    }

    fn create_swapchain(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.forward_swapchain = None;
        self.forward_swapchain = Some(ForwardSwapchain::new(self.context.clone(), dimensions)?);

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
