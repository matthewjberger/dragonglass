use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    gltf::{Asset, Geometry, Vertex},
    resources::{
        AllocatedImage, CpuToGpuBuffer, GeometryBuffer, ImageDescription, ImageView, Sampler,
        ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::vk;
use nalgebra_glm as glm;
use petgraph::{graph::NodeIndex, visit::Dfs};
use std::{mem, sync::Arc};
use vk_mem::Allocator;

fn load_textures(
    context: &Context,
    command_pool: &CommandPool,
    textures: &[gltf::image::Data],
) -> Result<Vec<Texture>> {
    textures
        .iter()
        .map(|texture| {
            let description = ImageDescription::from_gltf(&texture)?;
            Texture::new(context, command_pool, &description)
        })
        .collect::<Result<Vec<_>>>()
}

pub struct PushConstantBlockMaterial {
    pub base_color_factor: glm::Vec4,
    // pub emissive_factor: glm::Vec3,
    pub color_texture_set: i32,
    // pub metallic_roughness_texture_set: i32, // B channel - metalness values. G channel - roughness values
    // pub normal_texture_set: i32,
    // pub occlusion_texture_set: i32, // R channel - occlusion values
    // pub emissive_texture_set: i32,
    // pub metallic_factor: f32,
    // pub roughness_factor: f32,
    // pub alpha_mode: i32,
    // pub alpha_cutoff: f32,
}

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
        command_pool: &CommandPool,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
        shader_cache: &mut ShaderCache,
        asset: Asset,
    ) -> Result<Self> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let number_of_meshes = asset.number_of_meshes();
        let number_of_textures = asset.textures.len();
        let textures = load_textures(context, command_pool, &asset.textures)?;

        let descriptor_pool =
            Self::descriptor_pool(device.clone(), number_of_meshes, number_of_textures)?;
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_sets = descriptor_pool
            .allocate_descriptor_sets(descriptor_set_layout.handle, number_of_meshes as _)?;

        let geometry_buffer = Self::geometry_buffer(context, command_pool, &asset.geometry)?;

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
            .vertex_inputs(vertex_inputs().to_vec())
            .vertex_attributes(vertex_attributes().to_vec())
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

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        self.pipeline
            .as_ref()
            .context("Failed to get scene pipeline!")?
            .bind(&self.device.handle, command_buffer);

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get scene pipeline layout!")?
            .handle;

        self.geometry_buffer
            .bind(&self.device.handle, command_buffer)?;

        for scene in self.asset.scenes.iter() {
            for graph in scene.graphs.iter() {
                let mut dfs = Dfs::new(graph, NodeIndex::new(0));
                while let Some(node_index) = dfs.next(&graph) {
                    let node = &self.asset.nodes[graph[node_index]];

                    // Get the descriptor set and bind it
                    // let descriptor_set = self.nodes.iter().filter(|node| node.mesh.is_some()).nth();
                    // unsafe {
                    //     self.device.handle.cmd_bind_descriptor_sets(
                    //         command_buffer,
                    //         vk::PipelineBindPoint::GRAPHICS,
                    //         pipeline_layout,
                    //         0,
                    //         &[self.descriptor_set],
                    //         &[],
                    //     );
                    // }

                    let mesh = match node.mesh.as_ref() {
                        Some(mesh) => mesh,
                        _ => continue,
                    };

                    for primitive in mesh.primitives.iter() {
                        let material = self.asset.material_at_index(primitive.material_index)?;

                        // TODO: Update push constant block here

                        // unsafe {
                        //     self.device.handle.cmd_draw_indexed(
                        //         command_buffer,
                        //         self.number_of_indices as _,
                        //         1,
                        //         0,
                        //         0,
                        //         0,
                        //     )
                        // };
                    }
                }
            }
        }

        Ok(())
    }
}

pub fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 3] {
    let float_size = std::mem::size_of::<f32>();
    let position_description = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(0)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset(0)
        .build();

    let normal_description = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(1)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset((3 * float_size) as _)
        .build();

    let tex_coord_0_description = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(2)
        .format(vk::Format::R32G32_SFLOAT)
        .offset((6 * float_size) as _)
        .build();

    // let tex_coord_1_description = vk::VertexInputAttributeDescription::builder()
    //     .binding(0)
    //     .location(3)
    //     .format(vk::Format::R32G32_SFLOAT)
    //     .offset((8 * float_size) as _)
    //     .build();

    // let joint_0_description = vk::VertexInputAttributeDescription::builder()
    //     .binding(0)
    //     .location(4)
    //     .format(vk::Format::R32G32B32A32_SFLOAT)
    //     .offset((10 * float_size) as _)
    //     .build();

    // let weight_0_description = vk::VertexInputAttributeDescription::builder()
    //     .binding(0)
    //     .location(5)
    //     .format(vk::Format::R32G32B32A32_SFLOAT)
    //     .offset((14 * float_size) as _)
    //     .build();

    [
        position_description,
        normal_description,
        tex_coord_0_description,
        // tex_coord_1_description,
        // joint_0_description,
        // weight_0_description,
    ]
}

pub fn vertex_inputs() -> [vk::VertexInputBindingDescription; 1] {
    let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(std::mem::size_of::<Vertex>() as _)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    [vertex_input_binding_description]
}

pub struct Texture {
    pub image: AllocatedImage,
    pub view: ImageView,
    pub sampler: Sampler, // TODO: Use samplers specified in file
}

impl Texture {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<Self> {
        let image = description.as_image(context.allocator.clone())?;
        image.upload_data(context, command_pool, description)?;
        let view = Self::image_view(context.device.clone(), &image, description)?;
        let sampler = Self::sampler(context.device.clone(), description.mip_levels)?;
        let texture = Self {
            image,
            view,
            sampler,
        };
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

    fn sampler(device: Arc<Device>, mip_levels: u32) -> Result<Sampler> {
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
