use super::{
    debug::DebugLayer, instance::Instance, logical_device::LogicalDevice,
    physical_device::PhysicalDevice, surface::Surface,
};
use anyhow::Result;
use log::info;
use raw_window_handle::RawWindowHandle;
use vk_mem::{Allocator, AllocatorCreateInfo};

// The order the struct members are declared in
// determines the order they are 'Drop'ped in
// when this struct is dropped
pub struct Context {
    pub allocator: vk_mem::Allocator,
    pub logical_device: LogicalDevice,
    pub debug_layer: Option<DebugLayer>,
    pub physical_device: PhysicalDevice,
    pub surface: Surface,
    pub instance: Instance,
    pub entry: ash::Entry,
}

impl Context {
    pub fn new(raw_window_handle: &RawWindowHandle) -> Result<Self> {
        let entry = ash::Entry::new()?;
        let instance = Instance::new(&entry)?;
        let surface = Surface::new(&entry, &instance.handle, &raw_window_handle)?;
        let physical_device = PhysicalDevice::new(&instance.handle, &surface)?;
        let debug_layer = if DebugLayer::enabled() {
            info!("Loading debug layer");
            Some(DebugLayer::new(&entry, &instance.handle)?)
        } else {
            None
        };
        let logical_device = LogicalDevice::from_physical(&instance.handle, &physical_device)?;

        let allocator_create_info = AllocatorCreateInfo {
            device: logical_device.handle.clone(),
            instance: instance.handle.clone(),
            physical_device: physical_device.handle,
            ..Default::default()
        };

        let allocator = Allocator::new(&allocator_create_info)?;

        let context = Self {
            allocator,
            logical_device,
            debug_layer,
            physical_device,
            surface,
            instance,
            entry,
        };

        Ok(context)
    }
}
