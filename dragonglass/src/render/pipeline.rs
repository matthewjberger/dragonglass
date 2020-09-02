use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct ComputePipeline {
    handle: vk::Pipeline,
    device: Arc<LogicalDevice>,
}

impl ComputePipeline {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::ComputePipelineCreateInfo,
    ) -> Result<Self> {
        let handle = unsafe {
            let result = device.handle.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[create_info],
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
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_pipeline(self.handle, None);
        }
    }
}

pub struct GraphicsPipeline {
    pub handle: vk::Pipeline,
    device: Arc<LogicalDevice>,
}

impl GraphicsPipeline {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::GraphicsPipelineCreateInfo,
    ) -> Result<Self> {
        let handle = unsafe {
            let result = device.handle.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info],
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
