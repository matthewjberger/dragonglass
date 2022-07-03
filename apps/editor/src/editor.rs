use anyhow::{Context, Result};
use dragonglass::{
    app::{App, MouseOrbit, Resources},
    gui::{
        egui::{
            self, global_dark_light_mode_switch, menu, Align, DragValue, Id, LayerId,
            SelectableLabel, Ui,
        },
        GizmoWidget,
    },
    world::{
        legion::Entity,
        load_gltf,
        petgraph::{graph::NodeIndex, EdgeDirection::Outgoing},
        rapier3d::{geometry::InteractionGroups, prelude::RigidBodyType},
        register_component, Ecs, EntityStore, IntoQuery, MeshRender, Name, RigidBody, SceneGraph,
        Transform, Viewport,
    },
};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

#[derive(Default, Serialize, Deserialize)]
pub struct Selected;

pub struct Editor {
    camera: MouseOrbit,
    moving_selected: bool,
    selected_entity: Option<Entity>,
    gizmo: GizmoWidget,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            camera: MouseOrbit::default(),
            moving_selected: false,
            selected_entity: None,
            gizmo: GizmoWidget::new(),
        }
    }
}

impl Editor {
    fn load_hdr(path: impl AsRef<Path>, resources: &mut Resources) -> Result<()> {
        // FIXME: We are loading the hdr even if it's already loaded here
        resources.world.load_hdr(path)?;
        resources.world.scene.skybox = Some(resources.world.hdr_textures.len() - 1);

        // // FIXME: Don't reload entire scene whenever something is added
        // match resources.renderer.load_world(&resources.world) {
        //     Ok(_) => {
        //         info!("Reloaded gltf world");
        //     }
        //     Err(error) => {
        //         warn!("Failed to load gltf world: {}", error);
        //     }
        // }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn deselect_all(&mut self, resources: &mut Resources) -> Result<()> {
        let mut query = <(Entity, &Selected)>::query();

        let entities = query
            .iter(&mut resources.world.ecs)
            .map(|(e, _)| *e)
            .collect::<Vec<_>>();

        for entity in entities.into_iter() {
            let mut entry = resources
                .world
                .ecs
                .entry(entity)
                .context("Failed to find entity!")?;
            log::info!("Deselecting entity: {:?}", entity);
            entry.remove_component::<Selected>();
        }

        Ok(())
    }

    pub fn load_world_from_file(&self, path: &PathBuf, resources: &mut Resources) -> Result<()> {
        let raw_path = match path.to_str() {
            Some(raw_path) => raw_path,
            None => return Ok(()),
        };

        if let Some(extension) = path.extension() {
            match extension.to_str() {
                Some("glb") | Some("gltf") => {
                    load_gltf(raw_path, resources.world)?;
                }
                Some("hdr") => Self::load_hdr(raw_path, resources)?,
                Some("dga") => {
                    resources.world.reload(raw_path)?;
                    log::info!("Loaded world!");
                }
                _ => log::warn!(
                    "File extension {:#?} is not a valid '.dga', '.glb', '.gltf', or '.hdr' extension",
                    extension
                ),
            }

            // TODO: Probably don't want this added every time
            // resources.renderer.load_world(resources.world)?;

            // TODO: Don't add an additional collider to existing entities...
            let mut query = <(Entity, &MeshRender)>::query();
            let entities = query
                .iter(&mut resources.world.ecs)
                .map(|(e, _)| *e)
                .collect::<Vec<_>>();

            for entity in entities.into_iter() {
                resources
                    .world
                    .add_rigid_body(entity, RigidBodyType::Static)?;
                resources
                    .world
                    .add_trimesh_collider(entity, InteractionGroups::all())?;
            }
        }

        Ok(())
    }

    fn print_node(&mut self, ecs: &mut Ecs, graph: &SceneGraph, index: NodeIndex, ui: &mut Ui) {
        let entity = graph[index];
        let entry = ecs.entry_ref(entity).expect("Failed to find entity!");
        let debug_name = format!("{:?}", entity);
        let label = entry
            .get_component::<Name>()
            .ok()
            .unwrap_or(&Name(debug_name))
            .0
            .to_string();

        let selected = self.selected_entity == Some(entity);

        let context_menu = |ui: &mut Ui| {
            if ui.button("Rename...").clicked() {
                // UI TODO: Allow renaming entities
                ui.close_menu();
            }

            if ui.button("Delete...").clicked() {
                // UI TODO: Allow deleting entities
                ui.close_menu();
            }

            if ui.button("Add Child...").clicked() {
                // UI TODO: Allow adding child entities
                ui.close_menu();
            }
        };

        let response = if graph.has_children(index) {
            egui::CollapsingHeader::new(label.to_string())
                .selectable(true)
                .selected(selected)
                .show(ui, |ui| {
                    let mut neighbors = graph.neighbors(index, Outgoing);
                    while let Some(child) = neighbors.next_node(&graph.0) {
                        self.print_node(ecs, graph, child, ui);
                    }
                })
                .header_response
                .context_menu(context_menu)
        } else {
            ui.add(SelectableLabel::new(selected, label.to_string()))
                .context_menu(context_menu)
        };

        if response.clicked() {
            self.selected_entity = Some(entity);
        }

        if response.double_clicked() {
            // TODO: Allow renaming entity
        }
    }
}

