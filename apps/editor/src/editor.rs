use anyhow::{Context, Result};
use dragonglass::{
    app::{App, MouseOrbit, Resources},
    gui::{
        egui::{
            self, global_dark_light_mode_switch, menu, Align, DragValue, Id, LayerId,
            SelectableLabel, Slider, Ui,
        },
        egui_gizmo::GizmoMode,
        GizmoWidget,
    },
    world::{
        legion::Entity,
        load_gltf,
        petgraph::{graph::NodeIndex, EdgeDirection::Outgoing},
        rapier3d::{geometry::InteractionGroups, prelude::RigidBodyType},
        register_component, Ecs, EntityStore, IntoQuery, MeshRender, Name, RigidBody, SceneGraph,
        Transform,
    },
};
use log::{info, warn};
use nalgebra::UnitQuaternion;
use nalgebra_glm as glm;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winit::event::{ElementState, MouseButton, VirtualKeyCode};

const EDITOR_COLLISION_GROUP: InteractionGroups = InteractionGroups::new(0b1, 0b1);

#[derive(Default, Serialize, Deserialize)]
pub struct Selected;

pub struct Editor {
    camera: MouseOrbit,
    selected_entity: Option<Entity>,
    gizmo: GizmoWidget,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            camera: MouseOrbit::default(),
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

        // FIXME: Don't reload entire scene whenever something is added
        match resources.renderer.load_world(&resources.world) {
            Ok(_) => {
                info!("Reloaded gltf world");
            }
            Err(error) => {
                warn!("Failed to load gltf world: {}", error);
            }
        }

        Ok(())
    }

    pub fn select_entity(&mut self, entity: Entity, resources: &mut Resources) -> Result<()> {
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
        Ok(())
    }

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
            resources.renderer.load_world(resources.world)?;

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
                    .add_trimesh_collider(entity, EDITOR_COLLISION_GROUP)?;
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

    fn top_panel(&mut self, resources: &mut Resources) -> Result<()> {
        let context = &resources.gui.context();

        egui::TopBottomPanel::top("top_panel")
            .resizable(true)
            .show(context, |ui| {
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
        Ok(())
    }

    fn bottom_panel(&mut self, resources: &mut Resources) -> Result<()> {
        let context = &resources.gui.context();

        egui::TopBottomPanel::bottom("console")
            .resizable(true)
            .show(context, |ui| {
                ui.heading("Console");
                ui.allocate_space(ui.available_size());
            });

        Ok(())
    }

    fn left_panel(&mut self, resources: &mut Resources) -> Result<()> {
        let context = &resources.gui.context();

        egui::SidePanel::left("scene_explorer")
            .resizable(true)
            .show(context, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Tools");
                    self.gizmo.render_mode_selection(ui);

                    ui.heading("Post Processing");

                    ui.add(
                        Slider::new(
                            &mut resources
                                .config
                                .graphics
                                .post_processing
                                .chromatic_aberration
                                .strength,
                            0.0..=10.0,
                        )
                        .text("Chromatic Aberration Strength"),
                    );

                    ui.add(
                        Slider::new(
                            &mut resources
                                .config
                                .graphics
                                .post_processing
                                .film_grain
                                .strength,
                            0.0..=10.0,
                        )
                        .text("Film Grain Strength"),
                    );

                    ui.end_row();

                    ui.heading("Scenegraph");
                    ui.label(&resources.world.scene.name);
                    let scene = &mut resources.world.scene;
                    let ecs = &mut resources.world.ecs;
                    for graph in scene.graphs.iter_mut() {
                        self.print_node(ecs, graph, NodeIndex::new(0), ui);
                    }
                    ui.end_row();

                    ui.allocate_space(ui.available_size());
                });
            });
        Ok(())
    }

    fn right_panel(&mut self, resources: &mut Resources) -> Result<()> {
        let context = &resources.gui.context();

        egui::SidePanel::right("inspector")
            .resizable(true)
            .show(context, |ui| -> Result<()> {
                ui.heading("Inspector");
                let entity = match self.selected_entity {
                    Some(entity) => entity,
                    None => return Ok(()),
                };

                self.translation_widget(resources, entity, ui)?;
                self.rotation_widget(resources, entity, ui)?;
                self.scale_widget(resources, entity, ui)?;
                ui.allocate_space(ui.available_size());

                Ok(())
            });
        Ok(())
    }

    fn translation_widget(
        &mut self,
        resources: &mut Resources,
        entity: Entity,
        ui: &mut Ui,
    ) -> Result<()> {
        let ecs = &mut resources.world.ecs;
        let mut entry = ecs.entry(entity).context("Failed to find entity!")?;
        let mut should_sync = false;

        ui.heading("Translation");
        ui.horizontal(|ui| {
            let transform = entry
                .get_component_mut::<Transform>()
                .expect("Entity does not have a transform!");

            ui.label("X");
            let x_response = ui.add(DragValue::new(&mut transform.translation.x).speed(0.1));

            ui.label("Y");
            let y_response = ui.add(DragValue::new(&mut transform.translation.y).speed(0.1));

            ui.label("Z");
            let z_response = ui.add(DragValue::new(&mut transform.translation.z).speed(0.1));

            should_sync = x_response.changed() || y_response.changed() || z_response.changed();
        });

        if should_sync && entry.get_component::<RigidBody>().is_ok() {
            resources
                .world
                .sync_rigid_body_to_transform(entity)
                .expect("Failed to sync rigid body to transform!");
        }

        ui.end_row();

        Ok(())
    }

    fn rotation_widget(
        &mut self,
        resources: &mut Resources,
        entity: Entity,
        ui: &mut Ui,
    ) -> Result<()> {
        let ecs = &mut resources.world.ecs;
        let mut entry = ecs.entry(entity).context("Failed to find entity!")?;
        let mut should_sync = false;

        ui.label("Rotation");
        ui.horizontal(|ui| {
            let transform = entry
                .get_component_mut::<Transform>()
                .expect("Entity does not have a transform!");

            let mut angles = glm::quat_euler_angles(&transform.rotation);
            angles = glm::vec3(
                angles.x.to_degrees(),
                angles.y.to_degrees(),
                angles.z.to_degrees(),
            );

            ui.label("X");
            let x_response = ui.add(DragValue::new(&mut angles.x).speed(0.1));

            ui.label("Y");
            let y_response = ui.add(DragValue::new(&mut angles.y).speed(0.1));

            ui.label("Z");
            let z_response = ui.add(DragValue::new(&mut angles.z).speed(0.1));

            should_sync = x_response.changed() || y_response.changed() || z_response.changed();

            if should_sync {
                let quat_x = glm::quat_angle_axis(angles.x.to_radians(), &glm::Vec3::x());
                let quat_y = glm::quat_angle_axis(angles.y.to_radians(), &glm::Vec3::y());
                let quat_z = glm::quat_angle_axis(angles.z.to_radians(), &glm::Vec3::z());
                transform.rotation = quat_z * quat_y * quat_x;
            }
        });

        if should_sync && entry.get_component::<RigidBody>().is_ok() {
            resources
                .world
                .sync_rigid_body_to_transform(entity)
                .expect("Failed to sync rigid body to transform!");
        }

        ui.end_row();

        Ok(())
    }

    fn scale_widget(
        &mut self,
        resources: &mut Resources,
        entity: Entity,
        ui: &mut Ui,
    ) -> Result<()> {
        let ecs = &mut resources.world.ecs;
        let mut entry = ecs.entry(entity).context("Failed to find entity!")?;
        let mut should_sync = false;

        ui.label("Scale");
        ui.horizontal(|ui| {
            let transform = entry
                .get_component_mut::<Transform>()
                .expect("Entity does not have a transform!");

            ui.label("X");
            let x_response = ui.add(DragValue::new(&mut transform.scale.x).speed(0.1));

            ui.label("Y");
            let y_response = ui.add(DragValue::new(&mut transform.scale.y).speed(0.1));

            ui.label("Z");
            let z_response = ui.add(DragValue::new(&mut transform.scale.z).speed(0.1));

            should_sync = x_response.changed() || y_response.changed() || z_response.changed();
        });

        if should_sync && entry.get_component::<RigidBody>().is_ok() {
            resources
                .world
                .sync_rigid_body_to_transform(entity)
                .expect("Failed to sync rigid body to transform!");
        }

        ui.end_row();

        Ok(())
    }

    fn viewport_panel(&mut self, resources: &mut Resources) -> Result<()> {
        let context = &resources.gui.context();

        egui::Area::new("Viewport")
            .fixed_pos((0.0, 0.0))
            .show(context, |ui| {
                ui.with_layer_id(LayerId::background(), |ui| {
                    if let Some(entity) = self.selected_entity {
                        let (projection, view) = resources
                            .world
                            .active_camera_matrices(resources.system.aspect_ratio())
                            .expect("Failed to get camera matrices!");
                        let transform = resources
                            .world
                            .entity_global_transform(entity)
                            .expect("Failed to get entity transform!");
                        if let Some(gizmo_result) =
                            self.gizmo.render(ui, transform.matrix(), view, projection)
                        {
                            let model_matrix: glm::Mat4 = gizmo_result.transform.into();
                            let gizmo_transform = Transform::from(model_matrix);
                            let mut entry = resources.world.ecs.entry_mut(entity).unwrap();
                            let mut transform = entry.get_component_mut::<Transform>().unwrap();
                            transform.translation = gizmo_transform.translation;
                            transform.rotation = gizmo_transform.rotation;
                            transform.scale = gizmo_transform.scale;
                            if entry.get_component::<RigidBody>().is_ok() {
                                resources
                                    .world
                                    .sync_rigid_body_to_transform(entity)
                                    .expect("Failed to sync rigid body to transform!");
                            }
                        }
                    }
                });
            });

        Ok(())
    }
}

