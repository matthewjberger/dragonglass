use crate::vulkan::core::{BufferToBufferCopyBuilder, CommandPool};
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use log::error;
use std::sync::Arc;
use vk_mem::Allocator;

pub struct GpuBuffer {
    buffer: Buffer,
    allocator: Arc<Allocator>,
}

impl GpuBuffer {
    pub fn new(
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST | usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = Buffer::new(
            allocator.clone(),
            &allocation_create_info,
            buffer_create_info,
        )?;
        let gpu_buffer = Self { buffer, allocator };
        Ok(gpu_buffer)
    }

    pub fn handle(&self) -> vk::Buffer {
        self.buffer.handle
    }

    pub fn upload_data<T: Copy>(
        &self,
        data: &[T],
        offset: usize,
        pool: &CommandPool,
    ) -> Result<()> {
        let size = data.len() * std::mem::size_of::<T>();

        let staging_buffer = CpuToGpuBuffer::staging_buffer(self.allocator.clone(), size as _)?;
        staging_buffer.upload_data(data, 0)?;

        let region = vk::BufferCopy::builder()
            .size(size as _)
            .dst_offset(offset as _)
            .build();

        let info = BufferToBufferCopyBuilder::default()
            .source(staging_buffer.buffer.handle)
            .destination(self.buffer.handle)
            .regions(vec![region])
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        pool.copy_buffer_to_buffer(&info)?;

        Ok(())
    }

    pub fn vertex_buffer(allocator: Arc<Allocator>, size: vk::DeviceSize) -> Result<Self> {
        Self::new(allocator, size, vk::BufferUsageFlags::VERTEX_BUFFER)
    }

    pub fn index_buffer(allocator: Arc<Allocator>, size: vk::DeviceSize) -> Result<Self> {
        Self::new(allocator, size, vk::BufferUsageFlags::INDEX_BUFFER)
    }
}

pub struct CpuToGpuBuffer {
    buffer: Buffer,
}

impl CpuToGpuBuffer {
    fn new(
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
        let buffer = Buffer::new(allocator, &allocation_create_info, buffer_create_info)?;
        let cpu_to_gpu_buffer = Self { buffer };
        Ok(cpu_to_gpu_buffer)
    }

    pub fn handle(&self) -> vk::Buffer {
        self.buffer.handle
    }

    pub fn staging_buffer(allocator: Arc<Allocator>, size: vk::DeviceSize) -> Result<Self> {
        Self::new(allocator, size, vk::BufferUsageFlags::TRANSFER_SRC)
    }

    pub fn uniform_buffer(allocator: Arc<Allocator>, size: vk::DeviceSize) -> Result<Self> {
        Self::new(allocator, size, vk::BufferUsageFlags::UNIFORM_BUFFER)
    }

    pub fn upload_data<T>(&self, data: &[T], offset: usize) -> Result<()> {
        let data_pointer = self.map_memory()?;
        unsafe {
            let data_pointer = data_pointer.add(offset);
            (data_pointer as *mut T).copy_from_nonoverlapping(data.as_ptr(), data.len());
        }
        self.unmap_memory();
        Ok(())
    }

    pub fn upload_data_aligned<T: Copy>(
        &self,
        data: &[T],
        offset: usize,
        alignment: vk::DeviceSize,
    ) -> Result<()> {
        let data_pointer = self.map_memory()?;
        let size = self.buffer.allocation_info.get_size();
        unsafe {
            let data_pointer = data_pointer.add(offset);
            let mut align = ash::util::Align::new(data_pointer as _, alignment, size as _);
            align.copy_from_slice(data);
        }
        self.buffer.flush(0, size);
        self.unmap_memory();
        Ok(())
    }

    pub fn map_memory(&self) -> vk_mem::error::Result<*mut u8> {
        self.buffer.allocator.map_memory(&self.buffer.allocation)
    }

    pub fn unmap_memory(&self) {
        self.buffer.allocator.unmap_memory(&self.buffer.allocation)
    }
}

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
            allocator.create_buffer(&buffer_create_info, allocation_create_info)?;

        let buffer = Self {
            handle,
            allocation_info,
            allocation,
            allocator,
        };

        Ok(buffer)
    }

    pub fn flush(&self, offset: usize, size: usize) {
        self.allocator
            .flush_allocation(&self.allocation, offset, size);
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.allocator.destroy_buffer(self.handle, &self.allocation);
    }
}

pub struct GeometryBuffer {
    pub vertex_buffer: GpuBuffer,
    pub index_buffer: Option<GpuBuffer>,
    pub vertex_buffer_size: vk::DeviceSize,
    pub index_buffer_size: Option<vk::DeviceSize>,
}

impl GeometryBuffer {
    pub fn new(
        allocator: Arc<Allocator>,
        vertex_buffer_size: vk::DeviceSize,
        index_buffer_size: Option<vk::DeviceSize>,
    ) -> Result<Self> {
        let vertex_buffer = GpuBuffer::vertex_buffer(allocator.clone(), vertex_buffer_size)?;
        let index_buffer = if let Some(index_buffer_size) = index_buffer_size {
            let index_buffer = GpuBuffer::index_buffer(allocator, index_buffer_size)?;
            Some(index_buffer)
        } else {
            None
        };
        let geometry_buffer = Self {
            vertex_buffer,
            index_buffer,
            vertex_buffer_size,
            index_buffer_size,
        };
        Ok(geometry_buffer)
    }

    /// Assumes 32-bit index buffers
    pub fn bind(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) -> Result<()> {
        let offsets = [0];
        let vertex_buffers = [self.vertex_buffer.handle()];
        unsafe {
            device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &offsets);
            if let Some(index_buffer) = self.index_buffer.as_ref() {
                device.cmd_bind_index_buffer(
                    command_buffer,
                    index_buffer.handle(),
                    0,
                    vk::IndexType::UINT32,
                );
            }
        };

        Ok(())
    }
}
