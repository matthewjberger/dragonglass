use log::error;
use shader_compilation::compile_shaders;
use simplelog::*;
use std::{boxed::Box, error::Error, fs::File};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

fn main() -> Result<()> {
    init_logger()?;

    let shader_directory = "shaders";
    let shader_glob = shader_directory.to_owned() + "/*.glsl";
    if compile_shaders(&shader_glob).is_err() {
        error!("Failed to recompile shaders!");
    }

    Ok(())
}

fn init_logger() -> Result<()> {
    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::max(), Config::default(), TerminalMode::Mixed),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create("shadercompilation.log")?,
        ),
    ])?;

    Ok(())
}
