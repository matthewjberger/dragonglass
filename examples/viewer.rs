use anyhow::Result;
use dragonglass::{
    app::{run_app, App, AppConfiguration, AppState},
    world::{BoundingBoxVisible, Mesh, World},
};
use imgui::{im_str, Ui};
use log::{error, info, warn};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

#[derive(Default)]
struct CameraMultipliers {
    pub scroll: f32,
    pub rotation: f32,
    pub drag: f32,
}

#[derive(Default)]
pub struct Viewer {
    camera_multipliers: CameraMultipliers,
}

impl App for Viewer {
    fn initialize(&mut self, state: &mut AppState) {
        let entities = state
            .world
            .ecs
            .query::<&Mesh>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        entities.into_iter().for_each(|entity| {
            let _ = state.world.ecs.insert_one(entity, BoundingBoxVisible {});
        });
    }

    fn create_ui(&mut self, state: &mut AppState, ui: &Ui) {
        let number_of_entities = state.world.ecs.iter().count();
        let number_of_meshes = state.world.ecs.query::<&Mesh>().iter().count();
        ui.text(im_str!("Number of entities: {}", number_of_entities));
        ui.text(im_str!("Number of meshes: {}", number_of_meshes));
        ui.text(im_str!(
            "Number of animations: {}",
            state.world.animations.len()
        ));
        ui.text(im_str!(
            "Number of textures: {}",
            state.world.textures.len()
        ));
        ui.text(im_str!(
            "Number of materials: {}",
            state.world.materials.len()
        ));
        ui.separator();
        ui.text(im_str!("Controls"));

        // FIXME: Make renderer settings belong to world
        // if ui.button(im_str!("Toggle Wireframe"), [200.0, 20.0]) {
        //     renderer.toggle_wireframe();
        // }

        // ui.text(im_str!("Multipliers"));
        // let _ = ui
        //     .input_float(im_str!("Scroll"), &mut camera_multipliers.scroll)
        //     .step(0.1)
        //     .step_fast(1.0)
        //     .build();
        // let _ = ui
        //     .input_float(im_str!("Drag"), &mut camera_multipliers.drag)
        //     .step(0.1)
        //     .step_fast(1.0)
        //     .build();
        // let _ = ui
        //     .input_float(im_str!("Rotation"), &mut camera_multipliers.rotation)
        //     .step(0.1)
        //     .step_fast(1.0)
        //     .build();
        // ui.separator();

        for (_entity, mesh) in state.world.ecs.query::<&Mesh>().iter() {
            ui.text(im_str!("Mesh: {}", mesh.name));
        }
    }

    fn update(&mut self, _state: &mut AppState) {}

    fn cleanup(&mut self) {}

    fn on_key(&mut self, _state: &mut AppState, _keystate: ElementState, _keycode: VirtualKeyCode) {
    }

    fn handle_events(&mut self, _state: &mut AppState, _event: winit::event::Event<()>) {}
}

fn main() -> Result<()> {
    run_app(Viewer::default(), AppConfiguration::default())
}
