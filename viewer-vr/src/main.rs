//! Illustrates rendering using Vulkan with multiview. Supports any Vulkan 1.1 capable environment.
//!
//! Renders a smooth gradient across the entire view, with different colors per eye.
//!
//! This example uses minimal abstraction for clarity. Real-world code should encapsulate and
//! largely decouple its Vulkan and OpenXR components and handle errors gracefully.

use std::{
    ffi::{CStr, CString},
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use ash::{
    util::read_spv,
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::{self, Handle},
};
use openxr as xr;

fn main() {
    // Handle interrupts gracefully
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    })
    .expect("setting Ctrl-C handler");

    let entry = xr::Entry::linked();

    // OpenXR will fail to initialize if we ask for an extension that OpenXR can't provide! So we
    // need to check all our extensions before initializing OpenXR with them. Note that even if the
    // extension is present, it's still possible you may not be able to use it. For example: the
    // hand tracking extension may be present, but the hand sensor might not be plugged in or turned
    // on. There are often additional checks that should be made before using certain features!
    let available_extensions = entry.enumerate_extensions().unwrap();

    // If a required extension isn't present, you want to ditch out here! It's possible something
    // like your rendering API might not be provided by the active runtime. APIs like OpenGL don't
    // have universal support.
    assert!(available_extensions.khr_vulkan_enable);

    // Initialize OpenXR with the extensions we've found!
    let mut enabled_extensions = xr::ExtensionSet::default();
    enabled_extensions.khr_vulkan_enable = true;
    let xr_instance = entry
        .create_instance(
            &xr::ApplicationInfo {
                application_name: "openxrs example",
                application_version: 0,
                engine_name: "openxrs example",
                engine_version: 0,
            },
            &enabled_extensions,
            &[],
        )
        .unwrap();
    let instance_props = xr_instance.properties().unwrap();
    println!(
        "loaded OpenXR runtime: {} {}",
        instance_props.runtime_name, instance_props.runtime_version
    );

    // Request a form factor from the device (HMD, Handheld, etc.)
    let system = xr_instance
        .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .unwrap();

    // Check what blend mode is valid for this device (opaque vs transparent displays). We'll just
    // take the first one available!
    let environment_blend_mode = xr_instance
        .enumerate_environment_blend_modes(system, VIEW_TYPE)
        .unwrap()[0];

    // OpenXR wants to ensure apps are using the correct graphics card, so the renderer MUST be set
    // up before Instance::create_session. This is crucial on devices that have multiple graphics
    // cards, like laptops with integrated graphics chips in addition to dedicated graphics cards.

    let vk_instance_exts = xr_instance
        .vulkan_instance_extensions(system)
        .unwrap()
        .split(' ')
        .map(|x| CString::new(x).unwrap())
        .collect::<Vec<_>>();
    println!(
        "required Vulkan instance extensions: {:?}",
        vk_instance_exts
    );
    let vk_instance_ext_ptrs = vk_instance_exts
        .iter()
        .map(|x| x.as_ptr())
        .collect::<Vec<_>>();

    let vk_entry = ash::Entry::new().unwrap();

    // Check that we have the required Vulkan instance extensions.
    let instance_extensions = vk_entry.enumerate_instance_extension_properties().unwrap();
    for ext in &vk_instance_exts {
        unsafe {
            if !instance_extensions
                .iter()
                .any(|inst_ext| CStr::from_ptr(inst_ext.extension_name.as_ptr()) == ext.as_c_str())
            {
                panic!(
                    "OpenXR runtime requires missing Vulkan instance extension {:?}",
                    ext
                );
            }
        }
    }

    let vk_app_info = vk::ApplicationInfo::builder()
        .application_version(0)
        .engine_version(0)
        .api_version(vk::make_version(1, 1, 0)); // Vulkan 1.1 guarantees multiview support

    let vk_version = vk_entry
        .try_enumerate_instance_version()
        .unwrap()
        .unwrap_or_else(|| vk::make_version(1, 0, 0));
    let vk_version = xr::Version::new(
        vk::version_major(vk_version) as u16,
        vk::version_major(vk_version) as u16,
        0,
    );

    let reqs = xr_instance
        .graphics_requirements::<xr::Vulkan>(system)
        .unwrap();
    if reqs.min_api_version_supported > vk_version {
        panic!(
            "OpenXR runtime requires Vulkan version > {}",
            reqs.min_api_version_supported
        );
    }

    let vk_device_exts = xr_instance
        .vulkan_device_extensions(system)
        .unwrap()
        .split(' ')
        .map(|x| CString::new(x).unwrap())
        .collect::<Vec<_>>();
    println!("required Vulkan device extensions: {:?}", vk_device_exts);
    let vk_device_ext_ptrs = vk_device_exts
        .iter()
        .map(|x| x.as_ptr())
        .collect::<Vec<_>>();

    unsafe {
        let vk_instance = vk_entry
            .create_instance(
                &vk::InstanceCreateInfo::builder()
                    .application_info(&vk_app_info)
                    .enabled_extension_names(&vk_instance_ext_ptrs),
                None,
            )
            .expect("failed to create Vulkan instance");

        let vk_physical_device = vk::PhysicalDevice::from_raw(
            xr_instance
                .vulkan_graphics_device(system, vk_instance.handle().as_raw() as _)
                .unwrap() as _,
        );

        let vk_device_properties = vk_instance.get_physical_device_properties(vk_physical_device);
        if vk_device_properties.api_version < vk::make_version(1, 1, 0) {
            vk_instance.destroy_instance(None);
            panic!("Vulkan phyiscal device doesn't support version 1.1");
        }

        let queue_family_index = vk_instance
            .get_physical_device_queue_family_properties(vk_physical_device)
            .into_iter()
            .enumerate()
            .filter_map(|(queue_family_index, info)| {
                if info.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    Some(queue_family_index as u32)
                } else {
                    None
                }
            })
            .next()
            .expect("Vulkan device has no graphics queue");

        // Check that we have the required Vulkan device extensions.
        let device_extensions = vk_instance
            .enumerate_device_extension_properties(vk_physical_device)
            .unwrap();
        for ext in &vk_device_exts {
            if !device_extensions
                .iter()
                .any(|inst_ext| CStr::from_ptr(inst_ext.extension_name.as_ptr()) == ext.as_c_str())
            {
                panic!(
                    "OpenXR runtime requires missing Vulkan device extension {:?}",
                    ext
                );
            }
        }

        let vk_device = vk_instance
            .create_device(
                vk_physical_device,
                &vk::DeviceCreateInfo::builder()
                    .queue_create_infos(&[vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(queue_family_index)
                        .queue_priorities(&[1.0])
                        .build()])
                    .enabled_extension_names(&vk_device_ext_ptrs)
                    .push_next(&mut vk::PhysicalDeviceVulkan11Features {
                        multiview: vk::TRUE,
                        ..Default::default()
                    }),
                None,
            )
            .unwrap();
        let queue = vk_device.get_device_queue(queue_family_index, 0);

        let view_mask = !(!0 << VIEW_COUNT);
        let render_pass = vk_device
            .create_render_pass(
                &vk::RenderPassCreateInfo::builder()
                    .attachments(&[vk::AttachmentDescription {
                        format: COLOR_FORMAT,
                        samples: vk::SampleCountFlags::TYPE_1,
                        load_op: vk::AttachmentLoadOp::CLEAR,
                        store_op: vk::AttachmentStoreOp::STORE,
                        initial_layout: vk::ImageLayout::UNDEFINED,
                        final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        ..Default::default()
                    }])
                    .subpasses(&[vk::SubpassDescription::builder()
                        .color_attachments(&[vk::AttachmentReference {
                            attachment: 0,
                            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        }])
                        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                        .build()])
                    .dependencies(&[vk::SubpassDependency {
                        src_subpass: vk::SUBPASS_EXTERNAL,
                        dst_subpass: 0,
                        src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                        dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                        ..Default::default()
                    }])
                    .push_next(
                        &mut vk::RenderPassMultiviewCreateInfo::builder()
                            .view_masks(&[view_mask])
                            .correlation_masks(&[view_mask]),
                    ),
                None,
            )
            .unwrap();

        let vert = read_spv(&mut Cursor::new(&include_bytes!("fullscreen.vert.spv")[..])).unwrap();
        let frag = read_spv(&mut Cursor::new(
            &include_bytes!("debug_pattern.frag.spv")[..],
        ))
        .unwrap();
        let vert = vk_device
            .create_shader_module(&vk::ShaderModuleCreateInfo::builder().code(&vert), None)
            .unwrap();
        let frag = vk_device
            .create_shader_module(&vk::ShaderModuleCreateInfo::builder().code(&frag), None)
            .unwrap();

        let pipeline_layout = vk_device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder().set_layouts(&[]),
                None,
            )
            .unwrap();
        let noop_stencil_state = vk::StencilOpState {
            fail_op: vk::StencilOp::KEEP,
            pass_op: vk::StencilOp::KEEP,
            depth_fail_op: vk::StencilOp::KEEP,
            compare_op: vk::CompareOp::ALWAYS,
            compare_mask: 0,
            write_mask: 0,
            reference: 0,
        };
        let pipeline = vk_device
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&[
                        vk::PipelineShaderStageCreateInfo {
                            stage: vk::ShaderStageFlags::VERTEX,
                            module: vert,
                            p_name: b"main\0".as_ptr() as _,
                            ..Default::default()
                        },
                        vk::PipelineShaderStageCreateInfo {
                            stage: vk::ShaderStageFlags::FRAGMENT,
                            module: frag,
                            p_name: b"main\0".as_ptr() as _,
                            ..Default::default()
                        },
                    ])
                    .vertex_input_state(&vk::PipelineVertexInputStateCreateInfo::default())
                    .input_assembly_state(
                        &vk::PipelineInputAssemblyStateCreateInfo::builder()
                            .topology(vk::PrimitiveTopology::TRIANGLE_LIST),
                    )
                    .viewport_state(
                        &vk::PipelineViewportStateCreateInfo::builder()
                            .scissor_count(1)
                            .viewport_count(1),
                    )
                    .rasterization_state(
                        &vk::PipelineRasterizationStateCreateInfo::builder()
                            .cull_mode(vk::CullModeFlags::NONE)
                            .polygon_mode(vk::PolygonMode::FILL)
                            .line_width(1.0),
                    )
                    .multisample_state(
                        &vk::PipelineMultisampleStateCreateInfo::builder()
                            .rasterization_samples(vk::SampleCountFlags::TYPE_1),
                    )
                    .depth_stencil_state(
                        &vk::PipelineDepthStencilStateCreateInfo::builder()
                            .depth_test_enable(false)
                            .depth_write_enable(false)
                            .front(noop_stencil_state)
                            .back(noop_stencil_state),
                    )
                    .color_blend_state(
                        &vk::PipelineColorBlendStateCreateInfo::builder().attachments(&[
                            vk::PipelineColorBlendAttachmentState {
                                blend_enable: vk::TRUE,
                                src_color_blend_factor: vk::BlendFactor::ONE,
                                dst_color_blend_factor: vk::BlendFactor::ZERO,
                                color_blend_op: vk::BlendOp::ADD,
                                color_write_mask: vk::ColorComponentFlags::R
                                    | vk::ColorComponentFlags::G
                                    | vk::ColorComponentFlags::B,
                                ..Default::default()
                            },
                        ]),
                    )
                    .dynamic_state(
                        &vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&[
                            vk::DynamicState::VIEWPORT,
                            vk::DynamicState::SCISSOR,
                        ]),
                    )
                    .layout(pipeline_layout)
                    .render_pass(render_pass)
                    .subpass(0)
                    .build()],
                None,
            )
            .unwrap()[0];

        vk_device.destroy_shader_module(vert, None);
        vk_device.destroy_shader_module(frag, None);

        // A session represents this application's desire to display things! This is where we hook
        // up our graphics API. This does not start the session; for that, you'll need a call to
        // Session::begin, which we do in 'main_loop below.
        let (session, mut frame_wait, mut frame_stream) = xr_instance
            .create_session::<xr::Vulkan>(
                system,
                &xr::vulkan::SessionCreateInfo {
                    instance: vk_instance.handle().as_raw() as _,
                    physical_device: vk_physical_device.as_raw() as _,
                    device: vk_device.handle().as_raw() as _,
                    queue_family_index,
                    queue_index: 0,
                },
            )
            .unwrap();

        // OpenXR uses a couple different types of reference frames for positioning content; we need
        // to choose one for displaying our content! STAGE would be relative to the center of your
        // guardian system's bounds, and LOCAL would be relative to your device's starting location.
        let stage = session
            .create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)
            .unwrap();

        let cmd_pool = vk_device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .queue_family_index(queue_family_index)
                    .flags(
                        vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER
                            | vk::CommandPoolCreateFlags::TRANSIENT,
                    ),
                None,
            )
            .unwrap();
        let cmds = vk_device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_pool(cmd_pool)
                    .command_buffer_count(PIPELINE_DEPTH),
            )
            .unwrap();
        let fences = (0..PIPELINE_DEPTH)
            .map(|_| {
                vk_device
                    .create_fence(
                        &vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED),
                        None,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // Main loop
        let mut swapchain = None;
        let mut event_storage = xr::EventDataBuffer::new();
        let mut session_running = false;
        // Index of the current frame, wrapped by PIPELINE_DEPTH. Not to be confused with the
        // swapchain image index.
        let mut frame = 0;
        'main_loop: loop {
            if !running.load(Ordering::Relaxed) {
                println!("requesting exit");
                // The OpenXR runtime may want to perform a smooth transition between scenes, so we
                // can't necessarily exit instantly. Instead, we must notify the runtime of our
                // intent and wait for it to tell us when we're actually done.
                match session.request_exit() {
                    Ok(()) => {}
                    Err(xr::sys::Result::ERROR_SESSION_NOT_RUNNING) => break,
                    Err(e) => panic!("{}", e),
                }
            }

            while let Some(event) = xr_instance.poll_event(&mut event_storage).unwrap() {
                use xr::Event::*;
                match event {
                    SessionStateChanged(e) => {
                        // Session state change is where we can begin and end sessions, as well as
                        // find quit messages!
                        println!("entered state {:?}", e.state());
                        match e.state() {
                            xr::SessionState::READY => {
                                session.begin(VIEW_TYPE).unwrap();
                                session_running = true;
                            }
                            xr::SessionState::STOPPING => {
                                session.end().unwrap();
                                session_running = false;
                            }
                            xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => {
                                break 'main_loop;
                            }
                            _ => {}
                        }
                    }
                    InstanceLossPending(_) => {
                        break 'main_loop;
                    }
                    EventsLost(e) => {
                        println!("lost {} events", e.lost_event_count());
                    }
                    _ => {}
                }
            }

            if !session_running {
                // Don't grind up the CPU
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            // Block until the previous frame is finished displaying, and is ready for another one.
            // Also returns a prediction of when the next frame will be displayed, for use with
            // predicting locations of controllers, viewpoints, etc.
            let xr_frame_state = frame_wait.wait().unwrap();
            // Must be called before any rendering is done!
            frame_stream.begin().unwrap();

            if !xr_frame_state.should_render {
                frame_stream
                    .end(
                        xr_frame_state.predicted_display_time,
                        environment_blend_mode,
                        &[],
                    )
                    .unwrap();
                continue;
            }

            if swapchain.is_none() {
                // Now we need to find all the viewpoints we need to take care of! This is a
                // property of the view configuration type; in this example we use PRIMARY_STEREO,
                // so we should have 2 viewpoints.
                //
                // Because we are using multiview in this example, we require that all view
                // dimensions are identical.
                let views = xr_instance
                    .enumerate_view_configuration_views(system, VIEW_TYPE)
                    .unwrap();
                assert_eq!(views.len(), VIEW_COUNT as usize);
                assert_eq!(views[0], views[1]);

                // Create a swapchain for the viewpoints! A swapchain is a set of texture buffers
                // used for displaying to screen, typically this is a backbuffer and a front buffer,
                // one for rendering data to, and one for displaying on-screen.
                let resolution = vk::Extent2D {
                    width: views[0].recommended_image_rect_width,
                    height: views[0].recommended_image_rect_height,
                };
                let handle = session
                    .create_swapchain(&xr::SwapchainCreateInfo {
                        create_flags: xr::SwapchainCreateFlags::EMPTY,
                        usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                            | xr::SwapchainUsageFlags::SAMPLED,
                        format: COLOR_FORMAT.as_raw() as _,
                        // The Vulkan graphics pipeline we create is not set up for multisampling,
                        // so we hardcode this to 1. If we used a proper multisampling setup, we
                        // could set this to `views[0].recommended_swapchain_sample_count`.
                        sample_count: 1,
                        width: resolution.width,
                        height: resolution.height,
                        face_count: 1,
                        array_size: VIEW_COUNT,
                        mip_count: 1,
                    })
                    .unwrap();

                // We'll want to track our own information about the swapchain, so we can draw stuff
                // onto it! We'll also create a buffer for each generated texture here as well.
                let images = handle.enumerate_images().unwrap();
                swapchain = Some(Swapchain {
                    handle,
                    resolution,
                    buffers: images
                        .into_iter()
                        .map(|color_image| {
                            let color_image = vk::Image::from_raw(color_image);
                            let color = vk_device
                                .create_image_view(
                                    &vk::ImageViewCreateInfo::builder()
                                        .image(color_image)
                                        .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
                                        .format(COLOR_FORMAT)
                                        .subresource_range(vk::ImageSubresourceRange {
                                            aspect_mask: vk::ImageAspectFlags::COLOR,
                                            base_mip_level: 0,
                                            level_count: 1,
                                            base_array_layer: 0,
                                            layer_count: VIEW_COUNT,
                                        }),
                                    None,
                                )
                                .unwrap();
                            let framebuffer = vk_device
                                .create_framebuffer(
                                    &vk::FramebufferCreateInfo::builder()
                                        .render_pass(render_pass)
                                        .width(resolution.width)
                                        .height(resolution.height)
                                        .attachments(&[color])
                                        .layers(1), // Multiview handles addressing multiple layers
                                    None,
                                )
                                .unwrap();
                            Framebuffer { framebuffer, color }
                        })
                        .collect(),
                });
            }
            let swapchain = swapchain.as_mut().unwrap();

            // We need to ask which swapchain image to use for rendering! Which one will we get?
            // Who knows! It's up to the runtime to decide.
            let image_index = swapchain.handle.acquire_image().unwrap();

            // Wait until the image is available to render to. The compositor could still be
            // reading from it.
            swapchain.handle.wait_image(xr::Duration::INFINITE).unwrap();

            // Ensure the last use of this frame's resources is 100% done
            vk_device
                .wait_for_fences(&[fences[frame]], true, u64::MAX)
                .unwrap();
            vk_device.reset_fences(&[fences[frame]]).unwrap();

            let cmd = cmds[frame];
            vk_device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::builder()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            vk_device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::builder()
                    .render_pass(render_pass)
                    .framebuffer(swapchain.buffers[image_index as usize].framebuffer)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: swapchain.resolution,
                    })
                    .clear_values(&[vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    }]),
                vk::SubpassContents::INLINE,
            );

            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: swapchain.resolution.width as f32,
                height: swapchain.resolution.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: swapchain.resolution,
            }];
            vk_device.cmd_set_viewport(cmd, 0, &viewports);
            vk_device.cmd_set_scissor(cmd, 0, &scissors);

            // Draw the scene. Multiview means we only need to do this once, and the GPU will
            // automatically broadcast operations to all views. Shaders can use `gl_ViewIndex` to
            // e.g. select the correct view matrix.
            vk_device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            vk_device.cmd_draw(cmd, 3, 1, 0, 0);

            vk_device.cmd_end_render_pass(cmd);
            vk_device.end_command_buffer(cmd).unwrap();

            // Fetch the view transforms. To minimize latency, we intentionally do this *after*
            // recording commands to render the scene, i.e. at the last possible moment before
            // rendering begins in earnest on the GPU. Uniforms dependent on this data can be sent
            // to the GPU just-in-time by writing them to per-frame host-visible memory which the
            // GPU will only read once the command buffer is submitted.
            let (_, views) = session
                .locate_views(VIEW_TYPE, xr_frame_state.predicted_display_time, &stage)
                .unwrap();

            // Submit commands to the GPU, then tell OpenXR we're done with our part.
            vk_device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::builder().command_buffers(&[cmd]).build()],
                    fences[frame],
                )
                .unwrap();
            swapchain.handle.release_image().unwrap();

            // Tell OpenXR what to present for this frame
            let rect = xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di {
                    width: swapchain.resolution.width as _,
                    height: swapchain.resolution.height as _,
                },
            };
            frame_stream
                .end(
                    xr_frame_state.predicted_display_time,
                    environment_blend_mode,
                    &[
                        &xr::CompositionLayerProjection::new().space(&stage).views(&[
                            xr::CompositionLayerProjectionView::new()
                                .pose(views[0].pose)
                                .fov(views[0].fov)
                                .sub_image(
                                    xr::SwapchainSubImage::new()
                                        .swapchain(&swapchain.handle)
                                        .image_array_index(0)
                                        .image_rect(rect),
                                ),
                            xr::CompositionLayerProjectionView::new()
                                .pose(views[1].pose)
                                .fov(views[1].fov)
                                .sub_image(
                                    xr::SwapchainSubImage::new()
                                        .swapchain(&swapchain.handle)
                                        .image_array_index(1)
                                        .image_rect(rect),
                                ),
                        ]),
                    ],
                )
                .unwrap();
            frame = (frame + 1) % PIPELINE_DEPTH as usize;
        }

        // OpenXR MUST be allowed to clean up before we destroy Vulkan resources it could touch, so
        // first we must drop all its handles.
        drop((session, frame_wait, frame_stream, stage));

        // Ensure all in-flight frames are finished before destroying resources they might use
        vk_device.wait_for_fences(&fences, true, !0).unwrap();
        for fence in fences {
            vk_device.destroy_fence(fence, None);
        }

        if let Some(swapchain) = swapchain {
            for buffer in swapchain.buffers {
                vk_device.destroy_framebuffer(buffer.framebuffer, None);
                vk_device.destroy_image_view(buffer.color, None);
            }
        }

        vk_device.destroy_pipeline(pipeline, None);
        vk_device.destroy_pipeline_layout(pipeline_layout, None);
        vk_device.destroy_command_pool(cmd_pool, None);
        vk_device.destroy_render_pass(render_pass, None);
        vk_device.destroy_device(None);
        vk_instance.destroy_instance(None);
    }

    println!("exiting cleanly");
}

