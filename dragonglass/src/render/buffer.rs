use anyhow::Result;
use ash::vk;
use log::error;
use std::sync::Arc;
use vk_mem::Allocator;

pub struct Buffer {
    pub handle: vk::Buffer,
    pub allocation_info: vk_mem::AllocationInfo,
    allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
}

impl Buffer {
    pub fn new(
        allocator: Arc<Allocator>,
        allocation_create_info: &vk_mem::AllocationCreateInfo,
        buffer_create_info: vk::BufferCreateInfoBuilder,
    ) -> Result<Self> {
        let (handle, allocation, allocation_info) =
            allocator.create_buffer(&buffer_create_info, &allocation_create_info)?;

        let buffer = Self {
            handle,
            allocation_info,
            allocation,
            allocator,
        };

        Ok(buffer)
    }

    pub fn upload_data<T>(&self, data: &[T], offset: usize) -> Result<()> {
        let data_pointer = self.map_memory()?;
        unsafe {
            let data_pointer = data_pointer.add(offset);
            (data_pointer as *mut T).copy_from_nonoverlapping(data.as_ptr(), data.len());
        }
        self.unmap_memory()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn upload_data_aligned<T: Copy>(
        &self,
        data: &[T],
        offset: usize,
        alignment: vk::DeviceSize,
    ) -> Result<()> {
        let data_pointer = self.map_memory()?;
        unsafe {
            let mut align = ash::util::Align::new(
                data_pointer.add(offset) as _,
                alignment,
                self.allocation_info.get_size() as _,
            );
            align.copy_from_slice(data);
        }
        self.unmap_memory()?;
        Ok(())
    }

    pub fn map_memory(&self) -> vk_mem::error::Result<*mut u8> {
        self.allocator.map_memory(&self.allocation)
    }

    pub fn unmap_memory(&self) -> vk_mem::error::Result<()> {
        self.allocator.unmap_memory(&self.allocation)
    }

    pub fn cpu_to_gpu_buffer(
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            ..Default::default()
        };
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        Self::new(allocator, &allocation_create_info, buffer_create_info)
    }

    pub fn staging_buffer(allocator: Arc<Allocator>, size: vk::DeviceSize) -> Result<Self> {
        Self::cpu_to_gpu_buffer(allocator, size, vk::BufferUsageFlags::TRANSFER_SRC)
    }

    pub fn uniform_buffer(allocator: Arc<Allocator>, size: vk::DeviceSize) -> Result<Self> {
        Self::cpu_to_gpu_buffer(allocator, size, vk::BufferUsageFlags::UNIFORM_BUFFER)
    }

    pub fn device_local_buffer(
        allocator: Arc<Allocator>,
        staging_buffer: &Buffer,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(staging_buffer.allocation_info.get_size() as _)
            .usage(vk::BufferUsageFlags::TRANSFER_DST | usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        Self::new(allocator, &allocation_create_info, buffer_create_info)
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        if let Err(error) = self.allocator.destroy_buffer(self.handle, &self.allocation) {
            error!("{}", error);
        }
    }
}