impl App for Editor {
    fn initialize(&mut self, resources: &mut dragonglass::app::Resources) -> Result<()> {
        register_component::<Selected>("selected")?;
        resources.world.add_default_light()?;
        Ok(())
    }

    fn update(&mut self, resources: &mut dragonglass::app::Resources) -> Result<()> {
        if resources.input.is_key_pressed(VirtualKeyCode::Escape) {
            resources.system.exit_requested = true;
        }

        if resources.world.active_camera_is_main()? && !self.moving_selected {
            let camera_entity = resources.world.active_camera()?;
            self.camera.update(resources, camera_entity)?;
        }

        // // Run first animation
        // if let Some(animation) = resources.world.animations.first_mut() {
        //     animation.animate(
        //         &mut resources.world.ecs,
        //         0.75 * resources.system.delta_time as f32,
        //     )?;
        // }

        if self.moving_selected {
            let mut query = <(Entity, &Selected)>::query();
            let entities = query
                .iter_mut(&mut resources.world.ecs)
                .map(|(e, _)| (*e))
                .collect::<Vec<_>>();
            for entity in entities.into_iter() {
                let (right, up) = {
                    let camera_entity = resources.world.active_camera()?;
                    let camera_entry = resources.world.ecs.entry_ref(camera_entity)?;
                    let camera_transform = camera_entry.get_component::<Transform>()?;
                    (camera_transform.right(), camera_transform.up())
                };

                let mut entry = resources.world.ecs.entry_mut(entity)?;
                let speed = 10.0;
                let transform = entry.get_component_mut::<Transform>()?;
                let mouse_delta =
                    resources.input.mouse.position_delta * resources.system.delta_time as f32;
                if resources.input.mouse.is_right_clicked {
                    transform.translation += right * mouse_delta.x * speed;
                    transform.translation += up * -mouse_delta.y * speed;
                }
                resources.world.sync_rigid_body_to_transform(entity)?;
            }
        }

        Ok(())
    }

    fn gui_active(&mut self) -> bool {
        true
    }

