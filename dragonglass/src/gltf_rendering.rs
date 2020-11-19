use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    resources::{
        CpuToGpuBuffer, GeometryBuffer, ImageDescription, Sampler, ShaderCache, ShaderPathSet,
        ShaderPathSetBuilder, Texture,
    },
};
use anyhow::{anyhow, ensure, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use dragonglass_scene::{global_transform, walk_scenegraph, Asset, Geometry, Node, Scene, Vertex};
use gltf::material::AlphaMode;
use nalgebra_glm as glm;
use petgraph::{graph::NodeIndex, visit::Dfs};
use std::{mem, sync::Arc};

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    std::slice::from_raw_parts(data_ptr, std::mem::size_of::<T>())
}

#[derive(Debug)]
pub struct PushConstantMaterial {
    pub base_color_factor: glm::Vec4,
    pub emissive_factor: glm::Vec3,
    pub color_texture_index: i32,
    pub color_texture_set: i32,
    pub metallic_roughness_texture_index: i32,
    pub metallic_roughness_texture_set: i32, // B channel - metalness values. G channel - roughness values
    pub normal_texture_index: i32,
    pub normal_texture_set: i32,
    pub normal_texture_scale: f32,
    pub occlusion_texture_index: i32,
    pub occlusion_texture_set: i32, // R channel - occlusion values
    pub occlusion_strength: f32,
    pub emissive_texture_index: i32,
    pub emissive_texture_set: i32,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub alpha_mode: i32,
    pub alpha_cutoff: f32,
    pub is_unlit: i32,
}

