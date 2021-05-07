use anyhow::{bail, Result};
use glutin::{ContextBuilder, ContextWrapper, PossiblyCurrent};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

#[cfg(target_os = "windows")]
use glutin::platform::windows::RawContextExt;

pub unsafe fn load_context(
    window_handle: &impl HasRawWindowHandle,
) -> Result<ContextWrapper<PossiblyCurrent, ()>> {
    let raw_context = match window_handle.raw_window_handle() {
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
    };

    let context = raw_context.make_current().unwrap();

    gl::load_with(|symbol| context.get_proc_address(symbol) as *const _);

    Ok(context)
}