pub const COLOR_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
pub const VIEW_COUNT: u32 = 2;
const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;

struct Swapchain {
    handle: xr::Swapchain<xr::Vulkan>,
    buffers: Vec<Framebuffer>,
    resolution: vk::Extent2D,
}

struct Framebuffer {
    framebuffer: vk::Framebuffer,
    color: vk::ImageView,
}

/// Maximum number of frames in flight
const PIPELINE_DEPTH: u32 = 2;

//     #[cfg(feature = "vr")]
//     fn setup_vr() -> Result<()> {
//         // Create entry
//         let entry = xr::Entry::linked();

//         // Ensure required extensions are available
//         let available_extensions = entry.enumerate_extensions()?;
//         log::info!("OpenXR supported extensions: {:#?}", available_extensions);
//         assert!(available_extensions.khr_vulkan_enable);

//         // Create application info
//         let app_info = xr::ApplicationInfo {
//             application_name: "Dragonglass",
//             application_version: 0,
//             engine_name: "Dragonglass",
//             engine_version: 0,
//         };

//         // List required extensions
//         let mut required_extensions = xr::ExtensionSet::default();
//         required_extensions.khr_vulkan_enable = true;

//         // Create the OpenXR instance
//         let xr_instance = entry.create_instance(&app_info, &required_extensions, &[])?;

