use anyhow::Result;
use ash::{extensions::khr::Surface as AshSurface, vk::SurfaceKHR};
use ash_window::create_surface;
use raw_window_handle::HasRawWindowHandle;

pub struct Surface {
    pub handle_ash: AshSurface,
    pub handle_khr: SurfaceKHR,
}

impl Surface {
    pub fn new<T: HasRawWindowHandle>(
        entry: &ash::Entry,
        instance: &ash::Instance,
        window_handle: &T,
    ) -> Result<Self> {
        let handle_ash = AshSurface::new(entry, instance);
        let handle_khr = unsafe { create_surface(entry, instance, window_handle, None) }?;
        let surface = Self {
            handle_ash,
            handle_khr,
        };
        Ok(surface)
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.handle_ash.destroy_surface(self.handle_khr, None);
        }
    }
}
