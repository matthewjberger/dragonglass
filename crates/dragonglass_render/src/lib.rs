#[cfg(feature = "vulkan")]
mod vulkan;

#[cfg(feature = "opengl")]
mod opengl;

pub mod render;

pub use crate::render::{create_render_backend, Backend, Render};

unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    std::slice::from_raw_parts(data_ptr, std::mem::size_of::<T>())
}
