use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    gltf::{global_transform, Asset, Geometry, Vertex},
    resources::{
        AllocatedImage, CpuToGpuBuffer, GeometryBuffer, ImageDescription, ImageView, Sampler,
        ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use gltf::material::AlphaMode;
use nalgebra_glm as glm;
use petgraph::{graph::NodeIndex, visit::Dfs};
use std::{mem, sync::Arc};

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    std::slice::from_raw_parts(data_ptr, std::mem::size_of::<T>())
}

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

pub struct PushConstantMaterial {
    pub base_color_factor: glm::Vec4,
    pub emissive_factor: glm::Vec3,
    pub color_texture_set: i32,
    pub metallic_roughness_texture_set: i32, // B channel - metalness values. G channel - roughness values
    pub normal_texture_set: i32,
    pub occlusion_texture_set: i32, // R channel - occlusion values
    pub emissive_texture_set: i32,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub alpha_mode: i32,
    pub alpha_cutoff: f32,
}

impl Default for PushConstantMaterial {
    fn default() -> Self {
        Self {
            base_color_factor: glm::vec4(0.0, 0.0, 0.0, 1.0),
            emissive_factor: glm::Vec3::identity(),
            color_texture_set: -1,
            metallic_roughness_texture_set: -1,
            normal_texture_set: -1,
            occlusion_texture_set: -1,
            emissive_texture_set: -1,
            metallic_factor: 0.0,
            roughness_factor: 0.0,
            alpha_mode: gltf::material::AlphaMode::Opaque as i32,
            alpha_cutoff: 0.0,
        }
    }
}

impl PushConstantMaterial {
    fn from_gltf(primitive_material: &gltf::Material) -> Result<Self> {
        let mut material = Self::default();
        let pbr = primitive_material.pbr_metallic_roughness();
        material.base_color_factor = glm::Vec4::from(pbr.base_color_factor());
        material.metallic_factor = pbr.metallic_factor();
        material.roughness_factor = pbr.roughness_factor();
        material.emissive_factor = glm::Vec3::from(primitive_material.emissive_factor());
        material.alpha_mode = primitive_material.alpha_mode() as i32;
        material.alpha_cutoff = primitive_material.alpha_cutoff();
        if let Some(base_color_texture) = pbr.base_color_texture() {
            material.color_texture_set = base_color_texture.texture().index() as i32;
        }
        if let Some(metallic_roughness_texture) = pbr.metallic_roughness_texture() {
            material.metallic_roughness_texture_set =
                metallic_roughness_texture.texture().index() as i32;
        }
        if let Some(normal_texture) = primitive_material.normal_texture() {
            material.normal_texture_set = normal_texture.texture().index() as i32;
        }
        if let Some(occlusion_texture) = primitive_material.occlusion_texture() {
            material.occlusion_texture_set = occlusion_texture.texture().index() as i32;
        }
        if let Some(emissive_texture) = primitive_material.emissive_texture() {
            material.emissive_texture_set = emissive_texture.texture().index() as i32;
        }

        Ok(material)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AssetUniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct NodeDynamicUniformBuffer {
    pub model: glm::Mat4,
}

pub struct GltfPipelineData {
    pub uniform_buffer: CpuToGpuBuffer,
    pub dynamic_uniform_buffer: CpuToGpuBuffer,
    pub dynamic_alignment: u64,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
    pub textures: Vec<Texture>,
    pub geometry_buffer: GeometryBuffer,
    pub dummy: Texture,
}

impl GltfPipelineData {
    // This should match the number of textures defined in the shader
    pub const MAX_TEXTURES: usize = 100;

    pub fn new(context: &Context, command_pool: &CommandPool, asset: &Asset) -> Result<Self> {
        let textures = load_textures(context, command_pool, &asset.textures)?;
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        let uniform_buffer = CpuToGpuBuffer::uniform_buffer(
            allocator.clone(),
            mem::size_of::<AssetUniformBuffer>() as _,
        )?;

        let dynamic_alignment = context.dynamic_alignment_of::<NodeDynamicUniformBuffer>();
        let number_of_nodes = asset.nodes.len(); // TODO: Maybe only data is needed per-mesh rather than per-node
        let dynamic_uniform_buffer = CpuToGpuBuffer::uniform_buffer(
            allocator,
            (number_of_nodes as u64 * dynamic_alignment) as vk::DeviceSize,
        )?;

        let geometry_buffer = Self::geometry_buffer(context, command_pool, &asset.geometry)?;

        let empty_description = ImageDescription::empty(1, 1, vk::Format::R8G8B8A8_UNORM);
        let dummy = Texture::new(context, command_pool, &empty_description)?;

        let mut data = Self {
            descriptor_pool,
            uniform_buffer,
            dynamic_uniform_buffer,
            descriptor_set,
            dynamic_alignment,
            dummy,
            descriptor_set_layout,
            textures,
            geometry_buffer,
        };
        data.update_descriptor_set(device, number_of_nodes);
        data.update_dynamic_ubo(asset)?;
        Ok(data)
    }

    pub fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
        let ubo_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .build();
        let dynamic_ubo_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .build();
        let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(2)
            .descriptor_count(Self::MAX_TEXTURES as _)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let bindings = [ubo_binding, dynamic_ubo_binding, sampler_binding];
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        DescriptorSetLayout::new(device, create_info)
    }

    fn descriptor_pool(device: Arc<Device>) -> Result<DescriptorPool> {
        let ubo_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        };

        let dynamic_ubo_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
            descriptor_count: 1,
        };

