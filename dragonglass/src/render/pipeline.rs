use super::{DescriptorSetLayout, RenderPass, ShaderSet};
use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use derive_builder::Builder;
use std::sync::Arc;

pub struct GraphicsPipeline {
    pub handle: vk::Pipeline,
    device: Arc<LogicalDevice>,
}

impl GraphicsPipeline {
    pub fn new(
        device: Arc<LogicalDevice>,
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
        let pipeline = Self { handle, device };
        Ok(pipeline)
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_pipeline(self.handle, None);
        }
    }
}

pub struct PipelineLayout {
    pub handle: vk::PipelineLayout,
    device: Arc<LogicalDevice>,
}

impl PipelineLayout {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::PipelineLayoutCreateInfo,
    ) -> Result<Self> {
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

impl GraphicsPipeline {
    pub fn from_settings(
        device: Arc<LogicalDevice>,
        settings: GraphicsPipelineSettings,
    ) -> Result<(Self, PipelineLayout)> {
        let stages = settings.shader_set.stages()?;

        let input_assembly_create_info = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let rasterizer_create_info = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(settings.cull_mode)
            .front_face(settings.front_face)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0);

        let multisampling_create_info = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(settings.sample_shading_enabled)
            .rasterization_samples(settings.rasterization_samples)
            .min_sample_shading(0.2)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false);

        let depth_stencil_info = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(settings.depth_test_enabled)
            .depth_write_enable(settings.depth_write_enabled)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(settings.stencil_test_enabled)
            .front(settings.stencil_front_state)
            .back(settings.stencil_back_state);

        let color_blend_attachments = if settings.blended {
            Self::create_color_blend_attachments_blended()
        } else {
            Self::create_color_blend_attachments_opaque()
        };

        let color_blending_info = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        let pipeline_layout = Self::create_pipeline_layout(device.clone(), &settings);

        let mut viewport_create_info = vk::PipelineViewportStateCreateInfo::default();
        viewport_create_info.viewport_count = 1;
        viewport_create_info.scissor_count = 1;

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state_create_info = vk::PipelineDynamicStateCreateInfo::builder()
            .flags(vk::PipelineDynamicStateCreateFlags::empty())
            .dynamic_states(&dynamic_states);

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&settings.vertex_state_info)
            .input_assembly_state(&input_assembly_create_info)
            .rasterization_state(&rasterizer_create_info)
            .multisample_state(&multisampling_create_info)
            .depth_stencil_state(&depth_stencil_info)
            .color_blend_state(&color_blending_info)
            .viewport_state(&viewport_create_info)
            .dynamic_state(&dynamic_state_create_info)
            .layout(pipeline_layout.handle)
            .render_pass(settings.render_pass.handle)
            .subpass(0);

        let pipeline = Self::new(device, pipeline_create_info)?;

        Ok((pipeline, pipeline_layout))
    }

    pub fn create_color_blend_attachments_opaque() -> [vk::PipelineColorBlendAttachmentState; 1] {
        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(false)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ZERO)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD);
        [*color_blend_attachment]
    }

    pub fn create_color_blend_attachments_blended() -> [vk::PipelineColorBlendAttachmentState; 1] {
        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD);
        [*color_blend_attachment]
    }

    pub fn create_pipeline_layout(
        device: Arc<LogicalDevice>,
        settings: &GraphicsPipelineSettings,
    ) -> PipelineLayout {
        let descriptor_set_layouts = [settings.descriptor_set_layout.handle];

        if let Some(push_constant_range) = settings.push_constant_range.as_ref() {
            let push_constant_ranges = [*push_constant_range];
            let pipeline_layout_create_info_builder = vk::PipelineLayoutCreateInfo::builder()
                .push_constant_ranges(&push_constant_ranges)
                .set_layouts(&descriptor_set_layouts);
            PipelineLayout::new(device.clone(), *pipeline_layout_create_info_builder).unwrap()
        } else {
            let pipeline_layout_create_info_builder =
                vk::PipelineLayoutCreateInfo::builder().set_layouts(&descriptor_set_layouts);
            PipelineLayout::new(device, *pipeline_layout_create_info_builder).unwrap()
        }
    }

    pub fn bind(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) {
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.handle);
        }
    }
}
