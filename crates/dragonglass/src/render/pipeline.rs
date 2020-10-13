use super::ShaderSet;
use anyhow::Result;
use ash::vk;
use derive_builder::Builder;
use dragonglass_adapters::{DescriptorSetLayout, GraphicsPipeline, PipelineLayout, RenderPass};
use dragonglass_context::LogicalDevice;
use std::sync::Arc;

#[derive(Builder)]
#[builder(setter(into))]
pub struct GraphicsPipelineSettings {
    pub render_pass: Arc<RenderPass>,
    pub vertex_state_info: vk::PipelineVertexInputStateCreateInfo,
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
}

impl GraphicsPipelineSettings {
    pub fn create_pipeline(
        &self,
        device: Arc<LogicalDevice>,
    ) -> Result<(GraphicsPipeline, PipelineLayout)> {
        let stages = self.shader_set.stages()?;
        let input_assembly_create_info = Self::input_assembly_create_info();
        let rasterizer_create_info = self.rasterizer_create_info();
        let multisampling_create_info = self.multisampling_create_info();
        let depth_stencil_info = self.depth_stencil_info();
        let blend_attachment = [self.color_blend_attachment_state().build()];
        let color_blend_state = Self::color_blend_state(&blend_attachment);
        let pipeline_layout = self.create_pipeline_layout(device.clone());
        let viewport_create_info = Self::viewport_create_info();
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = Self::dynamic_state(&dynamic_states);
        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&self.vertex_state_info)
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
        let pipeline = GraphicsPipeline::new(device, pipeline_create_info)?;
        Ok((pipeline, pipeline_layout))
    }

    fn input_assembly_create_info<'a>() -> vk::PipelineInputAssemblyStateCreateInfoBuilder<'a> {
        vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false)
    }

    fn rasterizer_create_info(&self) -> vk::PipelineRasterizationStateCreateInfoBuilder {
        vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
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
            .color_write_mask(vk::ColorComponentFlags::all())
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
            .color_write_mask(vk::ColorComponentFlags::all())
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

    fn create_pipeline_layout(&self, device: Arc<LogicalDevice>) -> PipelineLayout {
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

    fn dynamic_state(
        dynamic_states: &[vk::DynamicState],
    ) -> vk::PipelineDynamicStateCreateInfoBuilder {
        vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(dynamic_states)
    }
}