//         // List instance properties to show it was created successfully
//         let instance_props = xr_instance.properties()?;
//         log::info!(
//             "Loaded OpenXR runtime: {} {}",
//             instance_props.runtime_name,
//             instance_props.runtime_version
//         );

//         // Request a form factor from the device (HMD, Handheld, etc.)
//         let xr_system = xr_instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;

//         // Declare view type
//         let view_type: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;

//         // Check what blend mode is valid for this device (opaque vs transparent displays). We'll just
//         // take the first one available!
//         let environment_blend_mode =
//             xr_instance.enumerate_environment_blend_modes(xr_system, view_type)?[0];

//         // Next steps need to be done in renderer

//         // Get required vulkan instance extensions
//         let vk_instance_exts = xr_instance
//             .vulkan_instance_extensions(xr_system)?
//             .split(' ')
//             .map(|x| Ok(std::ffi::CString::new(x)?))
//             .collect::<Result<Vec<_>>>()?;
//         log::info!(
//             "Required Vulkan instance extensions: {:#?}",
//             vk_instance_exts
//         );

//         // Get pointers to required vulkan instance extension names
//         let vk_instance_ext_ptrs = vk_instance_exts
//             .iter()
//             .map(|x| x.as_ptr())
//             .collect::<Vec<_>>();

