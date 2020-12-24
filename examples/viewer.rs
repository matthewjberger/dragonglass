use anyhow::Result;
use dragonglass::app::{create_logger, App};

fn main() -> Result<()> {
    create_logger()?;
    let app = App::new()?;
    app.run()?;
    Ok(())
}
