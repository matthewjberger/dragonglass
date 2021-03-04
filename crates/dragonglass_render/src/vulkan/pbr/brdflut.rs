use crate::vulkan::{
    core::{
        CommandPool, Context, Image, ImageNode, ImageView, RenderGraph, Sampler, ShaderCache,
        ShaderPathSetBuilder,
    },
    render::FullscreenRender,
};
use anyhow::{anyhow, Result};
use ash::vk::{self, Handle, ObjectType};
use std::ffi::CStr;

pub struct Brdflut {
    pub image: Box<dyn Image>,
    pub view: ImageView,
    pub sampler: Sampler,
}

impl Brdflut {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        shader_cache: &mut ShaderCache,
    ) -> Result<Brdflut> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let dimension = 512;
        let extent = vk::Extent2D::builder()
            .width(dimension)
            .height(dimension)
            .build();

        let fullscreen = "fullscreen";
        let color = "color";
        let mut rendergraph = RenderGraph::new(
            &[fullscreen],
            vec![ImageNode {
                name: color.to_string(),
                extent: extent,
                format: vk::Format::R16G16_SFLOAT,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [1.0, 1.0, 1.0, 1.0],
                    },
                },
                samples: vk::SampleCountFlags::TYPE_1,
                force_store: true,
                force_shader_read: true,
            }],
            &[(fullscreen, &color)],
        )?;

        rendergraph.build(device.clone(), allocator)?;

        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/postprocessing/fullscreen_triangle.vert.spv")
            .fragment("assets/shaders/environment/genbrdflut.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        let fullscreen_pass = rendergraph.pass_handle(fullscreen)?;
        let pipeline = FullscreenRender::new(
            context,
            fullscreen_pass,
            shader_cache,
            rendergraph.image_view(&color)?.handle,
            rendergraph.sampler("default")?.handle,
            shader_path_set,
        )?;

        command_pool.execute_once(|command_buffer| {
            rendergraph.execute_pass(command_buffer, fullscreen, 0, |pass, command_buffer| {
                device.update_viewport(command_buffer, pass.extent, false)?;
                pipeline.issue_commands(command_buffer)?;
                Ok(())
            })
        })?;

        let (image, view) = rendergraph.take_image(&color)?;

        if let Some(debug) = context.debug.as_ref() {
            // TODO: This can be wrapped up in context
            let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                .object_type(vk::ObjectType::IMAGE)
                .object_name(CStr::from_bytes_with_nul(b"brdflut\0")?)
                .object_handle(image.handle().as_raw())
                .build();
            unsafe {
                debug.debug_utils_set_object_name(context.device.handle.handle(), &name_info)?;
            }
        }

        Ok(Brdflut {
            image,
            view,
            sampler: Sampler::default(device)?,
        })
    }
}
