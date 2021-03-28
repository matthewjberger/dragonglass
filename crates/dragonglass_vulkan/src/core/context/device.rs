use anyhow::Result;
use ash::{
    version::{DeviceV1_0, InstanceV1_0},
    vk,
};

pub struct Device {
    pub handle: ash::Device,
}

impl Device {
    pub fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        create_info: vk::DeviceCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { instance.create_device(physical_device, &create_info, None) }?;
        Ok(Self { handle })
    }

    pub fn record_command_buffer(
        &self,
        buffer: vk::CommandBuffer,
        usage: vk::CommandBufferUsageFlags,
        mut action: impl FnMut(vk::CommandBuffer) -> Result<()>,
    ) -> Result<()> {
        let begin_info = vk::CommandBufferBeginInfo::builder().flags(usage);
        unsafe { self.handle.begin_command_buffer(buffer, &begin_info) }?;
        action(buffer)?;
        unsafe { self.handle.end_command_buffer(buffer) }?;
        Ok(())
    }

    pub fn update_viewport(
        &self,
        command_buffer: vk::CommandBuffer,
        extent: vk::Extent2D,
        flip_viewport: bool,
    ) -> Result<()> {
        let (y, height) = if flip_viewport {
            (extent.height as f32, -1.0 * extent.height as f32)
        } else {
            (0_f32, extent.height as f32)
        };
        let viewport = vk::Viewport::builder()
            .y(y)
            .width(extent.width as _)
            .height(height)
            .max_depth(1.0)
            .build();
        let viewports = [viewport];

        let scissor = vk::Rect2D::builder().extent(extent).build();
        let scissors = [scissor];

        unsafe {
            self.handle.cmd_set_viewport(command_buffer, 0, &viewports);
            self.handle.cmd_set_scissor(command_buffer, 0, &scissors);
        }

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            self.handle.destroy_device(None);
        }
    }
}
