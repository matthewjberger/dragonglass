use anyhow::{anyhow, Result};
use ash::{
    extensions::khr::Surface as AshSurface,
    version::{EntryV1_0, InstanceV1_0},
    vk,
    vk::SurfaceKHR,
};
use raw_window_handle::RawWindowHandle;

pub struct Surface {
    pub handle_ash: AshSurface,
    pub handle_khr: SurfaceKHR,
}

impl Surface {
    pub fn new(
        entry: &ash::Entry,
        instance: &ash::Instance,
        raw_window_handle: &RawWindowHandle,
    ) -> Result<Self> {
        let handle_ash = AshSurface::new(entry, instance);
        let handle_khr = Self::create_surface_khr(entry, instance, raw_window_handle)?;
        let surface = Self {
            handle_ash,
            handle_khr,
        };
        Ok(surface)
    }

    fn create_surface_khr<E: EntryV1_0, I: InstanceV1_0>(
        entry: &E,
        instance: &I,
        raw_window_handle: &RawWindowHandle,
    ) -> Result<SurfaceKHR> {
        let surface_khr = match raw_window_handle {
            #[cfg(target_os = "windows")]
            RawWindowHandle::Windows(handle) => {
                Self::create_surface_khr_windows(entry, instance, &handle)?
            }
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(handle) => {
                Self::create_surface_khr_linux(entry, instance, &handle)?
            }
            handle => {
                return Err(anyhow!(
                    "Unsupported raw window handle type was requested! {:#?}",
                    handle
                ))
            }
        };

        Ok(surface_khr)
    }

    #[cfg(target_os = "windows")]
    fn create_surface_khr_windows<E: EntryV1_0, I: InstanceV1_0>(
        entry: &E,
        instance: &I,
        handle: &raw_window_handle::windows::WindowsHandle,
    ) -> Result<SurfaceKHR> {
        let win32_create_info = vk::Win32SurfaceCreateInfoKHR::builder()
            .hwnd(handle.hwnd)
            .hinstance(handle.hinstance)
            .build();

        let win32_surface_loader = ash::extensions::khr::Win32Surface::new(entry, instance);

        let surface_khr =
            unsafe { win32_surface_loader.create_win32_surface(&win32_create_info, None) }?;

        Ok(surface_khr)
    }

    #[cfg(target_os = "linux")]
    fn create_surface_khr_linux<E: EntryV1_0, I: InstanceV1_0>(
        entry: &E,
        instance: &I,
        handle: &raw_window_handle::unix::XlibHandle,
    ) -> Result<SurfaceKHR> {
        use ash::extensions::khr::XlibSurface;

        let x11_create_info = vk::XlibSurfaceCreateInfoKHR::builder()
            .window(handle.window)
            .dpy(handle.display as *mut vk::Display);

        let xlib_surface_loader = XlibSurface::new(entry, instance);

        let surface_khr = xlib_surface_loader.create_xlib_surface(&x11_create_info, None)?;
        Ok(surface_khr)
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.handle_ash.destroy_surface(self.handle_khr, None);
        }
    }
}
