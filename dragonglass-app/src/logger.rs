use anyhow::{Context, Result};
use simplelog::{CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;

pub const LOG_FILE: &str = "dragonglass.log";

pub fn create_logger() -> Result<()> {
    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed),
        WriteLogger::new(
            LevelFilter::max(),
            Config::default(),
            File::create(LOG_FILE).context(format!(
                "Failed to create log file named: {}",
                LOG_FILE.to_string()
            ))?,
        ),
    ])?;
    Ok(())
}
