use super::{BufferToImageCopyBuilder, CommandPool, CpuToGpuBuffer, PipelineBarrierBuilder};
use crate::core::LogicalDevice;
use anyhow::{anyhow, bail, Context, Result};
use ash::{version::DeviceV1_0, vk};
use image::{DynamicImage, ImageBuffer, Pixel, RgbImage};
use log::error;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use vk_mem::Allocator;

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
        let image = image::open(path).with_context(|| format!("path: {}", path_display))?;
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
        graphics_queue: vk::Queue,
        pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<()> {
        // Create and upload data to staging buffer
        let buffer = CpuToGpuBuffer::staging_buffer(
            self.allocator.clone(),
            self.allocation_info.get_size() as _,
        )?;
        buffer.upload_data(&description.pixels, 0)?;

        // Transition to transfer_dst_optimal
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1)
            .build();
        let image_barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.handle)
            .subresource_range(subresource_range)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .build();
        let pipeline_barrier_info = PipelineBarrierBuilder::default()
            .graphics_queue(graphics_queue)
            .src_stage_mask(vk::PipelineStageFlags::TOP_OF_PIPE)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .image_memory_barriers(vec![image_barrier])
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        pool.transition_image_layout(&pipeline_barrier_info)?;

        // Copy the staging buffer to the image
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
            .source(buffer.handle())
            .destination(self.handle)
            .regions(vec![region])
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        pool.copy_buffer_to_image(&copy_info)?;

        // self.generate_mipmaps(&command_pool, &description)?;

        // Transition to shader_read_only_optimal
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(description.mip_levels - 1)
            .level_count(1)
            .layer_count(1)
            .build();
        let image_barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.handle)
            .subresource_range(subresource_range)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .build();
        let pipeline_barrier_info = PipelineBarrierBuilder::default()
            .graphics_queue(graphics_queue)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .image_memory_barriers(vec![image_barrier])
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        pool.transition_image_layout(&pipeline_barrier_info)?;

        Ok(())
    }

    // pub fn generate_mipmaps(
    //     &self,
    //     command_pool: &CommandPool,
    //     texture_description: &ImageDescription,
    // ) -> Result<()> {
    //     let format_properties = self
    //         .context
    //         .physical_device_format_properties(texture_description.format);

    //     if !format_properties
    //         .optimal_tiling_features
    //             .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR)
    //     {
    //         panic!(
    //             "Linear blitting is not supported for format: {:?}",
    //             texture_description.format
    //         );
    //     }
    //     let mut mip_width = texture_description.width as i32;
    //     let mut mip_height = texture_description.height as i32;
    //     for level in 1..texture_description.mip_levels {
    //         let next_mip_width = if mip_width > 1 {
    //             mip_width / 2
    //         } else {
    //             mip_width
    //         };
    //         let next_mip_height = if mip_height > 1 {
    //             mip_height / 2
    //         } else {
    //             mip_height
    //         };
    //         let barrier = vk::ImageMemoryBarrier::builder()
    //             .image(self.image())
    //             .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
    //             .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
    //             .subresource_range(vk::ImageSubresourceRange {
    //                 aspect_mask: vk::ImageAspectFlags::COLOR,
    //                 base_array_layer: 0,
    //                 layer_count: 1,
    //                 level_count: 1,
    //                 base_mip_level: level - 1,
    //             })
    //         .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
    //             .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
    //             .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
    //             .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
    //             .build();
    //         let barriers = [barrier];
    //         command_pool.transition_image_layout(
    //             &barriers,
    //             vk::PipelineStageFlags::TRANSFER,
    //             vk::PipelineStageFlags::TRANSFER,
    //         )?;
    //         let blit = vk::ImageBlit::builder()
    //             .src_offsets([
    //                 vk::Offset3D { x: 0, y: 0, z: 0 },
    //                 vk::Offset3D {
    //                     x: mip_width,
    //                     y: mip_height,
    //                     z: 1,
    //                 },
    //             ])
    //             .src_subresource(vk::ImageSubresourceLayers {
    //                 aspect_mask: vk::ImageAspectFlags::COLOR,
    //                 mip_level: level - 1,
    //                 base_array_layer: 0,
    //                 layer_count: 1,
    //             })
    //         .dst_offsets([
    //             vk::Offset3D { x: 0, y: 0, z: 0 },
    //             vk::Offset3D {
    //                 x: next_mip_width,
    //                 y: next_mip_height,
    //                 z: 1,
    //             },
    //         ])
    //             .dst_subresource(vk::ImageSubresourceLayers {
    //                 aspect_mask: vk::ImageAspectFlags::COLOR,
    //                 mip_level: level,
    //                 base_array_layer: 0,
    //                 layer_count: 1,
    //             })
    //         .build();
    //         let blits = [blit];

    //         command_pool.execute_command_once(
    //             self.context.graphics_queue(),
    //             |command_buffer| unsafe {
    //                 self.context
    //                     .logical_device()
    //                     .logical_device()
    //                     .cmd_blit_image(
    //                         command_buffer,
    //                         self.image(),
    //                         vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
    //                         self.image(),
    //                         vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    //                         &blits,
    //                         vk::Filter::LINEAR,
    //                     )
    //             },
    //         )?;

    //         let barrier = vk::ImageMemoryBarrier::builder()
    //             .image(self.image())
    //             .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
    //             .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
    //             .subresource_range(vk::ImageSubresourceRange {
    //                 aspect_mask: vk::ImageAspectFlags::COLOR,
    //                 base_array_layer: 0,
    //                 layer_count: 1,
    //                 level_count: 1,
    //                 base_mip_level: level - 1,
    //             })
    //         .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
    //             .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    //             .src_access_mask(vk::AccessFlags::TRANSFER_READ)
    //             .dst_access_mask(vk::AccessFlags::SHADER_READ)
    //             .build();
    //         let barriers = [barrier];

    //         command_pool.transition_image_layout(
    //             &barriers,
    //             vk::PipelineStageFlags::TRANSFER,
    //             vk::PipelineStageFlags::FRAGMENT_SHADER,
    //         )?;
    //         mip_width = next_mip_width;
    //         mip_height = next_mip_height;
    //     }
    //     Ok(())
    // }
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

