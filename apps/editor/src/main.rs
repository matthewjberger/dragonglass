mod editor;
mod widgets;

use anyhow::Result;
use dragonglass::{
    app::{run_application, AppConfig},
    render::Backend,
};
use editor::Editor;

fn main() -> Result<()> {
    run_application(
        Editor::default(),
        AppConfig {
            width: 1920,
            height: 1080,
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            backend: Backend::Vulkan,
            ..Default::default()
        },
    )
}
