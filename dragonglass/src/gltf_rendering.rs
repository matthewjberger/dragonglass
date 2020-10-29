use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    gltf::{Asset, Geometry, Node, SceneGraph, Vertex},
    resources::{
        AllocatedImage, CpuToGpuBuffer, GeometryBuffer, ImageDescription, ImageView, Sampler,
        ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use petgraph::graph::NodeIndex;
use std::{mem, sync::Arc};
use vk_mem::Allocator;

pub struct AssetUniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
}

pub struct MeshUniformBuffer {
    pub model: glm::Mat4,
}

pub struct AssetRendering {
    pub asset: Asset, // TODO: Only render a single asset for now
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    pub geometry_buffer: GeometryBuffer,
    pub asset_uniform_buffer: CpuToGpuBuffer,
    pub mesh_uniform_buffers: Vec<CpuToGpuBuffer>,
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    device: Arc<Device>,
}

impl AssetRendering {
    // FIXME: Shorten this parameter list
    pub fn new(
        context: &Context,
        pool: &CommandPool,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
        shader_cache: &mut ShaderCache,
        asset: Asset,
    ) -> Result<Self> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let number_of_meshes = asset.number_of_meshes();
        let number_of_textures = asset.textures.len();

        let descriptor_pool =
            Self::descriptor_pool(device.clone(), number_of_meshes, number_of_textures)?;
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_sets = descriptor_pool
            .allocate_descriptor_sets(descriptor_set_layout.handle, number_of_meshes as _)?;

        let geometry_buffer = Self::geometry_buffer(context, pool, &asset.geometry)?;

        let asset_uniform_buffer = Self::asset_uniform_buffer(allocator.clone())?;
        let mesh_uniform_buffers = Self::mesh_uniform_buffers(allocator, number_of_meshes)?;

        let mut rendering = Self {
            asset,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            geometry_buffer,
            asset_uniform_buffer,
            mesh_uniform_buffers,
            pipeline: None,
            pipeline_layout: None,
            device,
        };

        rendering.create_pipeline(render_pass, samples, shader_cache)?;
        // update descriptor sets

        Ok(rendering)
    }

    fn descriptor_pool(
        device: Arc<Device>,
        number_of_meshes: usize,
        number_of_textures: usize,
    ) -> Result<DescriptorPool> {
        let ubo_pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(number_of_meshes as _)
            .build();
        let sampler_pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(number_of_textures as _)
            .build();
        let pool_sizes = [ubo_pool_size, sampler_pool_size];

        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(number_of_meshes as _);

        DescriptorPool::new(device, pool_info)
    }

    fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
        let ubo_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .build();

        let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();

        let bindings = [ubo_binding, sampler_binding];
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        DescriptorSetLayout::new(device, create_info)
    }

    fn geometry_buffer(
        context: &Context,
        pool: &CommandPool,
        geometry: &Geometry,
    ) -> Result<GeometryBuffer> {
        let geometry_buffer = GeometryBuffer::new(
            context.allocator.clone(),
            (geometry.vertices.len() * std::mem::size_of::<Vertex>()) as _,
            Some((geometry.indices.len() * std::mem::size_of::<u32>()) as _),
        )?;

        geometry_buffer.vertex_buffer.upload_data(
            &geometry.vertices,
            0,
            pool,
            context.graphics_queue(),
        )?;

        geometry_buffer
            .index_buffer
            .as_ref()
            .context("Failed to access index buffer!")?
            .upload_data(&geometry.indices, 0, pool, context.graphics_queue())?;

        Ok(geometry_buffer)
    }

    fn asset_uniform_buffer(allocator: Arc<Allocator>) -> Result<CpuToGpuBuffer> {
        CpuToGpuBuffer::uniform_buffer(allocator, mem::size_of::<AssetUniformBuffer>() as _)
    }

    fn mesh_uniform_buffers(
        allocator: Arc<Allocator>,
        number_of_meshes: usize,
    ) -> Result<Vec<CpuToGpuBuffer>> {
        (0..number_of_meshes)
            .into_iter()
            .map(|_| {
                CpuToGpuBuffer::uniform_buffer(
                    allocator.clone(),
                    mem::size_of::<MeshUniformBuffer>() as _,
                )
            })
            .collect()
    }

    pub fn update_asset_ubo(&self, aspect_ratio: f32, view: glm::Mat4) -> Result<()> {
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);
        let ubo = AssetUniformBuffer { view, projection };
        self.asset_uniform_buffer.upload_data(&[ubo], 0)?;
        Ok(())
    }

    fn update_mesh_ubos(&self) -> Result<()> {
        for (index, uniform_buffer) in self.mesh_uniform_buffers.iter().enumerate() {
            let ubo = MeshUniformBuffer {
                // FIXME: Cache model matrices
                // FIXME: Upload cached model matrices
                model: glm::Mat4::identity(),
            };
            self.asset_uniform_buffer.upload_data(&[ubo], 0)?;
        }

        Ok(())
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        ShaderPathSetBuilder::default()
            .vertex("assets/shaders/object/object.vert.spv")
            .fragment("assets/shaders/object/object.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))
    }

    fn settings(
        &self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        descriptor_set_layout: Arc<DescriptorSetLayout>,
        samples: vk::SampleCountFlags,
    ) -> Result<GraphicsPipelineSettings> {
        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;
        GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_inputs(Vertex::inputs().to_vec())
            .vertex_attributes(Vertex::attributes().to_vec())
            .descriptor_set_layout(descriptor_set_layout)
            .rasterization_samples(samples)
            .build()
            .map_err(|error| anyhow!("{}", error))
    }

    pub fn create_pipeline(
        &mut self,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
        mut shader_cache: &mut ShaderCache,
    ) -> Result<()> {
        let settings = self.settings(
            &mut shader_cache,
            render_pass,
            self.descriptor_set_layout.clone(),
            samples,
        )?;
        self.pipeline = None;
        self.pipeline_layout = None;
        let (pipeline, pipeline_layout) = settings.create_pipeline(self.device.clone())?;
        self.pipeline = Some(pipeline);
        self.pipeline_layout = Some(pipeline_layout);
        Ok(())
    }
}
