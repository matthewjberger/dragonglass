use anyhow::Result;
use dragonglass::{
    app::{run_app, App, AppConfiguration},
    world::{BoundingBoxVisible, Mesh, World},
};
use imgui::{im_str, Ui};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

#[derive(Default)]
struct CameraMultipliers {
    scroll: f32,
    rotation: f32,
    drag: f32,
}

#[derive(Default)]
pub struct Viewer {
    camera_multipliers: CameraMultipliers,
}

impl App for Viewer {
    fn initialize(&mut self, _window: &mut Window, world: &mut World) {
        let entities = world
            .ecs
            .query::<&Mesh>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        entities.into_iter().for_each(|entity| {
            let _ = world.ecs.insert_one(entity, BoundingBoxVisible {});
        });
    }
    fn create_ui(&mut self, ui: &Ui, world: &mut World) {
        let number_of_entities = world.ecs.iter().count();
        let number_of_meshes = world.ecs.query::<&Mesh>().iter().count();
        ui.text(im_str!("Number of entities: {}", number_of_entities));
        ui.text(im_str!("Number of meshes: {}", number_of_meshes));
        ui.text(im_str!("Number of animations: {}", world.animations.len()));
        ui.text(im_str!("Number of textures: {}", world.textures.len()));
        ui.text(im_str!("Number of materials: {}", world.materials.len()));
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

        for (_entity, mesh) in world.ecs.query::<&Mesh>().iter() {
            ui.text(im_str!("Mesh: {}", mesh.name));
        }
    }
    fn update(&mut self, _world: &mut World) {}
    fn cleanup(&mut self) {}
    fn on_key(&mut self, state: ElementState, keycode: VirtualKeyCode) {
        // match (keycode, state) {
        //     (VirtualKeyCode::T, ElementState::Pressed) => renderer.toggle_wireframe(),
        //     (VirtualKeyCode::C, ElementState::Pressed) => {
        //         world.clear();
        //         if let Err(error) = renderer.load_world(&world) {
        //             warn!("Failed to load gltf world: {}", error);
        //         }
        //     }
        //     _ => {}
        // }
    }
    fn handle_events(&mut self, _event: winit::event::Event<()>) {}
}

fn main() -> Result<()> {
    run_app(Viewer::default(), AppConfiguration::default())
}
