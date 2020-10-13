use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use dragonglass_context::LogicalDevice;
use std::sync::Arc;

pub struct GraphicsPipeline {
    pub handle: vk::Pipeline,
    device: Arc<LogicalDevice>,
}

impl GraphicsPipeline {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::GraphicsPipelineCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe {
            let result = device.handle.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info.build()],
                None,
            );
            match result {
                Ok(pipelines) => Ok(pipelines[0]),
                Err((_, error_code)) => Err(error_code),
            }
        }?;
        let pipeline = Self { handle, device };
        Ok(pipeline)
    }

    pub fn bind(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) {
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.handle);
        }
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_pipeline(self.handle, None);
        }
    }
}

pub struct PipelineLayout {
    pub handle: vk::PipelineLayout,
    device: Arc<LogicalDevice>,
}

impl PipelineLayout {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::PipelineLayoutCreateInfo,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_pipeline_layout(&create_info, None) }?;
        let pipeline_layout = Self { handle, device };
        Ok(pipeline_layout)
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .handle
                .destroy_pipeline_layout(self.handle, None);
        }
    }
}
