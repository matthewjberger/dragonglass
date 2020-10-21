use super::CpuToGpuBuffer;
use crate::{
    adapters::{BlitImageBuilder, BufferToImageCopyBuilder, CommandPool, PipelineBarrierBuilder},
    context::{Context, LogicalDevice},
};
use anyhow::{anyhow, bail, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use derive_builder::Builder;
use image::{DynamicImage, ImageBuffer, Pixel, RgbImage};
use log::error;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use vk_mem::Allocator;

#[derive(Builder)]
pub struct ImageLayoutTransition {
    pub graphics_queue: vk::Queue,
    #[builder(default)]
    pub base_mip_level: u32,
    #[builder(default = "1")]
    pub level_count: u32,
    #[builder(default = "1")]
    pub layer_count: u32,
    pub old_layout: vk::ImageLayout,
    pub new_layout: vk::ImageLayout,
    pub src_access_mask: vk::AccessFlags,
    pub dst_access_mask: vk::AccessFlags,
    pub src_stage_mask: vk::PipelineStageFlags,
    pub dst_stage_mask: vk::PipelineStageFlags,
}

pub struct ImageDescription {
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub mip_levels: u32,
}

impl ImageDescription {
    pub fn from_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path> + Into<PathBuf>,
    {
        let path_display = path.as_ref().display().to_string();
        let image =
            image::open(path).map_err(|error| anyhow!("{}\npath: {}", error, path_display))?;
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
                .context("Failed to load image from raw pixels!")?;

        self.pixels = image_buffer
            .pixels()
            .flat_map(|pixel| pixel.to_rgba().channels().to_vec())
            .collect::<Vec<_>>();

        Ok(())
    }

    pub fn as_image(&self, allocator: Arc<Allocator>) -> Result<AllocatedImage> {
        let extent = vk::Extent3D::builder()
            .width(self.width)
            .height(self.height)
            .depth(1);

        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent.build())
            .mip_levels(self.mip_levels)
            .array_layers(1)
            .format(self.format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(
                vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::SAMPLED,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };

        AllocatedImage::new(allocator, &allocation_create_info, &create_info)
    }
}

pub trait Image {
    fn handle(&self) -> vk::Image;
}

pub struct RawImage(pub vk::Image);

impl Image for RawImage {
    fn handle(&self) -> vk::Image {
        self.0
    }
}

pub struct AllocatedImage {
    pub handle: vk::Image,
    allocation: vk_mem::Allocation,
    allocation_info: vk_mem::AllocationInfo,
    allocator: Arc<Allocator>,
}

impl Image for AllocatedImage {
    fn handle(&self) -> vk::Image {
        self.handle
    }
}

impl AllocatedImage {
    pub fn new(
        allocator: Arc<Allocator>,
        allocation_create_info: &vk_mem::AllocationCreateInfo,
        image_create_info: &vk::ImageCreateInfoBuilder,
    ) -> Result<Self> {
        let (handle, allocation, allocation_info) =
            allocator.create_image(image_create_info, allocation_create_info)?;

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
        context: &Context,
        pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<()> {
        let buffer = CpuToGpuBuffer::staging_buffer(
            self.allocator.clone(),
            self.allocation_info.get_size() as _,
        )?;
        buffer.upload_data(&description.pixels, 0)?;
        let graphics_queue = context.graphics_queue();
        self.transition_base_to_transfer_dst(graphics_queue, pool, description.mip_levels)?;
        self.copy_to_gpu_buffer(graphics_queue, pool, buffer.handle(), description)?;
        context.ensure_linear_blitting_supported(description.format)?;
        self.generate_mipmaps(graphics_queue, pool, description)?;
        self.transition_base_to_shader_read(graphics_queue, pool, description.mip_levels - 1)?;
        Ok(())
    }

    fn transition_base_to_transfer_dst(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        level_count: u32,
    ) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .graphics_queue(graphics_queue)
            .level_count(level_count)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .src_stage_mask(vk::PipelineStageFlags::TOP_OF_PIPE)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        self.transition(pool, &transition)
    }

    fn transition_base_to_shader_read(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        base_mip_level: u32,
    ) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .graphics_queue(graphics_queue)
            .base_mip_level(base_mip_level)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        self.transition(pool, &transition)
    }

    fn transition_mip_transfer_dst_to_src(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        base_mip_level: u32,
    ) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .graphics_queue(graphics_queue)
            .base_mip_level(base_mip_level)
            .level_count(1)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        self.transition(pool, &transition)
    }

    fn transition_mip_to_shader_read(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        base_mip_level: u32,
    ) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .graphics_queue(graphics_queue)
            .base_mip_level(base_mip_level)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        self.transition(pool, &transition)
    }

    fn transition(&self, pool: &CommandPool, info: &ImageLayoutTransition) -> Result<()> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(info.base_mip_level)
            .level_count(info.level_count)
            .layer_count(1)
            .build();
        let image_barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(info.old_layout)
            .new_layout(info.new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.handle)
            .subresource_range(subresource_range)
            .src_access_mask(info.src_access_mask)
            .dst_access_mask(info.dst_access_mask)
            .build();
        let pipeline_barrier_info = PipelineBarrierBuilder::default()
            .graphics_queue(info.graphics_queue)
            .src_stage_mask(info.src_stage_mask)
            .dst_stage_mask(info.dst_stage_mask)
            .image_memory_barriers(vec![image_barrier])
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        pool.transition_image_layout(&pipeline_barrier_info)?;
        Ok(())
    }

    fn copy_to_gpu_buffer(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        buffer: vk::Buffer,
        description: &ImageDescription,
    ) -> Result<()> {
        let extent = vk::Extent3D::builder()
            .width(description.width)
            .height(description.height)
            .depth(1)
            .build();
        let subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1)
            .build();
        let region = vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(subresource)
            .image_offset(vk::Offset3D::default())
            .image_extent(extent)
            .build();
        let copy_info = BufferToImageCopyBuilder::default()
            .graphics_queue(graphics_queue)
            .source(buffer)
            .destination(self.handle)
            .regions(vec![region])
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        pool.copy_buffer_to_image(&copy_info)?;
        Ok(())
    }

    pub fn generate_mipmaps(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<()> {
        let mut width = description.width as i32;
        let mut height = description.height as i32;
        for level in 1..description.mip_levels {
            self.transition_mip_transfer_dst_to_src(graphics_queue, pool, level - 1)?;
            let dimensions = MipmapBlitDimensions::new(width, height);
            self.blit_mipmap(graphics_queue, pool, &dimensions, level)?;
            self.transition_mip_to_shader_read(graphics_queue, pool, level - 1)?;
            width = dimensions.next_width;
            height = dimensions.next_height;
        }
        Ok(())
    }

    fn blit_mipmap(
        &self,
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        dimensions: &MipmapBlitDimensions,
        level: u32,
    ) -> Result<()> {
        let src_subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(level - 1)
            .layer_count(1)
            .build();

        let dst_subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(level)
            .layer_count(1)
            .build();

        let regions = vk::ImageBlit::builder()
            .src_offsets(dimensions.src_offsets())
            .src_subresource(src_subresource)
            .dst_offsets(dimensions.dst_offsets())
            .dst_subresource(dst_subresource)
            .build();

        let blit_image_info = BlitImageBuilder::default()
            .graphics_queue(graphics_queue)
            .src_image(self.handle)
            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .dst_image(self.handle)
            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .regions(vec![regions])
            .filter(vk::Filter::LINEAR)
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        pool.blit_image(&blit_image_info)
    }
}

impl Drop for AllocatedImage {
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

pub struct Sampler {
    pub handle: vk::Sampler,
    device: Arc<LogicalDevice>,
}

impl Sampler {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::SamplerCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_sampler(&create_info, None) }?;
        let sampler = Self { handle, device };
        Ok(sampler)
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { self.device.handle.destroy_sampler(self.handle, None) };
    }
}

struct MipmapBlitDimensions {
    pub width: i32,
    pub height: i32,
    pub next_width: i32,
    pub next_height: i32,
}

impl MipmapBlitDimensions {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            next_width: std::cmp::max(width / 2, 1),
            next_height: std::cmp::max(height / 2, 1),
        }
    }

    pub fn src_offsets(&self) -> [vk::Offset3D; 2] {
        [
            vk::Offset3D::default(),
            vk::Offset3D::builder()
                .x(self.width)
                .y(self.height)
                .z(1)
                .build(),
        ]
    }

    pub fn dst_offsets(&self) -> [vk::Offset3D; 2] {
        [
            vk::Offset3D::default(),
            vk::Offset3D::builder()
                .x(self.next_width)
                .y(self.next_height)
                .z(1)
                .build(),
        ]
    }
}
