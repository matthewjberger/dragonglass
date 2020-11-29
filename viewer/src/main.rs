#![warn(
    clippy::all,
    clippy::cognitive_complexity,
    clippy::dbg_macro,
    clippy::expect_used,
    clippy::if_not_else,
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
mod camera;
mod input;
mod logger;
mod settings;
mod state;
mod system;

use anyhow::Result;
use app::App;
use logger::create_logger;
use state::*;

#[derive(Default)]
struct Viewer;

impl State<(), ()> for Viewer {
    fn initialize(&mut self, data: StateData<'_, (), ()>) -> Result<Transition<(), ()>> {
        log::info!("Initializing state...");
        Ok(Transition::None)
    }

    fn finalize(&mut self, data: StateData<'_, (), ()>) -> Result<Transition<(), ()>> {
        log::info!("Finalizing state...");
        Ok(Transition::None)
    }

    fn pause(&mut self, data: StateData<'_, (), ()>) -> Result<Transition<(), ()>> {
        log::info!("Pausing state...");
        Ok(Transition::None)
    }

    fn resume(&mut self, data: StateData<'_, (), ()>) -> Result<Transition<(), ()>> {
        log::info!("Resuming state...");
        Ok(Transition::None)
    }

    fn update(&mut self, data: StateData<'_, (), ()>) -> Result<Transition<(), ()>> {
        log::info!("Updating state...");
        Ok(Transition::None)
    }
}

fn main() -> Result<()> {
    create_logger()?;
    let app = App::new()?;
    app.run(Box::new(Viewer::default()))?;
    Ok(())
}
