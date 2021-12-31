use anyhow::Result;
use dragonglass::{
    app::{Application, ApplicationRunner, MouseOrbit},
    gui::egui::{self, CollapsingHeader, CtxRef, Id, LayerId, Ui},
    render::Viewport,
    world::{
        legion::IntoQuery,
        load_gltf,
        rapier3d::prelude::{InteractionGroups, RigidBodyType},
        Entity, MeshRender, World,
    },
};
use log::{error, info, warn};
use std::path::{Path, PathBuf};
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

pub struct Editor {
    camera: MouseOrbit,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            camera: MouseOrbit::default(),
        }
    }
}

impl Editor {
    fn load_gltf(path: &str, application: &mut Application) -> Result<()> {
        load_gltf(path, &mut application.world)?;

        // FIXME: Don't reload entire scene whenever something is added
        match application.renderer.load_world(&application.world) {
            Ok(_) => {
                info!("Loaded gltf world: '{}'", path);
            }
            Err(error) => {
                warn!("Failed to load gltf world: {}", error);
            }
        }

        Ok(())
    }

    fn load_world_from_file(&self, application: &mut Application, path: &PathBuf) -> Result<()> {
        let raw_path = match path.to_str() {
            Some(raw_path) => raw_path,
            None => return Ok(()),
        };

        if let Some(extension) = path.extension() {
            match extension.to_str() {
                Some("glb") | Some("gltf") => Self::load_gltf(raw_path, application)?,
                Some("hdr") => Self::load_hdr(raw_path, application)?,
                Some("dga") => {
                    application.world = World::load(raw_path)?;
                    application.renderer.load_world(&application.world)?;
                    log::info!("Loaded world!");
                }
                _ => warn!(
                    "File extension {:#?} is not a valid '.dga', '.glb', '.gltf', or '.hdr' extension",
                    extension
                ),
            }
        }

        let mut query = <(Entity, &MeshRender)>::query();
        let entities = query
            .iter(&mut application.world.ecs)
            .map(|(e, _)| *e)
            .collect::<Vec<_>>();
        for entity in entities.into_iter() {
            application
                .world
                .add_rigid_body(entity, RigidBodyType::Static)?;
            application
                .world
                .add_trimesh_collider(entity, InteractionGroups::all())?;
        }

        Ok(())
    }

    fn load_hdr(path: impl AsRef<Path>, application: &mut Application) -> Result<()> {
        // FIXME: We are loading the hdr even if it's already loaded here
        application.world.load_hdr(path)?;
        application.world.scene.skybox = Some(application.world.hdr_textures.len() - 1);

        // FIXME: Don't reload entire scene whenever something is added
        match application.renderer.load_world(&application.world) {
            Ok(_) => {
                info!("Reloaded gltf world");
            }
            Err(error) => {
                warn!("Failed to load gltf world: {}", error);
            }
        }

        Ok(())
    }
}

impl ApplicationRunner for Editor {
    fn initialize(&mut self, application: &mut Application) -> Result<()> {
        application.world.add_default_light()?;
        Ok(())
    }

    fn update_gui(&mut self, ctx: CtxRef, application: &mut Application) -> Result<()> {
        let ctx = &ctx;

        egui::TopBottomPanel::top("top_panel")
            .resizable(true)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    egui::menu::menu(ui, "File", |ui| {
                        if ui.button("Open").clicked() {
                            let result = nfd::open_file_dialog(None, None).unwrap_or_else(|e| {
                                log::error!("Failed to open file!");
                                nfd::Response::Cancel
                            });

                            match result {
                                nfd::Response::Okay(file_path) => {
                                    self.load_world_from_file(
                                        application,
                                        &PathBuf::from(file_path),
                                    )
                                    .expect("Failed to load file!");
                                }
                                nfd::Response::OkayMultiple(files) => {
                                    info!("Files {:?}", files)
                                }
                                nfd::Response::Cancel => println!("User canceled"),
                            }
                        }
                        if ui.button("Quit").clicked() {
                            application.system.exit_requested = true;
                        }
                    });
                });
            });

        egui::SidePanel::left("left_panel")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Left Panel");

                // TODO: We need a recursive function to fill this out
                let mut offset = 0;
                for graph in application.world.scene.graphs.iter() {
                    let collapsing = CollapsingHeader::new(format!("Scene Graph {}", offset))
                        .selectable(true)
                        .selected(false);
                    let header_response = collapsing
                        .show(ui, |ui| {
                            ui.selectable_label(false, format!("Item {}", offset))
                                .clicked()
                        })
                        .header_response;
                    if header_response.clicked() {
                        log::info!("Clicked '{}'!", format!("Item {}", offset))
                    }

                    offset += 1;
                }

                ui.allocate_space(ui.available_size());
            });

        egui::SidePanel::right("right_panel")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Right Panel");
                ui.allocate_space(ui.available_size());
            });

        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Bottom Panel");
                ui.allocate_space(ui.available_size());
            });

        // Calculate the rect needed for rendering
        let viewport = Ui::new(
            ctx.clone(),
            LayerId::background(),
            Id::new("central_panel"),
            ctx.available_rect(),
            ctx.input().screen_rect(),
        )
        .max_rect();

        application.renderer.set_viewport(Viewport {
            x: viewport.min.x,
            y: viewport.min.y,
            width: viewport.max.x - viewport.min.x,
            height: viewport.max.y - viewport.min.y,
            min_depth: 0.0,
            max_depth: 1.0,
        });

        Ok(())
    }

    fn update_before_app(&mut self, application: &mut Application) -> Result<()> {
        if application.input.is_key_pressed(VirtualKeyCode::Escape) {
            application.system.exit_requested = true;
        }

        if !application.world.animations.is_empty() {
            application
                .world
                .animate(0, 0.75 * application.system.delta_time as f32)?;
        }

        if !application.input.allowed {
            return Ok(());
        }

        if application.world.active_camera_is_main()? {
            let camera_entity = application.world.active_camera()?;
            self.camera.update(application, camera_entity)?;
        }

        Ok(())
    }

    fn on_key(
        &mut self,
        application: &mut Application,
        keystate: ElementState,
        keycode: VirtualKeyCode,
    ) -> Result<()> {
        match (keycode, keystate) {
            (VirtualKeyCode::S, ElementState::Pressed) => {
                application.world.save("saved_map.dga")?;
                log::info!("Saved world!");
            }
            (VirtualKeyCode::C, ElementState::Pressed) => {
                application.renderer.clear();
                application.world.clear()?;
                if let Err(error) = application.renderer.load_world(&application.world) {
                    warn!("Failed to load gltf world: {}", error);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_file_dropped(&mut self, application: &mut Application, path: &PathBuf) -> Result<()> {
        self.load_world_from_file(application, path)
    }

    fn on_mouse(
        &mut self,
        application: &mut Application,
        button: MouseButton,
        state: ElementState,
    ) -> Result<()> {
        if !application.input.allowed {
            return Ok(());
        }

        if (MouseButton::Left, ElementState::Pressed) == (button, state) {
            let interact_distance = f32::MAX;
            if let Some(entity) =
                application.pick_object(interact_distance, InteractionGroups::all())?
            {
                log::info!("Picked entity: {:?}", entity);
            }
        }

        Ok(())
    }
}
