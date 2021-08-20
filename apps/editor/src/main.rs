mod editor;

use anyhow::Result;
use dragonglass::app::{run_application, AppConfig};
use editor::Editor;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dragonglass_editor",
    about = "The visual editor for the Dragonglass game engine."
)]
struct Options {}

fn main() -> Result<()> {
    let _options = Options::from_args();
    run_application(
        Editor::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            ..Default::default()
        },
    )
}
