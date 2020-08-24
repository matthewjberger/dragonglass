use anyhow::Result;
use log::info;

pub struct Renderer;

impl Renderer {
    pub fn new() -> Result<Self> {
        let renderer = Self {};
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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
