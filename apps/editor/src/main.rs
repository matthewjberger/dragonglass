mod editor;

use dragonglass::{
    app::{run_application, AppConfig},
    deps::anyhow::Result,
    render::Backend,
};
use editor::Editor;

fn main() -> Result<()> {
    run_application(
        Editor::default(),
        AppConfig {
            icon: Some("assets/icon/icon.png".to_string()),
            title: "Dragonglass Editor".to_string(),
            backend: Backend::Vulkan,
            ..Default::default()
        },
    )
}