//         // Get required vulkan device extensions
//         let vk_device_exts = xr_instance
//             .vulkan_device_extensions(xr_system)?
//             .split(' ')
//             .map(|x| Ok(std::ffi::CString::new(x)?))
//             .collect::<Result<Vec<_>>>()?;
//         log::info!("Required Vulkan device extensions: {:#?}", vk_device_exts);

//         // Get pointers to required vulkan device extension names
//         let vk_device_ext_ptrs = vk_device_exts
//             .iter()
//             .map(|x| x.as_ptr())
//             .collect::<Vec<_>>();

//         // Create OpenXR Version type from vulkan version
//         // use: vk::version_major(vk_version) as u16, etc in real code
//         // TODO: Use Vulkan app info api version 1.1 because it guarantees multiview support
//         let vk_version = xr::Version::new(1, 1, 0);

//         // Gather graphics requirements
//         let graphics_requirements = xr_instance.graphics_requirements::<xr::Vulkan>(xr_system)?;
//         if graphics_requirements.min_api_version_supported > vk_version {
//             anyhow::bail!(
//                 "OpenXR runtime requires Vulkan version > {}",
//                 graphics_requirements.min_api_version_supported
//             );
//         }

//         // TODO: Create physical device from raw ptr like this
//         // let vk_physical_device = vk::PhysicalDevice::from_raw(
//         //     xr_instance.vulkan_graphics_device(xr_system, vk_instance.handle().as_raw() as _)? as _,
//         // );

