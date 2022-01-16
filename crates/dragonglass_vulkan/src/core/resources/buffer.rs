use crate::core::{BufferToBufferCopyBuilder, CommandPool};
use anyhow::{Context, Result};
use ash::{vk, Device};
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, Allocator},
    MemoryLocation,
};
use std::sync::Arc;

pub struct GpuBuffer {
    buffer: Buffer,
    allocator: Arc<Allocator>,
}

impl GpuBuffer {
    pub fn new(
        name: &str,
        device: Arc<Device>,
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST | usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = Buffer::new(
            name,
            device.clone(),
            allocator.clone(),
            buffer_create_info,
            MemoryLocation::GpuOnly,
        )?;
        Ok(Self { buffer, allocator })
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
            .build()?;

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
        name: &str,
        device: Arc<Device>,
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = Buffer::new(
            name,
            device.clone(),
            allocator.clone(),
            buffer_create_info,
            MemoryLocation::GpuOnly,
        )?;
        Ok(Self { buffer })
    }

    pub fn handle(&self) -> vk::Buffer {
        self.buffer.handle
    }

    pub fn staging_buffer(
        device: Arc<Device>,
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
    ) -> Result<Self> {
        Self::new(
            "Staging buffer",
            device,
            allocator,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
        )
    }

    pub fn uniform_buffer(
        device: Arc<Device>,
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
    ) -> Result<Self> {
        Self::new(
            "Uniform Buffer",
            device,
            allocator,
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )
    }

    pub fn upload_data<T>(&self, data: &[T], offset: usize) -> Result<()> {
        let data_pointer = self.mapped_ptr()?;
        unsafe {
            let data_pointer = data_pointer.add(offset);
            (data_pointer as *mut T).copy_from_nonoverlapping(data.as_ptr(), data.len());
        }
        Ok(())
    }

    pub fn upload_data_aligned<T: Copy>(
        &self,
        data: &[T],
        offset: usize,
        alignment: vk::DeviceSize,
    ) -> Result<()> {
        let data_pointer = self.mapped_ptr()?;
        let size = self.buffer.allocation_info.get_size();
        unsafe {
            let data_pointer = data_pointer.add(offset);
            let mut align = ash::util::Align::new(data_pointer as _, alignment, size as _);
            align.copy_from_slice(data);
        }
        Ok(())
    }

    pub fn mapped_ptr(&self) -> std::ptr::NonNull<std::ffi::c_void> {
        self.buffer
            .allocation
            .mapped_ptr()
            .context("Failed to map buffer!")
    }
}

pub struct Buffer {
    pub handle: vk::Buffer,
    allocation: Allocation,
    allocator: Arc<Allocator>,
    device: Arc<Device>,
}

impl Buffer {
    pub fn new(
        name: &str,
        device: Arc<Device>,
        allocator: Arc<Allocator>,
        buffer_create_info: vk::BufferCreateInfoBuilder,
        location: MemoryLocation,
    ) -> Result<Self> {
        let handle = unsafe { device.create_buffer(&buffer_create_info, None)? };
        let requirements = unsafe { device.get_buffer_memory_requirements(handle) };
        let allocation = allocator.allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true, // Buffers are always linear
        })?;
        unsafe {
            device.bind_buffer_memory(handle, allocation.memory(), allocation.offset())?;
        }
        Ok(Self {
            handle,
            allocation,
            allocator,
            device,
        })
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.allocator
            .free(self.allocation)
            .expect("Failed to free allocate buffer!");
        unsafe {
            self.device.destroy_buffer(self.handle, None);
        }
    }
}

pub struct GeometryBuffer {
    pub vertex_buffer: GpuBuffer,
    pub vertex_buffer_size: vk::DeviceSize,
    pub index_buffer: Option<GpuBuffer>,
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
        Ok(Self {
            vertex_buffer,
            vertex_buffer_size,
            index_buffer,
            index_buffer_size,
        })
    }

    pub fn reallocate_vertex_buffer(
        &mut self,
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
    ) -> Result<()> {
        self.vertex_buffer = GpuBuffer::vertex_buffer(allocator.clone(), size)?;
        self.vertex_buffer_size = size;
        Ok(())
    }

    pub fn reallocate_index_buffer(
        &mut self,
        allocator: Arc<Allocator>,
        size: vk::DeviceSize,
    ) -> Result<()> {
        self.index_buffer = Some(GpuBuffer::index_buffer(allocator, size)?);
        self.index_buffer_size = Some(size);
        Ok(())
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
