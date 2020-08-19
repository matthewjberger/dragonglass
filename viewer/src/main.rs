mod app;
mod logger;

use anyhow::Result;
use app::App;
use logger::Logger;

fn main() -> Result<()> {
    Logger::setup_logger()?;
    App::run()
}