        let sampler_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: Self::MAX_TEXTURES as _,
        };

        let pool_sizes = [ubo_pool_size, dynamic_ubo_pool_size, sampler_pool_size];

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, create_info)
    }

    fn geometry_buffer(
        context: &Context,
        pool: &CommandPool,
        geometry: &Geometry,
    ) -> Result<GeometryBuffer> {
        let has_indices = !geometry.indices.is_empty();
        let index_buffer_size = if has_indices {
            Some((geometry.indices.len() * std::mem::size_of::<u32>()) as _)
        } else {
            None
        };

        let geometry_buffer = GeometryBuffer::new(
            context.allocator.clone(),
            (geometry.vertices.len() * std::mem::size_of::<Vertex>()) as _,
            index_buffer_size,
        )?;

        geometry_buffer.vertex_buffer.upload_data(
            &geometry.vertices,
            0,
            pool,
            context.graphics_queue(),
        )?;

        if has_indices {
            geometry_buffer
                .index_buffer
                .as_ref()
                .context("Failed to access index buffer!")?
                .upload_data(&geometry.indices, 0, pool, context.graphics_queue())?;
        }

        Ok(geometry_buffer)
    }

    fn update_descriptor_set(&self, device: Arc<Device>, number_of_nodes: usize) {
        let uniform_buffer_size = mem::size_of::<AssetUniformBuffer>() as vk::DeviceSize;
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.uniform_buffer.handle())
            .offset(0)
            .range(uniform_buffer_size)
            .build();
        let buffer_infos = [buffer_info];

        let dynamic_uniform_buffer_size =
            (number_of_nodes as u64 * self.dynamic_alignment) as vk::DeviceSize;
        let dynamic_buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.dynamic_uniform_buffer.handle())
            .offset(0)
            .range(dynamic_uniform_buffer_size)
            .build();
        let dynamic_buffer_infos = [dynamic_buffer_info];

        let mut image_infos = self
            .textures
            .iter()
            .map(|texture| {
                vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(texture.view.handle)
                    .sampler(texture.sampler.handle)
                    .build()
            })
            .collect::<Vec<_>>();

        let number_of_images = image_infos.len();
        let required_images = Self::MAX_TEXTURES;
        if number_of_images < required_images {
            let remaining = required_images - number_of_images;
            for _ in 0..remaining {
                image_infos.push(
                    vk::DescriptorImageInfo::builder()
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image_view(self.dummy.view.handle)
                        .sampler(self.dummy.sampler.handle)
                        .build(),
                );
            }
        }

        let ubo_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_infos)
            .build();

        let dynamic_ubo_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .buffer_info(&dynamic_buffer_infos)
            .build();

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        let descriptor_writes = [
            ubo_descriptor_write,
            dynamic_ubo_descriptor_write,
            sampler_descriptor_write,
        ];

        unsafe {
            device
                .handle
                .update_descriptor_sets(&descriptor_writes, &[])
        }
    }

    fn update_dynamic_ubo(&mut self, asset: &Asset) -> Result<()> {
        let mut buffers = vec![NodeDynamicUniformBuffer::default(); asset.nodes.len()];
        for scene in asset.scenes.iter() {
            for graph in scene.graphs.iter() {
                let mut dfs = Dfs::new(graph, NodeIndex::new(0));
                while let Some(node_index) = dfs.next(&graph) {
                    let offset = graph[node_index];
                    let model = global_transform(graph, node_index, &asset.nodes);
                    buffers[offset] = NodeDynamicUniformBuffer { model };
                }
            }
        }
        let alignment = self.dynamic_alignment;
        self.dynamic_uniform_buffer
            .upload_data_aligned(&buffers, 0, alignment)?;
        Ok(())
    }
}

pub struct GltfRenderer {
    command_buffer: vk::CommandBuffer,
    pipeline_layout: vk::PipelineLayout,
    dynamic_alignment: u64,
    descriptor_set: vk::DescriptorSet,
    has_indices: bool,
}