pub struct ImageBundle {
    pub image: Image,
    pub view: ImageView,
    pub sampler: Sampler,
}

impl ImageBundle {
    pub fn new(
        device: Arc<LogicalDevice>,
        graphics_queue: vk::Queue,
        allocator: Arc<Allocator>,
        command_pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<Self> {
        let image = Self::create_image(allocator, &description)?;
        image.upload_data(graphics_queue, &command_pool, &description)?;
        let view = Self::create_image_view(device.clone(), &image, &description)?;
        let sampler = Self::create_sampler(device, description.mip_levels)?;

        let image_bundle = Self {
            image,
            view,
            sampler,
        };

        Ok(image_bundle)
    }

    fn create_image(allocator: Arc<Allocator>, description: &ImageDescription) -> Result<Image> {
        let extent = vk::Extent3D {
            width: description.width,
            height: description.height,
            depth: 1,
        };

        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(description.mip_levels)
            .array_layers(1)
            .format(description.format)
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

        Image::new(allocator, &allocation_create_info, &create_info)
    }

    fn create_image_view(
        device: Arc<LogicalDevice>,
        image: &Image,
        description: &ImageDescription,
    ) -> Result<ImageView> {
        // TODO: Use builders
        let components = vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        };

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: description.mip_levels,
            base_array_layer: 0,
            layer_count: 1,
        };

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(description.format)
            .components(components)
            .subresource_range(subresource_range);
        ImageView::new(device, create_info)
    }

    fn create_sampler(device: Arc<LogicalDevice>, mip_levels: u32) -> Result<Sampler> {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(mip_levels as _);
        Sampler::new(device, sampler_info)
    }
}
