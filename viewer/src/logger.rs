use simplelog::*;
use anyhow::{Context, Result};
use std::fs::File;

pub struct Logger;

impl Logger {
    pub const LOG_FILE: &'static str = "dragonglass.log";

    pub fn setup_logger() -> Result<()> {
        CombinedLogger::init(vec![
            TermLogger::new(LevelFilter::max(), Config::default(), TerminalMode::Mixed),
            WriteLogger::new(
                LevelFilter::max(),
                Config::default(),
                File::create(Self::LOG_FILE)
                    .with_context(|| format!("log file path: {}", Self::LOG_FILE.to_string()))?,
            ),
        ])?;
        Ok(())
    }
}