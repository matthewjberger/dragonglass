use crate::core::{
    BlitImageBuilder, BufferToImageCopyBuilder, CommandPool, Context, Device,
    PipelineBarrierBuilder,
};
use anyhow::{anyhow, bail, Context as AnyhowContext, Result};
use ash::vk;
use derive_builder::Builder;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, Allocator},
    MemoryLocation,
};
use image::{DynamicImage, ImageBuffer, Pixel, RgbImage};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use super::CpuToGpuBuffer;

#[derive(Builder)]
pub struct ImageLayoutTransition {
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
    pub fn empty(width: u32, height: u32, format: vk::Format) -> Self {
        Self {
            format,
            width,
            height,
            pixels: Vec::new(),
            mip_levels: Self::calculate_mip_levels(width, height),
        }
    }

    // FIXME: MOve this to the 'world' crate
    #[allow(dead_code)]
    pub fn from_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path> + Into<PathBuf>,
    {
        let path_display = path.as_ref().display().to_string();
        let image =
            image::open(path).map_err(|error| anyhow!("{}\npath: {}", error, path_display))?;
        Self::from_image(&image)
    }

    #[allow(dead_code)]
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

    pub fn from_texture(data: &dragonglass_world::Texture) -> Result<Self> {
        let format = Self::map_to_vulkan_format(&data.format);
        let mut description = Self {
            format,
            width: data.width,
            height: data.height,
            pixels: data.pixels.to_vec(),
            mip_levels: Self::calculate_mip_levels(data.width, data.height),
        };
        description.convert_24bit_formats()?;
        Ok(description)
    }

    fn map_to_vulkan_format(format: &dragonglass_world::Format) -> vk::Format {
        match format {
            dragonglass_world::Format::R8 => vk::Format::R8_UNORM,
            dragonglass_world::Format::R8G8 => vk::Format::R8G8_UNORM,
            dragonglass_world::Format::R8G8B8A8 => vk::Format::R8G8B8A8_UNORM,
            dragonglass_world::Format::B8G8R8A8 => vk::Format::B8G8R8A8_UNORM,
            dragonglass_world::Format::R8G8B8 => vk::Format::R8G8B8_UNORM,
            dragonglass_world::Format::B8G8R8 => vk::Format::B8G8R8_UNORM,

            dragonglass_world::Format::R16 => vk::Format::R16_UNORM,
            dragonglass_world::Format::R16G16 => vk::Format::R16G16_UNORM,
            dragonglass_world::Format::R16G16B16 => vk::Format::R16G16B16_UNORM,
            dragonglass_world::Format::R16G16B16A16 => vk::Format::R16G16B16A16_UNORM,

            dragonglass_world::Format::R16F => vk::Format::R16_SFLOAT,
            dragonglass_world::Format::R16G16F => vk::Format::R16G16_SFLOAT,
            dragonglass_world::Format::R16G16B16F => vk::Format::R16G16B16_SFLOAT,
            dragonglass_world::Format::R16G16B16A16F => vk::Format::R16G16B16A16_SFLOAT,

            dragonglass_world::Format::R32 => vk::Format::R32_UINT,
            dragonglass_world::Format::R32G32 => vk::Format::R32G32_UINT,
            dragonglass_world::Format::R32G32B32 => vk::Format::R32G32B32_UINT,
            dragonglass_world::Format::R32G32B32A32 => vk::Format::R32G32B32A32_UINT,

            dragonglass_world::Format::R32F => vk::Format::R32_SFLOAT,
            dragonglass_world::Format::R32G32F => vk::Format::R32G32_SFLOAT,
            dragonglass_world::Format::R32G32B32F => vk::Format::R32G32B32_SFLOAT,
            dragonglass_world::Format::R32G32B32A32F => vk::Format::R32G32B32A32_SFLOAT,
        }
    }

    pub fn calculate_mip_levels(width: u32, height: u32) -> u32 {
        ((width.min(height) as f32).log2().floor() + 1.0) as u32
    }

