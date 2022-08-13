use na::{linalg::QR, Isometry3, Translation3, UnitQuaternion};
use nalgebra as na;
use nalgebra_glm as glm;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug)]
pub struct DecomposedTransform {
    pub translation: glm::Vec3,
    pub rotation: glm::Quat,
    pub scale: glm::Vec3,
}

impl DecomposedTransform {
    pub fn as_isometry(&self) -> Isometry3<f32> {
        Isometry3::from_parts(
            Translation3::from(self.translation),
            UnitQuaternion::from_quaternion(self.rotation),
        )
    }

    pub fn euler_angles(
        &self,
    ) -> na::Matrix<f32, na::Const<3>, na::Const<1>, na::ArrayStorage<f32, 3, 1>> {
        glm::quat_euler_angles(&self.rotation)
    }
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
    pub matrix: glm::Mat4,
}

impl Transform {
    pub fn new(matrix: glm::Mat4) -> Self {
        Self { matrix }
    }

    /// Decomposes a 4x4 augmented rotation matrix without shear into translation, rotation, and scaling components
    pub fn decompose(&self) -> DecomposedTransform {
        let transform = &self.matrix;
        let translation = glm::vec3(transform.m14, transform.m24, transform.m34);

        let qr_decomposition = QR::new(*transform);
        let rotation = glm::to_quat(&qr_decomposition.q());

        let scale = transform.m44
            * glm::vec3(
                (transform.m11.powi(2) + transform.m21.powi(2) + transform.m31.powi(2)).sqrt(),
                (transform.m12.powi(2) + transform.m22.powi(2) + transform.m32.powi(2)).sqrt(),
                (transform.m13.powi(2) + transform.m23.powi(2) + transform.m33.powi(2)).sqrt(),
            );

        DecomposedTransform {
            translation,
            rotation,
            scale,
        }
    }
}
