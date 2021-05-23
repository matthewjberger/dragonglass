mod editor;

use anyhow::Result;
use dragonglass::{
    app::{run_application, AppConfig},
    render::Backend,
};
use editor::Editor;

use clap::arg_enum;
use structopt::StructOpt;

arg_enum! {
    #[derive(Debug)]
    enum RenderBackend {
        Vulkan,
        OpenGL,
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dragonglass_editor",
    about = "The visual editor for the Dragonglass game engine."
)]
struct Options {
    #[structopt(short = "b", long = "backend", possible_values = &RenderBackend::variants(), case_insensitive = true, default_value = "OpenGL")]
    backend: RenderBackend,
}

fn main() -> Result<()> {
    let options = Options::from_args();
    let backend = match options.backend {
        RenderBackend::Vulkan => Backend::Vulkan,
        RenderBackend::OpenGL => Backend::OpenGl,
    };
    run_application(
        Editor::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            backend,
            ..Default::default()
        },
    )
}
