use anyhow::*;
use dragonglass_world::{Filter, WrappingMode};

pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float; // 1.

    pub fn empty(device: &wgpu::Device) -> Self {
        let size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Empty Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        Self {
            texture,
            view,
            sampler,
        }
    }

    pub fn from_world_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world_texture: &dragonglass_world::Texture,
        label: &str,
    ) -> Result<Self> {
        let size = wgpu::Extent3d {
            width: world_texture.width,
            height: world_texture.height,
            depth_or_array_layers: 1,
        };

        let format = Self::map_texture_format(world_texture.format);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &world_texture.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(world_texture.bytes_per_row()),
                rows_per_image: std::num::NonZeroU32::new(world_texture.height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("WorldTextureView"),
            format: Some(format),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        let sampler = device.create_sampler(&Self::map_sampler(&world_texture.sampler));

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }

    fn map_texture_format(
        _texture_format: dragonglass_world::TextureFormat,
    ) -> wgpu::TextureFormat {
        // FIXME: Map texture formats
        wgpu::TextureFormat::Rgba8UnormSrgb
    }

    fn map_sampler(sampler: &dragonglass_world::Sampler) -> wgpu::SamplerDescriptor<'static> {
        let min_filter = match sampler.min_filter {
            Filter::Linear => wgpu::FilterMode::Linear,
            Filter::Nearest => wgpu::FilterMode::Nearest,
        };

        let mipmap_filter = match sampler.min_filter {
            Filter::Linear => wgpu::FilterMode::Linear,
            Filter::Nearest => wgpu::FilterMode::Nearest,
        };

        let mag_filter = match sampler.mag_filter {
            Filter::Nearest => wgpu::FilterMode::Nearest,
            Filter::Linear => wgpu::FilterMode::Linear,
        };

        let address_mode_u = match sampler.wrap_s {
            WrappingMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            WrappingMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
            WrappingMode::Repeat => wgpu::AddressMode::Repeat,
        };

        let address_mode_v = match sampler.wrap_t {
            WrappingMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            WrappingMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
            WrappingMode::Repeat => wgpu::AddressMode::Repeat,
        };

        let address_mode_w = wgpu::AddressMode::Repeat;

        wgpu::SamplerDescriptor {
            address_mode_u,
            address_mode_v,
            address_mode_w,
            mag_filter,
            min_filter,
            mipmap_filter,
            ..Default::default()
        }
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        label: &str,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
        }
    }
}
