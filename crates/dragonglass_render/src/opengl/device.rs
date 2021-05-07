use crate::Render;
use anyhow::{bail, Result};
use dragonglass_world::{legion::EntityStore, Camera, PerspectiveCamera, World};
use gl::types::*;
use glutin::{ContextBuilder, ContextWrapper, PossiblyCurrent};
use imgui::{Context as ImguiContext, DrawData};
use log::{error, info};
use nalgebra_glm as glm;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::sync::Arc;

#[cfg(target_os = "windows")]
use glutin::platform::windows::{RawContextExt, WindowExtWindows};

pub struct OpenGLRenderBackend {
    context: ContextWrapper<PossiblyCurrent, ()>,
}

impl OpenGLRenderBackend {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
        imgui: &mut ImguiContext,
    ) -> Result<Self> {
        let raw_context = unsafe {
            match window_handle.raw_window_handle() {
                #[cfg(target_os = "windows")]
                RawWindowHandle::Windows(handle) => {
                    ContextBuilder::new().build_raw_context(handle.hwnd)?
                    // handle.hinstance
                    // handle.hwnd
                }

                #[cfg(any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                RawWindowHandle::Wayland(handle) => {
                    // handle.surface
                    //handle.display;
                }

                #[cfg(any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                RawWindowHandle::Xlib(handle) => {
                    // handle.display as *mut _
                    // handle.window
                }

                #[cfg(any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                RawWindowHandle::Xcb(handle) => {
                    // handle.connection as *mut _
                    // handle.window
                }

                #[cfg(any(target_os = "android"))]
                RawWindowHandle::Android(handle) => {
                    // handle.a_native_window as _
                }

                _ => bail!("The target operating system is not supported!"),
            }
        };

        let context = unsafe { raw_context.make_current().unwrap() };

        gl::load_with(|symbol| context.get_proc_address(symbol) as *const _);

        Ok(Self { context })
    }
}

impl Render for OpenGLRenderBackend {
    fn load_skybox(&mut self, path: &str) -> Result<()> {
        Ok(())
    }

    fn load_world(&mut self, world: &World) -> Result<()> {
        Ok(())
    }

    fn reload_asset_shaders(&mut self) -> Result<()> {
        Ok(())
    }

    fn render(&mut self, dimensions: &[u32; 2], world: &World, draw_data: &DrawData) -> Result<()> {
        let red: [GLfloat; 4] = [1.0, 0.0, 0.0, 0.0];
        unsafe {
            gl::ClearBufferfv(gl::COLOR, 0, &red as *const f32);
        }
        self.context.swap_buffers()?;
        Ok(())
    }

    fn toggle_wireframe(&mut self) {}
}
