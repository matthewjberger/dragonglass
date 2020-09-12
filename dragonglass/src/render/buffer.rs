use anyhow::Result;
use ash::vk;
use log::error;
use std::sync::Arc;
use vk_mem::Allocator;

pub struct GeometryBuffer;

pub struct Buffer {
    pub handle: vk::Buffer,
    allocation: vk_mem::Allocation,
    allocation_info: vk_mem::AllocationInfo,
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
            allocation,
            allocation_info,
            allocator,
        };

        Ok(buffer)
    }

    pub fn upload_to_buffer<T>(&self, data: &[T], offset: usize) -> Result<()> {
        let data_pointer = self.map_memory()?;
        unsafe {
            let data_pointer = data_pointer.add(offset);
            (data_pointer as *mut T).copy_from_nonoverlapping(data.as_ptr(), data.len());
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

    pub fn staging_buffer<T: Copy>(allocator: Arc<Allocator>, data: &[T]) -> Result<Self> {
        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            ..Default::default()
        };
        let buffer_size = (data.len() * std::mem::size_of::<T>()) as ash::vk::DeviceSize;
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        Self::new(allocator, &allocation_create_info, buffer_create_info)
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

// pub struct GeometryBuffer {
//     pub vertex_buffer: Buffer,
//     pub index_buffer: Option<Buffer>,
//     pub number_of_vertices: u32,
//     pub number_of_indices: u32,
// }

// impl GeometryBuffer {
//     pub fn new<T: Copy>(
//         context: Arc<Context>,
//         command_pool: &CommandPool,
//         vertices: &[T],
//         indices: Option<&[u32]>,
//     ) -> Result<Self> {
//         let vertex_buffer =
//             context.create_buffer(command_pool, &vertices, vk::BufferUsageFlags::VERTEX_BUFFER)?;

//         let mut number_of_indices = 0;
//         let index_buffer = if let Some(indices) = indices {
//             number_of_indices = indices.len() as u32;
//             index_buffer = context.create_buffer(
//                 command_pool,
//                 &indices,
//                 vk::BufferUsageFlags::INDEX_BUFFER,
//             )?;
//             Some(index_buffer)
//         } else {
//             None
//         };

//         let buffer = Self {
//             vertex_buffer,
//             index_buffer,
//             number_of_indices,
//         };

//         Ok(buffer)
//     }

//     pub fn bind(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) {
//         let offsets = [0];
//         let vertex_buffers = [self.vertex_buffer.buffer()];

//         unsafe {
//             device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &offsets);

//             if let Some(index_buffer) = self.index_buffer.as_ref() {
//                 device.cmd_bind_index_buffer(
//                     command_buffer,
//                     index_buffer.buffer(),
//                     0,
//                     vk::IndexType::UINT32,
//                 );
//             }
//         }
//     }
// }