    fn convert_24bit_formats(&mut self) -> Result<()> {
        // 24-bit formats are unsupported, so they
        // need to have an alpha channel added to make them 32-bit
        let format = match self.format {
            vk::Format::R8G8B8_UNORM => vk::Format::R8G8B8A8_UNORM,
            vk::Format::B8G8R8_UNORM => vk::Format::B8G8R8A8_UNORM,
            _ => return Ok(()),
        };
        self.format = format;
        self.attach_alpha_channel()
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

    pub fn as_image(
        &self,
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
    ) -> Result<AllocatedImage> {
        self.create_image(device, allocator, vk::ImageCreateFlags::empty(), 1)
    }

    pub fn as_cubemap(
        &self,
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
    ) -> Result<AllocatedImage> {
        self.create_image(device, allocator, vk::ImageCreateFlags::CUBE_COMPATIBLE, 6)
    }

    fn create_image(
        &self,
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        flags: vk::ImageCreateFlags,
        layers: u32,
    ) -> Result<AllocatedImage> {
        let extent = vk::Extent3D::builder()
            .width(self.width)
            .height(self.height)
            .depth(1);

        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent.build())
            .mip_levels(self.mip_levels)
            .array_layers(layers)
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
            .flags(flags);

        AllocatedImage::new(device, allocator, &create_info)
    }
}

pub fn transition_image(
    image: vk::Image,
    pool: &CommandPool,
    info: &ImageLayoutTransition,
) -> Result<()> {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(info.base_mip_level)
        .level_count(info.level_count)
        .layer_count(info.layer_count)
        .build();
    let image_barrier = vk::ImageMemoryBarrier::builder()
        .old_layout(info.old_layout)
        .new_layout(info.new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(subresource_range)
        .src_access_mask(info.src_access_mask)
        .dst_access_mask(info.dst_access_mask)
        .build();
    let pipeline_barrier_info = PipelineBarrierBuilder::default()
        .src_stage_mask(info.src_stage_mask)
        .dst_stage_mask(info.dst_stage_mask)
        .image_memory_barriers(vec![image_barrier])
        .build()?;
    pool.transition_image_layout(&pipeline_barrier_info)?;
    Ok(())
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
    allocation: Allocation,
    allocator: Arc<RwLock<Allocator>>,
    device: Arc<Device>,
}

impl Image for AllocatedImage {
    fn handle(&self) -> vk::Image {
        self.handle
    }
}

impl AllocatedImage {
    pub fn new(
        device: Arc<Device>,
        allocator: Arc<RwLock<Allocator>>,
        image_create_info: &vk::ImageCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_image(image_create_info, None) }?;
        let requirements = unsafe { device.handle.get_image_memory_requirements(handle) };
        let allocation_create_info = AllocationCreateDesc {
            // TODO: Allow custom naming allocations
            name: "Image Allocation",
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: true, // Linear texture
        };
        let allocation = {
            let mut allocator = allocator.write().expect("Failed to acquire allocator!");
            allocator.allocate(&allocation_create_info)?
        };
        unsafe {
            device
                .handle
                .bind_image_memory(handle, allocation.memory(), allocation.offset())?
        };
        Ok(Self {
            handle,
            allocation,
            allocator,
            device,
        })
    }

    pub fn upload_data(
        &self,
        context: &Context,
        pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<()> {
        let buffer = CpuToGpuBuffer::staging_buffer(
            self.device.clone(),
            self.allocator.clone(),
            self.allocation.size(),
        )?;
        buffer.upload_data(&description.pixels, 0)?;
        self.transition_base_to_transfer_dst(pool, description.mip_levels)?;
        self.copy_to_gpu_buffer(pool, buffer.handle(), description)?;
        context.ensure_linear_blitting_supported(description.format)?;
        self.generate_mipmaps(pool, description)?;
        self.transition_base_to_shader_read(pool, description.mip_levels - 1)?;
        Ok(())
    }

    fn transition_base_to_transfer_dst(&self, pool: &CommandPool, level_count: u32) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .level_count(level_count)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .src_stage_mask(vk::PipelineStageFlags::TOP_OF_PIPE)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .build()?;
        transition_image(self.handle, pool, &transition)
    }

    fn transition_base_to_shader_read(
        &self,
        pool: &CommandPool,
        base_mip_level: u32,
    ) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .base_mip_level(base_mip_level)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .build()?;
        transition_image(self.handle, pool, &transition)
    }

    fn transition_mip_transfer_dst_to_src(
        &self,
        pool: &CommandPool,
        base_mip_level: u32,
    ) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .base_mip_level(base_mip_level)
            .level_count(1)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .build()?;
        transition_image(self.handle, pool, &transition)
    }

    fn transition_mip_to_shader_read(&self, pool: &CommandPool, base_mip_level: u32) -> Result<()> {
        let transition = ImageLayoutTransitionBuilder::default()
            .base_mip_level(base_mip_level)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .build()?;
        transition_image(self.handle, pool, &transition)
    }

    fn copy_to_gpu_buffer(
        &self,
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
            .source(buffer)
            .destination(self.handle)
            .regions(vec![region])
            .build()?;
        pool.copy_buffer_to_image(&copy_info)?;
        Ok(())
    }

    pub fn generate_mipmaps(
        &self,
        pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<()> {
        let mut width = description.width as i32;
        let mut height = description.height as i32;
        for level in 1..description.mip_levels {
            self.transition_mip_transfer_dst_to_src(pool, level - 1)?;
            let dimensions = MipmapBlitDimensions::new(width, height);
            self.blit_mipmap(pool, &dimensions, level)?;
            self.transition_mip_to_shader_read(pool, level - 1)?;
            width = dimensions.next_width;
            height = dimensions.next_height;
        }
        Ok(())
    }

    fn blit_mipmap(
        &self,
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
            .src_image(self.handle)
            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .dst_image(self.handle)
            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .regions(vec![regions])
            .filter(vk::Filter::LINEAR)
            .build()?;

        pool.blit_image(&blit_image_info)
    }
}

