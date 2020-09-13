use super::{BufferToImageCopyBuilder, CommandPool, CpuToGpuBuffer};
use crate::core::LogicalDevice;
use anyhow::{anyhow, bail, Context, Result};
use ash::{version::DeviceV1_0, vk};
use derive_builder::Builder;
use image::{DynamicImage, ImageBuffer, Pixel, RgbImage};
use log::error;
use std::sync::Arc;
use vk_mem::Allocator;

#[derive(Builder)]
pub struct ImageLayoutTransition {
    pub old_layout: vk::ImageLayout,
    pub new_layout: vk::ImageLayout,
    pub src_access_mask: vk::AccessFlags,
    pub dst_access_mask: vk::AccessFlags,
    pub src_stage_mask: vk::PipelineStageFlags,
    pub dst_stage_mask: vk::PipelineStageFlags,
}

pub struct TextureDescription {
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub mip_levels: u32,
}

impl TextureDescription {
    pub fn from_file(path: &str) -> Result<Self> {
        let image = image::open(path).with_context(|| format!("path: {}", path.to_string()))?;
        Self::from_image(&image)
    }

    pub fn from_image(image: &DynamicImage) -> Result<Self> {
        let (format, (width, height)) = match image {
            DynamicImage::ImageRgb8(buffer) => (vk::Format::R8G8B8_UNORM, buffer.dimensions()),
            DynamicImage::ImageRgba8(buffer) => (vk::Format::R8G8B8A8_UNORM, buffer.dimensions()),
            DynamicImage::ImageBgr8(buffer) => (vk::Format::B8G8R8_UNORM, buffer.dimensions()),
            DynamicImage::ImageBgra8(buffer) => (vk::Format::B8G8R8A8_UNORM, buffer.dimensions()),
            DynamicImage::ImageRgb16(buffer) => (vk::Format::R16G16B16_UNORM, buffer.dimensions()),
            DynamicImage::ImageRgba16(buffer) => {
                (vk::Format::R16G16B16A16_UNORM, buffer.dimensions())
            }
            _ => bail!("Failed to match the provided image format to a vulkan format!"),
        };

        let mut description = Self {
            format,
            width,
            height,
            pixels: image.to_bytes(),
            mip_levels: Self::calculate_mip_levels(width, height),
        };
        description.convert_24bit_formats()?;
        Ok(description)
    }

    pub fn calculate_mip_levels(width: u32, height: u32) -> u32 {
        ((width.min(height) as f32).log2().floor() + 1.0) as u32
    }

    fn convert_24bit_formats(&mut self) -> Result<()> {
        // 24-bit formats are unsupported, so they
        // need to have an alpha channel added to make them 32-bit
        match self.format {
            vk::Format::R8G8B8_UNORM => {
                self.format = vk::Format::R8G8B8A8_UNORM;
                self.attach_alpha_channel()?;
            }
            vk::Format::B8G8R8_UNORM => {
                self.format = vk::Format::B8G8R8A8_UNORM;
                self.attach_alpha_channel()?;
            }
            _ => {}
        };

        Ok(())
    }

    fn attach_alpha_channel(&mut self) -> Result<()> {
        let image_buffer: RgbImage =
            ImageBuffer::from_raw(self.width, self.height, self.pixels.to_vec())
                .ok_or_else(|| anyhow!("Failed to load image rom raw pixels!"))?;

        self.pixels = image_buffer
            .pixels()
            .flat_map(|pixel| pixel.to_rgba().channels().to_vec())
            .collect::<Vec<_>>();

        Ok(())
    }
}

pub struct Image {
    pub handle: vk::Image,
    allocation: vk_mem::Allocation,
    allocation_info: vk_mem::AllocationInfo,
    allocator: Arc<Allocator>,
}

impl Image {
    pub fn new(
        allocator: Arc<Allocator>,
        allocation_create_info: &vk_mem::AllocationCreateInfo,
        image_create_info: &vk::ImageCreateInfoBuilder,
    ) -> Result<Self> {
        let (handle, allocation, allocation_info) =
            allocator.create_image(&image_create_info, &allocation_create_info)?;

        let texture = Self {
            handle,
            allocation,
            allocation_info,
            allocator,
        };

        Ok(texture)
    }

    pub fn upload_data(
        &self,
        command_pool: &CommandPool,
        description: &TextureDescription,
    ) -> Result<()> {
        let region = vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: description.width,
                height: description.height,
                depth: 1,
            })
            .build();
        let regions = vec![region];

        let buffer = CpuToGpuBuffer::staging_buffer(
            self.allocator.clone(),
            self.allocation_info.get_size() as _,
        )?;
        buffer.upload_data(&description.pixels, 0)?;

        // let transition = ImageLayoutTransitionBuilder::default()
        //     .old_layout(vk::ImageLayout::UNDEFINED)
        //     .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        //     .src_access_mask(vk::AccessFlags::empty())
        //     .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        //     .src_stage_mask(vk::PipelineStageFlags::TOP_OF_PIPE)
        //     .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
        //     .build()
        //     .map_err(|error| anyhow!("{}", error))?;

        // self.transition(&command_pool, &transition, description.mip_levels)?;

        let buffer_to_image_copy = BufferToImageCopyBuilder::default()
            .source(buffer.handle())
            .destination(self.handle)
            .regions(regions)
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        command_pool.copy_buffer_to_image(&buffer_to_image_copy)?;

        // self.generate_mipmaps(&command_pool, &description)?;

        let barrier = vk::ImageMemoryBarrier::builder()
            .image(self.handle)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_array_layer: 0,
                layer_count: 1,
                level_count: 1,
                base_mip_level: description.mip_levels - 1,
            })
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .build();
        let barriers = [barrier];

        // command_pool.transition_image_layout(
        //     &barriers,
        //     vk::PipelineStageFlags::TRANSFER,
        //     vk::PipelineStageFlags::FRAGMENT_SHADER,
        // )?;

        Ok(())
    }

    pub fn transition(
        &self,
        command_pool: &CommandPool,
        transition: &ImageLayoutTransition,
        mip_levels: u32,
    ) -> Result<()> {
        let barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(transition.old_layout)
            .new_layout(transition.new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.handle)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(transition.src_access_mask)
            .dst_access_mask(transition.dst_access_mask)
            .build();
        let barriers = [barrier];

        // command_pool.transition_image_layout(
        //     &barriers,
        //     transition.src_stage_mask,
        //     transition.dst_stage_mask,
        // )?;

        Ok(())
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if let Err(error) = self.allocator.destroy_image(self.handle, &self.allocation) {
            error!("{}", error);
        }
    }
}

pub struct ImageView {
    pub handle: vk::ImageView,
    device: Arc<LogicalDevice>,
}

impl ImageView {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::ImageViewCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_image_view(&create_info, None) }?;
        let image_view = Self { handle, device };
        Ok(image_view)
    }
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_image_view(self.handle, None);
        }
    }
}
