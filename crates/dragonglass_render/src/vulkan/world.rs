use crate::byte_slice_from;
use anyhow::{ensure, Context as AnyhowContext, Result};
use dragonglass_vulkan::{
    ash::vk,
    core::{
        CommandPool, Context, CpuToGpuBuffer, DescriptorPool, DescriptorSetLayout, Device,
        GeometryBuffer, GraphicsPipelineSettingsBuilder, ImageDescription, Pipeline,
        PipelineLayout, RenderPass, Sampler, ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
        Texture,
    },
    geometry::Cube,
    pbr::EnvironmentMapSet,
    render::CubeRender,
};
use dragonglass_world::{
    legion::EntityStore, AlphaMode, Filter, Geometry, Hidden, LightKind, Material, Mesh,
    MeshRender, Skin, Transform, Vertex, World, WrappingMode,
};
use nalgebra_glm as glm;
use std::{mem, sync::Arc};

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

impl From<&Material> for PushConstantMaterial {
    fn from(material: &Material) -> Self {
        Self {
            base_color_factor: material.base_color_factor,
            metallic_factor: material.metallic_factor,
            roughness_factor: material.roughness_factor,
            emissive_factor: material.emissive_factor,
            alpha_mode: material.alpha_mode as i32,
            alpha_cutoff: material.alpha_cutoff,
            is_unlit: if material.is_unlit { 1 } else { 0 },
            color_texture_index: material.color_texture_index,
            color_texture_set: material.color_texture_set,
            metallic_roughness_texture_index: material.metallic_roughness_texture_index,
            metallic_roughness_texture_set: material.metallic_roughness_texture_set,
            normal_texture_index: material.normal_texture_index,
            normal_texture_set: material.normal_texture_set,
            normal_texture_scale: material.normal_texture_scale,
            occlusion_texture_index: material.occlusion_texture_index,
            occlusion_texture_set: material.occlusion_texture_set,
            occlusion_strength: material.occlusion_strength,
            emissive_texture_index: material.emissive_texture_index,
            emissive_texture_set: material.emissive_texture_set,
        }
    }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct Light {
    pub direction: glm::Vec3,
    pub range: f32,

    pub color: glm::Vec3,
    pub intensity: f32,

    pub position: glm::Vec3,
    pub inner_cone_cos: f32,

    pub outer_cone_cos: f32,
    pub kind: i32,

    pub padding: glm::Vec2,
}

impl Light {
    pub fn from_node(transform: &Transform, light: &dragonglass_world::Light) -> Self {
        let mut inner_cone_cos: f32 = 0.0;
        let mut outer_cone_cos: f32 = 0.0;
        let kind = match light.kind {
            LightKind::Directional => 0,
            LightKind::Point => 1,
            LightKind::Spot {
                inner_cone_angle,
                outer_cone_angle,
            } => {
                inner_cone_cos = inner_cone_angle;
                outer_cone_cos = outer_cone_angle;
                2
            }
        };
        Self {
            direction: -1.0 * glm::quat_rotate_vec3(&transform.rotation, &glm::Vec3::z()),
            range: light.range,
            color: light.color,
            intensity: light.intensity,
            position: transform.translation,
            inner_cone_cos,
            outer_cone_cos,
            kind,
            padding: glm::vec2(0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorldUniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    pub camera_position: glm::Vec3,
    pub number_of_lights: u32,
    pub joint_matrices: [glm::Mat4; PbrPipelineData::MAX_NUMBER_OF_JOINTS],
    pub lights: [Light; PbrPipelineData::MAX_NUMBER_OF_LIGHTS],
}

#[derive(Default, Debug, Clone, Copy)]
pub struct EntityDynamicUniformBuffer {
    pub model: glm::Mat4,
    // X is the joint count.
    // Y is the joint matrix offset.
    // A vec4 is needed to meet shader uniform data layout requirements
    pub node_info: glm::Vec4,
}

pub struct PbrPipelineData {
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

impl PbrPipelineData {
    // These should match the constants defined in the shader
    pub const MAX_NUMBER_OF_TEXTURES: usize = 200; // TODO: check that this is not larger than the physical device's maxDescriptorSetSamplers
    pub const MAX_NUMBER_OF_JOINTS: usize = 1000;
    pub const MAX_NUMBER_OF_LIGHTS: usize = 4; // TODO: Increase this once a deferred or forward+ pipeline is in use

    // This does not need to be matched in the shader
    pub const MAX_NUMBER_OF_MESHES: usize = 500;

    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        world: &World,
        environment_maps: &EnvironmentMapSet,
    ) -> Result<Self> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let mut textures = Vec::new();
        let mut samplers = Vec::new();
        for texture in world.textures.iter() {
            let description = ImageDescription::from_texture(texture)?;
            textures.push(Texture::new(context, command_pool, &description)?);
            samplers.push(map_sampler(
                device.clone(),
                description.mip_levels,
                &texture.sampler,
            )?);
        }

        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        let uniform_buffer = CpuToGpuBuffer::uniform_buffer(
            device.clone(),
            allocator.clone(),
            mem::size_of::<WorldUniformBuffer>() as _,
        )?;

        let dynamic_alignment = context.dynamic_alignment_of::<EntityDynamicUniformBuffer>();
        let dynamic_uniform_buffer = CpuToGpuBuffer::uniform_buffer(
            device.clone(),
            allocator,
            (Self::MAX_NUMBER_OF_MESHES as u64 * dynamic_alignment) as vk::DeviceSize,
        )?;

        let geometry_buffer = Self::geometry_buffer(context, command_pool, &world.geometry)?;

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
        data.update_descriptor_set(context, device, environment_maps);
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
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .build();
        let brdflut_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(3)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let prefilter_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(4)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let irradiance_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(5)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let bindings = [
            ubo_binding,
            dynamic_ubo_binding,
            sampler_binding,
            brdflut_binding,
            prefilter_binding,
            irradiance_binding,
        ];
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

        let brdflut_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        };

        let prefilter_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        };

        let irradiance_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        };

