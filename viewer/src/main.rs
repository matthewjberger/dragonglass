mod app;
mod logger;
mod settings;

use anyhow::Result;
use app::App;
use logger::create_logger;

fn main() -> Result<()> {
    create_logger()?;
    App::run()
}