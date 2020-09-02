use super::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct GraphicsPipeline {
    pub handle: vk::Pipeline,
    device: Arc<LogicalDevice>,
}

impl GraphicsPipeline {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::GraphicsPipelineCreateInfo,
    ) -> Result<Self> {
        let pipeline_create_info_arr = [create_info];
        let handle = unsafe {
            let result = device.handle.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info],
                None,
            );
            match result {
                Ok(pipelines) => Ok(pipelines[0]),
                Err((pipelines, error_code)) => Err(error_code),
            }
        }?;
        let pipeline = Self { handle, device };
        Ok(pipeline)
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_pipeline(self.handle, None);
        }
    }
}
