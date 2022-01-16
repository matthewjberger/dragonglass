use crate::core::{DescriptorSetLayout, Device, RenderPass, ShaderSet};
use anyhow::Result;
use ash::vk;
use derive_builder::Builder;
use std::sync::Arc;

pub struct Pipeline {
    pub handle: vk::Pipeline,
    bindpoint: vk::PipelineBindPoint,
    device: Arc<Device>,
}

impl Pipeline {
    pub fn new_graphics(
        device: Arc<Device>,
        create_info: vk::GraphicsPipelineCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe {
            let result = device.handle.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info.build()],
                None,
            );
            match result {
                Ok(pipelines) => Ok(pipelines[0]),
                Err((_, error_code)) => Err(error_code),
            }
        }?;
        let bindpoint = vk::PipelineBindPoint::GRAPHICS;
        let pipeline = Self {
            handle,
            bindpoint,
            device,
        };
        Ok(pipeline)
    }

    #[allow(dead_code)]
    pub fn new_compute(
        device: Arc<Device>,
        create_info: vk::ComputePipelineCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe {
            let result = device.handle.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[create_info.build()],
                None,
            );
            match result {
                Ok(pipelines) => Ok(pipelines[0]),
                Err((_, error_code)) => Err(error_code),
            }
        }?;
        let bindpoint = vk::PipelineBindPoint::COMPUTE;
        let pipeline = Self {
            handle,
            bindpoint,
            device,
        };
        Ok(pipeline)
    }

    pub fn bind(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) {
        unsafe {
            device.cmd_bind_pipeline(command_buffer, self.bindpoint, self.handle);
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_pipeline(self.handle, None);
        }
    }
}

pub struct PipelineLayout {
    pub handle: vk::PipelineLayout,
    device: Arc<Device>,
}

impl PipelineLayout {
    pub fn new(device: Arc<Device>, create_info: vk::PipelineLayoutCreateInfo) -> Result<Self> {
        let handle = unsafe { device.handle.create_pipeline_layout(&create_info, None) }?;
        let pipeline_layout = Self { handle, device };
        Ok(pipeline_layout)
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .handle
                .destroy_pipeline_layout(self.handle, None);
        }
    }
}

#[derive(Builder)]
#[builder(setter(into))]
pub struct GraphicsPipelineSettings {
    pub render_pass: Arc<RenderPass>,
    pub vertex_inputs: Vec<vk::VertexInputBindingDescription>,
    pub vertex_attributes: Vec<vk::VertexInputAttributeDescription>,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub shader_set: ShaderSet,

    #[builder(default)]
    pub blended: bool,

    #[builder(default = "true")]
    pub depth_test_enabled: bool,

    #[builder(default = "true")]
    pub depth_write_enabled: bool,

    #[builder(default)]
    pub stencil_test_enabled: bool,

    #[builder(default)]
    pub stencil_front_state: vk::StencilOpState,

    #[builder(default)]
    pub stencil_back_state: vk::StencilOpState,

    #[builder(default)]
    pub push_constant_range: Option<vk::PushConstantRange>,

    #[builder(default = "vk::SampleCountFlags::TYPE_1")]
    pub rasterization_samples: vk::SampleCountFlags,

    #[builder(default)]
    pub sample_shading_enabled: bool,

    #[builder(default = "vk::CullModeFlags::NONE")]
    pub cull_mode: vk::CullModeFlags,

    #[builder(default = "vk::FrontFace::COUNTER_CLOCKWISE")]
    pub front_face: vk::FrontFace,

    #[builder(default = "vk::PrimitiveTopology::TRIANGLE_LIST")]
    pub topology: vk::PrimitiveTopology,

    #[builder(default = "vk::PolygonMode::FILL")]
    pub polygon_mode: vk::PolygonMode,

    #[builder(default)]
    pub dynamic_states: Vec<vk::DynamicState>,
}

