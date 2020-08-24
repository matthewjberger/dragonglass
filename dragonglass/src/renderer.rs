use super::core::Context;
use anyhow::Result;
use log::info;
use raw_window_handle::RawWindowHandle;
use std::sync::Arc;

pub struct Renderer {
    _context: Arc<Context>,
}

impl Renderer {
    pub fn new(raw_window_handle: &RawWindowHandle) -> Result<Self> {
        let renderer = Self {
            _context: Arc::new(Context::new(&raw_window_handle)?),
        };
        Ok(renderer)
    }

    pub fn initialize(&mut self) -> Result<()> {
        info!("Initializing renderer");
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        Ok(())
    }
}
