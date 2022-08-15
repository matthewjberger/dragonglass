use anyhow::{Context, Result};
use dragonglass::{
    app::Resources,
    gui::egui::{DragValue, Ui},
    world::{Entity, RigidBody, Transform},
};
use nalgebra_glm as glm;

pub fn translation_widget(resources: &mut Resources, entity: Entity, ui: &mut Ui) -> Result<()> {
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

pub fn rotation_widget(resources: &mut Resources, entity: Entity, ui: &mut Ui) -> Result<()> {
    let ecs = &mut resources.world.ecs;
    let mut entry = ecs.entry(entity).context("Failed to find entity!")?;

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

        ui.label(format!("X {} ", &mut angles.x));
        ui.label(format!("Y {} ", &mut angles.y));
        ui.label(format!("Z {} ", &mut angles.z));
    });

    ui.end_row();

    Ok(())
}

pub fn scale_widget(resources: &mut Resources, entity: Entity, ui: &mut Ui) -> Result<()> {
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
