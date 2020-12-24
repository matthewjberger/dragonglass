mod app;
mod camera;
mod gui;
mod input;
mod logger;
mod settings;
mod system;

use anyhow::Result;
use app::App;
use logger::create_logger;

fn main() -> Result<()> {
    create_logger()?;
    let app = App::new()?;
    app.run()?;
    Ok(())
}
