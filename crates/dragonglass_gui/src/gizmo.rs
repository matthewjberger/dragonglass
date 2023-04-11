use egui::{color_picker::Alpha, pos2, Align2, Color32, Slider, TextStyle, Ui, Widget};
use egui_gizmo::{Gizmo, GizmoMode, GizmoOrientation, GizmoResult, GizmoVisuals};
use nalgebra_glm as glm;

pub struct GizmoWidget {
    pub mode: GizmoMode,
    orientation: GizmoOrientation,
    last_gizmo_response: Option<GizmoResult>,
    snap_angle: f32,
    snap_distance: f32,
    visuals: GizmoVisuals,
}

impl Default for GizmoWidget {
    fn default() -> Self {
        Self {
            mode: GizmoMode::Rotate,
            orientation: GizmoOrientation::Global,
            last_gizmo_response: None,
            snap_angle: egui_gizmo::DEFAULT_SNAP_ANGLE,
            snap_distance: egui_gizmo::DEFAULT_SNAP_DISTANCE,
            visuals: GizmoVisuals {
                stroke_width: 4.0,
                gizmo_size: 75.0,
                highlight_color: Some(Color32::GOLD),
                x_color: Color32::from_rgb(255, 0, 148),
                y_color: Color32::from_rgb(148, 255, 0),
                z_color: Color32::from_rgb(0, 148, 255),
                s_color: Color32::WHITE,
                inactive_alpha: 0.5,
                highlight_alpha: 1.0,
            },
        }
    }
}

impl GizmoWidget {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render_mode_selection(&mut self, ui: &mut Ui) {
        egui::ComboBox::from_label("Mode")
            .selected_text(format!("{:?}", self.mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.mode, GizmoMode::Rotate, "Rotate");
                ui.selectable_value(&mut self.mode, GizmoMode::Translate, "Translate");
                ui.selectable_value(&mut self.mode, GizmoMode::Scale, "Scale");
            });

        egui::ComboBox::from_label("Orientation")
            .selected_text(format!("{:?}", self.orientation))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.orientation, GizmoOrientation::Global, "Global");
                ui.selectable_value(&mut self.orientation, GizmoOrientation::Local, "Local");
            });

        ui.end_row();
    }

    pub fn render_controls(&mut self, ui: &mut Ui) {
        self.render_mode_selection(ui);

        egui::ComboBox::from_label("Orientation")
            .selected_text(format!("{:?}", self.orientation))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.orientation, GizmoOrientation::Global, "Global");
                ui.selectable_value(&mut self.orientation, GizmoOrientation::Local, "Local");
            });

        ui.end_row();

        Slider::new(&mut self.visuals.gizmo_size, 10.0..=500.0)
            .text("Gizmo size")
            .ui(ui);
        Slider::new(&mut self.visuals.stroke_width, 0.1..=10.0)
            .text("Stroke width")
            .ui(ui);
        Slider::new(&mut self.visuals.inactive_alpha, 0.0..=1.0)
            .text("Inactive alpha")
            .ui(ui);
        Slider::new(&mut self.visuals.highlight_alpha, 0.0..=1.0)
            .text("Highlighted alpha")
            .ui(ui);

        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut self.visuals.x_color,
                Alpha::Opaque,
            );
            egui::Label::new("X axis color").wrap(false).ui(ui);
        });

        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut self.visuals.y_color,
                Alpha::Opaque,
            );
            egui::Label::new("Y axis color").wrap(false).ui(ui);
        });
        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut self.visuals.z_color,
                Alpha::Opaque,
            );
            egui::Label::new("Z axis color").wrap(false).ui(ui);
        });
        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut self.visuals.s_color,
                Alpha::Opaque,
            );
            egui::Label::new("Screen axis color").wrap(false).ui(ui);
        });

        ui.end_row();
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        model: glm::Mat4,
        view: glm::Mat4,
        projection: glm::Mat4,
    ) -> Option<GizmoResult> {
        // Snapping is enabled with ctrl key.
        let snapping = ui.input().modifiers.command;

        // Snap angle to use for rotation when snapping is enabled.
        // Smaller snap angle is used when shift key is pressed.
        let snap_angle = if ui.input().modifiers.shift {
            self.snap_angle / 2.0
        } else {
            self.snap_angle
        };

        // Snap distance to use for translation when snapping is enabled.
        // Smaller snap distance is used when shift key is pressed.
        let snap_distance = if ui.input().modifiers.shift {
            self.snap_distance / 2.0
        } else {
            self.snap_distance
        };

        let gizmo = Gizmo::new("My gizmo")
            .view_matrix(view)
            .projection_matrix(projection)
            .model_matrix(model)
            .mode(self.mode)
            .orientation(self.orientation)
            .snapping(snapping)
            .snap_angle(snap_angle)
            .snap_distance(snap_distance)
            .visuals(self.visuals);

        let response = gizmo.interact(ui);

        if let Some(gizmo_response) = self.last_gizmo_response {
            self.show_gizmo_status(ui, gizmo_response);
        }

        response
    }

    fn show_gizmo_status(&mut self, ui: &Ui, response: GizmoResult) {
        let length = glm::Vec3::from(response.value).magnitude();

        let text = match response.mode {
            GizmoMode::Rotate => format!("{:.1}°, {:.2} rad", length.to_degrees(), length),

            GizmoMode::Translate | GizmoMode::Scale => format!(
                "dX: {:.2}, dY: {:.2}, dZ: {:.2}",
                response.value[0], response.value[1], response.value[2]
            ),
        };

        let rect = ui.clip_rect();
        ui.painter().text(
            pos2(rect.left() + 400.0, rect.bottom() - 400.0),
            Align2::LEFT_BOTTOM,
            text,
            TextStyle::Heading,
            Color32::WHITE,
        );
    }
}
