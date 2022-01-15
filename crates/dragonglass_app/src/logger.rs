use dragonglass_deps::{
    anyhow::{Context, Result},
    simplelog::{
        ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode, WriteLogger,
    },
};
use std::fs::File;

pub const LOG_FILE: &str = "dragonglass.log";

pub fn create_logger() -> Result<()> {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
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