//         /********* Vulkan Multiview ******/
//         // TODO: Get physical device properties and make sure it supports Vulkan version 1.1

//         // TODO: Add multiview PhysicalDeviceVulkan11Feature
//         // in the create_info for the logical device,
//         // push_next(&mut vk::PhysicalDeviceVulkan11Features {
//         //     multiview: vk::TRUE,
//         //     ..Default::default()
//         // })

//         let view_count = 2;
//         let view_mask = !(!0 << view_count);
//         // TODO: When specifying scene renderpass, add this to the end
//         // .push_next(
//         //     &mut vk::RenderPassMultiviewCreateInfo::builder()
//         //         .view_masks(&[view_mask])
//         //         .correlation_masks(&[view_mask]),
//         // ),
//         /*********************************/
//         // Create session, using instance, physical device, and logical device from Vulkan context in renderer
//         // Note: This doesn't start the session
//         //
//         // let (session, mut frame_wait, mut frame_stream) = xr_instance
//         //     .create_session::<xr::Vulkan>(
//         //         system,
//         //         &xr::vulkan::SessionCreateInfo {
//         //             instance: vk_instance.handle().as_raw() as _,
//         //             physical_device: vk_physical_device.as_raw() as _,
//         //             device: vk_device.handle().as_raw() as _,
//         //             queue_family_index,
//         //             queue_index: 0,
//         //         },
//         //     )?;

//         // OpenXR uses a couple different types of reference frames for positioning content; we need
//         // to choose one for displaying our content! STAGE would be relative to the center of your
//         // guardian system's bounds, and LOCAL would be relative to your device's starting location.
//         //
//         // let stage = session.create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)?;

//         let mut event_storage = xr::EventDataBuffer::new();
//         let mut session_running = false;
//         /********* Main Loop **************/
//         // The OpenXR runtime may want to perform a smooth transition between scenes, so we
//         // can't necessarily exit instantly. Instead, we must notify the runtime of our
//         // intent and wait for it to tell us when we're actually done.
//         //
//         // if exit_requested {
//         //     match session.request_exit() {
//         //         Ok(()) => {}
//         //         Err(xr::sys::Result::ERROR_SESSION_NOT_RUNNING) => break,
//         //         Err(e) => bail!("{}", e),
//         //     }
//         // }

//         // TODO: See main loop from openxrs rust example

//         /**********************************/
//         Ok(())
//     }
// }