impl App for Editor {
    fn initialize(&mut self, resources: &mut dragonglass::app::Resources) -> Result<()> {
        register_component::<Selected>("selected")?;
        resources.world.add_default_light()?;
        Ok(())
    }

    fn update(&mut self, resources: &mut dragonglass::app::Resources) -> Result<()> {
        if resources.world.active_camera_is_main()? {
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

        Ok(())
    }

    fn gui_active(&mut self) -> bool {
        true
    }

    fn update_gui(&mut self, resources: &mut Resources) -> Result<()> {
        self.top_panel(resources)?;
        self.left_panel(resources)?;
        self.right_panel(resources)?;
        self.bottom_panel(resources)?;
        self.viewport_panel(resources)?;
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
                EDITOR_COLLISION_GROUP,
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
            (Some(VirtualKeyCode::Escape), ElementState::Pressed) => {
                self.deselect_all(resources)?;
            }
            (Some(VirtualKeyCode::T), ElementState::Pressed) => {
                self.gizmo.mode = GizmoMode::Translate;
            }
            (Some(VirtualKeyCode::R), ElementState::Pressed) => {
                self.gizmo.mode = GizmoMode::Rotate;
            }
            (Some(VirtualKeyCode::S), ElementState::Pressed) => {
                self.gizmo.mode = GizmoMode::Scale;
            }
            (Some(VirtualKeyCode::C), ElementState::Pressed) => {
                resources.world.clear()?;
                self.selected_entity = None;
                if let Err(error) = resources.renderer.load_world(&resources.world) {
                    warn!("Failed to load gltf world: {}", error);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