        let pool_sizes = [
            ubo_pool_size,
            dynamic_ubo_pool_size,
            sampler_pool_size,
            brdflut_pool_size,
            prefilter_pool_size,
            irradiance_pool_size,
        ];

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
            context.device.clone(),
            context.allocator.clone(),
            (geometry.vertices.len() * std::mem::size_of::<Vertex>()) as _,
            index_buffer_size,
        )?;

        geometry_buffer
            .vertex_buffer
            .upload_data(&geometry.vertices, 0, pool)?;

        if has_indices {
            geometry_buffer
                .index_buffer
                .as_ref()
                .context("Failed to access index buffer!")?
                .upload_data(&geometry.indices, 0, pool)?;
        }

        Ok(geometry_buffer)
    }

    fn update_descriptor_set(
        &self,
        context: &Context,
        device: Arc<Device>,
        environment_maps: &EnvironmentMapSet,
    ) {
        let uniform_buffer_size = mem::size_of::<WorldUniformBuffer>() as vk::DeviceSize;
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.uniform_buffer.handle())
            .offset(0)
            .range(uniform_buffer_size)
            .build();
        let buffer_infos = [buffer_info];

        let dynamic_buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.dynamic_uniform_buffer.handle())
            .offset(0)
            .range(context.dynamic_alignment_of::<EntityDynamicUniformBuffer>())
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

        let brdflut_image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(environment_maps.brdflut.view.handle)
            .sampler(environment_maps.brdflut.sampler.handle)
            .build();
        let brdflut_image_infos = [brdflut_image_info];

        let prefilter_image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(environment_maps.prefilter.view.handle)
            .sampler(environment_maps.prefilter.sampler.handle)
            .build();
        let prefilter_image_infos = [prefilter_image_info];

        let irradiance_image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(environment_maps.irradiance.view.handle)
            .sampler(environment_maps.irradiance.sampler.handle)
            .build();
        let irradiance_image_infos = [irradiance_image_info];

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

        let brdflut_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(3)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&brdflut_image_infos)
            .build();

        let prefilter_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(4)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&prefilter_image_infos)
            .build();

        let irradiance_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(5)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&irradiance_image_infos)
            .build();

        let descriptor_writes = [
            ubo_descriptor_write,
            dynamic_ubo_descriptor_write,
            sampler_descriptor_write,
            brdflut_descriptor_write,
            prefilter_descriptor_write,
            irradiance_descriptor_write,
        ];

        unsafe {
            device
                .handle
                .update_descriptor_sets(&descriptor_writes, &[])
        }
    }

    pub fn update_dynamic_ubo(&mut self, world: &World) -> Result<()> {
        let world_joint_matrices = world.joint_matrices()?;
        let number_of_joints = world_joint_matrices.len();
        ensure!(
            number_of_joints < Self::MAX_NUMBER_OF_JOINTS,
            "Too many joints in world: {}/{}",
            number_of_joints,
            Self::MAX_NUMBER_OF_JOINTS
        );

        self.update_node_ubos(world)?;

        Ok(())
    }

    fn update_node_ubos(&mut self, world: &World) -> Result<()> {
        let mut buffers = vec![EntityDynamicUniformBuffer::default(); Self::MAX_NUMBER_OF_MESHES];
        let mut joint_offset = 0;
        let mut weight_offset = 0;
        let mut ubo_offset = 0;
        for graph in world.scene.graphs.iter() {
            graph.walk(|node_index| {
                let entity = graph[node_index];

                let model = world.global_transform(graph, node_index)?;

                let mut node_info = glm::vec4(0.0, 0.0, 0.0, 0.0);

                if let Ok(skin) = world.ecs.entry_ref(entity)?.get_component::<Skin>() {
                    let joint_count = skin.joints.len();
                    node_info.x = joint_count as f32;
                    node_info.y = joint_offset as f32;
                    joint_offset += joint_count;
                }

                if let Ok(mesh) = world.ecs.entry_ref(entity)?.get_component::<Mesh>() {
                    let weight_count = mesh.weights.len();
                    node_info.z = weight_count as f32;
                    node_info.w = weight_offset as f32;
                    weight_offset += weight_count;
                }

                buffers[ubo_offset] = EntityDynamicUniformBuffer { model, node_info };
                ubo_offset += 1;

                Ok(())
            })?;
        }
        let alignment = self.dynamic_alignment;
        self.dynamic_uniform_buffer
            .upload_data_aligned(&buffers, 0, alignment)?;
        Ok(())
    }
}

