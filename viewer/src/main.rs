#![warn(
    clippy::all,
    clippy::as_conversions,
    clippy::cognitive_complexity,
    clippy::dbg_macro,
    clippy::default_trait_access,
    clippy::expect_used,
    clippy::if_not_else,
    clippy::indexing_slicing,
    clippy::inefficient_to_string,
    clippy::needless_borrow,
    clippy::todo,
    clippy::too_many_lines,
    clippy::unreachable,
    clippy::unused_self,
    clippy::use_self,
    clippy::wildcard_dependencies,
    clippy::wildcard_imports
)]

mod app;
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
