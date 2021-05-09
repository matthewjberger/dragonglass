use anyhow::{bail, Result};
use glutin::{ContextBuilder, ContextWrapper, PossiblyCurrent};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

#[cfg(target_os = "windows")]
use glutin::platform::windows::RawContextExt;

#[cfg(target_os = "linux")]
use glutin::platform::unix::{EventLoopWindowTargetExtUnix, RawContextExt, WindowExtUnix};

pub unsafe fn load_context(
    window_handle: &impl HasRawWindowHandle,
) -> Result<ContextWrapper<PossiblyCurrent, ()>> {
    let raw_context = match window_handle.raw_window_handle() {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Windows(handle) => ContextBuilder::new().build_raw_context(handle.hwnd)?,

        // TODO: Support other OS's here

        _ => bail!("The target operating system is not supported!"),
    };

    let context = raw_context.make_current().unwrap();

    gl::load_with(|symbol| context.get_proc_address(symbol) as *const _);

    Ok(context)
}