impl GltfRenderer {
    pub fn new(
        command_buffer: vk::CommandBuffer,
        pipeline_layout: &PipelineLayout,
        pipeline_data: &GltfPipelineData,
        has_indices: bool,
    ) -> Self {
        Self {
            command_buffer,
            pipeline_layout: pipeline_layout.handle,
            dynamic_alignment: pipeline_data.dynamic_alignment,
            descriptor_set: pipeline_data.descriptor_set,
            has_indices,
        }
    }

    pub fn draw_asset(
        &self,
        device: &ash::Device,
        asset: &Asset,
        alpha_mode: AlphaMode,
    ) -> Result<()> {
        for scene in asset.scenes.iter() {
            for graph in scene.graphs.iter() {
                let mut dfs = Dfs::new(graph, NodeIndex::new(0));
                while let Some(node_index) = dfs.next(&graph) {
                    let node_offset = graph[node_index];
                    let node = &asset.nodes[node_offset];
                    let mesh = match node.mesh.as_ref() {
                        Some(mesh) => mesh,
                        _ => continue,
                    };

                    unsafe {
                        device.cmd_bind_descriptor_sets(
                            self.command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.pipeline_layout,
                            0,
                            &[self.descriptor_set],
                            &[(node_offset as u64 * self.dynamic_alignment) as _],
                        );
                    }

                    for primitive in mesh.primitives.iter() {
                        if let Some(material_index) = primitive.material_index {
                            let primitive_material = asset.material_at_index(material_index)?;
                            if primitive_material.alpha_mode() != alpha_mode {
                                continue;
                            }

                            let material = PushConstantMaterial::from_gltf(&primitive_material)?;

                            unsafe {
                                device.cmd_push_constants(
                                    self.command_buffer,
                                    self.pipeline_layout,
                                    vk::ShaderStageFlags::ALL_GRAPHICS,
                                    0,
                                    byte_slice_from(&material),
                                );

                                if self.has_indices {
                                    device.cmd_draw_indexed(
                                        self.command_buffer,
                                        primitive.number_of_indices as _,
                                        1,
                                        primitive.first_index as _,
                                        0,
                                        0,
                                    );
                                } else {
                                    device.cmd_draw(
                                        self.command_buffer,
                                        primitive.number_of_vertices as _,
                                        1,
                                        primitive.first_vertex as _,
                                        0,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct AssetRendering {
    pub asset: Arc<Asset>,
    pub pipeline_data: GltfPipelineData,
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    device: Arc<Device>,
}

impl AssetRendering {
    pub fn new(context: &Context, command_pool: &CommandPool, asset: Arc<Asset>) -> Result<Self> {
        let pipeline_data = GltfPipelineData::new(context, command_pool, &asset)?;
        Ok(Self {
            pipeline: None,
            pipeline_layout: None,
            pipeline_data,
            device: context.device.clone(),
            asset,
        })
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/gltf/gltf.vert.spv")
            .fragment("assets/shaders/gltf/gltf.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(shader_path_set)
    }

    pub fn create_pipeline(
        &mut self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
    ) -> Result<()> {
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
            .size(mem::size_of::<PushConstantMaterial>() as u32)
            .build();

        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let settings = GraphicsPipelineSettingsBuilder::default()
            .render_pass(render_pass)
            .vertex_inputs(vertex_inputs())
            .vertex_attributes(vertex_attributes())
            .descriptor_set_layout(self.pipeline_data.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .sample_shading_enabled(true)
            .cull_mode(vk::CullModeFlags::NONE)
            .push_constant_range(push_constant_range)
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        self.pipeline = None;
        self.pipeline_layout = None;
        let (pipeline, pipeline_layout) = settings.create_pipeline(self.device.clone())?;
        self.pipeline = Some(pipeline);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get pipeline for rendering asset!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering asset!")?;

        let renderer = GltfRenderer::new(
            command_buffer,
            pipeline_layout,
            &self.pipeline_data,
            self.pipeline_data.geometry_buffer.index_buffer.is_some(),
        );

        self.pipeline_data
            .geometry_buffer
            .bind(&self.device.handle, command_buffer)?;

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            match alpha_mode {
                AlphaMode::Opaque => {
                    pipeline.bind(&self.device.handle, command_buffer);
                    renderer.draw_asset(&self.device.handle, &self.asset, *alpha_mode)?;
                }
                AlphaMode::Blend => { /* TODO: Implement a blend pipeline */ }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn update_ubo(&self, aspect_ratio: f32, view: glm::Mat4) -> Result<()> {
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);
        let ubo = AssetUniformBuffer { view, projection };
        self.pipeline_data.uniform_buffer.upload_data(&[ubo], 0)?;
        Ok(())
    }
}

fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 3] {
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

    [
        position_description,
        normal_description,
        tex_coord_0_description,
    ]
}

fn vertex_inputs() -> [vk::VertexInputBindingDescription; 1] {
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
