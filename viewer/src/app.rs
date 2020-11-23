use crate::{camera::OrbitalCamera, input::Input, settings::Settings, system::System};
use anyhow::Result;
use dragonglass::RenderingDevice;
use dragonglass_scene::{load_gltf_asset, Asset};
use image::ImageFormat;
use log::{error, info, warn};
use winit::{
    dpi::PhysicalSize,
    event::ElementState,
    event::KeyboardInput,
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

#[cfg(feature = "vr")]
use openxr as xr;

pub struct App {
    asset: Option<Asset>,
    camera: OrbitalCamera,
    _settings: Settings,
    input: Input,
    system: System,
    _window: Window,
    rendering_device: RenderingDevice,
    event_loop: EventLoop<()>,
}

impl App {
    pub const TITLE: &'static str = "Dragonglass - GLTF Model Viewer";

    fn load_icon(icon_bytes: &[u8], format: ImageFormat) -> Result<Icon> {
        let (rgba, width, height) = {
            let image = image::load_from_memory_with_format(icon_bytes, format)?.into_rgba();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            (rgba, width, height)
        };
        let icon = Icon::from_rgba(rgba, width, height)?;
        Ok(icon)
    }

    pub fn new() -> Result<Self> {
        let settings = Settings::load_current_settings()?;

        let icon = Self::load_icon(
            include_bytes!("../../assets/icon/icon.png"),
            ImageFormat::Png,
        )?;

        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_window_icon(Some(icon))
            .with_title(Self::TITLE)
            .with_inner_size(PhysicalSize::new(settings.width, settings.height))
            .build(&event_loop)?;

        let logical_size = window.inner_size();
        let window_dimensions = [logical_size.width, logical_size.height];
        let rendering_device = RenderingDevice::new(&window, &window_dimensions)?;

        #[cfg(feature = "vr")]
        Self::setup_vr()?;

        let app = Self {
            asset: None,
            camera: OrbitalCamera::default(),
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            _window: window,
            rendering_device,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut camera,
            mut input,
            mut system,
            mut rendering_device,
            mut asset,
            event_loop,
            ..
        } = self;

        input.allowed = true;

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            match event {
                Event::MainEventsCleared => {
                    if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                        *control_flow = ControlFlow::Exit;
                    }

                    Self::update_camera(&mut camera, &input, &system);

                    if let Some(gltf_asset) = asset.as_mut() {
                        if !gltf_asset.animations.is_empty() {
                            gltf_asset.animate(0, 0.75 * system.delta_time as f32);
                        }
                    }

                    if let Err(error) = rendering_device.render(
                        &system.window_dimensions,
                        camera.view_matrix(),
                        camera.position(),
                        &asset,
                    ) {
                        error!("{}", error);
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::DroppedFile(path),
                    ..
                } => {
                    if let Some(raw_path) = path.to_str() {
                        if let Some(extension) = path.extension() {
                            match extension.to_str() {
                                Some("glb") | Some("gltf") => {
                                    let gltf_asset = load_gltf_asset(path.clone()).unwrap();
                                    if let Err(error) = rendering_device.load_asset(&gltf_asset) {
                                        warn!("Failed to load asset: {}", error);
                                    }
                                    camera = OrbitalCamera::default();
                                    info!("Loaded gltf asset: '{}'", raw_path);
                                    asset = Some(gltf_asset);
                                }
                                Some("hdr") => {
                                    if let Err(error) = rendering_device.load_skybox(raw_path) {
                                        error!("Viewer error: {}", error);
                                    }
                                    camera = OrbitalCamera::default();
                                    info!("Loaded hdr cubemap: '{}'", raw_path);
                                }
                                _ => warn!(
                                    "File extension {:#?} is not a valid '.glb', '.gltf', or 'hdr' extension",
                                    extension),
                            }
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput {
                        input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                        ..
                    },
                    ..
                } => {
                    if let VirtualKeyCode::T = keycode {
                        rendering_device.toggle_wireframe();
                    }
                }
                _ => {}
            }
        });
    }

    fn update_camera(camera: &mut OrbitalCamera, input: &Input, system: &System) {
        if !input.allowed {
            return;
        }
        let scroll_multiplier = 0.01;
        let rotation_multiplier = 0.05;
        let drag_multiplier = 0.001;

        camera.forward(input.mouse.wheel_delta.y * scroll_multiplier);

        if input.is_key_pressed(VirtualKeyCode::R) {
            *camera = OrbitalCamera::default();
        }

        if input.mouse.is_left_clicked {
            let delta = input.mouse.position_delta;
            let rotation = delta * rotation_multiplier * system.delta_time as f32;
            camera.rotate(&rotation);
        } else if input.mouse.is_right_clicked {
            let delta = input.mouse.position_delta;
            let pan = delta * drag_multiplier;
            camera.pan(&pan);
        }
    }

    #[cfg(feature = "vr")]
    fn setup_vr() -> Result<()> {
        // Create entry
        let entry = xr::Entry::linked();

        // Ensure required extensions are available
        let available_extensions = entry.enumerate_extensions()?;
        log::info!("OpenXR supported extensions: {:#?}", available_extensions);
        assert!(available_extensions.khr_vulkan_enable);

        // Create application info
        let app_info = xr::ApplicationInfo {
            application_name: "Dragonglass",
            application_version: 0,
            engine_name: "Dragonglass",
            engine_version: 0,
        };

        // List required extensions
        let mut required_extensions = xr::ExtensionSet::default();
        required_extensions.khr_vulkan_enable = true;

        // Create the OpenXR instance
        let xr_instance = entry.create_instance(&app_info, &required_extensions, &[])?;

        // List instance properties to show it was created successfully
        let instance_props = xr_instance.properties()?;
        log::info!(
            "Loaded OpenXR runtime: {} {}",
            instance_props.runtime_name,
            instance_props.runtime_version
        );

        // Request a form factor from the device (HMD, Handheld, etc.)
        let xr_system = xr_instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;

        // Declare view type
        let view_type: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;

        // Check what blend mode is valid for this device (opaque vs transparent displays). We'll just
        // take the first one available!
        let environment_blend_mode =
            xr_instance.enumerate_environment_blend_modes(xr_system, view_type)?[0];

        // Next steps need to be done in renderer

        // Get required vulkan instance extensions
        let vk_instance_exts = xr_instance
            .vulkan_instance_extensions(xr_system)?
            .split(' ')
            .map(|x| Ok(std::ffi::CString::new(x)?))
            .collect::<Result<Vec<_>>>()?;
        log::info!(
            "Required Vulkan instance extensions: {:#?}",
            vk_instance_exts
        );

        // Get pointers to required vulkan instance extension names
        let vk_instance_ext_ptrs = vk_instance_exts
            .iter()
            .map(|x| x.as_ptr())
            .collect::<Vec<_>>();

        // Get required vulkan device extensions
        let vk_device_exts = xr_instance
            .vulkan_device_extensions(xr_system)?
            .split(' ')
            .map(|x| Ok(std::ffi::CString::new(x)?))
            .collect::<Result<Vec<_>>>()?;
        log::info!("Required Vulkan device extensions: {:#?}", vk_device_exts);

        // Get pointers to required vulkan device extension names
        let vk_device_ext_ptrs = vk_device_exts
            .iter()
            .map(|x| x.as_ptr())
            .collect::<Vec<_>>();

        // Create OpenXR Version type from vulkan version
        // use: vk::version_major(vk_version) as u16, etc in real code
        // TODO: Use Vulkan app info api version 1.1 because it guarantees multiview support
        let vk_version = xr::Version::new(1, 1, 0);

        // Gather graphics requirements
        let graphics_requirements = xr_instance.graphics_requirements::<xr::Vulkan>(xr_system)?;
        if graphics_requirements.min_api_version_supported > vk_version {
            anyhow::bail!(
                "OpenXR runtime requires Vulkan version > {}",
                graphics_requirements.min_api_version_supported
            );
        }

        // TODO: Create physical device from raw ptr like this
        // let vk_physical_device = vk::PhysicalDevice::from_raw(
        //     xr_instance.vulkan_graphics_device(xr_system, vk_instance.handle().as_raw() as _)? as _,
        // );

        /********* Vulkan Multiview ******/
        // TODO: Get physical device properties and make sure it supports Vulkan version 1.1

        // TODO: Add multiview PhysicalDeviceVulkan11Feature
        // in the create_info for the logical device,
        // push_next(&mut vk::PhysicalDeviceVulkan11Features {
        //     multiview: vk::TRUE,
        //     ..Default::default()
        // })

        let view_count = 2;
        let view_mask = !(!0 << view_count);
        // TODO: When specifying scene renderpass, add this to the end
        // .push_next(
        //     &mut vk::RenderPassMultiviewCreateInfo::builder()
        //         .view_masks(&[view_mask])
        //         .correlation_masks(&[view_mask]),
        // ),
        /*********************************/

        // Create session, using instance, physical device, and logical device from Vulkan context in renderer
        // Note: This doesn't start the session
        //
        // let (session, mut frame_wait, mut frame_stream) = xr_instance
        //     .create_session::<xr::Vulkan>(
        //         system,
        //         &xr::vulkan::SessionCreateInfo {
        //             instance: vk_instance.handle().as_raw() as _,
        //             physical_device: vk_physical_device.as_raw() as _,
        //             device: vk_device.handle().as_raw() as _,
        //             queue_family_index,
        //             queue_index: 0,
        //         },
        //     )?;

        // OpenXR uses a couple different types of reference frames for positioning content; we need
        // to choose one for displaying our content! STAGE would be relative to the center of your
        // guardian system's bounds, and LOCAL would be relative to your device's starting location.
        //
        // let stage = session.create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)?;

        let mut event_storage = xr::EventDataBuffer::new();
        let mut session_running = false;
        /********* Main Loop **************/

        // The OpenXR runtime may want to perform a smooth transition between scenes, so we
        // can't necessarily exit instantly. Instead, we must notify the runtime of our
        // intent and wait for it to tell us when we're actually done.
        //
        // if exit_requested {
        //     match session.request_exit() {
        //         Ok(()) => {}
        //         Err(xr::sys::Result::ERROR_SESSION_NOT_RUNNING) => break,
        //         Err(e) => bail!("{}", e),
        //     }
        // }

        // TODO: See main loop from openxrs rust example

        /**********************************/
        Ok(())
    }
}