impl GraphicsPipelineSettings {
    pub fn create_pipeline(&self, device: Arc<Device>) -> Result<(Pipeline, PipelineLayout)> {
        let stages = self.shader_set.stages()?;
        let vertex_state_info = self.vertex_input_state();
        let input_assembly_create_info = self.input_assembly_create_info();
        let rasterizer_create_info = self.rasterizer_create_info();
        let multisampling_create_info = self.multisampling_create_info();
        let depth_stencil_info = self.depth_stencil_info();
        let blend_attachment = [self.color_blend_attachment_state().build()];
        let color_blend_state = Self::color_blend_state(&blend_attachment);
        let pipeline_layout = self.create_pipeline_layout(device.clone());
        let viewport_create_info = Self::viewport_create_info();
        let dynamic_state = self.dynamic_state();
        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&vertex_state_info)
            .input_assembly_state(&input_assembly_create_info)
            .rasterization_state(&rasterizer_create_info)
            .multisample_state(&multisampling_create_info)
            .depth_stencil_state(&depth_stencil_info)
            .color_blend_state(&color_blend_state)
            .viewport_state(&viewport_create_info)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout.handle)
            .render_pass(self.render_pass.handle)
            .subpass(0);
        let pipeline = Pipeline::new_graphics(device, pipeline_create_info)?;
        Ok((pipeline, pipeline_layout))
    }

    fn vertex_input_state(&self) -> vk::PipelineVertexInputStateCreateInfoBuilder {
        vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&self.vertex_inputs)
            .vertex_attribute_descriptions(&self.vertex_attributes)
    }

    fn input_assembly_create_info<'a>(
        &self,
    ) -> vk::PipelineInputAssemblyStateCreateInfoBuilder<'a> {
        vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(self.topology)
            .primitive_restart_enable(false)
    }

    fn rasterizer_create_info(&self) -> vk::PipelineRasterizationStateCreateInfoBuilder {
        vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(self.polygon_mode)
            .line_width(1.0)
            .cull_mode(self.cull_mode)
            .front_face(self.front_face)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0)
    }

    fn multisampling_create_info(&self) -> vk::PipelineMultisampleStateCreateInfoBuilder {
        vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(self.sample_shading_enabled)
            .rasterization_samples(self.rasterization_samples)
            .min_sample_shading(0.2)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false)
    }

    fn depth_stencil_info(&self) -> vk::PipelineDepthStencilStateCreateInfoBuilder {
        vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(self.depth_test_enabled)
            .depth_write_enable(self.depth_write_enabled)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(self.stencil_test_enabled)
            .front(self.stencil_front_state)
            .back(self.stencil_back_state)
    }

    fn color_blend_attachment_state(&self) -> vk::PipelineColorBlendAttachmentStateBuilder {
        if self.blended {
            Self::blend_attachment_blended()
        } else {
            Self::blend_attachment_opaque()
        }
    }

    fn blend_attachment_opaque<'a>() -> vk::PipelineColorBlendAttachmentStateBuilder<'a> {
        vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::default())
            .blend_enable(false)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ZERO)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
    }

    fn blend_attachment_blended<'a>() -> vk::PipelineColorBlendAttachmentStateBuilder<'a> {
        vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::default())
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
    }

    fn color_blend_state(
        attachment: &[vk::PipelineColorBlendAttachmentState],
    ) -> vk::PipelineColorBlendStateCreateInfoBuilder {
        vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(attachment)
            .blend_constants([0.0, 0.0, 0.0, 0.0])
    }

    fn create_pipeline_layout(&self, device: Arc<Device>) -> PipelineLayout {
        let descriptor_set_layouts = [self.descriptor_set_layout.handle];

        if let Some(push_constant_range) = self.push_constant_range.as_ref() {
            let push_constant_ranges = [*push_constant_range];
            let pipeline_layout_create_info_builder = vk::PipelineLayoutCreateInfo::builder()
                .push_constant_ranges(&push_constant_ranges)
                .set_layouts(&descriptor_set_layouts);
            PipelineLayout::new(device, *pipeline_layout_create_info_builder).unwrap()
        } else {
            let pipeline_layout_create_info_builder =
                vk::PipelineLayoutCreateInfo::builder().set_layouts(&descriptor_set_layouts);
            PipelineLayout::new(device, *pipeline_layout_create_info_builder).unwrap()
        }
    }

    fn viewport_create_info<'a>() -> vk::PipelineViewportStateCreateInfoBuilder<'a> {
        vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(1)
            .scissor_count(1)
    }

    fn dynamic_state(&self) -> vk::PipelineDynamicStateCreateInfoBuilder {
        vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&self.dynamic_states)
    }
}
