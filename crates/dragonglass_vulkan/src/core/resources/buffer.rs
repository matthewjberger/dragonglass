use crate::core::{BufferToBufferCopyBuilder, CommandPool, Device};
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, Allocator},
    MemoryLocation,
};
use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::{Arc, RwLock},
};

pub struct GpuBuffer {
    buffer: Buffer,
    allocator: Arc<RwLock<Allocator>>,
    device: Arc<Device>,
}

impl GpuBuffer {
    pub fn new(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST | usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = Buffer::new(
            device.clone(),
            allocator.clone(),
            buffer_create_info,
            MemoryLocation::GpuOnly,
        )?;
        Ok(Self {
            buffer,
            allocator,
            device,
        })
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

        let staging_buffer =
            CpuToGpuBuffer::staging_buffer(self.device.clone(), self.allocator.clone(), size as _)?;
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

    pub fn vertex_buffer(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
    ) -> Result<Self> {
        Self::new(device, allocator, size, vk::BufferUsageFlags::VERTEX_BUFFER)
    }

    pub fn index_buffer(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
    ) -> Result<Self> {
        Self::new(device, allocator, size, vk::BufferUsageFlags::INDEX_BUFFER)
    }
}

pub struct CpuToGpuBuffer {
    buffer: Buffer,
}

impl CpuToGpuBuffer {
    fn new(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = Buffer::new(
            device,
            allocator,
            buffer_create_info,
            MemoryLocation::CpuToGpu,
        )?;
        Ok(Self { buffer })
    }

    pub fn size(&self) -> u64 {
        self.buffer.allocation.size()
    }

    pub fn handle(&self) -> vk::Buffer {
        self.buffer.handle
    }

    pub fn staging_buffer(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
    ) -> Result<Self> {
        Self::new(device, allocator, size, vk::BufferUsageFlags::TRANSFER_SRC)
    }

    pub fn uniform_buffer(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
    ) -> Result<Self> {
        Self::new(
            device,
            allocator,
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )
    }

    pub fn upload_data<T>(&self, data: &[T], offset: usize) -> Result<()> {
        let data_pointer = self.mapped_ptr()?.as_ptr();
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
        let data_pointer = self.mapped_ptr()?.as_ptr();
        let size = self.buffer.allocation.size();
        unsafe {
            let data_pointer = data_pointer.add(offset);
            let mut align = ash::util::Align::new(data_pointer as _, alignment, size as _);
            align.copy_from_slice(data);
        }
        Ok(())
    }

    pub fn mapped_ptr(&self) -> Result<NonNull<c_void>> {
        self.buffer
            .allocation
            .mapped_ptr()
            .context("Failed to get mapped buffer ptr!")
    }
}

pub struct Buffer {
    pub handle: vk::Buffer,
    allocation: Allocation,
    allocator: Arc<RwLock<Allocator>>,
    device: Arc<Device>,
}

impl Buffer {
    pub fn new(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        buffer_create_info: vk::BufferCreateInfoBuilder,
        location: MemoryLocation,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_buffer(&buffer_create_info, None) }?;
        let requirements = unsafe { device.handle.get_buffer_memory_requirements(handle) };
        let allocation_create_info = AllocationCreateDesc {
            // TODO: Allow custom naming allocations
            name: "Buffer Allocation",
            requirements,
            location,
            linear: true, // Buffers are always linear
        };
        let allocation = {
            let mut allocator = allocator.write().expect("Failed to acquire allocator!");
            allocator.allocate(&allocation_create_info)?
        };
        unsafe {
            device
                .handle
                .bind_buffer_memory(handle, allocation.memory(), allocation.offset())?
        };
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
        let mut allocator = self
            .allocator
            .write()
            .expect("Failed to acquire allocator!");
        allocator
            .free(self.allocation.clone())
            .expect("Failed to free allocated buffer!");
        unsafe { self.device.handle.destroy_buffer(self.handle, None) };
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
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        vertex_buffer_size: vk::DeviceSize,
        index_buffer_size: Option<vk::DeviceSize>,
    ) -> Result<Self> {
        let vertex_buffer =
            GpuBuffer::vertex_buffer(device.clone(), allocator.clone(), vertex_buffer_size)?;
        let index_buffer = if let Some(index_buffer_size) = index_buffer_size {
            let index_buffer = GpuBuffer::index_buffer(device, allocator, index_buffer_size)?;
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
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
    ) -> Result<()> {
        self.vertex_buffer = GpuBuffer::vertex_buffer(device, allocator, size)?;
        self.vertex_buffer_size = size;
        Ok(())
    }

    pub fn reallocate_index_buffer(
        &mut self,
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        size: vk::DeviceSize,
    ) -> Result<()> {
        self.index_buffer = Some(GpuBuffer::index_buffer(device, allocator, size)?);
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