impl Default for PushConstantMaterial {
    fn default() -> Self {
        Self {
            base_color_factor: glm::vec4(1.0, 1.0, 1.0, 1.0),
            emissive_factor: glm::Vec3::identity(),
            color_texture_index: -1,
            color_texture_set: -1,
            metallic_roughness_texture_index: -1,
            metallic_roughness_texture_set: -1,
            normal_texture_index: -1,
            normal_texture_set: -1,
            normal_texture_scale: 1.0,
            occlusion_texture_index: -1,
            occlusion_texture_set: -1,
            occlusion_strength: 1.0,
            emissive_texture_index: -1,
            emissive_texture_set: -1,
            metallic_factor: 1.0,
            roughness_factor: 0.0,
            alpha_mode: gltf::material::AlphaMode::Opaque as i32,
            alpha_cutoff: 0.0,
            is_unlit: 0,
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
        material.is_unlit = if primitive_material.unlit() { 1 } else { 0 };
        if let Some(base_color_texture) = pbr.base_color_texture() {
            material.color_texture_index = base_color_texture.texture().source().index() as i32;
            material.color_texture_set = base_color_texture.tex_coord() as i32;
        }
        if let Some(metallic_roughness_texture) = pbr.metallic_roughness_texture() {
            material.metallic_roughness_texture_index =
                metallic_roughness_texture.texture().source().index() as i32;
            material.metallic_roughness_texture_set = metallic_roughness_texture.tex_coord() as i32;
        }
        if let Some(normal_texture) = primitive_material.normal_texture() {
            material.normal_texture_index = normal_texture.texture().source().index() as i32;
            material.normal_texture_set = normal_texture.tex_coord() as i32;
            material.normal_texture_scale = normal_texture.scale();
        }
        if let Some(occlusion_texture) = primitive_material.occlusion_texture() {
            material.occlusion_texture_index = occlusion_texture.texture().source().index() as i32;
            material.occlusion_texture_set = occlusion_texture.tex_coord() as i32;
            material.occlusion_strength = occlusion_texture.strength();
        }
        if let Some(emissive_texture) = primitive_material.emissive_texture() {
            material.emissive_texture_index = emissive_texture.texture().source().index() as i32;
            material.emissive_texture_set = emissive_texture.tex_coord() as i32;
        }

        Ok(material)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AssetUniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    pub camera_position: glm::Vec4,
    pub joint_matrices: [glm::Mat4; GltfPipelineData::MAX_NUMBER_OF_JOINTS],
}

#[derive(Default, Debug, Clone, Copy)]
pub struct NodeDynamicUniformBuffer {
    pub model: glm::Mat4,
    // X value is the joint count.
    // Y value is the joint matrix offset.
    // A vec4 is necessary for proper alignment
    pub joint_info: glm::Vec4,
}

pub struct GltfPipelineData {
    pub uniform_buffer: CpuToGpuBuffer,
    pub dynamic_uniform_buffer: CpuToGpuBuffer,
    pub dynamic_alignment: u64,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
    pub textures: Vec<Texture>,
    pub samplers: Vec<Sampler>,
    pub geometry_buffer: GeometryBuffer,
    pub dummy_texture: Texture,
    pub dummy_sampler: Sampler,
}

impl GltfPipelineData {
    // These should match the constants defined in the shader
    pub const MAX_NUMBER_OF_TEXTURES: usize = 200; // TODO: check that this is not larger than the physical device's maxDescriptorSetSamplers
    pub const MAX_NUMBER_OF_JOINTS: usize = 128;

    pub fn new(context: &Context, command_pool: &CommandPool, asset: &Asset) -> Result<Self> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let mut textures = Vec::new();
        let mut samplers = Vec::new();
        for (texture, gltf_texture) in asset.textures.iter().zip(asset.gltf.textures()) {
            let description = ImageDescription::from_gltf(texture)?;
            let texture = Texture::new(context, command_pool, &description)?;
            textures.push(texture);

            let sampler = sampler_from_gltf(
                device.clone(),
                description.mip_levels,
                &gltf_texture.sampler(),
            )?;
            samplers.push(sampler);
        }

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
        let dummy_texture = Texture::new(context, command_pool, &empty_description)?;
        let dummy_sampler = Sampler::default(device.clone())?;

        let data = Self {
            descriptor_pool,
            uniform_buffer,
            dynamic_uniform_buffer,
            descriptor_set,
            dynamic_alignment,
            descriptor_set_layout,
            textures,
            samplers,
            geometry_buffer,
            dummy_texture,
            dummy_sampler,
        };
        data.update_descriptor_set(device);
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
            .descriptor_count(Self::MAX_NUMBER_OF_TEXTURES as _)
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
            descriptor_count: Self::MAX_NUMBER_OF_TEXTURES as _,
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

    fn update_descriptor_set(&self, device: Arc<Device>) {
        let uniform_buffer_size = mem::size_of::<AssetUniformBuffer>() as vk::DeviceSize;
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.uniform_buffer.handle())
            .offset(0)
            .range(uniform_buffer_size)
            .build();
        let buffer_infos = [buffer_info];

        let dynamic_buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.dynamic_uniform_buffer.handle())
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();
        let dynamic_buffer_infos = [dynamic_buffer_info];

        let mut image_infos = self
            .textures
            .iter()
            .zip(self.samplers.iter())
            .map(|(texture, sampler)| {
                vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(texture.view.handle)
                    .sampler(sampler.handle)
                    .build()
            })
            .collect::<Vec<_>>();

        let number_of_images = image_infos.len();
        let required_images = Self::MAX_NUMBER_OF_TEXTURES;
        if number_of_images < required_images {
            let remaining = required_images - number_of_images;
            for _ in 0..remaining {
                image_infos.push(
                    vk::DescriptorImageInfo::builder()
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image_view(self.dummy_texture.view.handle)
                        .sampler(self.dummy_sampler.handle)
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

    pub fn update_dynamic_ubo(&self, asset: &Asset) -> Result<()> {
        let asset_joint_matrices = asset.joint_matrices()?;
        let number_of_joints = asset_joint_matrices.len();
        ensure!(
            number_of_joints < Self::MAX_NUMBER_OF_JOINTS,
            "Too many joints in asset: {}/{}",
            number_of_joints,
            Self::MAX_NUMBER_OF_JOINTS
        );

        let scene = asset
            .scenes
            .first()
            .context("Failed to get first scene to render!")?;
        self.update_node_ubos(scene, &asset.nodes)?;

        Ok(())
    }

    fn update_node_ubos(&self, scene: &Scene, nodes: &[Node]) -> Result<()> {
        let mut buffers = vec![NodeDynamicUniformBuffer::default(); nodes.len()];
        let mut joint_offset = 0;
        for graph in scene.graphs.iter() {
            walk_scenegraph(graph, |node_index| {
                let offset = graph[node_index];
                let model = global_transform(graph, node_index, nodes);

                let mut joint_info = glm::vec4(0.0, 0.0, 0.0, 0.0);
                if let Some(skin) = nodes[offset].skin.as_ref() {
                    let joint_count = skin.joints.len();
                    joint_info = glm::vec4(joint_count as f32, joint_offset as f32, 0.0, 0.0);
                    joint_offset += joint_count;
                }

                buffers[offset] = NodeDynamicUniformBuffer { model, joint_info };

                Ok(())
            })?;
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
        let scene = asset
            .scenes
            .first()
            .context("Failed to get first scene to render!")?;
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
                    let material = match primitive.material_index {
                        Some(material_index) => {
                            let primitive_material = asset.material_at_index(material_index)?;
                            if primitive_material.alpha_mode() != alpha_mode {
                                continue;
                            }

                            PushConstantMaterial::from_gltf(&primitive_material)?
                        }
                        None => PushConstantMaterial::default(),
                    };

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
        Ok(())
    }
}

pub struct AssetRendering {
    pub pipeline_data: GltfPipelineData,
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_blended: Option<GraphicsPipeline>,
    pub pipeline_wireframe: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub wireframe_enabled: bool,
    device: Arc<Device>,
}

impl AssetRendering {
    pub fn new(context: &Context, command_pool: &CommandPool, asset: &Asset) -> Result<Self> {
        let pipeline_data = GltfPipelineData::new(context, command_pool, &asset)?;
        Ok(Self {
            pipeline_data,
            pipeline: None,
            pipeline_blended: None,
            pipeline_wireframe: None,
            pipeline_layout: None,
            wireframe_enabled: false,
            device: context.device.clone(),
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

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(vertex_inputs())
            .vertex_attributes(vertex_attributes())
            .descriptor_set_layout(self.pipeline_data.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .sample_shading_enabled(true)
            .cull_mode(vk::CullModeFlags::NONE)
            .push_constant_range(push_constant_range);

        let mut blend_settings = settings.clone();
        blend_settings.blended(true);

        let mut wireframe_settings = settings.clone();
        wireframe_settings.polygon_mode(vk::PolygonMode::LINE);

        self.pipeline = None;
        self.pipeline_blended = None;
        self.pipeline_wireframe = None;
        self.pipeline_layout = None;

        // TODO: Reuse the pipeline layout across these pipelines since they are the same
        let (pipeline, pipeline_layout) = settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        let (pipeline_blended, _) = blend_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        let (pipeline_wireframe, _) = wireframe_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        self.pipeline = Some(pipeline);
        self.pipeline_blended = Some(pipeline_blended);
        self.pipeline_wireframe = Some(pipeline_wireframe);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer, asset: &Asset) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get pipeline for rendering asset!")?;

        let pipeline_blended = self
            .pipeline_blended
            .as_ref()
            .context("Failed to get blend pipeline for rendering asset!")?;

        let pipeline_wireframe = self
            .pipeline_wireframe
            .as_ref()
            .context("Failed to get wireframe pipeline for rendering asset!")?;

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
            if self.wireframe_enabled {
                pipeline_wireframe.bind(&self.device.handle, command_buffer);
            } else {
                match alpha_mode {
                    AlphaMode::Opaque | AlphaMode::Mask => {
                        pipeline.bind(&self.device.handle, command_buffer);
                    }
                    AlphaMode::Blend => {
                        pipeline_blended.bind(&self.device.handle, command_buffer);
                    }
                }
            }
            renderer.draw_asset(&self.device.handle, asset, *alpha_mode)?;
        }

        Ok(())
    }
}

fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 7] {
    let float_size = std::mem::size_of::<f32>();

    let position = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(0)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset(0)
        .build();

    let normal = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(1)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset((3 * float_size) as _)
        .build();

    let uv_0 = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(2)
        .format(vk::Format::R32G32_SFLOAT)
        .offset((6 * float_size) as _)
        .build();

    let uv_1 = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(3)
        .format(vk::Format::R32G32_SFLOAT)
        .offset((8 * float_size) as _)
        .build();

    let joint_0 = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(4)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset((10 * float_size) as _)
        .build();

    let weight_0 = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(5)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset((14 * float_size) as _)
        .build();

    let color_0 = vk::VertexInputAttributeDescription::builder()
        .binding(0)
        .location(6)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset((18 * float_size) as _)
        .build();

    [position, normal, uv_0, uv_1, joint_0, weight_0, color_0]
}

fn vertex_inputs() -> [vk::VertexInputBindingDescription; 1] {
    let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(std::mem::size_of::<Vertex>() as _)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    [vertex_input_binding_description]
}

fn sampler_from_gltf(
    device: Arc<Device>,
    mip_levels: u32,
    sampler: &gltf::texture::Sampler,
) -> Result<Sampler> {
    let mut min_filter = vk::Filter::LINEAR;
    let mut mipmap_mode = vk::SamplerMipmapMode::LINEAR;
    if let Some(min) = sampler.min_filter() {
        min_filter = match min {
            gltf::texture::MinFilter::Linear
            | gltf::texture::MinFilter::LinearMipmapLinear
            | gltf::texture::MinFilter::LinearMipmapNearest => vk::Filter::LINEAR,
            gltf::texture::MinFilter::Nearest
            | gltf::texture::MinFilter::NearestMipmapLinear
            | gltf::texture::MinFilter::NearestMipmapNearest => vk::Filter::NEAREST,
        };
        mipmap_mode = match min {
            gltf::texture::MinFilter::Linear
            | gltf::texture::MinFilter::LinearMipmapLinear
            | gltf::texture::MinFilter::LinearMipmapNearest => vk::SamplerMipmapMode::LINEAR,
            gltf::texture::MinFilter::Nearest
            | gltf::texture::MinFilter::NearestMipmapLinear
            | gltf::texture::MinFilter::NearestMipmapNearest => vk::SamplerMipmapMode::NEAREST,
        };
    }

    let mut mag_filter = vk::Filter::LINEAR;
    if let Some(mag) = sampler.mag_filter() {
        mag_filter = match mag {
            gltf::texture::MagFilter::Nearest => vk::Filter::NEAREST,
            gltf::texture::MagFilter::Linear => vk::Filter::LINEAR,
        };
    }

    let address_mode_u = match sampler.wrap_s() {
        gltf::texture::WrappingMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        gltf::texture::WrappingMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        gltf::texture::WrappingMode::Repeat => vk::SamplerAddressMode::REPEAT,
    };

    let address_mode_v = match sampler.wrap_t() {
        gltf::texture::WrappingMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        gltf::texture::WrappingMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        gltf::texture::WrappingMode::Repeat => vk::SamplerAddressMode::REPEAT,
    };

    let address_mode_w = vk::SamplerAddressMode::REPEAT;

    let sampler_info = vk::SamplerCreateInfo::builder()
        .min_filter(min_filter)
        .mag_filter(mag_filter)
        .address_mode_u(address_mode_u)
        .address_mode_v(address_mode_v)
        .address_mode_w(address_mode_w)
        .anisotropy_enable(true)
        .max_anisotropy(16.0)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS)
        .mipmap_mode(mipmap_mode)
        .mip_lod_bias(0.0)
        .min_lod(0.0)
        .max_lod(mip_levels as _);
    Sampler::new(device, sampler_info)
}