impl Drop for AllocatedImage {
    fn drop(&mut self) {
        let mut allocator = self
            .allocator
            .write()
            .expect("Failed to acquire allocator!");
        allocator
            .free(self.allocation.clone())
            .expect("Failed to free allocated image!");
        unsafe { self.device.handle.destroy_image(self.handle, None) };
    }
}

pub struct ImageView {
    pub handle: vk::ImageView,
    device: Arc<Device>,
}

impl ImageView {
    pub fn new(device: Arc<Device>, create_info: vk::ImageViewCreateInfoBuilder) -> Result<Self> {
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
    device: Arc<Device>,
}

impl Sampler {
    pub fn new(device: Arc<Device>, create_info: vk::SamplerCreateInfoBuilder) -> Result<Self> {
        let handle = unsafe { device.handle.create_sampler(&create_info, None) }?;
        let sampler = Self { handle, device };
        Ok(sampler)
    }

    pub fn default(device: Arc<Device>) -> Result<Self> {
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
            .max_lod(1.0);
        Self::new(device, sampler_info)
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

pub struct Texture {
    pub image: AllocatedImage,
    pub view: ImageView,
}

impl Texture {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<Self> {
        let image = description.as_image(context.device.clone(), context.allocator.clone())?;
        image.upload_data(context, command_pool, description)?;
        let view = Self::image_view(context.device.clone(), &image, description)?;
        let texture = Self { image, view };
        Ok(texture)
    }

    fn image_view(
        device: Arc<Device>,
        image: &AllocatedImage,
        description: &ImageDescription,
    ) -> Result<ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1)
            .level_count(description.mip_levels);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(description.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(subresource_range.build());

        ImageView::new(device, create_info)
    }
}

pub struct Cubemap {
    pub image: AllocatedImage,
    pub view: ImageView,
    pub sampler: Sampler,
}

impl Cubemap {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<Self> {
        let image = description.as_cubemap(context.device.clone(), context.allocator.clone())?;
        if !description.pixels.is_empty() {
            image.upload_data(context, command_pool, description)?;
        }
        let view = Self::image_view(context.device.clone(), &image, description)?;
        let sampler = Self::sampler(context.device.clone(), description.mip_levels as _)?;
        Ok(Self {
            image,
            view,
            sampler,
        })
    }

    fn image_view(
        device: Arc<Device>,
        image: &AllocatedImage,
        description: &ImageDescription,
    ) -> Result<ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(6)
            .level_count(description.mip_levels);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::CUBE)
            .format(description.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(subresource_range.build());

        ImageView::new(device, create_info)
    }

    fn sampler(device: Arc<Device>, mip_levels: f32) -> Result<Sampler> {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(mip_levels);
        Sampler::new(device, sampler_info)
    }
}