    fn update_gui(&mut self, resources: &mut Resources) -> Result<()> {
        let ctx = &resources.gui.context();

        egui::TopBottomPanel::top("top_panel")
            .resizable(true)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    menu::bar(ui, |ui| {
                        global_dark_light_mode_switch(ui);
                        ui.menu_button("File", |ui| {
                            // TODO: Distinguish between loading levels and importing assets
                            if ui.button("Load Level").clicked() {
                                let path = FileDialog::new()
                                    .add_filter("Dragonglass Asset", &["dga"])
                                    .set_directory("/")
                                    .pick_file();
                                if let Some(path) = path {
                                    self.load_world_from_file(&path, resources)
                                        .expect("Failed to load asset!");
                                }
                                ui.close_menu();
                            }

                            if ui.button("Import gltf/glb").clicked() {
                                let path = FileDialog::new()
                                    .add_filter("GLTF Asset", &["glb", "gltf"])
                                    .set_directory("/")
                                    .pick_file();
                                if let Some(path) = path {
                                    self.load_world_from_file(&path, resources)
                                        .expect("Failed to load asset!");
                                }
                                ui.close_menu();
                            }

                            if ui.button("Save").clicked() {
                                let path = FileDialog::new()
                                    .add_filter("Dragonglass Asset", &["dga"])
                                    .set_directory("/")
                                    .save_file();

                                if let Some(path) = path {
                                    let mut query = <(Entity, &Selected)>::query();

                                    let entities = query
                                        .iter(&mut resources.world.ecs)
                                        .map(|(e, _)| *e)
                                        .collect::<Vec<_>>();

                                    for entity in entities.into_iter() {
                                        resources
                                            .world
                                            .remove_rigid_body(entity)
                                            .expect("Failed to remove rigid body!");
                                    }

                                    resources.world.save(&path).expect("Failed to save world!");
                                }
                                ui.close_menu();
                            }

                            if ui.button("Quit").clicked() {
                                resources.system.exit_requested = true;
                            }
                        });
                    });
                });
            });

        egui::SidePanel::left("scene_explorer")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading(&resources.world.scene.name);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let scene = &mut resources.world.scene;
                    let ecs = &mut resources.world.ecs;
                    for graph in scene.graphs.iter_mut() {
                        self.print_node(ecs, graph, NodeIndex::new(0), ui);
                    }
                    ui.end_row();

                    self.gizmo.render_controls(ui);

                    ui.allocate_space(ui.available_size());
                });
            });

        egui::SidePanel::right("inspector")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                if let Some(entity) = self.selected_entity {
                    ui.heading("Transform");

                    let mut should_sync = false;

                    let mut entry = resources
                        .world
                        .ecs
                        .entry(entity)
                        .expect("Failed to find entity!");

                    ui.with_layout(egui::Layout::top_down(Align::LEFT), |ui| {
                        let transform = entry
                            .get_component_mut::<Transform>()
                            .expect("Entity does not have a transform!");

                        ui.label("X");
                        let x_response =
                            ui.add(DragValue::new(&mut transform.translation.x).speed(0.1));

                        ui.label("Y");
                        let y_response =
                            ui.add(DragValue::new(&mut transform.translation.y).speed(0.1));

                        ui.label("Z");
                        let z_response =
                            ui.add(DragValue::new(&mut transform.translation.z).speed(0.1));

                        should_sync =
                            x_response.changed() || y_response.changed() || z_response.changed();
                    });

                    if should_sync && entry.get_component::<RigidBody>().is_ok() {
                        resources
                            .world
                            .sync_rigid_body_to_transform(entity)
                            .expect("Failed to sync rigid body to transform!");
                    }
                }

                ui.allocate_space(ui.available_size());
            });

        egui::TopBottomPanel::bottom("console")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Console");
                ui.allocate_space(ui.available_size());
            });

        // This is the space leftover on screen after the UI is drawn
        // We can restrict rendering to this viewport to
        // prevent drawing the gui over the scene
        let central_rect = Ui::new(
            ctx.clone(),
            LayerId::background(),
            Id::new("central_panel"),
            ctx.available_rect(),
            ctx.input().screen_rect(),
        )
        .max_rect();

        egui::Area::new("Viewport")
            .fixed_pos((0.0, 0.0))
            .show(ctx, |ui| {
                if let Some(entity) = self.selected_entity {
                    let (projection, view) = resources
                        .world
                        .active_camera_matrices(resources.system.viewport.aspect_ratio())
                        .expect("Failed to get camera matrices!");
                    let transform = resources
                        .world
                        .entity_global_transform(entity)
                        .expect("Failed to get entity transform!");
                    let _result = self.gizmo.render(ui, transform.matrix(), view, projection);
                }
            });

        // TODO: Don't render underneath the gui
        let _viewport = Viewport {
            x: central_rect.min.x,
            y: central_rect.min.y,
            width: central_rect.width(),
            height: central_rect.height(),
        };
        // resources.renderer.set_viewport(viewport);

        Ok(())
    }

    fn on_mouse(
        &mut self,
        button: &winit::event::MouseButton,
        button_state: &ElementState,
        resources: &mut Resources,
    ) -> Result<()> {
        if (MouseButton::Left, ElementState::Pressed) == (*button, *button_state) {
            let interact_distance = f32::MAX;
            if let Some(entity) = resources.world.pick_object(
                &resources.mouse_ray_configuration()?,
                interact_distance,
                InteractionGroups::all(),
            )? {
                let mut query = <(Entity, &Selected)>::query();
                let already_selected = query
                    .iter(&mut resources.world.ecs)
                    .map(|(e, _)| *e)
                    .any(|e| e == entity);
                if already_selected {
                    return Ok(());
                }

                self.deselect_all(resources)?;
                let mut entry = resources
                    .world
                    .ecs
                    .entry(entity)
                    .context("Failed to find entity")?;
                entry.add_component(Selected::default());
                self.selected_entity = Some(entity);
                log::info!("Selected entity: {:?}", entity);
            }
        }
        Ok(())
    }

    fn on_file_dropped(
        &mut self,
        path: &PathBuf,
        resources: &mut dragonglass::app::Resources,
    ) -> Result<()> {
        self.load_world_from_file(path, resources)?;
        Ok(())
    }

    fn on_key(
        &mut self,
        input: winit::event::KeyboardInput,
        resources: &mut dragonglass::app::Resources,
    ) -> Result<()> {
        match (input.virtual_keycode, input.state) {
            (Some(VirtualKeyCode::G), ElementState::Pressed) => {
                self.moving_selected = !self.moving_selected;
            }
            (Some(VirtualKeyCode::S), ElementState::Pressed) => {
                resources.world.save("saved_map.dga")?;
                log::info!("Saved world!");
            }
            (Some(VirtualKeyCode::C), ElementState::Pressed) => {
                resources.world.clear()?;
                self.selected_entity = None;
                // if let Err(error) = resources.renderer.load_world(&resources.world) {
                //     warn!("Failed to load gltf world: {}", error);
                // }
            }
            _ => {}
        }
        Ok(())
    }
}