pub struct WorldRender {
    pub cube_render: CubeRender,
    pub pbr_pipeline_data: PbrPipelineData,
    pub pipeline: Option<Pipeline>,
    pub pipeline_blended: Option<Pipeline>,
    pub pipeline_wireframe: Option<Pipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub wireframe_enabled: bool,
    device: Arc<Device>,
}

impl WorldRender {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        world: &World,
        environment_maps: &EnvironmentMapSet,
    ) -> Result<Self> {
        let pipeline_data = PbrPipelineData::new(context, command_pool, world, environment_maps)?;
        let cube = Cube::new(
            context.device.clone(),
            context.allocator.clone(),
            command_pool,
        )?;
        let cube_render = CubeRender::new(context.device.clone(), cube);
        Ok(Self {
            cube_render,
            pbr_pipeline_data: pipeline_data,
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
            .vertex("assets/shaders/world/world.vert.spv")
            .fragment("assets/shaders/world/world.frag.spv")
            .build()?;
        Ok(shader_path_set)
    }

    pub fn create_pipeline(
        &mut self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
    ) -> Result<()> {
        self.cube_render
            .create_pipeline(shader_cache, render_pass.clone(), samples)?;

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
            .descriptor_set_layout(self.pbr_pipeline_data.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .sample_shading_enabled(true)
            .cull_mode(vk::CullModeFlags::BACK)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
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
        let (pipeline, pipeline_layout) = settings.build()?.create_pipeline(self.device.clone())?;

        let (pipeline_blended, _) = blend_settings
            .build()?
            .create_pipeline(self.device.clone())?;

        let (pipeline_wireframe, _) = wireframe_settings
            .build()?
            .create_pipeline(self.device.clone())?;

        self.pipeline = Some(pipeline);
        self.pipeline_blended = Some(pipeline_blended);
        self.pipeline_wireframe = Some(pipeline_wireframe);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(
        &self,
        command_buffer: vk::CommandBuffer,
        world: &World,
        aspect_ratio: f32,
    ) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get pipeline for rendering world!")?;

        let pipeline_blended = self
            .pipeline_blended
            .as_ref()
            .context("Failed to get blend pipeline for rendering world!")?;

        let pipeline_wireframe = self
            .pipeline_wireframe
            .as_ref()
            .context("Failed to get wireframe pipeline for rendering world!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering world!")?;

        let (_projection, _view) = world.active_camera_matrices(aspect_ratio)?;

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            let has_indices = self
                .pbr_pipeline_data
                .geometry_buffer
                .index_buffer
                .is_some();
            let mut ubo_offset: i32 = -1;
            for graph in world.scene.graphs.iter() {
                graph.walk(|node_index| {
                    ubo_offset += 1;
                    let entity = graph[node_index];

                    if world
                        .ecs
                        .entry_ref(entity)?
                        .get_component::<Hidden>()
                        .is_ok()
                    {
                        return Ok(());
                    }

                    let _transform = world.entity_global_transform(entity)?;

                    // FIXME: Don't always render lights, add a debug flag to the component or something
                    // Render lights as colored boxes for debugging
                    // if let Ok(light) = world
                    //     .ecs
                    //     .entry_ref(entity)?
                    //     .get_component::<dragonglass_world::Light>()
                    // {
                    //     let offset = glm::translation(&transform.translation);
                    //     let rotation = glm::quat_to_mat4(&transform.rotation);
                    //     let extents = glm::vec3(0.25, 0.25, 0.25);
                    //     let scale = glm::scaling(&extents);
                    //     self.cube_render.issue_commands(
                    //         command_buffer,
                    //         projection * view * offset * rotation * scale,
                    //         glm::vec3_to_vec4(&light.color),
                    //         true,
                    //     )?;
                    // }

                    match world.ecs.entry_ref(entity)?.get_component::<MeshRender>() {
                        Ok(mesh_render) => {
                            if let Some(mesh) = world.geometry.meshes.get(&mesh_render.name) {
                                if self.wireframe_enabled {
                                    pipeline_wireframe.bind(&self.device.handle, command_buffer);
                                } else {
                                    match alpha_mode {
                                        AlphaMode::Opaque | AlphaMode::Mask => {
                                            pipeline.bind(&self.device.handle, command_buffer);
                                        }
                                        AlphaMode::Blend => {
                                            pipeline_blended
                                                .bind(&self.device.handle, command_buffer);
                                        }
                                    }
                                }

                                self.pbr_pipeline_data
                                    .geometry_buffer
                                    .bind(&self.device.handle, command_buffer)?;

                                unsafe {
                                    self.device.handle.cmd_bind_descriptor_sets(
                                        command_buffer,
                                        vk::PipelineBindPoint::GRAPHICS,
                                        pipeline_layout.handle,
                                        0,
                                        &[self.pbr_pipeline_data.descriptor_set],
                                        &[(ubo_offset as u64
                                            * self.pbr_pipeline_data.dynamic_alignment)
                                            as _],
                                    );
                                }

                                for primitive in mesh.primitives.iter() {
                                    let material = match primitive.material_index {
                                        Some(material_index) => {
                                            let primitive_material =
                                                world.material_at_index(material_index)?;
                                            if primitive_material.alpha_mode != *alpha_mode {
                                                continue;
                                            }
                                            PushConstantMaterial::from(primitive_material)
                                        }
                                        None => PushConstantMaterial::from(&Material::default()),
                                    };

                                    unsafe {
                                        self.device.handle.cmd_push_constants(
                                            command_buffer,
                                            pipeline_layout.handle,
                                            vk::ShaderStageFlags::ALL_GRAPHICS,
                                            0,
                                            byte_slice_from(&material),
                                        );

                                        if has_indices {
                                            self.device.handle.cmd_draw_indexed(
                                                command_buffer,
                                                primitive.number_of_indices as _,
                                                1,
                                                primitive.first_index as _,
                                                0,
                                                0,
                                            );
                                        } else {
                                            self.device.handle.cmd_draw(
                                                command_buffer,
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
                        Err(_) => return Ok(()),
                    }

                    Ok(())
                })?;
            }
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

fn map_sampler(
    device: Arc<Device>,
    mip_levels: u32,
    sampler: &dragonglass_world::Sampler,
) -> Result<Sampler> {
    let min_filter = match sampler.min_filter {
        Filter::Linear => vk::Filter::LINEAR,
        Filter::Nearest => vk::Filter::NEAREST,
    };

    let mipmap_mode = match sampler.min_filter {
        Filter::Linear => vk::SamplerMipmapMode::LINEAR,
        Filter::Nearest => vk::SamplerMipmapMode::NEAREST,
    };

    let mag_filter = match sampler.mag_filter {
        Filter::Nearest => vk::Filter::NEAREST,
        Filter::Linear => vk::Filter::LINEAR,
    };

    let address_mode_u = match sampler.wrap_s {
        WrappingMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        WrappingMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        WrappingMode::Repeat => vk::SamplerAddressMode::REPEAT,
    };

    let address_mode_v = match sampler.wrap_t {
        WrappingMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        WrappingMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        WrappingMode::Repeat => vk::SamplerAddressMode::REPEAT,
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
